//! Skills: capabilities Airth develops for itself, which the owner installs.
//!
//! A skill is a small program with a manifest and tests, kept per realm:
//!
//! ```text
//! realms/<name>/skills/
//!   skills.json          every skill: proposal, development run, evaluation, installation
//!   drafts/<skill>/      where a development run builds it (its whole workspace)
//!   installed/<skill>/   the copy runs use, frozen at installation
//! ```
//!
//! A draft holds `skill.json` (name, description, input schema, the command
//! to run, a timeout), the program, and `tests.json` (cases: input, fixture
//! files, expected exit code and output). Installed, the skill becomes a tool
//! of the realm's runs: its command runs in the run's sandbox, in the run's
//! workspace, with the input JSON on stdin and its own files in
//! `$AIRTH_SKILL_DIR`.
//!
//! Lifecycle: a run proposes a skill it lacks (`propose_skill`); a
//! development run builds it in its draft folder, which cannot reach anything
//! else of the realm; the runtime evaluates it (manifest checks, then every
//! test in the sandbox); the owner reviews and installs it. Installation
//! copies exactly the files that were evaluated (checked by hash), and runs
//! only load an installed skill whose files still match that hash.

use crate::cancel::CancelToken;
use crate::realm::write_atomic;
use crate::tools::{Access, Workspace, run_shell_with_input};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static LOCK: Mutex<()> = Mutex::new(());

pub const MANIFEST: &str = "skill.json";
pub const TESTS: &str = "tests.json";
const MAX_FILES: usize = 200;
const MAX_BYTES: u64 = 2_000_000;
const MAX_TIMEOUT: u64 = 600;
const MAX_TESTS: usize = 50;
/// Names of Airth's own tools, which a skill cannot take.
pub const RESERVED: &[&str] = &[
    "mail",
    "read_file",
    "list_directory",
    "write_file",
    "edit_file",
    "execute_command",
    "http_request",
    "update_plan",
    "remember",
    "propose_skill",
    "test_skill",
    "calendar",
    "reminders",
    "notes",
    "mail_draft",
    "safari",
    "clipboard",
    "open_on_mac",
];

fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    /// Shell command; `$AIRTH_SKILL_DIR` is the skill's folder.
    pub run: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    #[serde(default)]
    pub input: Value,
    /// Files created in the test's workspace first (relative path → content).
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub expect: Expect,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Expect {
    /// Expected exit code (default 0).
    pub exit: Option<i32>,
    /// Text stdout must contain.
    pub contains: Vec<String>,
    pub not_contains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    pub duration_ms: u64,
}

/// The runtime's verdict on a draft, for the files with this hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    pub at: DateTime<Utc>,
    pub hash: String,
    /// What is wrong with the skill's files, before any test runs.
    pub problems: Vec<String>,
    pub results: Vec<TestResult>,
}

impl Evaluation {
    pub fn passed(&self) -> bool {
        self.problems.is_empty() && !self.results.is_empty() && self.results.iter().all(|r| r.ok)
    }

    pub fn describe(&self) -> String {
        let ok = self.results.iter().filter(|r| r.ok).count();
        if !self.problems.is_empty() {
            format!("{} problem(s) with the skill's files", self.problems.len())
        } else {
            format!("{ok}/{} tests passed", self.results.len())
        }
    }

    /// For the agent: every problem and failed test, in detail.
    pub fn report(&self) -> String {
        let mut out = format!("Evaluation: {}\n", self.describe());
        for p in &self.problems {
            out.push_str(&format!("- problem: {p}\n"));
        }
        for r in &self.results {
            if r.ok {
                out.push_str(&format!("- ✓ {}\n", r.name));
            } else {
                out.push_str(&format!("- ✗ {}: {}\n", r.name, r.detail));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    /// A run asked for it; nothing is built yet.
    Proposed,
    Developing,
    /// The latest evaluation passed: waiting for the owner.
    Ready,
    /// The latest evaluation failed.
    Failed,
    Installed,
    Rejected,
}

impl SkillStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillStatus::Proposed => "proposed",
            SkillStatus::Developing => "developing",
            SkillStatus::Ready => "ready",
            SkillStatus::Failed => "failed",
            SkillStatus::Installed => "installed",
            SkillStatus::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installed {
    pub hash: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRecord {
    pub name: String,
    pub status: SkillStatus,
    pub description: String,
    /// Why it is needed (from the proposal).
    #[serde(default)]
    pub reason: String,
    /// What it should do, in detail.
    #[serde(default)]
    pub spec: String,
    /// "owner", or "run:<id>" for a proposal made by a run.
    pub proposed_by: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub dev_run: Option<String>,
    #[serde(default)]
    pub evaluation: Option<Evaluation>,
    #[serde(default)]
    pub installed: Option<Installed>,
}

/// A realm's skills.
#[derive(Debug, Clone)]
pub struct SkillStore {
    dir: PathBuf,
}

impl SkillStore {
    /// `dir` is the realm's `skills/` folder.
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    /// The realm's `skills/` folder, which also holds catalog skills and MCP servers.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self) -> PathBuf {
        self.dir.join("skills.json")
    }

    pub fn draft_dir(&self, name: &str) -> PathBuf {
        self.dir.join("drafts").join(name)
    }

    pub fn installed_root(&self) -> PathBuf {
        self.dir.join("installed")
    }

    pub fn installed_dir(&self, name: &str) -> PathBuf {
        self.installed_root().join(name)
    }

    pub fn list(&self) -> Result<Vec<SkillRecord>> {
        match std::fs::read_to_string(self.file()) {
            Ok(text) => serde_json::from_str(&text).context("parsing skills.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get(&self, name: &str) -> Result<SkillRecord> {
        self.list()?
            .into_iter()
            .find(|s| s.name == name)
            .with_context(|| format!("no skill '{name}' (see `airth skill list`)"))
    }

    /// Record a new skill; an existing record with this name is returned as is.
    pub fn propose(&self, record: SkillRecord) -> Result<(SkillRecord, bool)> {
        validate_name(&record.name)?;
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        if let Some(existing) = all.iter().find(|s| s.name == record.name) {
            return Ok((existing.clone(), false));
        }
        all.push(record.clone());
        write_atomic(&self.file(), &serde_json::to_string_pretty(&all)?)?;
        Ok((record, true))
    }

    pub fn update(&self, name: &str, change: impl FnOnce(&mut SkillRecord)) -> Result<SkillRecord> {
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        let record = all
            .iter_mut()
            .find(|s| s.name == name)
            .with_context(|| format!("no skill '{name}'"))?;
        change(record);
        let record = record.clone();
        write_atomic(&self.file(), &serde_json::to_string_pretty(&all)?)?;
        Ok(record)
    }

    /// Forget a skill entirely: record, draft and installed copy.
    pub fn remove(&self, name: &str) -> Result<bool> {
        validate_name(name)?;
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        let before = all.len();
        all.retain(|s| s.name != name);
        if all.len() == before {
            return Ok(false);
        }
        write_atomic(&self.file(), &serde_json::to_string_pretty(&all)?)?;
        let _ = std::fs::remove_dir_all(self.draft_dir(name));
        let _ = std::fs::remove_dir_all(self.installed_dir(name));
        Ok(true)
    }

    /// Copy the draft into `installed/`. Only a draft whose latest evaluation
    /// passed, and whose files have not changed since, can be installed.
    pub fn install(&self, name: &str) -> Result<SkillRecord> {
        let record = self.get(name)?;
        let draft = self.draft_dir(name);
        let Some(eval) = &record.evaluation else {
            bail!("skill '{name}' has not been evaluated yet");
        };
        if !eval.passed() {
            bail!(
                "skill '{name}' did not pass its evaluation ({})",
                eval.describe()
            );
        }
        let hash = hash_dir(&draft)?;
        if hash != eval.hash {
            bail!("skill '{name}' changed since it was evaluated; evaluate it again first");
        }
        let target = self.installed_dir(name);
        let tmp = self.installed_root().join(format!(".{name}.tmp"));
        let _ = std::fs::remove_dir_all(&tmp);
        copy_dir(&draft, &tmp)?;
        if hash_dir(&tmp)? != hash {
            bail!("copying skill '{name}' failed");
        }
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&tmp, &target)?;
        self.update(name, |r| {
            r.status = SkillStatus::Installed;
            r.installed = Some(Installed {
                hash,
                at: Utc::now(),
            });
        })
    }

    /// Disable an installed skill (the record and the draft are kept).
    pub fn uninstall(&self, name: &str) -> Result<SkillRecord> {
        let _ = std::fs::remove_dir_all(self.installed_dir(name));
        self.update(name, |r| {
            r.installed = None;
            if r.status == SkillStatus::Installed {
                r.status = SkillStatus::Ready;
            }
        })
    }

    /// The installed skills runs may use: those whose files still match what
    /// the owner installed. Others are reported and skipped.
    pub fn enabled(&self) -> Vec<(Manifest, PathBuf)> {
        let mut out = Vec::new();
        for record in self.list().unwrap_or_default() {
            let Some(installed) = &record.installed else {
                continue;
            };
            let dir = self.installed_dir(&record.name);
            match hash_dir(&dir) {
                Ok(h) if h == installed.hash => {}
                _ => {
                    eprintln!(
                        "warning: skill {} is disabled: its installed files changed",
                        record.name
                    );
                    continue;
                }
            }
            match read_manifest(&dir) {
                Ok(m) => out.push((m, dir)),
                Err(e) => eprintln!("warning: skill {}: {e:#}", record.name),
            }
        }
        out
    }
}

/// Tool names: lowercase letters, digits and '_', not one of Airth's own.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = (2..=40).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        bail!("skill names use lowercase letters, digits and '_', e.g. \"csv_summary\"");
    }
    if RESERVED.contains(&name) {
        bail!("'{name}' is one of Airth's own tools");
    }
    Ok(())
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    let text = std::fs::read_to_string(dir.join(MANIFEST))
        .with_context(|| format!("{MANIFEST} is missing"))?;
    serde_json::from_str(&text).with_context(|| format!("{MANIFEST} is not a valid manifest"))
}

/// Everything wrong with a draft's manifest and tests, for `name`.
fn check(dir: &Path, name: &str) -> (Option<Manifest>, Vec<TestCase>, Vec<String>) {
    let mut problems = Vec::new();
    if let Err(e) = list_files(dir) {
        problems.push(format!("{e:#}"));
    }
    let manifest = match read_manifest(dir) {
        Ok(m) => Some(m),
        Err(e) => {
            problems.push(format!("{e:#}"));
            None
        }
    };
    if let Some(m) = &manifest {
        if m.name != name {
            problems.push(format!(
                "the manifest's name is '{}', expected '{name}'",
                m.name
            ));
        }
        if m.description.trim().len() < 10 {
            problems.push("the description must say what the skill does and when to use it".into());
        }
        if m.input_schema.get("type").and_then(Value::as_str) != Some("object") {
            problems.push("input_schema must be a JSON schema with \"type\": \"object\"".into());
        }
        if m.run.trim().is_empty() {
            problems.push("run is empty".into());
        }
        if m.timeout_secs == 0 || m.timeout_secs > MAX_TIMEOUT {
            problems.push(format!("timeout_secs must be 1 to {MAX_TIMEOUT}"));
        }
    }
    let tests: Vec<TestCase> = match std::fs::read_to_string(dir.join(TESTS)) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{TESTS} is not a list of test cases: {e}"));
                Vec::new()
            }
        },
        Err(_) => {
            problems.push(format!("{TESTS} is missing"));
            Vec::new()
        }
    };
    if tests.is_empty() && dir.join(TESTS).exists() {
        problems.push("there must be at least one test".into());
    }
    if tests.len() > MAX_TESTS {
        problems.push(format!("at most {MAX_TESTS} tests"));
    }
    for t in &tests {
        for path in t.files.keys() {
            if !is_relative_inside(path) {
                problems.push(format!(
                    "test '{}': fixture path '{path}' must be relative",
                    t.name
                ));
            }
        }
    }
    (manifest, tests, problems)
}

/// Check a draft and run its tests, each in a fresh folder under `scratch`,
/// in the sandbox, with the draft readable as `$AIRTH_SKILL_DIR`. `deny`
/// lists folders the tests must not reach (the Airth home, other realms).
pub fn evaluate(
    dir: &Path,
    name: &str,
    scratch: &Path,
    deny: &[PathBuf],
    cancel: &CancelToken,
) -> Result<Evaluation> {
    let hash = hash_dir(dir)?;
    let (manifest, tests, problems) = check(dir, name);
    let mut results = Vec::new();
    if let (Some(m), true) = (&manifest, problems.is_empty()) {
        for (i, test) in tests.iter().enumerate() {
            if cancel.is_cancelled() {
                break;
            }
            results.push(run_test(
                m,
                test,
                dir,
                &scratch.join(format!("t{i}")),
                deny,
                cancel,
            ));
        }
    }
    Ok(Evaluation {
        at: Utc::now(),
        hash,
        problems,
        results,
    })
}

fn run_test(
    m: &Manifest,
    test: &TestCase,
    skill_dir: &Path,
    work: &Path,
    deny: &[PathBuf],
    cancel: &CancelToken,
) -> TestResult {
    let start = Instant::now();
    let outcome = (|| -> Result<(bool, String)> {
        let _ = std::fs::remove_dir_all(work);
        std::fs::create_dir_all(work)?;
        for (path, content) in &test.files {
            let p = work.join(path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(p, content)?;
        }
        let mut ws = Workspace::new(work)?;
        for d in deny {
            ws = ws.with_rule(d, Access::None);
        }
        let ws = ws
            .with_rule(skill_dir, Access::Read)
            .with_rule(work, Access::ReadWrite)
            .with_secrets(
                vec![("AIRTH_SKILL_DIR".into(), skill_dir.display().to_string())],
                vec![],
            )
            .with_sandbox(true);
        let out = run_shell_with_input(
            &m.run,
            &ws,
            cancel,
            Duration::from_secs(m.timeout_secs),
            Some(&serde_json::to_string(&test.input)?),
        )?;
        let want = test.expect.exit.unwrap_or(0);
        let mut failures = Vec::new();
        if out.code != Some(want) {
            failures.push(match out.code {
                Some(c) => format!("exit code {c}, expected {want}"),
                None => format!("did not finish (timeout {}s)", m.timeout_secs),
            });
        }
        for c in &test.expect.contains {
            if !out.stdout.contains(c.as_str()) {
                failures.push(format!("stdout lacks {c:?}"));
            }
        }
        for c in &test.expect.not_contains {
            if out.stdout.contains(c.as_str()) {
                failures.push(format!("stdout contains {c:?}"));
            }
        }
        if failures.is_empty() {
            return Ok((true, String::new()));
        }
        Ok((
            false,
            format!(
                "{}\nstdout: {}\nstderr: {}",
                failures.join("; "),
                tail(&out.stdout, 1500),
                tail(&out.stderr, 1500)
            ),
        ))
    })();
    let _ = std::fs::remove_dir_all(work);
    let (ok, detail) = outcome.unwrap_or_else(|e| (false, format!("{e:#}")));
    TestResult {
        name: test.name.clone(),
        ok,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

fn tail(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.len() <= max {
        return s.to_string();
    }
    let mut cut = s.len() - max;
    while !s.is_char_boundary(cut) {
        cut += 1;
    }
    format!("…{}", &s[cut..])
}

fn is_relative_inside(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// The files of a skill folder, relative and sorted. Refuses symlinks and
/// oversized skills.
pub fn list_files(dir: &Path) -> Result<Vec<PathBuf>> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>, bytes: &mut u64) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path)?;
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            if meta.file_type().is_symlink() {
                bail!(
                    "{} is a symlink; skills contain plain files only",
                    rel.display()
                );
            } else if meta.is_dir() {
                walk(&path, base, out, bytes)?;
            } else {
                *bytes += meta.len();
                out.push(rel);
            }
            if out.len() > MAX_FILES || *bytes > MAX_BYTES {
                bail!("a skill is limited to {MAX_FILES} files and {MAX_BYTES} bytes");
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out, &mut 0)?;
    Ok(out)
}

/// A digest of every file's path and content.
pub fn hash_dir(dir: &Path) -> Result<String> {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    let files = list_files(dir)?;
    for rel in &files {
        feed(rel.to_string_lossy().as_bytes());
        feed(&[0]);
        let content = std::fs::read(dir.join(rel))?;
        feed(&(content.len() as u64).to_le_bytes());
        feed(&content);
    }
    Ok(format!("{hash:016x}-{}", files.len()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for rel in list_files(from)? {
        let target = to.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from.join(&rel), &target)?;
    }
    std::fs::create_dir_all(to)?;
    Ok(())
}

/// The format a development run must follow, told to it in its objective.
pub fn development_brief(record: &SkillRecord) -> String {
    format!(
        r#"Develop the skill "{name}" for this realm, so future runs can use it as a tool.

What it does: {description}
Why it is needed: {reason}
Details: {spec}

Your workspace is the skill's own folder; nothing else of the realm is reachable from it. Create:

1. `skill.json`:
   {{"name": "{name}", "description": "<what it does and when to use it; this is what future runs read>", "input_schema": {{"type": "object", "properties": {{...}}, "required": [...]}}, "run": "<shell command>", "timeout_secs": 60}}
2. The program `run` executes. It gets the tool input as JSON on stdin, prints its result on stdout, and exits non-zero on error with a message on stderr. It runs in the calling run's workspace (so relative paths are that run's files), inside Airth's sandbox; its own files are in "$AIRTH_SKILL_DIR", so `run` looks like `python3 "$AIRTH_SKILL_DIR/main.py"`. Use only what this machine has (sh, python3 standard library, ...): no package installs.
3. `tests.json`: a list of cases, each {{"name": "...", "input": {{...}}, "files": {{"relative/path": "content"}}, "expect": {{"exit": 0, "contains": ["..."], "not_contains": ["..."]}}}}. `files` are created in the test's workspace before it runs. Cover normal use, edge cases and errors.

Use `test_skill` to run the same evaluation the runtime will run, fix what fails, and finish when every test passes. Keep the skill small and focused. The owner reviews the files and the results before installing it."#,
        name = record.name,
        description = record.description,
        reason = if record.reason.is_empty() {
            "(not given)"
        } else {
            &record.reason
        },
        spec = if record.spec.is_empty() {
            "(none beyond the description)"
        } else {
            &record.spec
        },
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A working skill: counts words of a file in the caller's workspace.
    pub(crate) fn write_word_count(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST),
            serde_json::json!({
                "name": "word_count",
                "description": "Count the words in a text file of the workspace.",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
                "run": "python3 \"$AIRTH_SKILL_DIR/main.py\"",
                "timeout_secs": 30
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("main.py"),
            "import json, sys\nargs = json.load(sys.stdin)\ntry:\n    text = open(args['path']).read()\nexcept OSError as e:\n    sys.exit(f'cannot read {args[\"path\"]}: {e.strerror}')\nprint(f\"{len(text.split())} words\")\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(TESTS),
            serde_json::json!([
                {"name": "counts", "input": {"path": "a.txt"}, "files": {"a.txt": "one two three"}, "expect": {"contains": ["3 words"]}},
                {"name": "missing file", "input": {"path": "nope.txt"}, "expect": {"exit": 1}}
            ])
            .to_string(),
        )
        .unwrap();
    }

    fn store() -> (tempfile::TempDir, SkillStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(dir.path().join("skills"));
        (dir, store)
    }

    fn record(name: &str) -> SkillRecord {
        SkillRecord {
            name: name.into(),
            status: SkillStatus::Proposed,
            description: "Count words".into(),
            reason: String::new(),
            spec: String::new(),
            proposed_by: "owner".into(),
            created_at: Utc::now(),
            dev_run: None,
            evaluation: None,
            installed: None,
        }
    }

    #[test]
    fn evaluates_then_installs_only_what_was_evaluated() {
        let (tmp, store) = store();
        store.propose(record("word_count")).unwrap();
        let draft = store.draft_dir("word_count");
        write_word_count(&draft);
        let eval = evaluate(
            &draft,
            "word_count",
            &tmp.path().join("scratch"),
            &[],
            &CancelToken::default(),
        )
        .unwrap();
        assert!(eval.passed(), "{}", eval.report());
        assert!(store.install("word_count").is_err(), "not recorded yet");
        store
            .update("word_count", |r| r.evaluation = Some(eval.clone()))
            .unwrap();

        // A change after the evaluation blocks installation.
        std::fs::write(draft.join("extra.txt"), "x").unwrap();
        assert!(store.install("word_count").is_err());
        std::fs::remove_file(draft.join("extra.txt")).unwrap();

        let r = store.install("word_count").unwrap();
        assert_eq!(r.status, SkillStatus::Installed);
        assert_eq!(store.enabled().len(), 1);

        // Tampering with the installed copy disables it.
        std::fs::write(
            store.installed_dir("word_count").join("main.py"),
            "print(1)",
        )
        .unwrap();
        assert!(store.enabled().is_empty());
    }

    #[test]
    fn failing_tests_and_bad_manifests_are_reported() {
        let (tmp, store) = store();
        let draft = store.draft_dir("word_count");
        write_word_count(&draft);
        std::fs::write(
            draft.join(TESTS),
            r#"[{"name": "wrong", "input": {"path": "a.txt"}, "files": {"a.txt": "one"}, "expect": {"contains": ["2 words"]}}]"#,
        )
        .unwrap();
        let eval = evaluate(
            &draft,
            "word_count",
            &tmp.path().join("s"),
            &[],
            &CancelToken::default(),
        )
        .unwrap();
        assert!(!eval.passed());
        assert!(
            eval.report().contains("lacks \"2 words\""),
            "{}",
            eval.report()
        );

        let eval = evaluate(
            &draft,
            "other_name",
            &tmp.path().join("s"),
            &[],
            &CancelToken::default(),
        )
        .unwrap();
        assert!(
            eval.problems
                .iter()
                .any(|p| p.contains("expected 'other_name'"))
        );
        assert!(eval.results.is_empty());
    }

    #[test]
    fn names_are_tool_names() {
        assert!(validate_name("csv_summary").is_ok());
        for bad in ["CSV", "a", "read_file", "x-y", "1abc"] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }
}
