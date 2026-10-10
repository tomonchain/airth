//! The hub is the single interface for talking to Airth agents: dispatch work
//! in a realm, continue a run with a follow-up message, review and adopt a
//! run's change, and list what agents can do (skills) and have done (runs).
//!
//! Clients (the CLI and the UI) go through here instead of wiring models, tools
//! and permissions themselves. The hub builds each run's permission gate from
//! its realm's policy, and its workspace from the realm's isolation rules: a
//! client chooses *who* approves, never *whether* approval is needed or what
//! the run may reach.

use crate::agent::{Agent, AgentConfig, Briefing, Observer, system_prompt};
use crate::cancel::CancelToken;
use crate::home::Home;
use crate::isolation::{self, ChangeStatus, CheckResult, CheckRun, Isolation};
use crate::memory::MemoryStore;
use crate::models::{self, ContentBlock, Message, Model, Role};
use crate::permissions::{Approver, Capability, PermissionGate};
use crate::realm::{PROVIDER_KEYS, Realm};
use crate::runtime::Runtime;
use crate::state::{State, Status};
use crate::tools::{self, Access, Workspace};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Provider settings that are credentials.
const CREDENTIAL_KEYS: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

/// How long one check command may run.
const CHECK_TIMEOUT: Duration = Duration::from_secs(1800);
const CHECK_OUTPUT: usize = 20_000;

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
    /// For isolated runs: whether the change is open, adopted or discarded.
    pub change: Option<ChangeStatus>,
    /// For isolated runs with changes: whether the latest checks passed.
    pub checks_passed: Option<bool>,
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
    home: Home,
    /// A model used for every realm instead of each realm's configuration (tests).
    model: Option<Arc<dyn Model>>,
}

impl Hub {
    pub fn new(home: Home) -> Self {
        Self { home, model: None }
    }

    #[cfg(test)]
    pub fn with_model(mut self, model: Box<dyn Model>) -> Self {
        self.model = Some(Arc::from(model));
        self
    }

    pub fn home(&self) -> &Home {
        &self.home
    }

    /// The model for a realm: its own provider credentials if it has some
    /// (realm secrets), else the environment's.
    pub fn model_for(&self, realm: &Realm) -> Result<Arc<dyn Model>> {
        if let Some(model) = &self.model {
            return Ok(model.clone());
        }
        let secrets = realm.secrets()?;
        let get = |key: &str| {
            secrets
                .iter()
                .find(|(k, v)| k == key && !v.is_empty())
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(key).ok())
        };
        Ok(Arc::from(models::from_settings(&get)?))
    }

    /// The capabilities an agent dispatched by this hub has.
    pub fn skills(&self) -> Vec<Skill> {
        tools::realm_tools(MemoryStore::new("/dev/null"), String::new())
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

    /// Create and save a new run, without starting it. In an isolated realm
    /// this also creates the run's worktree.
    pub fn create_run(&self, realm: &Realm, objective: &str) -> Result<State> {
        let objective = objective.trim();
        if objective.is_empty() {
            bail!("the objective is empty");
        }
        if realm.config.archived {
            bail!("realm '{}' is archived", realm.name());
        }
        let mut state = State::new(objective, Some(&realm.runs_dir()));
        state.realm = realm.name().to_string();
        if realm.config.isolation == Isolation::Worktree {
            let root = realm.root()?;
            state.change = Some(
                isolation::create(&root, &realm.worktrees_dir(), &state.id)
                    .context("cannot create the run's isolated worktree")?,
            );
        }
        state.messages.push(Message::user_text(objective));
        state.save()?;
        Ok(state)
    }

    /// Load a run (`run` is an id or `last`) and add a follow-up message, ready
    /// to drive. Interrupted runs are repaired first.
    pub fn follow_up(&self, realm: &Realm, run: &str, message: &str) -> Result<State> {
        let message = message.trim();
        if message.is_empty() {
            bail!("the message is empty");
        }
        let mut state = self.load_run(realm, run)?;
        if self.is_live(realm, &state.id) {
            bail!(
                "run {} is in progress; wait for it or stop it first",
                state.id
            );
        }
        if let Some(change) = &state.change
            && change.status != ChangeStatus::Open
        {
            bail!(
                "this run's change was {}; start a new run instead",
                change.status.as_str()
            );
        }
        state.close_dangling_tool_calls();
        append_user_text(&mut state.messages, message);
        state.status = Status::Running;
        state.report = None;
        state.save()?;
        Ok(state)
    }

    /// Drive a run until it completes, fails, stops or hits the iteration
    /// limit. Then, for an isolated run, commit its change and run the checks.
    pub fn drive(
        &self,
        realm: &Realm,
        state: &mut State,
        config: AgentConfig,
        hooks: RunHooks,
    ) -> Result<()> {
        let model = self.model_for(realm)?;
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        let workspace = self.workspace_for(realm, state)?;
        let briefing = self.briefing(realm, state, &workspace)?;
        let runtime = Runtime::new(
            tools::realm_tools(realm.memory(), format!("run:{}", state.id)),
            PermissionGate::new(realm.config.policy.clone(), hooks.approver),
            workspace.clone(),
            hooks.cancel.clone(),
        );
        let observer = hooks.observer;
        let result = Agent::new(
            &*model,
            runtime,
            config,
            &mut *observer,
            hooks.cancel.clone(),
        )
        .with_system(system_prompt(&briefing))
        .run(state);

        if let Some(change) = &mut state.change {
            let message = format!(
                "airth: {}\n\nRun {} in realm {}.",
                first_line(&state.objective, 72),
                state.id,
                state.realm
            );
            if let Err(e) = isolation::commit(change, &message) {
                state.report = Some(format!(
                    "{}\n\nCould not commit the run's change: {e:#}",
                    state.report.as_deref().unwrap_or_default()
                ));
            }
            state.save()?;
            observer.changed(state);
        }
        let wants_checks = state.status == Status::Completed
            && state.change.as_ref().is_some_and(|c| c.has_changes())
            && !realm.config.checks.is_empty();
        if wants_checks && !hooks.cancel.is_cancelled() {
            self.run_checks(realm, state, &workspace, observer, &hooks.cancel)?;
        }
        result
    }

    /// Start an agent on a new objective and drive it until it stops.
    pub fn dispatch(
        &self,
        realm: &Realm,
        objective: &str,
        config: AgentConfig,
        hooks: RunHooks,
    ) -> Result<State> {
        let mut state = self.create_run(realm, objective)?;
        self.drive(realm, &mut state, config, hooks)?;
        Ok(state)
    }

    /// Continue a run with a follow-up message and drive it until it stops.
    pub fn send(
        &self,
        realm: &Realm,
        run: &str,
        message: &str,
        config: AgentConfig,
        hooks: RunHooks,
    ) -> Result<State> {
        let mut state = self.follow_up(realm, run, message)?;
        self.drive(realm, &mut state, config, hooks)?;
        Ok(state)
    }

    /// Run the realm's checks on a run's change again.
    pub fn check(
        &self,
        realm: &Realm,
        run: &str,
        observer: &mut dyn Observer,
        cancel: &CancelToken,
    ) -> Result<State> {
        let mut state = self.load_run(realm, run)?;
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        match &state.change {
            Some(c) if c.status == ChangeStatus::Open => {}
            Some(c) => bail!("this run's change was {}", c.status.as_str()),
            None => bail!("this run has no isolated change to check"),
        }
        if realm.config.checks.is_empty() {
            bail!(
                "realm '{}' has no checks configured (airth realm checks ...)",
                realm.name()
            );
        }
        let workspace = self.workspace_for(realm, &state)?;
        self.run_checks(realm, &mut state, &workspace, observer, cancel)?;
        Ok(state)
    }

    fn run_checks(
        &self,
        realm: &Realm,
        state: &mut State,
        workspace: &Workspace,
        observer: &mut dyn Observer,
        cancel: &CancelToken,
    ) -> Result<()> {
        let Some(change) = &mut state.change else {
            return Ok(());
        };
        change.checks = Some(CheckRun {
            commit: change.head.clone(),
            started_at: Utc::now(),
            finished: false,
            results: Vec::new(),
        });
        state.save()?;
        observer.changed(state);
        for command in &realm.config.checks {
            if cancel.is_cancelled() {
                break;
            }
            let start = Instant::now();
            let (ok, output) = match tools::run_shell(command, workspace, cancel, CHECK_TIMEOUT) {
                Ok(out) => (out.success(), out.streams()),
                Err(e) => (false, format!("{e:#}")),
            };
            let output = workspace.redact(&tail(&output, CHECK_OUTPUT));
            let checks = state
                .change
                .as_mut()
                .and_then(|c| c.checks.as_mut())
                .expect("checks were just started");
            checks.results.push(CheckResult {
                command: command.clone(),
                ok,
                output,
                duration_ms: start.elapsed().as_millis() as u64,
            });
            state.save()?;
            observer.changed(state);
            if !ok {
                break;
            }
        }
        if let Some(checks) = state.change.as_mut().and_then(|c| c.checks.as_mut()) {
            checks.finished = !cancel.is_cancelled();
        }
        state.save()?;
        observer.changed(state);
        Ok(())
    }

    /// The diff of a run's change against where it started.
    pub fn diff(&self, realm: &Realm, run: &str) -> Result<String> {
        let state = self.load_run(realm, run)?;
        let change = state
            .change
            .as_ref()
            .context("this run has no isolated change")?;
        isolation::diff(change)
    }

    /// Merge a run's change into the realm's checkout. Its checks must have
    /// passed on the latest commit, if the realm has checks.
    pub fn adopt(&self, realm: &Realm, run: &str) -> Result<State> {
        let mut state = self.load_run(realm, run)?;
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        let checks_needed = !realm.config.checks.is_empty();
        let change = state
            .change
            .as_mut()
            .context("this run has no isolated change")?;
        if checks_needed && !change.checks_passed() {
            bail!("the checks have not passed on this change's latest commit (run `airth check`)");
        }
        let message = format!(
            "Adopt Airth run {}: {}",
            state.id,
            first_line(&state.objective, 60)
        );
        isolation::adopt(change, &message)?;
        state.save()?;
        Ok(state)
    }

    /// Throw a run's change away.
    pub fn discard(&self, realm: &Realm, run: &str) -> Result<State> {
        let mut state = self.load_run(realm, run)?;
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        let change = state
            .change
            .as_mut()
            .context("this run has no isolated change")?;
        isolation::discard(change)?;
        state.save()?;
        Ok(state)
    }

    /// Where a run works and what it may reach. Every run is denied the Airth
    /// home and the other realms' roots; an isolated run may read (not write)
    /// the owner's checkout, and write only its worktree and the git data it
    /// needs.
    pub fn workspace_for(&self, realm: &Realm, state: &State) -> Result<Workspace> {
        let root = match &state.change {
            Some(c) if c.status == ChangeStatus::Open => c.path.clone(),
            Some(c) => bail!("this run's change was {}", c.status.as_str()),
            None => realm.root()?,
        };
        let mut ws = Workspace::new(&root)?.with_rule(self.home.dir(), Access::None);
        for other in self.home.realms()? {
            if other.name() != realm.name()
                && let Some(other_root) = &other.config.root
            {
                ws = ws.with_rule(other_root, Access::None);
            }
        }
        let mut env = realm.command_env()?;
        if let Some(change) = &state.change {
            ws = ws.with_rule(&change.repo, Access::Read);
            let common = isolation::common_dir(&change.repo)?;
            ws = ws
                .with_rule(common.join("objects"), Access::ReadWrite)
                .with_rule(worktree_git_dir(&common, &root), Access::ReadWrite);
            if root.join("Cargo.toml").exists() {
                // Share build output across the realm's runs: a fresh worktree
                // would otherwise rebuild everything.
                let target = realm.cache_dir().join("target");
                std::fs::create_dir_all(&target)?;
                ws = ws.with_rule(&target, Access::ReadWrite);
                env.push(("CARGO_TARGET_DIR".into(), target.display().to_string()));
            }
        }
        // Provider settings like the model name are not secret; keys are.
        let not_secret = |k: &str| PROVIDER_KEYS.contains(&k) && !CREDENTIAL_KEYS.contains(&k);
        let mut redact: Vec<(String, String)> = realm
            .secrets()?
            .into_iter()
            .filter(|(k, _)| !not_secret(k))
            .collect();
        for key in CREDENTIAL_KEYS {
            if let Ok(value) = std::env::var(key) {
                redact.push((key.to_string(), value));
            }
        }
        Ok(ws.with_secrets(env, redact).with_sandbox(true))
    }

    fn briefing(&self, realm: &Realm, state: &State, ws: &Workspace) -> Result<Briefing> {
        let owner = if realm.config.share_owner_profile {
            self.home.owner()?.prompt_section()
        } else {
            None
        };
        Ok(Briefing {
            workspace: ws.root().display().to_string(),
            realm: Some(realm.name().to_string()),
            realm_description: realm.config.description.clone(),
            owner,
            memory: realm.memory().list()?.into_iter().map(|m| m.text).collect(),
            isolated: state.change.is_some(),
            checks: realm.config.checks.clone(),
        })
    }

    /// All saved runs of a realm, oldest first. Unreadable files are reported and skipped.
    pub fn runs(&self, realm: &Realm) -> Result<Vec<RunSummary>> {
        Ok(self
            .load_all(realm)?
            .into_iter()
            .map(|s| RunSummary {
                live: self.is_live(realm, &s.id),
                change: s.change.as_ref().map(|c| c.status),
                checks_passed: s
                    .change
                    .as_ref()
                    .filter(|c| c.has_changes() && c.checks.as_ref().is_some_and(|k| k.finished))
                    .map(|c| c.checks_passed()),
                id: s.id,
                objective: s.objective,
                status: s.status,
                iterations: s.iterations,
                updated_at: s.updated_at,
            })
            .collect())
    }

    fn load_all(&self, realm: &Realm) -> Result<Vec<State>> {
        let dir = realm.runs_dir();
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

    pub fn load_run(&self, realm: &Realm, run: &str) -> Result<State> {
        if run == "last" {
            return self.load_all(realm)?.pop().context("no runs yet");
        }
        validate_id(run)?;
        let path = realm.runs_dir().join(format!("{run}.json"));
        if !path.exists() {
            bail!(
                "no run '{run}' in realm '{}' (see `airth runs`)",
                realm.name()
            );
        }
        State::load(&path)
    }

    /// Whether a live process (this one or another) is driving the run.
    pub fn is_live(&self, realm: &Realm, id: &str) -> bool {
        validate_id(id).is_ok() && RunLock::is_held(&realm.runs_dir(), id)
    }

    /// Copy runs saved by Airth v0.1 in `<dir>/.airth/runs` into a realm.
    pub fn import_runs(&self, realm: &Realm, dir: &Path) -> Result<usize> {
        let legacy = dir.join(".airth/runs");
        let Ok(entries) = std::fs::read_dir(&legacy) else {
            return Ok(0);
        };
        std::fs::create_dir_all(realm.runs_dir())?;
        let mut count = 0;
        for entry in entries {
            let path = entry?.path();
            let Some(name) = path.file_name() else {
                continue;
            };
            let target = realm.runs_dir().join(name);
            if path.extension().is_some_and(|e| e == "json") && !target.exists() {
                let mut state = State::load(&path)?;
                state.realm = realm.name().to_string();
                if state.status == Status::Running {
                    state.status = Status::Stopped;
                }
                let text = serde_json::to_string_pretty(&state)?;
                std::fs::write(&target, text)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// The per-worktree git directory: `<common>/worktrees/<name>`, found by
/// matching its `gitdir` file back to the worktree.
fn worktree_git_dir(common: &Path, worktree: &Path) -> PathBuf {
    let dir = common.join("worktrees");
    let fallback = dir.join(worktree.file_name().unwrap_or_default());
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return fallback;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            std::fs::read_to_string(p.join("gitdir")).is_ok_and(|g| {
                Path::new(g.trim())
                    .parent()
                    .and_then(|w| w.canonicalize().ok())
                    .is_some_and(|w| w == worktree)
            })
        })
        .unwrap_or(fallback)
}

fn first_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(max - 1).collect::<String>())
    }
}

/// The end of a long output: that is where errors usually are.
fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("... [{start} bytes cut]\n{}", &text[start..])
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
    use crate::home::NewRealm;
    use crate::models::{ModelRequest, ModelResponse, StopReason};
    use crate::permissions::{Answer, PermissionRequest, Rule};
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Scripted {
        responses: Mutex<Vec<ModelResponse>>,
        seen: Arc<Mutex<Vec<Vec<Message>>>>,
        systems: Arc<Mutex<Vec<String>>>,
    }

    impl Model for Scripted {
        fn name(&self) -> String {
            "scripted".into()
        }
        fn complete(&self, req: &ModelRequest) -> Result<ModelResponse> {
            self.seen.lock().unwrap().push(req.messages.to_vec());
            self.systems.lock().unwrap().push(req.system.to_string());
            Ok(self.responses.lock().unwrap().remove(0))
        }
    }

    struct Fixed(bool);
    impl Approver for Fixed {
        fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
            Ok(if self.0 {
                Answer::Yes
            } else {
                Answer::No("not in tests".into())
            })
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

    fn call(id: &str, name: &str, input: serde_json::Value) -> ModelResponse {
        resp(
            vec![ContentBlock::ToolUse {
                id: id.into(),
                name: name.into(),
                input,
            }],
            StopReason::ToolUse,
        )
    }

    fn done(text: &str) -> ModelResponse {
        resp(
            vec![ContentBlock::Text { text: text.into() }],
            StopReason::EndTurn,
        )
    }

    fn config(max_iterations: u32) -> AgentConfig {
        AgentConfig { max_iterations }
    }

    fn hooks(observer: &mut NoObserver, approve: bool) -> RunHooks<'_> {
        RunHooks {
            approver: Box::new(Fixed(approve)),
            observer,
            cancel: CancelToken::default(),
        }
    }

    /// A home with one realm rooted at a fresh directory.
    struct Fixture {
        _home: tempfile::TempDir,
        root: tempfile::TempDir,
        home: Home,
        realm: Realm,
    }

    fn fixture() -> Fixture {
        let home_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "work".into(),
                description: "the day job".into(),
                root: Some(root.path().to_path_buf()),
                isolation: None,
            })
            .unwrap();
        Fixture {
            _home: home_dir,
            root,
            home,
            realm,
        }
    }

    type Seen = Arc<Mutex<Vec<Vec<Message>>>>;

    fn make_hub(
        home: &Home,
        responses: Vec<ModelResponse>,
    ) -> (Hub, Seen, Arc<Mutex<Vec<String>>>) {
        let model = Scripted {
            responses: Mutex::new(responses),
            ..Scripted::default()
        };
        let (seen, systems) = (model.seen.clone(), model.systems.clone());
        (
            Hub::new(home.clone()).with_model(Box::new(model)),
            seen,
            systems,
        )
    }

    #[test]
    fn skills_expose_tools_with_capabilities() {
        let f = fixture();
        let skills = Hub::new(f.home.clone()).skills();
        let cap = |name: &str| skills.iter().find(|s| s.name == name).unwrap().capability;
        assert_eq!(cap("read_file"), Capability::Read);
        assert_eq!(cap("write_file"), Capability::Write);
        assert_eq!(cap("execute_command"), Capability::Execute);
        assert_eq!(cap("remember"), Capability::Memory);
    }

    #[test]
    fn send_continues_a_stopped_run_with_a_fresh_budget() {
        let f = fixture();
        let (hub, seen, _) = make_hub(
            &f.home,
            vec![call("1", "list_directory", json!({})), done("Done.")],
        );

        let first = hub
            .dispatch(
                &f.realm,
                "look around",
                config(1),
                hooks(&mut NoObserver, false),
            )
            .unwrap();
        assert_eq!(first.status, Status::Stopped);
        assert_eq!(first.realm, "work");
        assert_eq!(hub.runs(&f.realm).unwrap().len(), 1);
        assert!(
            !hub.is_live(&f.realm, &first.id),
            "lock released after the run"
        );

        let second = hub
            .send(
                &f.realm,
                "last",
                "keep going",
                config(1),
                hooks(&mut NoObserver, false),
            )
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
        assert_eq!(hub.runs(&f.realm).unwrap()[0].status, Status::Completed);
    }

    #[test]
    fn interrupted_runs_can_be_continued() {
        let f = fixture();
        let hub = Hub::new(f.home.clone());
        let mut state = hub.create_run(&f.realm, "obj").unwrap();
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
        std::fs::write(
            f.realm.runs_dir().join(format!("{}.lock", state.id)),
            "999999",
        )
        .unwrap();

        assert_eq!(
            hub.runs(&f.realm).unwrap()[0].display_status(),
            "interrupted"
        );
        let resumed = hub.follow_up(&f.realm, &state.id, "carry on").unwrap();
        assert_eq!(resumed.status, Status::Running);
        let last = resumed.messages.last().unwrap();
        assert!(matches!(
            &last.content[0],
            ContentBlock::ToolResult { is_error: true, .. }
        ));
    }

    #[test]
    fn live_runs_cannot_be_continued() {
        let f = fixture();
        let hub = Hub::new(f.home.clone());
        let state = hub.create_run(&f.realm, "obj").unwrap();
        let _lock = RunLock::acquire(&f.realm.runs_dir(), &state.id).unwrap();
        assert!(hub.is_live(&f.realm, &state.id));
        assert!(hub.follow_up(&f.realm, &state.id, "hi").is_err());
    }

    #[test]
    fn send_rejects_unknown_or_malformed_runs() {
        let f = fixture();
        let hub = Hub::new(f.home.clone());
        assert!(hub.follow_up(&f.realm, "last", "hi").is_err());
        assert!(hub.follow_up(&f.realm, "nope", "hi").is_err());
        assert!(hub.follow_up(&f.realm, "../x", "hi").is_err());
    }

    /// v0.2 acceptance: two realms, neither sees the other's private data.
    #[test]
    fn realms_do_not_see_each_other() {
        let f = fixture();
        std::fs::write(f.root.path().join("contract.txt"), "work-confidential").unwrap();
        f.realm
            .memory()
            .add("The client is Initech", "owner")
            .unwrap();
        f.realm
            .set_secret("WORK_TOKEN", "work-token-value")
            .unwrap();

        let personal = f
            .home
            .create_realm(NewRealm {
                name: "personal".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let mut policy = personal.config.policy.clone();
        policy.execute = Rule::Allow;
        let mut personal = personal;
        personal.config.policy = policy;
        personal.save().unwrap();
        personal.memory().add("Likes hiking", "owner").unwrap();

        let contract = f.root.path().canonicalize().unwrap().join("contract.txt");
        let secrets = f.home.dir().join("realms/work/secrets.env");
        let (hub, seen, systems) = make_hub(
            &f.home,
            vec![
                call("1", "read_file", json!({"path": contract})),
                call("2", "read_file", json!({"path": secrets})),
                call(
                    "3",
                    "execute_command",
                    json!({"command": format!("echo \"[$WORK_TOKEN]\"; cat {} {}", contract.display(), secrets.display())}),
                ),
                done("ok"),
            ],
        );
        let state = hub
            .dispatch(
                &personal,
                "snoop",
                config(10),
                hooks(&mut NoObserver, false),
            )
            .unwrap();
        assert_eq!(state.status, Status::Completed);

        let system = &systems.lock().unwrap()[0];
        assert!(system.contains("Likes hiking"));
        assert!(!system.contains("Initech"));

        assert!(
            state.events[0].result.contains("outside the workspace"),
            "{}",
            state.events[0].result
        );
        assert!(
            state.events[1].result.contains("outside the workspace"),
            "{}",
            state.events[1].result
        );
        let shell = &state.events[2].result;
        assert!(shell.contains("[]"), "{shell}");
        assert!(!shell.contains("work-token-value"), "{shell}");
        if crate::sandbox::available() {
            assert!(!shell.contains("work-confidential"), "{shell}");
        }
        let everything = format!("{:?}", seen.lock().unwrap());
        assert!(!everything.contains("work-token-value"));

        // The work realm itself can read its file, and never sees its token in clear.
        let (hub, _, systems) = make_hub(
            &f.home,
            vec![
                call("1", "read_file", json!({"path": "contract.txt"})),
                call(
                    "2",
                    "execute_command",
                    json!({"command": "echo $WORK_TOKEN"}),
                ),
                done("ok"),
            ],
        );
        let state = hub
            .dispatch(&f.realm, "read", config(10), hooks(&mut NoObserver, true))
            .unwrap();
        assert_eq!(state.events[0].result, "work-confidential");
        assert!(
            state.events[1].result.contains("[secret:WORK_TOKEN]"),
            "{}",
            state.events[1].result
        );
        assert!(systems.lock().unwrap()[0].contains("Initech"));
    }

    /// v0.1 acceptance, minus the model: the change happens in a worktree, is
    /// committed and checked, and reaches the owner's checkout only on adoption.
    #[test]
    fn isolated_runs_are_checked_then_adopted() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        let repo = crate::isolation::tests::repo();
        let mut realm = home
            .create_realm(NewRealm {
                name: "code".into(),
                description: String::new(),
                root: Some(repo.path().to_path_buf()),
                isolation: None,
            })
            .unwrap();
        assert_eq!(realm.config.isolation, Isolation::Worktree);
        realm.config.checks = vec!["grep -q improved a.txt".into()];
        realm.save().unwrap();

        let (hub, _, systems) = make_hub(
            &home,
            vec![
                call(
                    "1",
                    "write_file",
                    json!({"path": "a.txt", "content": "improved\n"}),
                ),
                // Writing the owner's checkout from a command is stopped by the sandbox.
                call(
                    "2",
                    "execute_command",
                    json!({"command": if crate::sandbox::available() {
                        format!("echo x > {}/a.txt", repo.path().display())
                    } else {
                        "true".into()
                    }}),
                ),
                done("Improved a.txt."),
            ],
        );
        let state = hub
            .dispatch(&realm, "improve", config(10), hooks(&mut NoObserver, true))
            .unwrap();
        assert!(systems.lock().unwrap()[0].contains("isolated git worktree"));
        assert_eq!(state.status, Status::Completed);
        if crate::sandbox::available() {
            assert!(
                state.events[1].result.contains("Operation not permitted"),
                "{}",
                state.events[1].result
            );
        }
        let change = state.change.clone().unwrap();
        assert_eq!(change.files, vec!["M\ta.txt"]);
        assert!(change.checks_passed(), "{:?}", change.checks);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "one\n"
        );
        assert!(hub.diff(&realm, &state.id).unwrap().contains("+improved"));
        let summary = &hub.runs(&realm).unwrap()[0];
        assert_eq!(summary.checks_passed, Some(true));

        std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["config", "user.email", "t@t"])
            .status()
            .unwrap();
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["config", "user.name", "t"])
            .status()
            .unwrap();
        let adopted = hub.adopt(&realm, &state.id).unwrap();
        assert_eq!(adopted.change.unwrap().status, ChangeStatus::Adopted);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "improved\n"
        );
        assert!(hub.follow_up(&realm, &state.id, "more").is_err());
    }

    #[test]
    fn failed_checks_block_adoption() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        let repo = crate::isolation::tests::repo();
        let mut realm = home
            .create_realm(NewRealm {
                name: "code".into(),
                description: String::new(),
                root: Some(repo.path().to_path_buf()),
                isolation: None,
            })
            .unwrap();
        realm.config.checks = vec!["false".into(), "echo never".into()];
        realm.save().unwrap();
        let (hub, _, _) = make_hub(
            &home,
            vec![
                call("1", "write_file", json!({"path": "b.txt", "content": "x"})),
                done("Done."),
            ],
        );
        let state = hub
            .dispatch(&realm, "add b", config(10), hooks(&mut NoObserver, true))
            .unwrap();
        let checks = state.change.as_ref().unwrap().checks.clone().unwrap();
        assert!(checks.finished);
        assert_eq!(checks.results.len(), 1, "stops at the first failure");
        assert!(!checks.results[0].ok);
        assert!(hub.adopt(&realm, &state.id).is_err());
        hub.discard(&realm, &state.id).unwrap();
        assert!(!repo.path().join("b.txt").exists());
    }
}
