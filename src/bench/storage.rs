//! Benchmark results, kept in the Airth home:
//!
//! ```text
//! benchmarks/
//!   config.json          the baseline runs are compared with
//!   prices.json          extra model prices, USD per million tokens (optional)
//!   suites/              more suites (see suite.rs)
//!   results/<id>.json    one benchmark run: environment, options, results
//!   results/<id>/traces/ the full record of each task attempt (a run state)
//!   reports/<id>.md      a readable report of the run
//!   work/<id>/           while a run executes: each task's realm and canary
//! ```
//!
//! Results carry a schema version; older ones are migrated when read, so
//! history stays readable after upgrades.

use super::metrics::{Metrics, Price, Prices};
use super::suite::{Category, valid_id};
use crate::realm::write_atomic;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The version of the result format this Airth writes.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Outcome {
    /// The independent verification passed.
    Pass,
    /// It did not.
    Fail,
    /// It did not, and the run had used up its time or budget.
    Timeout,
    /// The harness or the model failed (no outcome to verify).
    Error,
    /// Not executed (missing requirement, no sandbox, cancelled). Never
    /// counted in rates.
    Skipped,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail => "FAIL",
            Outcome::Timeout => "TIMEOUT",
            Outcome::Error => "ERROR",
            Outcome::Skipped => "SKIPPED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityOutcome {
    /// The attack reached the agent, and what it aimed at did not happen.
    Prevented,
    /// What it aimed at happened.
    Succeeded,
    /// No evidence the attack reached the agent (the model may only have
    /// declined): it proves nothing either way.
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckOutcome {
    pub check: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// One attempt at one task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task: String,
    pub category: Category,
    #[serde(default)]
    pub title: String,
    /// 1-based, for repeated runs.
    pub attempt: u32,
    pub outcome: Outcome,
    /// Why, in a sentence (the failing check, the error, why it was skipped).
    #[serde(default)]
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityOutcome>,
    #[serde(default)]
    pub checks: Vec<CheckOutcome>,
    #[serde(default)]
    pub metrics: Metrics,
    /// How the agent's run ended, and what it said: its claim, kept apart
    /// from the verified outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Workspace files the run added, changed or removed (`+`, `~`, `-`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// The attempt's full record is kept (see `Store::trace`).
    #[serde(default)]
    pub trace: bool,
    pub started_at: DateTime<Utc>,
}

impl TaskResult {
    pub fn executed(&self) -> bool {
        self.outcome != Outcome::Skipped
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Where a run happened, to know what two runs differ in.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    pub airth_version: String,
    pub commit: Option<String>,
    /// Built from a checkout with uncommitted changes.
    pub dirty: bool,
    pub os: String,
    pub arch: String,
    /// Commands ran in the OS sandbox.
    pub sandbox: bool,
    /// The main model (`provider/model`), or the one chosen for the run.
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_choice: Option<String>,
    /// The model connections' settings (provider, model, effort per tier;
    /// never credentials).
    pub model_config: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RunOptions {
    /// Only these tasks (all when empty).
    pub tasks: Vec<String>,
    pub repeat: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Stop starting tasks once the run has used this many tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Keep each task's workspace after it ran (for inspection).
    pub keep: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRun {
    pub schema: u32,
    pub id: String,
    pub suite: String,
    pub suite_version: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    pub environment: Environment,
    pub options: RunOptions,
    /// Task attempts the run set out to do.
    pub planned: usize,
    /// The task attempt in progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// The process executing the run, while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub results: Vec<TaskResult>,
}

impl BenchRun {
    /// Status for display: a run marked running that no process executes
    /// any more was interrupted.
    pub fn display_status(&self) -> &'static str {
        match self.status {
            RunStatus::Running if !self.pid.is_some_and(crate::hub::process_alive) => "interrupted",
            RunStatus::Running => "running",
            RunStatus::Completed => "completed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    baseline: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn suites_dir(&self) -> PathBuf {
        self.dir.join("suites")
    }

    fn results_dir(&self) -> PathBuf {
        self.dir.join("results")
    }

    pub fn work_dir(&self, run: &str) -> PathBuf {
        self.dir.join("work").join(run)
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        valid_id(id, "benchmark run id")?;
        Ok(self.results_dir().join(format!("{id}.json")))
    }

    /// A fresh run id: the start time, and a random part so runs started at
    /// once (from the app and a terminal) never share a workspace.
    pub fn new_id(&self) -> Result<String> {
        let id = format!(
            "bench-{}-{}",
            Utc::now().format("%Y%m%d-%H%M%S"),
            &crate::ui::random_token()?[..6]
        );
        if self.path(&id)?.exists() {
            return self.new_id();
        }
        Ok(id)
    }

    pub fn save(&self, run: &BenchRun) -> Result<()> {
        std::fs::create_dir_all(self.results_dir())?;
        write_atomic(&self.path(&run.id)?, &serde_json::to_string_pretty(run)?)
    }

    pub fn load(&self, id: &str) -> Result<BenchRun> {
        let path = self.path(id)?;
        let text = match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("no benchmark run '{id}' (see `airth bench results`)")
            }
            r => r.with_context(|| format!("reading {}", path.display()))?,
        };
        let value: Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        serde_json::from_value(migrate(value)?)
            .with_context(|| format!("reading benchmark run {id}"))
    }

    /// Every run, newest first. Unreadable ones are reported and skipped.
    pub fn list(&self) -> Result<Vec<BenchRun>> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.results_dir()) else {
            return Ok(out);
        };
        for entry in entries {
            let path = entry?.path();
            let Some(id) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".json"))
            else {
                continue;
            };
            match self.load(id) {
                Ok(run) => out.push(run),
                Err(e) => eprintln!("warning: skipping benchmark run {id}: {e:#}"),
            }
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        Ok(out)
    }

    /// A run by id, or `last` for the newest.
    pub fn find(&self, id: &str) -> Result<BenchRun> {
        if id == "last" {
            return self
                .list()?
                .into_iter()
                .next()
                .context("no benchmark run yet (see `airth bench run`)");
        }
        self.load(id)
    }

    fn trace_path(&self, run: &str, task: &str, attempt: u32) -> Result<PathBuf> {
        valid_id(task, "task id")?;
        Ok(self
            .results_dir()
            .join(run)
            .join("traces")
            .join(format!("{task}-{attempt}.json")))
    }

    /// Keep the full record of a task attempt.
    pub fn save_trace(&self, run: &str, task: &str, attempt: u32, trace: &Value) -> Result<()> {
        let path = self.trace_path(run, task, attempt)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        write_atomic(&path, &serde_json::to_string(trace)?)
    }

    pub fn trace(&self, run: &str, task: &str, attempt: u32) -> Result<Value> {
        self.path(run)?;
        let path = self.trace_path(run, task, attempt)?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("no trace for {task} (attempt {attempt}) in run {run}"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn report_path(&self, run: &str) -> PathBuf {
        self.dir.join("reports").join(format!("{run}.md"))
    }

    pub fn save_report(&self, run: &str, text: &str) -> Result<()> {
        let path = self.report_path(run);
        std::fs::create_dir_all(path.parent().unwrap())?;
        write_atomic(&path, text)
    }

    fn config(&self) -> Config {
        std::fs::read_to_string(self.dir.join("config.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn baseline(&self) -> Option<String> {
        self.config().baseline
    }

    pub fn set_baseline(&self, id: Option<&str>) -> Result<()> {
        if let Some(id) = id {
            self.load(id)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        let config = Config {
            baseline: id.map(String::from),
        };
        write_atomic(
            &self.dir.join("config.json"),
            &serde_json::to_string_pretty(&config)?,
        )
    }

    /// List prices, with the owner's own in `prices.json`.
    pub fn prices(&self) -> Prices {
        let path = self.dir.join("prices.json");
        let overrides: BTreeMap<String, Price> = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("warning: ignoring {}: {e}", path.display());
                BTreeMap::new()
            }),
            Err(_) => BTreeMap::new(),
        };
        Prices::with(overrides)
    }
}

/// Bring a stored run up to the current schema.
fn migrate(value: Value) -> Result<Value> {
    let schema = value["schema"].as_u64().unwrap_or(1);
    if schema > u64::from(SCHEMA) {
        bail!("it was written by a newer Airth (result schema {schema}); update Airth to read it");
    }
    // Schema 1 is the first; later versions add their steps here.
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn run(id: &str) -> BenchRun {
        BenchRun {
            schema: SCHEMA,
            id: id.into(),
            suite: "core".into(),
            suite_version: "1".into(),
            status: RunStatus::Completed,
            started_at: Utc::now(),
            finished_at: None,
            environment: Environment::default(),
            options: RunOptions::default(),
            planned: 0,
            current: None,
            pid: None,
            error: None,
            results: Vec::new(),
        }
    }

    #[test]
    fn saves_lists_and_refuses_newer_schemas() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        assert!(store.find("last").is_err());
        let a = store.new_id().unwrap();
        store.save(&run(&a)).unwrap();
        let b = store.new_id().unwrap();
        assert_ne!(a, b);
        store.save(&run(&b)).unwrap();
        assert_eq!(store.find("last").unwrap().id, b);
        assert_eq!(store.list().unwrap().len(), 2);
        assert!(store.load("../etc").is_err());

        let mut newer = serde_json::to_value(run("bench-future")).unwrap();
        newer["schema"] = 99.into();
        std::fs::write(
            dir.path().join("results/bench-future.json"),
            newer.to_string(),
        )
        .unwrap();
        let err = store.load("bench-future").unwrap_err();
        assert!(format!("{err:#}").contains("newer Airth"));
        // The listing still shows the others.
        assert_eq!(store.list().unwrap().len(), 2);

        store.set_baseline(Some(&a)).unwrap();
        assert_eq!(store.baseline(), Some(a));
        assert!(store.set_baseline(Some("bench-nope")).is_err());
    }

    #[test]
    fn a_run_whose_process_is_gone_was_interrupted() {
        let mut r = run("x");
        r.status = RunStatus::Running;
        r.pid = Some(std::process::id() as i32);
        assert_eq!(r.display_status(), "running");
        r.pid = None;
        assert_eq!(r.display_status(), "interrupted");
    }
}
