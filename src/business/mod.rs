//! A business run from a realm (decision 30). What belongs to it lives in
//! the realm's folder, versioned with it (decision 43): `business/` holds its
//! profile (`profile.md`), its goal with the owner's strategy as its body
//! (`goal.md`) and its archive (`archive/`, past posts and documents);
//! `agents/<name>.md` holds each agent's job (decision 36). Every run of the
//! realm is told the profile, the strategy and where the archive is; runs may
//! read these folders but never write them (decision 44), so only the owner
//! changes what the business is and what its agents are for. Folders laid
//! out before decision 43 (`business/bots/`, `business/strategy.md`) are
//! moved once, the first time they are read.

pub mod config;
pub mod store;

// The access layer's areas, under the names the rest of Airth uses.
pub use store::{drafts, time};

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The business folder, under the realm's root.
pub const DIR: &str = "business";
const PROFILE: &str = "profile.md";
const GOAL: &str = "goal.md";
const AGENTS: &str = "agents";
const ARCHIVE: &str = "archive";
/// Beyond this, the profile is cut in the run's context; the run can read
/// the rest from the file.
const MAX_PROFILE_CHARS: usize = 20_000;
/// Archive files counted for the context; the run lists the folder itself.
const MAX_COUNTED: usize = 10_000;

/// The folders that configure a business, at its root (decision 43). Runs
/// read them; only the owner writes them (decision 44).
const CONFIG: [&str; 4] = [DIR, "agents", "routines", "prompts"];

pub fn config_dirs(root: &Path) -> Vec<PathBuf> {
    CONFIG.iter().map(|d| root.join(d)).collect()
}

/// What a change does to the business's config: the config files it
/// touches and the agents whose job it changes, so the owner sees it before
/// adopting (decision 44). `files` are git name-status lines.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ConfigChanges {
    pub files: Vec<String>,
    pub agents: Vec<String>,
}

pub fn config_changes(files: &[String]) -> ConfigChanges {
    let mut out = ConfigChanges::default();
    // A rename lists both paths, after the status.
    for path in files.iter().flat_map(|f| f.split('\t').skip(1)) {
        let mut parts = path.splitn(2, '/');
        let (Some(top), Some(rest)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !CONFIG.contains(&top) || out.files.iter().any(|f| f == path) {
            continue;
        }
        out.files.push(path.to_string());
        if let Some(agent) = rest.strip_suffix(".md")
            && top == "agents"
            && !agent.contains('/')
        {
            out.agents.push(agent.to_string());
        }
    }
    out
}

pub fn dir(root: &Path) -> PathBuf {
    root.join(DIR)
}

/// Move a folder laid out before decision 43: `business/bots/<name>.md` to
/// `agents/<name>.md`, and `business/strategy.md` to the body of
/// `business/goal.md`. Nothing is overwritten: a file whose new place is
/// taken stays where it was.
fn migrate(root: &Path) -> Result<()> {
    let old = dir(root).join("bots");
    if let Ok(entries) = std::fs::read_dir(&old) {
        let agents = root.join(AGENTS);
        std::fs::create_dir_all(&agents).with_context(|| format!("creating {AGENTS}/"))?;
        for entry in entries.flatten() {
            let to = agents.join(entry.file_name());
            if !to.exists() {
                std::fs::rename(entry.path(), &to)
                    .with_context(|| format!("moving {} to {AGENTS}/", entry.path().display()))?;
            }
        }
        let _ = std::fs::remove_dir(&old);
    }
    let strategy = dir(root).join("strategy.md");
    if strategy.exists() && !dir(root).join(GOAL).exists() {
        std::fs::rename(&strategy, dir(root).join(GOAL))
            .context("moving strategy.md to goal.md")?;
    }
    Ok(())
}

/// A file's front matter, as written, and its body.
fn split(text: &str) -> (&str, &str) {
    if text.starts_with("---")
        && let Some(end) = text[3..].find("\n---")
    {
        let close = 3 + end + 4;
        let line_end = text[close..]
            .find('\n')
            .map_or(text.len(), |n| close + n + 1);
        return text.split_at(line_end);
    }
    ("", text)
}

/// Write `body` into the file at `path`, keeping its front matter.
fn write_body(path: &Path, body: &str) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let (front, _) = split(&old);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    crate::realm::write_atomic(path, &format!("{front}{}\n", body.trim()))
}

/// The owner's strategy (the body of `goal.md`), empty when there is none.
pub fn strategy(root: &Path) -> Result<String> {
    migrate(root)?;
    let text = read_optional(&dir(root), GOAL)?.unwrap_or_default();
    Ok(split(&text).1.trim().to_string())
}

/// Write the owner's strategy, keeping the goal's fields (only the owner
/// does: runs cannot write the business folder).
pub fn set_strategy(root: &Path, text: &str) -> Result<()> {
    migrate(root)?;
    write_body(&dir(root).join(GOAL), text)
}

/// Write what the business is (only the owner does).
pub fn set_profile(root: &Path, text: &str) -> Result<()> {
    let dir = dir(root);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    crate::realm::write_atomic(&dir.join(PROFILE), &format!("{}\n", text.trim()))
}

/// An agent's job, as the owner wrote it: the body of `agents/<name>.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub text: String,
    /// SHA-256 of the whole file: recorded with each of the agent's runs.
    pub version: String,
}

/// The job of the agent `name`.
pub fn job(root: &Path, name: &str) -> Result<Job> {
    crate::realm::validate_name(name).context("invalid agent name")?;
    migrate(root)?;
    let path = format!("{AGENTS}/{name}.md");
    let file = std::fs::read_to_string(root.join(&path))
        .with_context(|| format!("the agent {name} has no job: write it in {path}"))?;
    let text = split(&file).1.trim().to_string();
    if text.is_empty() {
        anyhow::bail!("the agent {name} has no job: {path} is empty");
    }
    Ok(Job {
        version: crate::update::sha256_hex(file.as_bytes()),
        text,
    })
}

/// What an agent may use in a task (decision 64), from its file's front
/// matter, comma-separated: the realm's integrations, skills (MCP servers,
/// catalog skills) and repositories by name. Nothing declared: nothing
/// beyond read-only web.
#[derive(Debug, Default, PartialEq)]
pub struct Scope {
    pub integrations: Vec<String>,
    pub skills: Vec<String>,
    pub repos: Vec<String>,
}

pub fn scope(root: &Path, name: &str) -> Result<Scope> {
    crate::realm::validate_name(name).context("invalid agent name")?;
    let path = format!("{AGENTS}/{name}.md");
    let file = std::fs::read_to_string(root.join(&path))
        .with_context(|| format!("no agent {name}: {path} is missing"))?;
    let front = crate::frontmatter::parse(&file);
    let list = |key: &str| -> Vec<String> {
        front
            .text(key)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    Ok(Scope {
        integrations: list("integrations"),
        skills: list("skills"),
        repos: list("repos"),
    })
}

/// A task's budget (decision 64): minutes and model cost in dollars, from
/// `business/profile.md` (`task_max_minutes`, `task_max_cost_usd`), else
/// 30 minutes and $1.
pub fn task_budget(root: &Path) -> (u32, f64) {
    let front = std::fs::read_to_string(root.join(DIR).join(PROFILE))
        .map(|t| crate::frontmatter::parse(&t))
        .ok();
    let field = |key: &str| front.as_ref().and_then(|f| f.text(key));
    (
        field("task_max_minutes")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(30),
        field("task_max_cost_usd")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1.0),
    )
}

/// Every agent of the business with its job, by name.
pub fn bots(root: &Path) -> Result<Vec<(String, Job)>> {
    migrate(root)?;
    let dir = root.join(AGENTS);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".md"))
        else {
            continue;
        };
        // A file that is not a valid agent (bad name, empty) is not listed.
        if let Ok(job) = job(root, name) {
            out.push((name.to_string(), job));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Write the job of the agent `name`, keeping its fields, and hiring it when
/// it is new (only the owner does: runs cannot write `agents/`).
pub fn set_job(root: &Path, name: &str, text: &str) -> Result<()> {
    crate::realm::validate_name(name).context("invalid agent name")?;
    if text.trim().is_empty() {
        anyhow::bail!("an agent needs a job");
    }
    migrate(root)?;
    write_body(&root.join(AGENTS).join(format!("{name}.md")), text)
}

/// Let the agent `name` go: its file is removed.
pub fn remove_job(root: &Path, name: &str) -> Result<()> {
    crate::realm::validate_name(name).context("invalid agent name")?;
    migrate(root)?;
    let path = root.join(AGENTS).join(format!("{name}.md"));
    std::fs::remove_file(&path).with_context(|| format!("no agent {name}"))
}

fn read_optional(dir: &Path, file: &str) -> Result<Option<String>> {
    match std::fs::read_to_string(dir.join(file)) {
        Ok(text) => Ok(Some(text.trim().to_string()).filter(|t| !t.is_empty())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {DIR}/{file}")),
    }
}

/// `text`, or its beginning and where to read the rest.
fn capped(text: &str, file: &str) -> String {
    if text.chars().count() > MAX_PROFILE_CHARS {
        let cut: String = text.chars().take(MAX_PROFILE_CHARS).collect();
        format!("{cut}\n[cut here: read {DIR}/{file} for the rest]\n")
    } else {
        format!("{text}\n")
    }
}

/// What a run of the realm is told about its business, or None when the
/// realm runs none (no profile, no strategy and no archive).
pub fn briefing(root: &Path) -> Result<Option<String>> {
    let dir = dir(root);
    let profile = read_optional(&dir, PROFILE)?;
    let strategy = Some(strategy(root)?).filter(|s| !s.is_empty());
    let archived = count_files(&dir.join(ARCHIVE));
    if profile.is_none() && strategy.is_none() && archived == 0 {
        return Ok(None);
    }
    let mut out = String::new();
    if let Some(p) = &profile {
        out.push_str(&capped(p, PROFILE));
    }
    if let Some(s) = &strategy {
        out.push_str(&format!(
            "\nThe owner's strategy, which all work for the business follows:\n{}",
            capped(s, GOAL)
        ));
    }
    if archived > 0 {
        out.push_str(&format!(
            "The business's archive (past posts and documents) is in {DIR}/{ARCHIVE}/: {archived} file{}. Read what the task needs; do not load all of it.\n",
            if archived == 1 { "" } else { "s" }
        ));
    }
    out.push_str(&format!(
        "The {DIR}/ folder is the owner's: you can read it, not change it."
    ));
    Ok(Some(out))
}

fn count_files(dir: &Path) -> usize {
    fn walk(dir: &Path, n: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if *n >= MAX_COUNTED {
                return;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => walk(&entry.path(), n),
                Ok(t) if t.is_file() => *n += 1,
                _ => {}
            }
        }
    }
    let mut n = 0;
    walk(dir, &mut n);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_realm_without_a_business_has_no_briefing() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(briefing(root.path()).unwrap(), None);
        std::fs::create_dir_all(dir(root.path())).unwrap();
        std::fs::write(dir(root.path()).join(PROFILE), "  \n").unwrap();
        assert_eq!(briefing(root.path()).unwrap(), None, "an empty profile");
    }

    #[test]
    fn the_briefing_holds_the_profile_and_points_to_the_archive() {
        let root = tempfile::tempdir().unwrap();
        let business = dir(root.path());
        std::fs::create_dir_all(business.join("archive/2025")).unwrap();
        std::fs::write(business.join(PROFILE), "Hedgen: on-chain research on X.\n").unwrap();
        std::fs::write(business.join("archive/2025/a.md"), "post").unwrap();
        std::fs::write(business.join("archive/b.md"), "post").unwrap();
        let b = briefing(root.path()).unwrap().unwrap();
        assert!(b.starts_with("Hedgen: on-chain research on X."), "{b}");
        assert!(b.contains("business/archive/: 2 files"), "{b}");
        assert!(b.contains("not change it"), "{b}");
        assert!(!b.contains("strategy"), "{b}");
        std::fs::write(
            business.join(GOAL),
            "---\ngoal: TODO\n---\nGrow the newsletter first.",
        )
        .unwrap();
        let b = briefing(root.path()).unwrap().unwrap();
        assert!(
            b.contains("The owner's strategy, which all work for the business follows:\nGrow the newsletter first."),
            "{b}"
        );
    }

    #[test]
    fn the_owner_writes_the_strategy() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(strategy(root.path()).unwrap(), "");
        set_strategy(root.path(), "  Four articles a month.\n\n").unwrap();
        assert_eq!(strategy(root.path()).unwrap(), "Four articles a month.");
        assert!(
            briefing(root.path())
                .unwrap()
                .unwrap()
                .contains("Four articles a month.")
        );
    }

    #[test]
    fn a_change_to_the_config_is_named_with_its_agents() {
        let files: Vec<String> = [
            "M\tagents/writer.md",
            "A\tbusiness/goal.md",
            "R100\tagents/old.md\tagents/new.md",
            "D\tprompts/post.md",
            "M\tagents/notes/x.txt",
            "M\tsrc/main.rs",
            "M\tagentsx/y.md",
            "M\tbusiness",
        ]
        .map(String::from)
        .to_vec();
        let c = config_changes(&files);
        assert_eq!(
            c.files,
            vec![
                "agents/writer.md",
                "business/goal.md",
                "agents/old.md",
                "agents/new.md",
                "prompts/post.md",
                "agents/notes/x.txt"
            ]
        );
        assert_eq!(c.agents, vec!["writer", "old", "new"]);
        assert_eq!(
            config_changes(&["M\tREADME.md".into()]),
            ConfigChanges::default()
        );
    }

    #[test]
    fn the_owner_writes_the_profile() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(briefing(root.path()).unwrap(), None);
        set_profile(root.path(), " A finance research media on X. ").unwrap();
        assert!(
            briefing(root.path())
                .unwrap()
                .unwrap()
                .starts_with("A finance research media on X.")
        );
    }

    #[test]
    fn a_strategy_alone_is_a_business() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir(root.path())).unwrap();
        // Written before decision 43: moved to goal.md when first read.
        std::fs::write(dir(root.path()).join("strategy.md"), "Sell more.").unwrap();
        assert!(
            briefing(root.path())
                .unwrap()
                .unwrap()
                .contains("Sell more.")
        );
        assert!(dir(root.path()).join(GOAL).exists());
    }

    #[test]
    fn an_agent_has_a_job_with_a_version() {
        let root = tempfile::tempdir().unwrap();
        let agents = root.path().join(AGENTS);
        std::fs::create_dir_all(&agents).unwrap();
        let err = job(root.path(), "writer").unwrap_err();
        assert!(format!("{err:#}").contains("write it in agents/writer.md"));
        std::fs::write(agents.join("writer.md"), "---\ntitle: Writer\n---\n \n").unwrap();
        assert!(format!("{:#}", job(root.path(), "writer").unwrap_err()).contains("empty"));
        std::fs::write(
            agents.join("writer.md"),
            "---\ntitle: Writer\n---\nWrite one article a week.\n",
        )
        .unwrap();
        let j = job(root.path(), "writer").unwrap();
        assert_eq!(j.text, "Write one article a week.");
        assert_eq!(j.version.len(), 64);
        assert!(job(root.path(), "../writer").is_err());
    }

    #[test]
    fn the_owner_hires_edits_and_lets_agents_go() {
        let root = tempfile::tempdir().unwrap();
        assert!(bots(root.path()).unwrap().is_empty());
        set_job(root.path(), "writer", "Write one article a week.").unwrap();
        set_job(root.path(), "analyst", "Watch the markets.").unwrap();
        let agents = root.path().join(AGENTS);
        std::fs::write(agents.join("notes.txt"), "not an agent").unwrap();
        std::fs::write(agents.join("empty.md"), "").unwrap();
        let names: Vec<String> = bots(root.path())
            .unwrap()
            .into_iter()
            .map(|b| b.0)
            .collect();
        assert_eq!(names, vec!["analyst", "writer"]);
        // Editing the job keeps the agent's fields.
        std::fs::write(
            agents.join("writer.md"),
            "---\ntitle: Writer\ndepartment: marketing\n---\nOld job.\n",
        )
        .unwrap();
        set_job(root.path(), "writer", "Write two articles a week.").unwrap();
        assert_eq!(
            std::fs::read_to_string(agents.join("writer.md")).unwrap(),
            "---\ntitle: Writer\ndepartment: marketing\n---\nWrite two articles a week.\n"
        );
        assert!(set_job(root.path(), "writer", "  ").is_err());
        assert!(set_job(root.path(), "../x", "job").is_err());
        remove_job(root.path(), "analyst").unwrap();
        assert!(remove_job(root.path(), "analyst").is_err());
        assert_eq!(bots(root.path()).unwrap().len(), 1);
    }

    /// Decision 43: a folder laid out before it moves once, and nothing is
    /// overwritten.
    #[test]
    fn bots_and_the_strategy_move_to_agents_and_the_goal() {
        let root = tempfile::tempdir().unwrap();
        let (r, business) = (root.path(), dir(root.path()));
        std::fs::create_dir_all(business.join("bots")).unwrap();
        std::fs::create_dir_all(r.join(AGENTS)).unwrap();
        std::fs::write(business.join("bots/writer.md"), "Write.").unwrap();
        std::fs::write(business.join("bots/analyst.md"), "Old analyst.").unwrap();
        std::fs::write(r.join("agents/analyst.md"), "New analyst.").unwrap();
        std::fs::write(business.join("strategy.md"), "Explain, never hype.").unwrap();
        let names: Vec<String> = bots(r).unwrap().into_iter().map(|b| b.0).collect();
        assert_eq!(names, vec!["analyst", "writer"]);
        assert_eq!(job(r, "analyst").unwrap().text, "New analyst.");
        assert_eq!(
            std::fs::read_to_string(business.join("bots/analyst.md")).unwrap(),
            "Old analyst."
        );
        assert!(!business.join("bots/writer.md").exists());
        assert_eq!(strategy(r).unwrap(), "Explain, never hype.");
        assert!(!business.join("strategy.md").exists());
        // The strategy is the goal's body: its fields stay.
        std::fs::write(
            business.join(GOAL),
            "---\nbaseline_minutes: 300\n---\nOld.\n",
        )
        .unwrap();
        set_strategy(r, "New line.").unwrap();
        assert_eq!(
            std::fs::read_to_string(business.join(GOAL)).unwrap(),
            "---\nbaseline_minutes: 300\n---\nNew line.\n"
        );
        assert_eq!(strategy(r).unwrap(), "New line.");
    }

    #[test]
    fn a_long_profile_is_cut_and_says_where_the_rest_is() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir(root.path())).unwrap();
        let long = "é".repeat(MAX_PROFILE_CHARS + 10);
        std::fs::write(dir(root.path()).join(PROFILE), &long).unwrap();
        let b = briefing(root.path()).unwrap().unwrap();
        assert!(b.contains("[cut here: read business/profile.md for the rest]"));
        assert_eq!(b.chars().filter(|c| *c == 'é').count(), MAX_PROFILE_CHARS);
    }
}
