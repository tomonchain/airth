//! A business run from a realm (decision 30). What belongs to it lives in
//! the realm's `business/` folder, versioned with the realm: its profile
//! (`profile.md`) and its archive (`archive/`, past posts and documents).
//! Every run of the realm is told the profile and where the archive is;
//! runs may read the folder but never write it, so only the owner changes
//! what the business is.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The business folder, under the realm's root.
pub const DIR: &str = "business";
const PROFILE: &str = "profile.md";
const ARCHIVE: &str = "archive";
/// Beyond this, the profile is cut in the run's context; the run can read
/// the rest from the file.
const MAX_PROFILE_CHARS: usize = 20_000;
/// Archive files counted for the context; the run lists the folder itself.
const MAX_COUNTED: usize = 10_000;

pub fn dir(root: &Path) -> PathBuf {
    root.join(DIR)
}

/// What a run of the realm is told about its business, or None when the
/// realm runs none (no profile and no archive).
pub fn briefing(root: &Path) -> Result<Option<String>> {
    let dir = dir(root);
    let profile = match std::fs::read_to_string(dir.join(PROFILE)) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).context("reading business/profile.md"),
    };
    let archived = count_files(&dir.join(ARCHIVE));
    if profile.as_deref().is_none_or(|p| p.trim().is_empty()) && archived == 0 {
        return Ok(None);
    }
    let mut out = String::new();
    if let Some(p) = profile.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        if p.chars().count() > MAX_PROFILE_CHARS {
            let cut: String = p.chars().take(MAX_PROFILE_CHARS).collect();
            out.push_str(&format!(
                "{cut}\n[cut here: read {DIR}/{PROFILE} for the rest]\n"
            ));
        } else {
            out.push_str(p);
            out.push('\n');
        }
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
