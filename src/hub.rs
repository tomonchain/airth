//! The hub is the single interface for talking to Airth agents: dispatch work,
//! continue a run with a follow-up message, list what agents can do (skills)
//! and what they have done (runs).
//!
//! Clients (the CLI and the UI) go through here instead of wiring models, tools
//! and permissions themselves. The hub always builds the permission gate with
//! the default policy: a client chooses *who* approves, never *whether*
//! approval is needed.

use crate::agent::{Agent, AgentConfig, Observer};
use crate::cancel::CancelToken;
use crate::models::{ContentBlock, Message, Model, Role};
use crate::permissions::{Approver, Capability, PermissionGate, Policy};
use crate::runtime::Runtime;
use crate::state::{State, Status};
use crate::tools::{self, Workspace};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// Something an agent can do, as exposed to clients.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub capability: Capability,
}

/// A saved run, as listed to clients.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub id: String,
    pub objective: String,
    pub status: Status,
    /// Some process is driving this run right now.
    pub live: bool,
    pub iterations: u32,
    pub updated_at: DateTime<Utc>,
}

impl RunSummary {
    /// Status for display: a run marked running that nobody drives was interrupted.
    pub fn display_status(&self) -> &'static str {
        if self.status == Status::Running && !self.live {
            "interrupted"
        } else {
            self.status.as_str()
        }
    }
}

/// How a client takes part in a run: who approves, who watches, how to stop it.
pub struct RunHooks<'a> {
    pub approver: Box<dyn Approver>,
    pub observer: &'a mut dyn Observer,
    pub cancel: CancelToken,
}

pub struct Hub {
    workspace: Workspace,
    model: Option<Box<dyn Model>>,
}

impl Hub {
    /// A hub without a model can still list skills and runs.
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            model: None,
        }
    }

    pub fn with_model(mut self, model: Box<dyn Model>) -> Self {
        self.model = Some(model);
        self
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn model_name(&self) -> Option<String> {
        self.model.as_ref().map(|m| m.name())
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.workspace.root().join(".airth/runs")
    }

    /// The capabilities an agent dispatched by this hub has.
    pub fn skills(&self) -> Vec<Skill> {
        tools::default_tools()
            .iter()
            .map(|t| {
                let spec = t.spec();
                Skill {
                    name: spec.name,
                    description: spec.description,
                    capability: t.capability(),
                }
            })
            .collect()
    }

    /// Create and save a new run, without starting it.
    pub fn create_run(&self, objective: &str) -> Result<State> {
        let objective = objective.trim();
        if objective.is_empty() {
            bail!("the objective is empty");
        }
        let mut state = State::new(objective, Some(&self.runs_dir()));
        state.messages.push(Message::user_text(objective));
        state.save()?;
        Ok(state)
    }

    /// Load a run (`run` is an id or `last`) and add a follow-up message, ready
    /// to drive. Interrupted runs are repaired first.
    pub fn follow_up(&self, run: &str, message: &str) -> Result<State> {
        let message = message.trim();
        if message.is_empty() {
            bail!("the message is empty");
        }
        let mut state = self.load_run(run)?;
        if self.is_live(&state.id) {
            bail!(
                "run {} is in progress; wait for it or stop it first",
                state.id
            );
        }
        state.close_dangling_tool_calls();
        append_user_text(&mut state.messages, message);
        state.status = Status::Running;
        state.report = None;
        state.save()?;
        Ok(state)
    }

    /// Drive a run until it completes, fails, stops or hits the iteration limit.
    pub fn drive(&self, state: &mut State, config: AgentConfig, hooks: RunHooks) -> Result<()> {
        let model = self
            .model
            .as_deref()
            .context("no model configured for this hub")?;
        let _lock = RunLock::acquire(&self.runs_dir(), &state.id)?;
        let runtime = Runtime::new(
            tools::default_tools(),
            PermissionGate::new(Policy::default(), hooks.approver),
            self.workspace.clone(),
            hooks.cancel.clone(),
        );
        Agent::new(model, runtime, config, hooks.observer, hooks.cancel).run(state)
    }

    /// Start an agent on a new objective and drive it until it stops.
    pub fn dispatch(&self, objective: &str, config: AgentConfig, hooks: RunHooks) -> Result<State> {
        let mut state = self.create_run(objective)?;
        self.drive(&mut state, config, hooks)?;
        Ok(state)
    }

    /// Continue a run with a follow-up message and drive it until it stops.
    pub fn send(
        &self,
        run: &str,
        message: &str,
        config: AgentConfig,
        hooks: RunHooks,
    ) -> Result<State> {
        let mut state = self.follow_up(run, message)?;
        self.drive(&mut state, config, hooks)?;
        Ok(state)
    }

    /// All saved runs, oldest first. Unreadable files are reported and skipped.
    pub fn runs(&self) -> Result<Vec<RunSummary>> {
        Ok(self
            .load_all()?
            .into_iter()
            .map(|s| RunSummary {
                live: self.is_live(&s.id),
                id: s.id,
                objective: s.objective,
                status: s.status,
                iterations: s.iterations,
                updated_at: s.updated_at,
            })
            .collect())
    }

    fn load_all(&self) -> Result<Vec<State>> {
        let dir = self.runs_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut runs = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                match State::load(&path) {
                    Ok(state) => runs.push(state),
                    Err(e) => eprintln!("warning: skipping {}: {e:#}", path.display()),
                }
            }
        }
        runs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(runs)
    }

    pub fn load_run(&self, run: &str) -> Result<State> {
        if run == "last" {
            return self.load_all()?.pop().context("no runs yet");
        }
        validate_id(run)?;
        let path = self.runs_dir().join(format!("{run}.json"));
        if !path.exists() {
            bail!("no run '{run}' (see `airth runs`)");
        }
        State::load(&path)
    }

    /// Whether a live process (this one or another) is driving the run.
    pub fn is_live(&self, id: &str) -> bool {
        validate_id(id).is_ok() && RunLock::is_held(&self.runs_dir(), id)
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        bail!("invalid run id '{id}'");
    }
    Ok(())
}

/// Marks a run as being driven by this process: `<id>.lock` holds our pid and is
/// removed when the run ends. A lock whose process is gone is stale.
struct RunLock(PathBuf);

impl RunLock {
    fn path(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!("{id}.lock"))
    }

    fn acquire(dir: &Path, id: &str) -> Result<Self> {
        if Self::is_held(dir, id) {
            bail!("run {id} is already in progress");
        }
        std::fs::create_dir_all(dir)?;
        let path = Self::path(dir, id);
        std::fs::write(&path, std::process::id().to_string())?;
        Ok(Self(path))
    }

    fn is_held(dir: &Path, id: &str) -> bool {
        match std::fs::read_to_string(Self::path(dir, id)) {
            Ok(pid) => pid.trim().parse().is_ok_and(process_alive),
            Err(_) => false,
        }
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(unix)]
fn process_alive(pid: i32) -> bool {
    // SAFETY: kill(2) with signal 0 only checks for existence and permission.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_alive(_pid: i32) -> bool {
    true
}

/// Add a user message, merging into the last message if it is already a user
/// turn (e.g. tool results from a run that hit its iteration limit).
fn append_user_text(messages: &mut Vec<Message>, text: &str) {
    match messages.last_mut() {
        Some(last) if last.role == Role::User => last.content.push(ContentBlock::Text {
            text: text.to_string(),
        }),
        _ => messages.push(Message::user_text(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::NoObserver;
    use crate::models::{ModelRequest, ModelResponse, StopReason};
    use crate::permissions::{Answer, PermissionRequest};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    struct Scripted {
        responses: Mutex<Vec<ModelResponse>>,
        seen: Arc<Mutex<Vec<Vec<Message>>>>,
    }

    impl Model for Scripted {
        fn name(&self) -> String {
            "scripted".into()
        }
        fn complete(&self, req: &ModelRequest) -> Result<ModelResponse> {
            self.seen.lock().unwrap().push(req.messages.to_vec());
            Ok(self.responses.lock().unwrap().remove(0))
        }
    }

    struct Never;
    impl Approver for Never {
        fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
            Ok(Answer::No("not in tests".into()))
        }
    }

    fn resp(content: Vec<ContentBlock>, stop_reason: StopReason) -> ModelResponse {
        ModelResponse {
            content,
            stop_reason,
            model: "scripted".into(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    fn config(max_iterations: u32) -> AgentConfig {
        AgentConfig { max_iterations }
    }

    fn hooks(observer: &mut NoObserver) -> RunHooks<'_> {
        RunHooks {
            approver: Box::new(Never),
            observer,
            cancel: CancelToken::default(),
        }
    }

    #[test]
    fn skills_expose_tools_with_capabilities() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Workspace::new(dir.path()).unwrap());
        let skills = hub.skills();
        let cap = |name: &str| skills.iter().find(|s| s.name == name).unwrap().capability;
        assert_eq!(cap("read_file"), Capability::Read);
        assert_eq!(cap("write_file"), Capability::Write);
        assert_eq!(cap("execute_command"), Capability::Execute);
    }

    #[test]
    fn dispatch_without_model_fails() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Workspace::new(dir.path()).unwrap());
        assert!(
            hub.dispatch("x", config(1), hooks(&mut NoObserver))
                .is_err()
        );
    }

    #[test]
    fn send_continues_a_stopped_run_with_a_fresh_budget() {
        let dir = tempfile::tempdir().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let model = Scripted {
            responses: Mutex::new(vec![
                resp(
                    vec![ContentBlock::ToolUse {
                        id: "1".into(),
                        name: "list_directory".into(),
                        input: json!({}),
                    }],
                    StopReason::ToolUse,
                ),
                resp(
                    vec![ContentBlock::Text {
                        text: "Done.".into(),
                    }],
                    StopReason::EndTurn,
                ),
            ]),
            seen: seen.clone(),
        };
        let hub = Hub::new(Workspace::new(dir.path()).unwrap()).with_model(Box::new(model));

        let first = hub
            .dispatch("look around", config(1), hooks(&mut NoObserver))
            .unwrap();
        assert_eq!(first.status, Status::Stopped);
        assert_eq!(first.iterations, 1);
        assert_eq!(hub.runs().unwrap().len(), 1);
        assert!(!hub.is_live(&first.id), "lock released after the run");

        let second = hub
            .send("last", "keep going", config(1), hooks(&mut NoObserver))
            .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.status, Status::Completed);
        assert_eq!(second.report.as_deref(), Some("Done."));
        assert_eq!(second.iterations, 2);

        // The follow-up was merged into the pending user turn (after the tool result).
        let sent = &seen.lock().unwrap()[1];
        assert_eq!(sent.len(), 3);
        assert_eq!(
            sent[2].content.last(),
            Some(&ContentBlock::Text {
                text: "keep going".into()
            })
        );

        // The continued run was saved back to the same file.
        let runs = hub.runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, Status::Completed);
    }

    #[test]
    fn interrupted_runs_can_be_continued() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Workspace::new(dir.path()).unwrap());
        let mut state = hub.create_run("obj").unwrap();
        state.messages.push(Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "t".into(),
                name: "list_directory".into(),
                input: json!({}),
            }],
        });
        state.save().unwrap();
        // A lock left by a process that no longer exists.
        std::fs::write(hub.runs_dir().join(format!("{}.lock", state.id)), "999999").unwrap();

        let summary = &hub.runs().unwrap()[0];
        assert_eq!(summary.display_status(), "interrupted");

        let resumed = hub.follow_up(&state.id, "carry on").unwrap();
        assert_eq!(resumed.status, Status::Running);
        let last = resumed.messages.last().unwrap();
        assert!(matches!(
            &last.content[0],
            ContentBlock::ToolResult { is_error: true, .. }
        ));
        assert_eq!(
            last.content[1],
            ContentBlock::Text {
                text: "carry on".into()
            }
        );
    }

    #[test]
    fn live_runs_cannot_be_continued() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Workspace::new(dir.path()).unwrap());
        let state = hub.create_run("obj").unwrap();
        let _lock = RunLock::acquire(&hub.runs_dir(), &state.id).unwrap();
        assert!(hub.is_live(&state.id));
        assert!(hub.follow_up(&state.id, "hi").is_err());
    }

    #[test]
    fn send_rejects_unknown_or_malformed_runs() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Workspace::new(dir.path()).unwrap());
        assert!(hub.follow_up("last", "hi").is_err());
        assert!(hub.follow_up("nope", "hi").is_err());
        assert!(hub.follow_up("../x", "hi").is_err());
    }
}
