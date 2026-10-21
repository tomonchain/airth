//! Executing a suite. Each task attempt gets, and loses afterwards:
//!
//! - a fresh workspace outside the Airth home, with the task's fixture;
//! - a dedicated realm kept apart from the owner's (under
//!   `benchmarks/work/`, not `realms/`), offline unless the task says
//!   otherwise, with synthetic credentials and memory only, and without the
//!   owner's profile;
//! - an unattended approver: no one approves at the bench, so the realm's
//!   policy decides, and commands only run where the OS sandbox confines
//!   them (elsewhere such tasks are skipped).
//!
//! The run itself goes through `Hub::drive`, the same path as any chat: the
//! bench measures Airth's runtime, it does not have one of its own. The
//! other realms, Airth's home and their credentials stay out of reach as
//! they do for any run; security tasks check exactly that.

use super::evaluator::{self, Evidence};
use super::metrics::{Metrics, Prices};
use super::storage::{BenchRun, Outcome, RunStatus, SecurityOutcome, Store, TaskResult};
use super::suite::{Suite, Task};
use crate::agent::{AgentConfig, Budget, Observer};
use crate::cancel::CancelToken;
use crate::hub::{Hub, RunHooks};
use crate::isolation::Isolation;
use crate::permissions::{Answer, Approver, PermissionRequest};
use crate::realm::{Realm, RealmConfig};
use crate::state::Status;
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Past a task's time limit, how long the watchdog lets a run finish its
/// current step before stopping it.
const GRACE: Duration = Duration::from_secs(120);
/// What the agent's final reply keeps in a result.
const MAX_REPORT: usize = 4000;

/// No one approves at the bench: whatever the task's policy asks for is
/// refused, as it would be for an objective whose owner is away.
struct Unattended;

impl Approver for Unattended {
    fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
        Ok(Answer::No(
            "no one can approve during a benchmark; the task's policy does not allow this".into(),
        ))
    }
}

/// Where one task attempt runs.
struct Attempt {
    workspace: PathBuf,
    /// The attempt's realm and canary, inside the Airth home.
    work: PathBuf,
    canary: PathBuf,
}

impl Attempt {
    fn cleanup(&self) {
        for dir in [&self.workspace, &self.work] {
            if dir.exists()
                && let Err(e) = std::fs::remove_dir_all(dir)
            {
                eprintln!("warning: could not remove {}: {e}", dir.display());
            }
        }
    }
}

/// The temporary workspaces of a run, outside the Airth home.
pub fn workspaces_dir(run: &str) -> PathBuf {
    std::env::temp_dir().join("airth-bench").join(run)
}

fn program_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Why a task cannot run here, if it cannot.
pub fn unavailable(task: &Task) -> Option<String> {
    if task.runs_commands() && !crate::sandbox::available() {
        return Some(
            "it runs commands unattended, and this platform has no OS sandbox to confine them"
                .into(),
        );
    }
    task.requires
        .iter()
        .find(|p| !program_available(p))
        .map(|p| format!("it needs {p}, which is not installed"))
}

/// A realm for one attempt, outside `realms/` so it is never listed, shown
/// or reachable as one of the owner's.
pub fn bench_realm(dir: &Path, root: Option<&Path>, task: Option<&Task>) -> Result<Realm> {
    let realm = Realm::create(
        dir,
        RealmConfig {
            name: "bench".into(),
            description: "Airth Bench: a benchmark task, with synthetic data only".into(),
            root: root.map(Path::to_path_buf),
            policy: task
                .map(Task::policy)
                .unwrap_or_else(super::suite::bench_policy),
            isolation: Isolation::None,
            checks: Vec::new(),
            delivery: Default::default(),
            repos: Vec::new(),
            share_owner_profile: false,
            integrations: Vec::new(),
            archived: false,
            offline: !task.is_some_and(|t| t.network),
            created_at: Utc::now(),
        },
    )?;
    if let Some(task) = task {
        for (k, v) in &task.secrets {
            realm.set_secret(k, v)?;
        }
        for fact in &task.memory {
            realm.memory().add(fact, "benchmark")?;
        }
    }
    Ok(realm)
}

pub struct Runner<'a> {
    pub hub: &'a Hub,
    pub store: &'a Store,
    pub prices: Prices,
    pub cancel: CancelToken,
}

impl Runner<'_> {
    /// Run every planned attempt of `suite`, saving `run` after each so a
    /// client can follow, then write its report.
    pub fn execute(
        &self,
        run: &mut BenchRun,
        suite: &Suite,
        observer: &mut dyn Observer,
        progress: &mut dyn FnMut(&BenchRun),
    ) -> Result<()> {
        let tasks: Vec<&Task> = suite
            .tasks
            .iter()
            .filter(|t| run.options.tasks.is_empty() || run.options.tasks.contains(&t.id))
            .collect();
        let repeat = run.options.repeat.max(1);
        'tasks: for attempt in 1..=repeat {
            for task in &tasks {
                let used: u64 = run
                    .results
                    .iter()
                    .map(|r| r.metrics.input_tokens + r.metrics.output_tokens)
                    .sum();
                let skip = if self.cancel.is_cancelled() {
                    Some("the benchmark was cancelled".to_string())
                } else if run.options.max_tokens.is_some_and(|max| used >= max) {
                    Some(format!(
                        "the benchmark used its token budget ({used} tokens)"
                    ))
                } else {
                    unavailable(task)
                };
                if let Some(reason) = skip {
                    run.results.push(skipped(task, attempt, reason));
                    continue;
                }
                run.current = Some(format!("{} ({attempt}/{repeat})", task.id));
                self.store.save(run)?;
                progress(run);
                let result = self.attempt(run, suite, task, attempt, observer);
                run.results.push(result);
                run.current = None;
                self.store.save(run)?;
                progress(run);
                if self.cancel.is_cancelled() {
                    // Whatever remains is recorded as skipped, not dropped.
                    for a in attempt..=repeat {
                        for t in &tasks {
                            if !run.results.iter().any(|r| r.task == t.id && r.attempt == a) {
                                run.results.push(skipped(
                                    t,
                                    a,
                                    "the benchmark was cancelled".into(),
                                ));
                            }
                        }
                    }
                    break 'tasks;
                }
            }
        }
        run.status = if self.cancel.is_cancelled() {
            RunStatus::Cancelled
        } else {
            RunStatus::Completed
        };
        run.finished_at = Some(Utc::now());
        run.current = None;
        run.pid = None;
        self.store.save(run)?;
        self.store.save_report(&run.id, &super::report(run))?;
        let _ = std::fs::remove_dir_all(self.store.work_dir(&run.id));
        if !run.options.keep {
            let _ = std::fs::remove_dir_all(workspaces_dir(&run.id));
        }
        progress(run);
        Ok(())
    }

    fn prepare(
        &self,
        run: &BenchRun,
        suite: &Suite,
        task: &Task,
        attempt: u32,
    ) -> Result<(Attempt, Realm)> {
        let name = format!("{}-{attempt}", task.id);
        let a = Attempt {
            workspace: workspaces_dir(&run.id).join(&name),
            work: self.store.work_dir(&run.id).join(&name),
            canary: self
                .store
                .work_dir(&run.id)
                .join(&name)
                .join("canary")
                .join("secret.txt"),
        };
        a.cleanup();
        std::fs::create_dir_all(&a.workspace)?;
        let workspace = a.workspace.canonicalize()?;
        if workspace.starts_with(self.hub.home().dir()) {
            anyhow::bail!("the temporary directory is inside the Airth home");
        }
        suite.materialize(task, &workspace)?;
        std::fs::create_dir_all(a.canary.parent().unwrap())?;
        if let Some(canary) = &task.canary {
            std::fs::write(&a.canary, canary)?;
        }
        let realm = bench_realm(&a.work.join("realm"), Some(&workspace), Some(task))?;
        Ok((a, realm))
    }

    fn attempt(
        &self,
        run: &BenchRun,
        suite: &Suite,
        task: &Task,
        attempt: u32,
        observer: &mut dyn Observer,
    ) -> TaskResult {
        let started_at = Utc::now();
        let mut result = TaskResult {
            task: task.id.clone(),
            category: task.category,
            title: task.title.clone(),
            attempt,
            outcome: Outcome::Error,
            reason: String::new(),
            security: None,
            checks: Vec::new(),
            metrics: Metrics::default(),
            run_status: None,
            report: None,
            files: Vec::new(),
            trace: false,
            started_at,
        };
        let (place, realm) = match self.prepare(run, suite, task, attempt) {
            Ok(p) => p,
            Err(e) => {
                result.reason = format!("could not prepare the task: {e:#}");
                return result;
            }
        };
        let canary = place.canary.display().to_string();
        let objective = task.objective.replace("{canary}", &canary);
        let before = evaluator::snapshot(&place.workspace);

        let mut state = match self.hub.create_run_in(&realm, &objective, None) {
            Ok(s) => s,
            Err(e) => {
                result.reason = format!("could not start the run: {e:#}");
                place.cleanup();
                return result;
            }
        };
        state.model_choice = run.options.model.clone();
        let limits = task.limits;
        let config = AgentConfig::new(limits.max_iterations).within(&Budget {
            max_iterations: None,
            max_tokens: limits.max_tokens,
            max_minutes: Some(limits.max_minutes),
        });

        // The task's own stop: the benchmark's, or the watchdog's when a
        // step runs far past the time limit.
        let stop = CancelToken::default();
        let deadline = Duration::from_secs(u64::from(limits.max_minutes) * 60) + GRACE;
        let started = Instant::now();
        let (outcome, timed_out) = std::thread::scope(|scope| {
            let (done, finished) = std::sync::mpsc::channel::<()>();
            let (stop, benchmark) = (&stop, &self.cancel);
            let watchdog = scope.spawn(move || {
                loop {
                    match finished.recv_timeout(Duration::from_millis(200)) {
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        _ => return false,
                    }
                    if benchmark.is_cancelled() {
                        stop.cancel();
                    }
                    if started.elapsed() >= deadline {
                        stop.cancel();
                        return true;
                    }
                }
            });
            let hooks = RunHooks {
                approver: Box::new(Unattended),
                observer: &mut *observer,
                cancel: stop.clone(),
            };
            let outcome = self.hub.drive(&realm, &mut state, config, hooks);
            drop(done);
            (outcome, watchdog.join().unwrap_or(false))
        });
        let duration_ms = started.elapsed().as_millis() as u64;

        result.metrics = Metrics::of(&state, duration_ms, &self.prices);
        result.metrics.interrupted = stop.is_cancelled();
        result.metrics.budget_exhausted |= timed_out;
        result.run_status = Some(state.status.as_str().to_string());
        result.report = state.report.as_deref().map(|r| clip(r, MAX_REPORT));

        // Verify in the run's own workspace, sandboxed and offline.
        let workspace = self
            .hub
            .workspace_for(&realm, &state)
            .map(|ws| ws.with_network(false));
        let verify_cancel = self.cancel.clone();
        match (&outcome, workspace) {
            _ if self.cancel.is_cancelled() => {
                result.outcome = Outcome::Skipped;
                result.reason = "the benchmark was cancelled while this task ran".into();
            }
            (Err(e), _) => {
                result.outcome = Outcome::Error;
                result.reason = format!("the run failed: {e:#}");
            }
            (Ok(()), Err(e)) => {
                result.outcome = Outcome::Error;
                result.reason = format!("cannot verify: {e:#}");
            }
            (Ok(()), Ok(ws)) => {
                let evidence = Evidence {
                    state: &state,
                    workspace: &ws,
                    before: &before,
                    cancel: &verify_cancel,
                };
                result.checks = task.verification.iter().map(|c| evidence.run(c)).collect();
                let failed = result.checks.iter().find(|c| !c.passed);
                let passed = failed.is_none();
                result.outcome = match failed {
                    None => Outcome::Pass,
                    Some(_) if timed_out || result.metrics.budget_exhausted => Outcome::Timeout,
                    Some(_) => Outcome::Fail,
                };
                result.reason = match failed {
                    None => String::new(),
                    Some(c) if c.detail.is_empty() => format!("failed: {}", c.check),
                    Some(c) => format!(
                        "failed: {} ({})",
                        c.check,
                        c.detail.lines().next().unwrap_or_default()
                    ),
                };
                if result.outcome == Outcome::Timeout {
                    result.reason = format!(
                        "{}; {}",
                        if timed_out {
                            "stopped by the time watchdog".to_string()
                        } else {
                            state.report.clone().unwrap_or_default()
                        },
                        result.reason
                    );
                }
                if let Some(security) = &task.security {
                    result.security = Some(evidence.security(security, &canary, passed));
                    if result.security == Some(SecurityOutcome::Inconclusive) {
                        result.reason =
                            "no evidence the attack reached the agent (it may only have declined)"
                                .into();
                    }
                }
            }
        }
        if state.status == Status::Failed && result.outcome == Outcome::Fail {
            result.reason = format!(
                "{} — the run failed: {}",
                result.reason,
                state.report.as_deref().unwrap_or_default()
            );
        }
        result.files = evaluator::changes(&before, &evaluator::snapshot(&place.workspace));

        let trace = serde_json::json!({
            "task": task,
            "objective": objective,
            "result": &result,
            "run": &state,
        });
        match self.store.save_trace(&run.id, &task.id, attempt, &trace) {
            Ok(()) => result.trace = true,
            Err(e) => eprintln!("warning: could not keep the trace of {}: {e:#}", task.id),
        }
        if run.options.keep {
            // Only the realm (inside the home) goes; the workspace stays.
            let _ = std::fs::remove_dir_all(&place.work);
        } else {
            place.cleanup();
        }
        result
    }
}

fn skipped(task: &Task, attempt: u32, reason: String) -> TaskResult {
    TaskResult {
        task: task.id.clone(),
        category: task.category,
        title: task.title.clone(),
        attempt,
        outcome: Outcome::Skipped,
        reason,
        security: None,
        checks: Vec::new(),
        metrics: Metrics::default(),
        run_status: None,
        report: None,
        files: Vec::new(),
        trace: false,
        started_at: Utc::now(),
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Look up models as a bench realm sees them (the shared settings), in a
/// throwaway realm, before the run starts.
fn probe<T>(
    hub: &Hub,
    store: &Store,
    run: &str,
    f: impl FnOnce(&Hub, &Realm) -> Result<T>,
) -> Result<T> {
    let dir = store.work_dir(run);
    let result = bench_realm(&dir.join("probe"), None, None).and_then(|realm| f(hub, &realm));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The model a run would use, to record with it (never credentials).
pub fn model_record(hub: &Hub, store: &Store, run: &str) -> Result<(String, serde_json::Value)> {
    probe(hub, store, run, |hub, realm| {
        let main = hub
            .connections_for(realm)
            .context("no model is set up (see `airth doctor`)")?
            .main
            .name();
        Ok((main, hub.model_settings(realm)?))
    })
}

/// Check a model choice (`provider/model`) can be built, before a run.
pub fn check_model_choice(hub: &Hub, store: &Store, run: &str, choice: &str) -> Result<()> {
    probe(hub, store, run, |hub, realm| {
        hub.chosen_model(realm, choice).map(|_| ())
    })
}
