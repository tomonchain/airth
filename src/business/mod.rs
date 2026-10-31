//! A business run from a realm (decision 30). What belongs to it lives in
//! the realm's `business/` folder, versioned with the realm: its profile
//! (`profile.md`), the owner's strategy (`strategy.md`), the jobs of its
//! bots (`bots/<name>.md`, decision 36) and its archive (`archive/`, past
//! posts and documents). Every run of the realm is told the profile, the
//! strategy and where the archive is; runs may read the folder but never
//! write it, so only the owner changes what the business is and what its
//! bots are for.

pub mod drafts;
pub mod status;
pub mod store;
pub mod time;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The business folder, under the realm's root.
pub const DIR: &str = "business";
const PROFILE: &str = "profile.md";
const STRATEGY: &str = "strategy.md";
const BOTS: &str = "bots";
const ARCHIVE: &str = "archive";
/// Beyond this, the profile is cut in the run's context; the run can read
/// the rest from the file.
const MAX_PROFILE_CHARS: usize = 20_000;
/// Archive files counted for the context; the run lists the folder itself.
const MAX_COUNTED: usize = 10_000;

pub fn dir(root: &Path) -> PathBuf {
    root.join(DIR)
}

/// The owner's strategy as written, empty when there is none yet.
pub fn strategy(root: &Path) -> Result<String> {
    Ok(read_optional(&dir(root), STRATEGY)?.unwrap_or_default())
}

/// Write the owner's strategy (only the owner does: runs cannot write the
/// business folder).
pub fn set_strategy(root: &Path, text: &str) -> Result<()> {
    let dir = dir(root);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    crate::realm::write_atomic(&dir.join(STRATEGY), &format!("{}\n", text.trim()))
}

/// A bot's job, as the owner wrote it in `business/bots/<name>.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub text: String,
    /// SHA-256 of the text: recorded with each of the bot's runs.
    pub version: String,
}

/// The job of the bot `name`.
pub fn job(root: &Path, name: &str) -> Result<Job> {
    crate::realm::validate_name(name).context("invalid bot name")?;
    let path = format!("{DIR}/{BOTS}/{name}.md");
    let text = std::fs::read_to_string(root.join(&path))
        .with_context(|| format!("the bot {name} has no job: write it in {path}"))?;
    if text.trim().is_empty() {
        anyhow::bail!("the bot {name} has no job: {path} is empty");
    }
    Ok(Job {
        version: crate::update::sha256_hex(text.as_bytes()),
        text,
    })
}

/// Every bot of the business with its job, by name.
pub fn bots(root: &Path) -> Result<Vec<(String, Job)>> {
    let dir = dir(root).join(BOTS);
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
        // A file that is not a valid bot (bad name, empty) is not listed.
        if let Ok(job) = job(root, name) {
            out.push((name.to_string(), job));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Write the job of the bot `name`, hiring it when it is new (only the owner
/// does: runs cannot write the business folder).
pub fn set_job(root: &Path, name: &str, text: &str) -> Result<()> {
    crate::realm::validate_name(name).context("invalid bot name")?;
    if text.trim().is_empty() {
        anyhow::bail!("a bot needs a job");
    }
    let dir = dir(root).join(BOTS);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    crate::realm::write_atomic(
        &dir.join(format!("{name}.md")),
        &format!("{}\n", text.trim()),
    )
}

/// Let the bot `name` go: its job file is removed.
pub fn remove_job(root: &Path, name: &str) -> Result<()> {
    crate::realm::validate_name(name).context("invalid bot name")?;
    let path = dir(root).join(BOTS).join(format!("{name}.md"));
    std::fs::remove_file(&path).with_context(|| format!("no bot {name}"))
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
    let strategy = read_optional(&dir, STRATEGY)?;
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
            capped(s, STRATEGY)
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
        std::fs::write(business.join(STRATEGY), "Grow the newsletter first.").unwrap();
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
    fn a_strategy_alone_is_a_business() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir(root.path())).unwrap();
        std::fs::write(dir(root.path()).join(STRATEGY), "Sell more.").unwrap();
        assert!(
            briefing(root.path())
                .unwrap()
                .unwrap()
                .contains("Sell more.")
        );
    }

    #[test]
    fn a_bot_has_a_job_with_a_version() {
        let root = tempfile::tempdir().unwrap();
        let bots = dir(root.path()).join(BOTS);
        std::fs::create_dir_all(&bots).unwrap();
        let err = job(root.path(), "writer").unwrap_err();
        assert!(format!("{err:#}").contains("write it in business/bots/writer.md"));
        std::fs::write(bots.join("writer.md"), " \n").unwrap();
        assert!(format!("{:#}", job(root.path(), "writer").unwrap_err()).contains("empty"));
        std::fs::write(bots.join("writer.md"), "Write one article a week.").unwrap();
        let j = job(root.path(), "writer").unwrap();
        assert_eq!(j.text, "Write one article a week.");
        assert_eq!(j.version.len(), 64);
        assert!(job(root.path(), "../writer").is_err());
    }

    #[test]
    fn the_owner_hires_edits_and_lets_bots_go() {
        let root = tempfile::tempdir().unwrap();
        assert!(bots(root.path()).unwrap().is_empty());
        set_job(root.path(), "writer", "Write one article a week.").unwrap();
        set_job(root.path(), "analyst", "Watch the markets.").unwrap();
        std::fs::write(dir(root.path()).join("bots/notes.txt"), "not a bot").unwrap();
        std::fs::write(dir(root.path()).join("bots/empty.md"), "").unwrap();
        let names: Vec<String> = bots(root.path())
            .unwrap()
            .into_iter()
            .map(|b| b.0)
            .collect();
        assert_eq!(names, vec!["analyst", "writer"]);
        set_job(root.path(), "writer", "Write two articles a week.").unwrap();
        assert_eq!(
            job(root.path(), "writer").unwrap().text,
            "Write two articles a week.\n"
        );
        assert!(set_job(root.path(), "writer", "  ").is_err());
        assert!(set_job(root.path(), "../x", "job").is_err());
        remove_job(root.path(), "analyst").unwrap();
        assert!(remove_job(root.path(), "analyst").is_err());
        assert_eq!(bots(root.path()).unwrap().len(), 1);
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
