//! The hub is the single interface for talking to Airth agents: dispatch work,
//! continue a run with a follow-up message, list what agents can do (skills)
//! and what they have done (runs).
//!
//! Clients (the CLI today; perhaps a server or other agents later) go through
//! here instead of wiring models, tools and permissions themselves. The hub
//! always builds the permission gate with the default policy: a client chooses
//! *who* approves, never *whether* approval is needed.

use crate::agent::{Agent, AgentConfig};
use crate::models::{ContentBlock, Message, Model, Role};
use crate::permissions::{Approver, Capability, PermissionGate, Policy};
use crate::runtime::Runtime;
use crate::state::{State, Status};
use crate::tools::{self, Workspace};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

/// Something an agent can do, as exposed to clients.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub capability: Capability,
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

    /// Start an agent on a new objective and drive it until it stops.
    pub fn dispatch(
        &self,
        objective: &str,
        config: AgentConfig,
        approver: Box<dyn Approver>,
    ) -> Result<State> {
        let mut state = State::new(objective, Some(&self.runs_dir()));
        self.drive(&mut state, config, approver)?;
        Ok(state)
    }

    /// Continue a finished run (`run` is an id or `last`) with a follow-up message.
    pub fn send(
        &self,
        run: &str,
        message: &str,
        config: AgentConfig,
        approver: Box<dyn Approver>,
    ) -> Result<State> {
        let mut state = self.load_run(run)?;
        if state.status == Status::Running {
            bail!(
                "run {} is still marked running (in progress elsewhere, or interrupted); cannot continue it",
                state.id
            );
        }
        append_user_text(&mut state.messages, message);
        state.status = Status::Running;
        state.report = None;
        self.drive(&mut state, config, approver)?;
        Ok(state)
    }

    /// All saved runs, oldest first. Unreadable files are reported and skipped.
    pub fn runs(&self) -> Result<Vec<State>> {
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

    fn load_run(&self, run: &str) -> Result<State> {
        if run == "last" {
            return self.runs()?.pop().context("no runs yet");
        }
        if run.is_empty() || run.contains(['/', '\\']) || run.starts_with('.') {
            bail!("invalid run id '{run}'");
        }
        let path = self.runs_dir().join(format!("{run}.json"));
        if !path.exists() {
            bail!("no run '{run}' (see `airth runs`)");
        }
        State::load(&path)
    }

    fn drive(
        &self,
        state: &mut State,
        config: AgentConfig,
        approver: Box<dyn Approver>,
    ) -> Result<()> {
        let model = self
            .model
            .as_deref()
            .context("no model configured for this hub")?;
        let runtime = Runtime::new(
            tools::default_tools(),
            PermissionGate::new(Policy::default(), approver),
            self.workspace.clone(),
        );
        Agent::new(model, runtime, config).run(state)
    }
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
    use crate::models::{ModelRequest, ModelResponse, StopReason};
    use crate::permissions::{Answer, PermissionRequest};
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Scripted {
        responses: RefCell<Vec<ModelResponse>>,
        seen: Rc<RefCell<Vec<Vec<Message>>>>,
    }

    impl Model for Scripted {
        fn name(&self) -> String {
            "scripted".into()
        }
        fn complete(&self, req: &ModelRequest) -> Result<ModelResponse> {
            self.seen.borrow_mut().push(req.messages.to_vec());
            Ok(self.responses.borrow_mut().remove(0))
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
        AgentConfig {
            max_iterations,
            verbose: false,
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
        assert!(hub.dispatch("x", config(1), Box::new(Never)).is_err());
    }

    #[test]
    fn send_continues_a_stopped_run_with_a_fresh_budget() {
        let dir = tempfile::tempdir().unwrap();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let model = Scripted {
            responses: RefCell::new(vec![
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
            .dispatch("look around", config(1), Box::new(Never))
            .unwrap();
        assert_eq!(first.status, Status::Stopped);
        assert_eq!(first.iterations, 1);
        assert_eq!(hub.runs().unwrap().len(), 1);

        let second = hub
            .send("last", "keep going", config(1), Box::new(Never))
            .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.status, Status::Completed);
        assert_eq!(second.report.as_deref(), Some("Done."));
        assert_eq!(second.iterations, 2);

        // The follow-up was merged into the pending user turn (after the tool result).
        let sent = &seen.borrow()[1];
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
    fn send_rejects_unknown_or_malformed_runs() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Workspace::new(dir.path()).unwrap());
        assert!(hub.send("last", "hi", config(1), Box::new(Never)).is_err());
        assert!(hub.send("nope", "hi", config(1), Box::new(Never)).is_err());
        assert!(hub.send("../x", "hi", config(1), Box::new(Never)).is_err());
    }
}
