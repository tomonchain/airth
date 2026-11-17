//! The business's own skills (decision 149): `skills/<name>/` in the
//! business folder, in the Agent Skills format (a `SKILL.md` and the files
//! it mentions). Read here as they are on disk; whether one is offered is
//! the founder's approval, kept in `business.db` by its hash.

use crate::tools::catalog::{BINARY, MAX_FILE, MAX_SKILL, front_matter};
use std::path::Path;

/// The folder, at the business folder's root.
pub const DIR: &str = "skills";
/// Folders deeper than this inside a skill are not read.
const MAX_DEPTH: usize = 6;

/// A skill as its files are now.
#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    /// Its folder's name.
    pub name: String,
    pub description: String,
    /// Its text files, by path from its folder, sorted.
    pub files: Vec<File>,
    /// What was left out: binaries, symbolic links, files too big.
    pub skipped: Vec<String>,
    /// SHA-256 of its files, each path with its content.
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub path: String,
    pub text: String,
    pub hash: String,
}

impl Skill {
    pub fn skill_md(&self) -> &str {
        self.files
            .iter()
            .find(|f| f.path == "SKILL.md")
            .map_or("", |f| f.text.as_str())
    }
}

/// A name as the standard has it: lowercase letters, digits and single
/// hyphens, neither first nor last, 64 at most.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// Every skill of the business folder at `root`, by name: each folder of
/// `skills/` with a valid name and a `SKILL.md`.
pub fn found(root: &Path) -> Vec<Skill> {
    let mut out: Vec<Skill> = std::fs::read_dir(root.join(DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| read(&e.path(), &e.file_name().to_string_lossy()))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn read(dir: &Path, name: &str) -> Option<Skill> {
    if !valid_name(name) {
        return None;
    }
    let mut paths = Vec::new();
    walk(dir, "", 0, &mut paths);
    paths.sort();
    let (mut files, mut skipped, mut total) = (Vec::new(), Vec::new(), 0u64);
    for rel in paths {
        // Never followed: a link could point anywhere on the machine.
        let Ok(meta) = std::fs::symlink_metadata(dir.join(&rel)) else {
            continue;
        };
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let take = meta.is_file()
            && meta.len() <= MAX_FILE
            && total + meta.len() <= MAX_SKILL
            && !BINARY.contains(&ext.as_str());
        let text = take
            .then(|| std::fs::read_to_string(dir.join(&rel)).ok())
            .flatten();
        match text {
            Some(text) => {
                total += meta.len();
                let hash = crate::update::sha256_hex(text.as_bytes());
                files.push(File {
                    path: rel,
                    text,
                    hash,
                });
            }
            None => skipped.push(rel),
        }
    }
    if !files.iter().any(|f| f.path == "SKILL.md") {
        return None;
    }
    let mut all = Vec::new();
    for f in &files {
        all.extend_from_slice(f.path.as_bytes());
        all.push(0);
        all.extend_from_slice(f.hash.as_bytes());
        all.push(0);
    }
    let skill = Skill {
        name: name.to_string(),
        description: String::new(),
        hash: crate::update::sha256_hex(&all),
        files,
        skipped,
    };
    let (_, description) = front_matter(skill.skill_md());
    Some(Skill {
        description,
        ..skill
    })
}

/// Paths under `dir`, from the skill's folder: files and links, never
/// hidden ones, never through a link.
fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let rel = format!("{prefix}{name}");
        match entry.file_type() {
            Ok(t) if t.is_dir() && depth < MAX_DEPTH => {
                walk(&entry.path(), &format!("{rel}/"), depth + 1, out)
            }
            Ok(t) if t.is_dir() => {}
            _ => out.push(rel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn names_follow_the_standard() {
        for ok in ["refunds", "tone-of-voice", "x2", "a"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            "Refunds",
            "-x",
            "x-",
            "a--b",
            "a_b",
            "a.b",
            "../x",
            &"a".repeat(65),
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn reads_each_skill_with_its_files_and_hash() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(
            r,
            "skills/refunds/SKILL.md",
            "---\nname: refunds\ndescription: How we refund.\n---\nAsk first.\n",
        );
        write(r, "skills/refunds/reference/policy.md", "30 days.");
        write(r, "skills/refunds/logo.png", "not text");
        write(r, "skills/refunds/.hidden", "x");
        write(r, "skills/no-skill-md/notes.md", "x");
        write(r, "skills/Bad_Name/SKILL.md", "x");
        write(r, "skills/README.md", "x");
        let found = found(r);
        assert_eq!(found.len(), 1);
        let s = &found[0];
        assert_eq!(s.name, "refunds");
        assert_eq!(s.description, "How we refund.");
        assert_eq!(
            s.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["SKILL.md", "reference/policy.md"]
        );
        assert_eq!(s.skipped, ["logo.png"]);

        // Any change to a file, or a file added, changes the hash.
        let before = s.hash.clone();
        write(r, "skills/refunds/reference/policy.md", "60 days.");
        let changed = found_one(r);
        assert_ne!(changed.hash, before);
        write(r, "skills/refunds/reference/policy.md", "30 days.");
        assert_eq!(found_one(r).hash, before);
        write(r, "skills/refunds/run.sh", "echo");
        assert_ne!(found_one(r).hash, before);
    }

    fn found_one(root: &Path) -> Skill {
        found(root).remove(0)
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_a_link() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secrets.env", "KEY=1");
        write(root.path(), "skills/x/SKILL.md", "---\nname: x\n---\n");
        std::os::unix::fs::symlink(
            outside.path().join("secrets.env"),
            root.path().join("skills/x/keys.md"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("skills/x/out")).unwrap();
        // A whole skill that is a link is not one either.
        std::os::unix::fs::symlink(root.path().join("skills/x"), root.path().join("skills/y"))
            .unwrap();
        let found = found(root.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].files.len(), 1);
        assert!(found[0].files.iter().all(|f| !f.text.contains("KEY")));
        assert_eq!(found[0].skipped, ["keys.md", "out"]);
    }
}
