//! The hub is the single interface for talking to Airth agents: dispatch work
//! in a realm, continue a run with a follow-up message, review and adopt a
//! run's change, and list what agents can do (skills) and have done (runs).
//!
//! Clients (the CLI and the UI) go through here instead of wiring models, tools
//! and permissions themselves. The hub builds each run's permission gate from
//! its realm's policy, and its workspace from the realm's isolation rules: a
//! client chooses *who* approves, never *whether* approval is needed or what
//! the run may reach.

use crate::agent::{Agent, AgentConfig, Briefing, Observer, run_context};
use crate::cancel::CancelToken;
use crate::github::{Checks, GitHub, PrState, PrStatus, Remote};
use crate::home::Home;
use crate::isolation::{self, ChangeStatus, CheckResult, CheckRun, Delivery, PullRequest, Update};
use crate::memory::MemoryStore;
use crate::models::router::{Connections, Routed, Tier};
use crate::models::{ContentBlock, Message, Model, ModelRequest, Role};
use crate::notify::Kind;
use crate::objective::{self, Objective, Schedule, Trigger};
use crate::permissions::{Approver, Capability, PermissionGate};
use crate::realm::{PROVIDER_KEYS, Realm, Repo};
use crate::runtime::Runtime;
use crate::state::{Origin, State, Status, render_plan};
use crate::tools::{self, Access, PlanHandle, RealmTools, Workspace};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) mod autonomy;
mod changes;
mod delegation;
mod delivery;
pub(crate) mod finance;
pub(crate) mod incidents;
mod models;
mod objectives;
mod pipelines;
#[cfg(test)]
pub(crate) use pipelines::TEST_FEEDS;
mod repos;
mod retries;
mod routines;
mod runs;
mod skills;
mod tasks;

pub use changes::Adoption;
pub use delivery::PrEvent;
pub use objectives::away_reason;
use runs::RunLock;
pub(crate) use runs::process_alive;

/// Provider settings that are credentials.
const CREDENTIAL_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "OPENAI_API_KEY",
    "XAI_API_KEY",
];

/// How a client takes part in a run: who approves, who watches, how to stop it.
pub struct RunHooks<'a> {
    pub approver: Box<dyn Approver>,
    pub observer: &'a mut dyn Observer,
    pub cancel: CancelToken,
}

#[derive(Clone)]
pub struct Hub {
    home: Home,
    /// Connections used for every realm instead of each realm's configuration (tests).
    connections: Option<Connections>,
}

impl Hub {
    pub fn new(home: Home) -> Self {
        Self {
            home,
            connections: None,
        }
    }

    #[cfg(test)]
    pub fn with_model(self, model: Box<dyn Model>) -> Self {
        self.with_connections(Connections::single(Arc::from(model)))
    }

    #[cfg(test)]
    pub fn with_connections(mut self, connections: Connections) -> Self {
        self.connections = Some(connections);
        self
    }

    pub fn home(&self) -> &Home {
        &self.home
    }

    /// Create and save a new run, without starting it. In an isolated realm
    /// this also creates the run's worktree.
    pub fn create_run(&self, realm: &Realm, objective: &str) -> Result<State> {
        self.create_run_in(realm, objective, None)
    }

    /// Create a run that works in one of the realm's repositories (None:
    /// its root, isolated if the root is a repository).
    pub fn create_run_in(
        &self,
        realm: &Realm,
        objective: &str,
        repo: Option<&str>,
    ) -> Result<State> {
        let objective = objective.trim();
        if objective.is_empty() {
            bail!("the objective is empty");
        }
        if realm.config.archived {
            bail!("realm '{}' is archived", realm.name());
        }
        let mut state = State::new(objective, Some(&realm.runs_dir()));
        state.realm = realm.name().to_string();
        if let Some(repo) = realm.repo(repo)? {
            let start = self.start_point(realm, &repo);
            state.change = Some(
                isolation::create_from(&repo.path, &realm.worktrees_dir(), &state.id, &start)
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
        let mut state = self.reopen(realm, run)?;
        append_user_text(&mut state.messages, message);
        state.status = Status::Running;
        state.report = None;
        state.save()?;
        Ok(state)
    }

    /// Load a run that did not complete (interrupted, stopped or failed) and
    /// tell the agent how to pick it up again, ready to drive.
    pub fn resume(&self, realm: &Realm, run: &str) -> Result<State> {
        let mut state = self.reopen(realm, run)?;
        if state.status == Status::Completed {
            bail!(
                "run {} completed; send it a message to continue it",
                state.id
            );
        }
        let message = resume_message(&state);
        append_user_text(&mut state.messages, &message);
        state.status = Status::Running;
        state.report = None;
        state.save()?;
        Ok(state)
    }

    /// A saved run that may be driven again: not live, change still open,
    /// and with no tool call left unanswered.
    fn reopen(&self, realm: &Realm, run: &str) -> Result<State> {
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
        let connections = match self.connections_for(realm) {
            Ok(connections) => connections,
            Err(e) => {
                state.status = Status::Failed;
                state.report = Some(format!("no model available: {e:#}"));
                state.save()?;
                let _ = self.objective_run_ended(realm, state);
                return Err(e);
            }
        };
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        let model = self.model_for_drive(realm, &connections, state)?;
        state.save()?;
        let workspace = self.workspace_for(realm, state)?;
        let context = frozen_context(
            state,
            run_context(&self.briefing(realm, state, &workspace)?),
        );
        let plan = PlanHandle::new(state.plan.clone().into());
        let runtime = Runtime::new(
            tools::realm_tools(RealmTools {
                memory: realm.memory(),
                source: format!("run:{}", state.id),
                plan: plan.clone(),
                integrations: realm.config.integrations.clone(),
                skills_dir: realm.skills_dir(),
                allowed: state.tools.clone(),
                delegation: self.delegation_for(realm, state)?,
                // A task works on files only in a repository of its scope.
                files: state.agent.is_none() || state.change.is_some(),
            }),
            PermissionGate::new(realm.config.policy.clone(), hooks.approver)
                .with_grants(state.grants.clone()),
            workspace.clone(),
            hooks.cancel.clone(),
        )
        .with_plan(plan);
        let runtime = match autonomy::BusinessAuthority::for_run(realm, state)? {
            Some(authority) => runtime.with_authority(Box::new(authority)),
            None => {
                runtime.with_authority(Box::new(autonomy::FounderAuthority::for_run(realm, state)?))
            }
        };
        let observer = hooks.observer;
        let mut config = config.within(&state.budget);
        // AIRTH_COMPACT_AT: tokens past which the conversation is summarized (0: never).
        if let Some(v) = self.settings(realm)?("AIRTH_COMPACT_AT")
            && let Ok(n) = v.trim().parse::<u64>()
        {
            config.compact_at = (n > 0).then_some(n.max(20_000));
        }
        let result = Agent::new(
            &model,
            runtime,
            config,
            &mut *observer,
            hooks.cancel.clone(),
        )
        .with_prices(crate::prices::Prices::load(self.home()))
        .with_context(context)
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
            && !self.repo_of(realm, state).checks.is_empty();
        if wants_checks && !hooks.cancel.is_cancelled() {
            self.run_checks(realm, state, &workspace, observer, &hooks.cancel)?;
        }
        if state.status == Status::Completed
            && let Err(e) = self.deliver(realm, state, false)
        {
            state.report = Some(format!(
                "{}\n\nCould not open the pull request: {e:#}",
                state.report.as_deref().unwrap_or_default()
            ));
            state.save()?;
            observer.changed(state);
        }
        if let Err(e) = self.task_ended(realm, state) {
            eprintln!("warning: could not tell how task {} ended: {e:#}", state.id);
        }
        if state.origin.is_some()
            && let Err(e) = self.objective_run_ended(realm, state)
        {
            eprintln!(
                "warning: could not record objective run {}: {e:#}",
                state.id
            );
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

    /// Resume a run that did not complete and drive it until it stops.
    pub fn resume_and_drive(
        &self,
        realm: &Realm,
        run: &str,
        config: AgentConfig,
        hooks: RunHooks,
    ) -> Result<State> {
        let mut state = self.resume(realm, run)?;
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

    /// Where a run works and what it may reach. Every run is denied the Airth
    /// home and the other realms' roots, and may only read the business's
    /// config; an isolated run may read (not write) the owner's checkout, and
    /// write only its worktree and the git data it needs.
    pub fn workspace_for(&self, realm: &Realm, state: &State) -> Result<Workspace> {
        let root = match &state.change {
            Some(c) if c.status == ChangeStatus::Open => c.path.clone(),
            Some(c) => bail!("this run's change was {}", c.status.as_str()),
            None => realm.root()?,
        };
        let mut ws = Workspace::new(&root)?;
        for denied in self.denied_to(realm)? {
            ws = ws.with_rule(denied, Access::None);
        }
        // How the business is configured belongs to the owner: a run reads
        // it, and changes it only in a worktree the owner adopts (decision 44).
        if state.change.is_none() {
            for dir in crate::business::config_dirs(&root) {
                ws = ws.with_rule(dir, Access::Read);
            }
            // Git runs hooks and reads its config as the owner, outside any
            // sandbox: a run in place may commit, never plant either, nor
            // move `.git` aside to do it (decision 46).
            let git = root.join(".git");
            ws = ws
                .with_rule(git.join("hooks"), Access::Read)
                .with_rule(git.join("config"), Access::Read)
                .with_pinned(git);
        }
        // The realm's other repositories can be read, to work across them.
        for repo in &realm.config.repos {
            if repo.path != root {
                ws = ws.with_rule(&repo.path, Access::Read);
            }
        }
        let mut env = realm.command_env()?;
        // Commands write nowhere else than the workspace (decision 48), so
        // each run gets its own temporary folder, in the realm's cache.
        let tmp = self.run_tmp(realm, &state.id);
        std::fs::create_dir_all(&tmp)?;
        let tmp = tmp.canonicalize()?;
        ws = ws.with_rule(&tmp, Access::ReadWrite);
        env.push(("TMPDIR".into(), tmp.display().to_string()));
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
        Ok(ws
            .with_secrets(env, self.redactions(realm)?)
            .with_sandbox(true)
            .with_network(!realm.config.offline)
            // A routine's commands are offline: its network goes through
            // actions the levels see (decision 51).
            .with_command_network(state.origin.is_none()))
    }

    /// A run's temporary folder: `TMPDIR` for its commands.
    fn run_tmp(&self, realm: &Realm, run: &str) -> PathBuf {
        realm.cache_dir().join("tmp").join(run)
    }

    /// What no run of `realm` may reach: Airth's home and other realms' folders.
    fn denied_to(&self, realm: &Realm) -> Result<Vec<PathBuf>> {
        let mut out = vec![self.home.dir().to_path_buf()];
        for other in self.home.realms()? {
            if other.name() != realm.name() {
                if let Some(root) = &other.config.root {
                    out.push(root.clone());
                }
                out.extend(other.config.repos.iter().map(|r| r.path.clone()));
            }
        }
        Ok(out)
    }

    /// Credential values to hide from the model, with their names.
    fn redactions(&self, realm: &Realm) -> Result<Vec<(String, String)>> {
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
        redact.extend(
            self.home
                .provider_settings()?
                .into_iter()
                .filter(|(k, _)| CREDENTIAL_KEYS.contains(&k.as_str())),
        );
        Ok(redact)
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
            business: crate::business::briefing(&realm.root()?)?,
            memory: realm.memory().list()?.into_iter().map(|m| m.text).collect(),
            isolated: state.change.is_some(),
            pull_requests: state.change.is_some()
                && self.repo_of(realm, state).delivery == Delivery::PullRequest,
            checks: self.repo_of(realm, state).checks,
        })
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

/// What the agent is told when its run is picked up again.
fn resume_message(state: &State) -> String {
    let how = match state.status {
        Status::Running => {
            "was interrupted: the process driving it exited before it finished".to_string()
        }
        Status::Stopped | Status::Failed => {
            let what = if state.status == Status::Failed {
                "failed"
            } else {
                "stopped"
            };
            match state.report.as_deref().filter(|r| !r.is_empty()) {
                Some(report) => format!("{what} ({})", first_line(report, 300)),
                None => what.to_string(),
            }
        }
        Status::Completed => "completed".to_string(),
    };
    let mut out = format!("[Airth runtime] This run {how}. The owner has resumed it.\n\n");
    if !state.plan.is_empty() {
        out.push_str(&format!(
            "Your plan so far:\n{}\n\n",
            render_plan(&state.plan)
        ));
    }
    out.push_str("Before continuing, check the current state of the workspace and of any system you changed: the step in progress, or the last action before the interruption, may have taken effect fully, partly or not at all. Do not repeat what already took effect. Then continue toward the objective, keeping your plan up to date.");
    out
}

fn first_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(max - 1).collect::<String>())
    }
}

/// What the owner last asked: the text of the latest user turn, without
/// Airth's own notes. None when the turn holds only those (a resume).
fn latest_ask(state: &State) -> Option<String> {
    let last = state.messages.iter().rev().find(|m| m.role == Role::User)?;
    let text: Vec<&str> = last
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } if !text.starts_with("[Airth runtime]") => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    let text = text.join("\n\n");
    (!text.trim().is_empty()).then_some(text)
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

/// The context a drive of `state` sends: always the one the run started
/// with, since changing it would invalidate the provider's cache of the whole
/// conversation. If the realm's context changed since (a new memory, an
/// edited profile or business profile), the new one is sent once, as a note after
/// the conversation.
fn frozen_context(state: &mut State, fresh: String) -> String {
    let Some(started) = state.context.clone() else {
        state.context = Some(fresh.clone());
        return fresh;
    };
    let latest = state.context_update.as_ref().unwrap_or(&started);
    if *latest != fresh && !state.messages.is_empty() {
        let note = ContentBlock::Text {
            text: format!(
                "[Airth runtime] What you were told about your realm, owner or memory changed since this run started. It is now:\n\n{fresh}"
            ),
        };
        match state.messages.last_mut() {
            Some(m) if m.role == Role::User => m.content.push(note),
            _ => state.messages.push(Message {
                role: Role::User,
                content: vec![note],
            }),
        }
        state.context_update = Some(fresh);
    }
    started
}

#[cfg(test)]
mod tests;
