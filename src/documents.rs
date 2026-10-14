//! A realm's documents: the files in its workspace (its root, or its own
//! `files/`), as the app shows them. The owner browses, previews and adds
//! files there; runs work with the same files.
//!
//! Every path comes from the page and is relative to the root: `..`,
//! absolute paths and links that lead outside the root are refused.

use crate::state::{EventStatus, State};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// The largest file the app uploads or previews.
pub const MAX_FILE: usize = 50 << 20;

/// Folders not worth browsing: tools' caches and build output.
const SKIPPED: &[&str] = &["node_modules", "target", "__pycache__"];

#[derive(Debug, Serialize, PartialEq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<DateTime<Utc>>,
}

/// A file a run wrote or edited, still in the workspace.
#[derive(Debug, Serialize, PartialEq)]
pub struct Made {
    pub path: String,
    pub run: String,
    pub at: DateTime<Utc>,
    pub edited: bool,
}

/// `rel` (from the page) inside `root`: an existing file or folder.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let joined = join(root, rel)?;
    let real = joined
        .canonicalize()
        .with_context(|| format!("{rel} does not exist"))?;
    if !real.starts_with(root) {
        bail!("{rel} is outside this realm's documents");
    }
    Ok(real)
}

fn join(root: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => bail!("{rel} is not a path inside this realm's documents"),
        }
    }
    Ok(out)
}

/// `path` relative to `root`, with "/" separators.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The entries of a folder: folders first, then by name. Hidden files and
/// tools' caches are left out.
pub fn list(dir: &Path) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
            continue;
        }
        // Follow links, as the Finder shows them.
        let Ok(meta) = std::fs::metadata(entry.path()) else {
            continue;
        };
        out.push(Entry {
            dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok().map(DateTime::<Utc>::from),
            name,
        });
    }
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// A name for a new file in `dir` that does not replace another:
/// "notes.md", then "notes 2.md", "notes 3.md"...
pub fn free_name(dir: &Path, name: &str) -> Result<String> {
    let name = valid_name(name)?;
    if !dir.join(name).exists() {
        return Ok(name.to_string());
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 2.. {
        let candidate = format!("{stem} {n}{ext}");
        if !dir.join(&candidate).exists() {
            return Ok(candidate);
        }
    }
    unreachable!()
}

/// `name` trimmed, if it can name a file: no path, no "." or "..".
fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        bail!("{name:?} is not a valid file name");
    }
    Ok(name)
}

/// Write an uploaded file in `dir` (inside `root`) without replacing one.
/// Returns its path relative to `root`.
pub fn add(root: &Path, dir: &str, name: &str, bytes: &[u8]) -> Result<String> {
    let dir = resolve(root, dir)?;
    if !dir.is_dir() {
        bail!("not a folder");
    }
    let name = free_name(&dir, name)?;
    let path = dir.join(&name);
    std::fs::write(&path, bytes).with_context(|| format!("cannot write {name}"))?;
    Ok(relative(root, &path))
}

/// The largest file the editor opens.
pub const MAX_TEXT: u64 = 2 << 20;

/// A text file opened in the editor, and its version: what a save must
/// match, so it never replaces a change made meanwhile (by a chat, say).
#[derive(Debug, Serialize)]
pub struct Text {
    pub text: String,
    pub version: String,
}

/// The version of a file: when it last changed, and its size.
pub fn version(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let nanos = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(format!("{nanos}-{}", meta.len()))
}

/// A file of the realm as text, for the editor. Refused when it is too
/// large or not text (an image, an archive...).
pub fn read_text(root: &Path, rel: &str) -> Result<Text> {
    let path = resolve(root, rel)?;
    if !path.is_file() {
        bail!("{rel} is not a file");
    }
    if std::fs::metadata(&path)?.len() > MAX_TEXT {
        bail!(
            "{rel} is too large to edit here (at most {} MB)",
            MAX_TEXT >> 20
        );
    }
    let version = version(&path)?;
    let bytes = std::fs::read(&path)?;
    if bytes.contains(&0) {
        bail!("{rel} is not a text file");
    }
    let text = String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("{rel} is not a text file"))?;
    Ok(Text { text, version })
}

/// Why a save did not happen.
#[derive(Debug)]
pub enum SaveError {
    /// The file changed since the editor read it (at `version`, now this).
    Changed(String),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for SaveError {
    fn from(e: anyhow::Error) -> Self {
        SaveError::Other(e)
    }
}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Other(e.into())
    }
}

/// Save the editor's text over a file read at `expected` (its version).
/// Returns the new version.
pub fn save_text(root: &Path, rel: &str, text: &str, expected: &str) -> Result<String, SaveError> {
    let path = resolve(root, rel)?;
    if !path.is_file() {
        return Err(anyhow::anyhow!("{rel} is not a file").into());
    }
    if text.len() as u64 > MAX_TEXT {
        return Err(
            anyhow::anyhow!("too large to save here (at most {} MB)", MAX_TEXT >> 20).into(),
        );
    }
    let now = version(&path)?;
    if now != expected {
        return Err(SaveError::Changed(now));
    }
    std::fs::write(&path, text)?;
    Ok(version(&path)?)
}

/// Create an empty file, or a folder, named `name` in `dir`. Unlike an
/// upload, the name is the owner's: an existing one is an error.
pub fn create(root: &Path, dir: &str, name: &str, folder: bool) -> Result<String> {
    let dir = resolve(root, dir)?;
    if !dir.is_dir() {
        bail!("not a folder");
    }
    let name = valid_name(name)?;
    let path = dir.join(name);
    if path.symlink_metadata().is_ok() {
        bail!("{name} already exists");
    }
    if folder {
        std::fs::create_dir(&path)
    } else {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| ())
    }
    .with_context(|| format!("cannot create {name}"))?;
    Ok(relative(root, &path))
}

/// Every file of the realm, for "go to file": relative paths, hidden files
/// and tools' caches left out, at most `limit` of them.
pub fn all_files(root: &Path, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = list(&dir) else {
            continue;
        };
        for e in entries {
            let path = dir.join(&e.name);
            if e.dir {
                // Not through links: they could lead out of the root, or in circles.
                if !path.is_symlink() {
                    dirs.push(path);
                }
            } else {
                out.push(relative(root, &path));
                if out.len() >= limit {
                    out.sort();
                    return out;
                }
            }
        }
    }
    out.sort();
    out
}

/// Move a file or folder to the owner's Trash (`~/.Trash`), under a name
/// that does not replace anything there.
pub fn trash(root: &Path, rel: &str) -> Result<()> {
    let path = resolve(root, rel)?;
    if path == root {
        bail!("the documents folder itself cannot be moved to the Trash");
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    let bin = PathBuf::from(home).join(".Trash");
    std::fs::create_dir_all(&bin)?;
    let name = path
        .file_name()
        .context("no file name")?
        .to_string_lossy()
        .to_string();
    let target = bin.join(free_name(&bin, &name)?);
    std::fs::rename(&path, &target).with_context(|| format!("cannot move {rel} to the Trash"))
}

/// The files runs wrote or edited that are still in the workspace, newest
/// first. Paths are as the runs gave them (relative to the root).
pub fn made_by_runs(root: &Path, runs: &[State], limit: usize) -> Vec<Made> {
    let mut latest: std::collections::HashMap<String, Made> = Default::default();
    for run in runs {
        for e in &run.events {
            let edited = match e.action.as_str() {
                "write_file" => false,
                "edit_file" => true,
                _ => continue,
            };
            if e.status != EventStatus::Ok {
                continue;
            }
            let Some(raw) = e.arguments["path"].as_str() else {
                continue;
            };
            let rel = if Path::new(raw).is_absolute() {
                match Path::new(raw).strip_prefix(root) {
                    Ok(p) => relative(Path::new(""), p),
                    Err(_) => continue,
                }
            } else {
                raw.trim_start_matches("./").to_string()
            };
            match resolve(root, &rel) {
                Ok(p) if p.is_file() => {}
                _ => continue,
            }
            let newer = latest.get(&rel).is_none_or(|m| m.at < e.timestamp);
            if newer {
                // Written once and edited later: still "made" by Airth.
                let edited = edited && latest.get(&rel).is_none_or(|m| m.edited);
                latest.insert(
                    rel.clone(),
                    Made {
                        path: rel,
                        run: run.id.clone(),
                        at: e.timestamp,
                        edited,
                    },
                );
            }
        }
    }
    let mut out: Vec<Made> = latest.into_values().collect();
    out.sort_by_key(|m| std::cmp::Reverse(m.at));
    out.truncate(limit);
    out
}

/// The type to serve a file with, by its extension. Anything that could run
/// as a page (HTML, SVG...) is served as plain bytes or as an image only.
pub fn mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Event;
    use serde_json::json;

    fn root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        (dir, root)
    }

    #[test]
    fn paths_stay_inside_the_root() {
        let (_d, root) = root();
        std::fs::create_dir(root.join("a")).unwrap();
        std::fs::write(root.join("a/n.md"), "x").unwrap();
        assert_eq!(resolve(&root, "a/n.md").unwrap(), root.join("a/n.md"));
        assert_eq!(resolve(&root, "").unwrap(), root);
        assert!(resolve(&root, "../x").is_err());
        assert!(resolve(&root, "a/../../x").is_err());
        assert!(resolve(&root, "/etc/hosts").is_err());
        // A link out of the root is refused too.
        std::os::unix::fs::symlink("/etc", root.join("out")).unwrap();
        assert!(resolve(&root, "out/hosts").is_err());
    }

    #[test]
    fn folders_first_hidden_left_out() {
        let (_d, root) = root();
        for f in ["b.txt", "A.txt", ".secret"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        for d in ["zdir", "node_modules"] {
            std::fs::create_dir(root.join(d)).unwrap();
        }
        let names: Vec<String> = list(&root).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["zdir", "A.txt", "b.txt"]);
    }

    #[test]
    fn adding_never_replaces_a_file() {
        let (_d, root) = root();
        assert_eq!(add(&root, "", "notes.md", b"1").unwrap(), "notes.md");
        assert_eq!(add(&root, "", "notes.md", b"2").unwrap(), "notes 2.md");
        assert_eq!(add(&root, "", "Makefile", b"3").unwrap(), "Makefile");
        assert_eq!(add(&root, "", "Makefile", b"4").unwrap(), "Makefile 2");
        assert_eq!(std::fs::read_to_string(root.join("notes.md")).unwrap(), "1");
        assert!(add(&root, "", "../x", b"").is_err());
        assert!(add(&root, "", "a/b", b"").is_err());
        assert!(add(&root, "..", "x", b"").is_err());
    }

    #[test]
    fn files_made_by_runs_newest_first() {
        let (_d, root) = root();
        std::fs::write(root.join("report.md"), "x").unwrap();
        std::fs::write(root.join("plan.md"), "x").unwrap();
        let event = |action: &str, path: &str, min: i64, status: EventStatus| Event {
            timestamp: DateTime::<Utc>::from_timestamp(min * 60, 0).unwrap(),
            action: action.into(),
            arguments: json!({ "path": path }),
            result: String::new(),
            status,
            tool_use_id: None,
        };
        let mut a = State::new("a", None);
        a.id = "run-a".into();
        a.events = vec![
            event("write_file", "report.md", 1, EventStatus::Ok),
            event("write_file", "gone.md", 2, EventStatus::Ok),
            event("write_file", "denied.md", 3, EventStatus::Denied),
        ];
        let mut b = State::new("b", None);
        b.id = "run-b".into();
        b.events = vec![
            event("edit_file", "./report.md", 5, EventStatus::Ok),
            event(
                "edit_file",
                &root.join("plan.md").to_string_lossy(),
                4,
                EventStatus::Ok,
            ),
            event("read_file", "report.md", 6, EventStatus::Ok),
        ];
        let made = made_by_runs(&root, &[a, b], 10);
        let got: Vec<(&str, &str, bool)> = made
            .iter()
            .map(|m| (m.path.as_str(), m.run.as_str(), m.edited))
            .collect();
        assert_eq!(
            got,
            [("report.md", "run-b", false), ("plan.md", "run-b", true)]
        );
    }

    #[test]
    fn a_save_never_replaces_a_change_made_meanwhile() {
        let (_d, root) = root();
        std::fs::write(root.join("plan.md"), "v1").unwrap();
        let first = read_text(&root, "plan.md").unwrap();
        assert_eq!(first.text, "v1");
        let saved = save_text(&root, "plan.md", "v2 (mine)", &first.version).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("plan.md")).unwrap(),
            "v2 (mine)"
        );
        // A chat changes it; the editor still has the version it saved.
        std::fs::write(root.join("plan.md"), "v3 (a chat's)").unwrap();
        assert!(matches!(
            save_text(&root, "plan.md", "v4", &saved),
            Err(SaveError::Changed(_))
        ));
        assert_eq!(
            std::fs::read_to_string(root.join("plan.md")).unwrap(),
            "v3 (a chat's)"
        );
        assert!(save_text(&root, "../x", "", "").is_err());
    }

    #[test]
    fn only_text_opens_in_the_editor() {
        let (_d, root) = root();
        std::fs::write(root.join("a.bin"), [0u8, 159, 146, 150]).unwrap();
        std::fs::write(root.join("b.txt"), "héllo").unwrap();
        assert!(read_text(&root, "a.bin").is_err());
        assert_eq!(read_text(&root, "b.txt").unwrap().text, "héllo");
        assert!(read_text(&root, "").is_err());
    }

    #[test]
    fn creating_files_and_listing_them_all() {
        let (_d, root) = root();
        assert_eq!(create(&root, "", "src", true).unwrap(), "src");
        assert_eq!(
            create(&root, "src", "main.rs", false).unwrap(),
            "src/main.rs"
        );
        assert!(create(&root, "src", "main.rs", false).is_err());
        assert!(create(&root, "", "../x", false).is_err());
        std::fs::write(root.join("README.md"), "x").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "x").unwrap();
        std::os::unix::fs::symlink("/etc", root.join("etc")).unwrap();
        assert_eq!(all_files(&root, 100), ["README.md", "src/main.rs"]);
        assert_eq!(all_files(&root, 1).len(), 1);
    }

    #[test]
    fn pages_are_never_served_as_pages() {
        assert_eq!(mime(Path::new("a.PDF")), "application/pdf");
        assert_eq!(mime(Path::new("a.html")), "application/octet-stream");
        assert_eq!(mime(Path::new("README")), "application/octet-stream");
    }
}
