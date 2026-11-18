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
    // Where each skill must really be: a folder swapped for a link on the
    // way is not read.
    let Ok(real) = std::fs::canonicalize(root) else {
        return Vec::new();
    };
    let mut out: Vec<Skill> = folders(root)
        .into_iter()
        .filter_map(|name| {
            read(
                &root.join(DIR).join(&name),
                &real.join(DIR).join(&name),
                &name,
            )
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The names of the skills, without reading them: each folder of
/// `skills/` with a valid name and a `SKILL.md`.
pub fn names(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = folders(root)
        .into_iter()
        // A plain file, as `found` reads it: a link is no skill.
        .filter(|n| {
            std::fs::symlink_metadata(root.join(DIR).join(n).join("SKILL.md"))
                .is_ok_and(|m| m.is_file())
        })
        .collect();
    out.sort();
    out
}

/// The folders of `skills/` that are no skill, each with why: a name the
/// standard refuses, or no `SKILL.md` Airth can read (missing, a link, too
/// big, not text). The founder sees them, not the agents.
pub fn not_skills(root: &Path, found: &[Skill]) -> Vec<(String, &'static str)> {
    // A `skills/` that is a link is read as no folder at all, as `found` does.
    if !std::fs::symlink_metadata(root.join(DIR)).is_ok_and(|m| m.is_dir()) {
        return Vec::new();
    }
    let found: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
    let mut out: Vec<(String, &'static str)> = std::fs::read_dir(root.join(DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.'))
        .filter_map(|n| {
            if !valid_name(&n) {
                Some((n, "its name: lowercase letters, digits and single hyphens"))
            } else if !found.contains(&n.as_str()) {
                Some((
                    n,
                    "no SKILL.md Airth can read: missing, a link, too big or not text",
                ))
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

fn folders(root: &Path) -> Vec<String> {
    std::fs::read_dir(root.join(DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| valid_name(n))
        .collect()
}

fn read(dir: &Path, real: &Path, name: &str) -> Option<Skill> {
    let mut paths = Vec::new();
    walk(dir, "", 0, &mut paths);
    paths.sort();
    let (mut files, mut skipped, mut total) = (Vec::new(), Vec::new(), 0u64);
    for rel in paths {
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let text = (!BINARY.contains(&ext.as_str()))
            .then(|| read_inside(dir, real, &rel, MAX_FILE.min(MAX_SKILL - total)))
            .flatten();
        match text {
            Some(text) => {
                total += text.len() as u64;
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

/// The text of file `rel` of the skill whose folder is `dir`, really at
/// `real`, if it is a plain file of that folder of at most `max` bytes.
/// Never through a link, even one swapped in while it is read: a link could
/// point anywhere on the machine, the business's secrets included. Nor a
/// second name of another file (a hard link), for the same reason.
fn read_inside(dir: &Path, real: &Path, rel: &str, max: u64) -> Option<String> {
    use std::io::Read;
    let path = dir.join(rel);
    let mut open = std::fs::OpenOptions::new();
    open.read(true);
    // Not through a link at the last step, and never stuck on a pipe.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut open, libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = open.open(&path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    // What was opened is the file at its real place inside the skill's
    // folder: a folder on the way swapped for a link opens something else.
    let at = std::fs::canonicalize(&path).ok()?;
    if !at.starts_with(real) {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let there = std::fs::metadata(&at).ok()?;
        if meta.nlink() > 1 || (there.dev(), there.ino()) != (meta.dev(), meta.ino()) {
            return None;
        }
    }
    let mut text = String::new();
    file.take(max).read_to_string(&mut text).ok()?;
    Some(text)
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
    fn says_why_a_folder_is_no_skill() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(r, "skills/refunds/SKILL.md", "---\nname: refunds\n---\n");
        write(r, "skills/Bad_Name/SKILL.md", "x");
        write(r, "skills/notes/readme.md", "x");
        write(r, "skills/.git/config", "x");
        let not: Vec<String> = not_skills(r, &found(r))
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(not, ["Bad_Name", "notes"]);
        // A skills folder that is a link: nothing of it is told.
        #[cfg(unix)]
        {
            let other = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(r.join("skills"), other.path().join("skills")).unwrap();
            assert!(not_skills(other.path(), &found(other.path())).is_empty());
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
        // Nor a second name of a file elsewhere.
        std::fs::hard_link(
            outside.path().join("secrets.env"),
            root.path().join("skills/x/hard.md"),
        )
        .unwrap();
        // A skill whose SKILL.md is a link is no skill, by name either.
        write(outside.path(), "w/SKILL.md", "---\nname: w\n---\n");
        std::fs::create_dir_all(root.path().join("skills/w")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("w/SKILL.md"),
            root.path().join("skills/w/SKILL.md"),
        )
        .unwrap();
        assert_eq!(names(root.path()), ["x"]);
        let found = found(root.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].files.len(), 1);
        assert!(found[0].files.iter().all(|f| !f.text.contains("KEY")));
        assert_eq!(found[0].skipped, ["hard.md", "keys.md", "out"]);
        // The whole skills folder swapped for a link: nothing is read.
        let elsewhere = tempfile::tempdir().unwrap();
        write(elsewhere.path(), "z/SKILL.md", "---\nname: z\n---\n");
        std::fs::rename(root.path().join("skills"), root.path().join("old")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.path().join("skills")).unwrap();
        assert!(super::found(root.path()).is_empty());
    }
}
