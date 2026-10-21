//! Benchmark suites: collections of reproducible tasks. A task brings its
//! fixture (the files the agent starts from), the realm it runs in (policy,
//! network, synthetic credentials and memory), its limits, and how its
//! outcome is verified, independently of what the agent claims.
//!
//! The `core` suite is built into Airth (`core.json`); more suites can be
//! dropped into `~/.airth/benchmarks/suites/` as JSON, with fixture folders
//! next to them.

use crate::permissions::{Capability, Policy, Rule};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const CORE: &str = include_str!("core.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Coding,
    Planning,
    Context,
    Recovery,
    Security,
    Personal,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Coding => "coding",
            Category::Planning => "planning",
            Category::Context => "context",
            Category::Recovery => "recovery",
            Category::Security => "security",
            Category::Personal => "personal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suite {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub tasks: Vec<Task>,
    /// Where fixture folders (`fixture.dir`) are found.
    #[serde(skip)]
    pub base: Option<PathBuf>,
    #[serde(skip)]
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique in the suite: letters, digits, `-`, `_`, `.`.
    pub id: String,
    pub category: Category,
    #[serde(default)]
    pub title: String,
    /// What the agent is asked. `{canary}` stands for the canary file's path.
    pub objective: String,
    #[serde(default)]
    pub fixture: Fixture,
    /// Programs the task needs on this machine (e.g. `cargo`).
    #[serde(default)]
    pub requires: Vec<String>,
    /// Rules that differ from the bench's (see `Task::policy`).
    #[serde(default)]
    pub policy: BTreeMap<Capability, Rule>,
    /// Runs may reach the network. Off by default, and enforced: requests
    /// are refused and commands are cut off by the sandbox.
    #[serde(default)]
    pub network: bool,
    /// Synthetic credentials, set as the bench realm's secrets.
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
    /// Synthetic facts the bench realm's memory starts with.
    #[serde(default)]
    pub memory: Vec<String>,
    /// Content of a file placed in Airth's own data, where no run may read:
    /// security tasks check it never reaches the agent.
    #[serde(default)]
    pub canary: Option<String>,
    #[serde(deserialize_with = "one_or_many")]
    pub verification: Vec<Check>,
    #[serde(default)]
    pub security: Option<Security>,
    #[serde(default)]
    pub limits: Limits,
}

/// The files a task starts from.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fixture {
    /// Path → content.
    pub files: BTreeMap<String, String>,
    /// A folder (relative to the suite file) copied into the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Large generated files, for long-context tasks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub noise: Vec<Noise>,
}

/// A generated log file: `lines` deterministic noise lines, with `insert`
/// lines placed at given line numbers (1-based).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Noise {
    pub path: String,
    pub lines: usize,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub insert: BTreeMap<usize, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub max_iterations: u32,
    pub max_minutes: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_iterations: 30,
            max_minutes: 10,
            max_tokens: None,
        }
    }
}

/// One verification of a task's outcome. All of a task's checks must pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Check {
    /// A command exits with 0 in the workspace (sandboxed, offline).
    Command {
        command: String,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
    FileExists {
        path: String,
    },
    FileAbsent {
        path: String,
    },
    /// A file (or any file matching a `*` pattern) contains `text`.
    FileContains {
        path: String,
        text: String,
    },
    /// A fixture file is as it started.
    FileUnchanged {
        path: String,
    },
    /// The agent's final reply contains `text` (case-insensitive).
    AnswerContains {
        text: String,
    },
    /// `text` never reached the model (conversation, tool results) or the
    /// final reply, nor, with `files`, a file of the workspace. (A command
    /// may write a credential it was given into a file without the model
    /// ever seeing it: that is how credentials are meant to be used.)
    NotLeaked {
        text: String,
        #[serde(default = "yes")]
        files: bool,
    },
}

fn default_timeout() -> u64 {
    300
}

fn yes() -> bool {
    true
}

impl Check {
    pub fn describe(&self) -> String {
        match self {
            Check::Command { command, .. } => format!("$ {command}"),
            Check::FileExists { path } => format!("{path} exists"),
            Check::FileAbsent { path } => format!("{path} does not exist"),
            Check::FileContains { path, text } => format!("{path} contains \"{text}\""),
            Check::FileUnchanged { path } => format!("{path} is unchanged"),
            Check::AnswerContains { text } => format!("the reply contains \"{text}\""),
            Check::NotLeaked { .. } => "the protected value never reached the agent".into(),
        }
    }
}

/// How a security task tells an attack reached the agent: a tool call whose
/// arguments contain one of these (after `{canary}` is replaced). Without
/// such evidence, a passing task is inconclusive: the model may only have
/// declined, which says nothing of what Airth would have prevented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Security {
    pub evidence: Vec<String>,
}

fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Check>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(Check),
        Many(Vec<Check>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(c) => vec![c],
        OneOrMany::Many(v) => v,
    })
}

/// What a bench run may do unless its task says otherwise: work on its
/// files and run commands (inside the sandbox) without asking, since no one
/// approves at the bench; never reach the network, skills or the owner's apps.
pub fn bench_policy() -> Policy {
    Policy {
        read: Rule::Allow,
        write: Rule::Allow,
        execute: Rule::Allow,
        memory: Rule::Allow,
        network: Rule::Deny,
        skill: Rule::Deny,
        apps: Rule::Deny,
    }
}

impl Task {
    pub fn policy(&self) -> Policy {
        let mut policy = bench_policy();
        if self.network {
            policy.network = Rule::Allow;
        }
        for (capability, rule) in &self.policy {
            policy.set(*capability, *rule);
        }
        policy
    }

    /// Unattended commands (the agent's, or a check's run on what the agent
    /// wrote) are only safe inside the OS sandbox.
    pub fn runs_commands(&self) -> bool {
        self.policy().execute != Rule::Deny
            || self
                .verification
                .iter()
                .any(|c| matches!(c, Check::Command { .. }))
    }

    fn validate(&self) -> Result<()> {
        valid_id(&self.id, "task id")?;
        if self.objective.trim().is_empty() {
            bail!("task {} has no objective", self.id);
        }
        if self.verification.is_empty() {
            bail!("task {} has no verification", self.id);
        }
        if self.limits.max_iterations == 0 || self.limits.max_minutes == 0 {
            bail!("task {} has a zero limit", self.id);
        }
        let paths = self
            .fixture
            .files
            .keys()
            .chain(self.fixture.noise.iter().map(|n| &n.path))
            .chain(self.verification.iter().filter_map(|c| match c {
                Check::FileExists { path }
                | Check::FileAbsent { path }
                | Check::FileContains { path, .. }
                | Check::FileUnchanged { path } => Some(path),
                _ => None,
            }));
        for path in paths {
            relative(path).with_context(|| format!("task {}", self.id))?;
        }
        if self.security.is_some() && self.category != Category::Security {
            bail!(
                "task {} has security evidence but is not a security task",
                self.id
            );
        }
        Ok(())
    }
}

/// Suite, task and run ids: lowercase letters, digits, `-`, `_`, `.`, so
/// they are safe in file names.
pub fn valid_id(id: &str, what: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_.".contains(c));
    if !ok {
        bail!("invalid {what} '{id}': use lowercase letters, digits, '-', '_' and '.'");
    }
    Ok(())
}

/// A path inside the workspace: relative, without `..`.
pub fn relative(path: &str) -> Result<&Path> {
    let p = Path::new(path);
    if path.is_empty()
        || !p
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        bail!("'{path}' must be a relative path inside the workspace");
    }
    Ok(p)
}

impl Suite {
    pub fn parse(text: &str, base: Option<PathBuf>) -> Result<Self> {
        let mut suite: Suite = serde_json::from_str(text).context("parsing the suite")?;
        suite.base = base;
        suite.validate()?;
        Ok(suite)
    }

    pub fn core() -> Self {
        let mut suite = Self::parse(CORE, None).expect("the core suite is valid");
        suite.builtin = true;
        suite
    }

    fn validate(&self) -> Result<()> {
        valid_id(&self.name, "suite name")?;
        if self.tasks.is_empty() {
            bail!("suite {} has no tasks", self.name);
        }
        let mut seen = std::collections::BTreeSet::new();
        for task in &self.tasks {
            task.validate()?;
            if !seen.insert(&task.id) {
                bail!("suite {} has two tasks named {}", self.name, task.id);
            }
            if task.fixture.dir.is_some() && self.base.is_none() {
                bail!(
                    "task {}: a built-in suite cannot use fixture folders",
                    task.id
                );
            }
        }
        Ok(())
    }

    /// The built-in suites, then those in `dir` (one JSON file each). A suite
    /// that cannot be read is reported and skipped.
    pub fn all(dir: &Path) -> Vec<Suite> {
        let mut out = vec![Self::core()];
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for path in files {
            let parsed = std::fs::read_to_string(&path)
                .map_err(anyhow::Error::from)
                .and_then(|text| Self::parse(&text, Some(dir.to_path_buf())));
            match parsed {
                Ok(s) if out.iter().any(|o| o.name == s.name) => eprintln!(
                    "warning: skipping suite {}: its name {} is taken",
                    path.display(),
                    s.name
                ),
                Ok(s) => out.push(s),
                Err(e) => eprintln!("warning: skipping suite {}: {e:#}", path.display()),
            }
        }
        out
    }

    /// Copy the fixture of `task` into `dest`.
    pub fn materialize(&self, task: &Task, dest: &Path) -> Result<()> {
        if let Some(dir) = &task.fixture.dir {
            let base = self.base.as_deref().context("no fixture folder")?;
            copy_dir(&base.join(relative(dir)?), dest)?;
        }
        for (path, content) in &task.fixture.files {
            write(dest, path, content)?;
        }
        for noise in &task.fixture.noise {
            write(dest, &noise.path, &noise_text(noise))?;
        }
        Ok(())
    }
}

fn write(dest: &Path, path: &str, content: &str) -> Result<()> {
    let file = dest.join(relative(path)?);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, content).with_context(|| format!("writing {}", file.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
        // Symlinks are left out: a fixture must not point outside itself.
    }
    Ok(())
}

/// Plausible, deterministic log lines, so a needle must be searched for.
fn noise_text(noise: &Noise) -> String {
    const SERVICES: &[&str] = &["api", "worker", "billing", "auth", "search", "mailer"];
    const EVENTS: &[&str] = &[
        "request served in {n}ms",
        "cache miss for key user:{n}",
        "retrying job {n} (attempt 2)",
        "connection pool size {n}",
        "scheduled report {n} queued",
        "health check ok ({n} checks)",
        "config reloaded, {n} keys",
        "token refreshed for client {n}",
    ];
    let mut state = noise
        .seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    let mut out = String::new();
    for line in 1..=noise.lines {
        if let Some(text) = noise.insert.get(&line) {
            out.push_str(text);
        } else {
            let (h, m, s) = (line / 3600 % 24, line / 60 % 60, line % 60);
            let event =
                EVENTS[next() % EVENTS.len()].replace("{n}", &(next() % 9000 + 100).to_string());
            out.push_str(&format!(
                "2026-01-14T{h:02}:{m:02}:{s:02}Z {:<7} {}",
                SERVICES[next() % SERVICES.len()],
                event
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_suite_is_valid_and_covers_every_category() {
        let suite = Suite::core();
        assert!(suite.tasks.len() >= 10);
        for c in [
            Category::Coding,
            Category::Planning,
            Category::Context,
            Category::Recovery,
            Category::Security,
            Category::Personal,
        ] {
            assert!(suite.tasks.iter().any(|t| t.category == c), "{c:?}");
        }
        // Every security task says how to tell an attack reached the agent.
        for t in suite
            .tasks
            .iter()
            .filter(|t| t.category == Category::Security)
        {
            assert!(t.security.is_some(), "{}", t.id);
        }
    }

    #[test]
    fn accepts_one_check_or_many_and_rejects_escaping_paths() {
        let one = r#"{"name":"s","version":"1","tasks":[{"id":"a","category":"coding","objective":"x",
            "verification":{"type":"command","command":"true"}}]}"#;
        let suite = Suite::parse(one, None).unwrap();
        assert!(matches!(
            suite.tasks[0].verification[0],
            Check::Command {
                timeout_secs: 300,
                ..
            }
        ));
        assert!(suite.tasks[0].runs_commands());
        let escaping = r#"{"name":"s","version":"1","tasks":[{"id":"a","category":"coding","objective":"x",
            "fixture":{"files":{"../x":"y"}},"verification":[{"type":"file_exists","path":"x"}]}]}"#;
        assert!(Suite::parse(escaping, None).is_err());
        let twice = r#"{"name":"s","version":"1","tasks":[
            {"id":"a","category":"coding","objective":"x","verification":{"type":"file_exists","path":"x"}},
            {"id":"a","category":"coding","objective":"x","verification":{"type":"file_exists","path":"x"}}]}"#;
        assert!(Suite::parse(twice, None).is_err());
    }

    #[test]
    fn policy_defaults_to_offline_and_unattended() {
        let suite = Suite::parse(
            r#"{"name":"s","version":"1","tasks":[{"id":"a","category":"security","objective":"x",
            "policy":{"execute":"deny"},"security":{"evidence":["x"]},
            "verification":{"type":"file_absent","path":"x"}}]}"#,
            None,
        )
        .unwrap();
        let t = &suite.tasks[0];
        let p = t.policy();
        assert_eq!(
            (p.execute, p.network, p.write),
            (Rule::Deny, Rule::Deny, Rule::Allow)
        );
        assert!(!t.runs_commands());
    }

    #[test]
    fn fixtures_are_written_with_deterministic_noise() {
        let suite = Suite::core();
        let task = suite
            .tasks
            .iter()
            .find(|t| !t.fixture.noise.is_empty())
            .unwrap();
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        suite.materialize(task, a.path()).unwrap();
        suite.materialize(task, b.path()).unwrap();
        let noise = &task.fixture.noise[0];
        let read = |d: &Path| std::fs::read_to_string(d.join(&noise.path)).unwrap();
        assert_eq!(read(a.path()), read(b.path()));
        let text = read(a.path());
        assert_eq!(text.lines().count(), noise.lines);
        for (line, needle) in &noise.insert {
            assert_eq!(text.lines().nth(line - 1).unwrap(), needle);
        }
    }
}
