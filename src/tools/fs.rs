use super::{Access, Tool, Workspace, str_arg, truncate};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::{Capability, is_permission_critical};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use similar::TextDiff;

const MAX_READ: usize = 100_000;
const MAX_ENTRIES: usize = 500;
/// Directories hidden from listings; they are noise for the agent.
const SKIPPED_DIRS: &[&str] = &[".git", "target", ".airth"];

pub struct ReadFile;

impl Tool for ReadFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description: "Read a UTF-8 text file from the workspace. Paths are relative to the workspace root. For a large file of which you need only a part (a function, a section, the lines around an error), read just those lines with offset and limit.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative file path"},
                    "offset": {"type": "integer", "description": "Optional: first line to read, from 1"},
                    "limit": {"type": "integer", "description": "Optional: how many lines to read"}
                },
                "required": ["path"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Read
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        Ok(format!("read {}", str_arg(input, "path")?))
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let path = ws.resolve(str_arg(input, "path")?)?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let offset = input["offset"].as_u64().map(|n| n.max(1) as usize);
        let limit = input["limit"].as_u64().map(|n| n as usize);
        if offset.is_none() && limit.is_none() {
            if text.len() <= MAX_READ {
                return Ok(text);
            }
            let shown = truncate(text.clone(), MAX_READ);
            let next = text.as_bytes()[..MAX_READ]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                + 1;
            return Ok(format!(
                "{shown}\n[the file has {} lines; read the rest with offset={next}]",
                text.lines().count()
            ));
        }
        let total = text.lines().count();
        let first = offset.unwrap_or(1);
        let selected: Vec<&str> = text
            .lines()
            .skip(first - 1)
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        let last = first + selected.len().saturating_sub(1);
        Ok(format!(
            "[lines {first}-{last} of {total}]\n{}",
            truncate(selected.join("\n"), MAX_READ)
        ))
    }
}

pub struct ListDirectory;

impl Tool for ListDirectory {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "list_directory".into(),
            description: "List files under a workspace directory. Set recursive=true to walk subdirectories (skips .git, target, .airth). Directories end with '/'.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative directory, default '.'"},
                    "recursive": {"type": "boolean", "description": "Walk subdirectories, default false"}
                }
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Read
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        Ok(format!("list {}", input["path"].as_str().unwrap_or(".")))
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let dir = ws.resolve(input["path"].as_str().unwrap_or("."))?;
        let recursive = input["recursive"].as_bool().unwrap_or(false);
        let mut entries = Vec::new();
        walk(&dir, ws, recursive, &mut entries)?;
        entries.sort();
        let total = entries.len();
        entries.truncate(MAX_ENTRIES);
        let mut out = entries.join("\n");
        if total > MAX_ENTRIES {
            out.push_str(&format!("\n... [{} more entries]", total - MAX_ENTRIES));
        }
        Ok(out)
    }
}

fn walk(
    dir: &std::path::Path,
    ws: &Workspace,
    recursive: bool,
    out: &mut Vec<String>,
) -> Result<()> {
    let read = std::fs::read_dir(dir).with_context(|| format!("cannot list {}", dir.display()))?;
    for entry in read {
        let entry = entry?;
        let path = entry.path();
        if ws.access(&path) == Access::None {
            continue; // Airth's own data or another realm
        }
        let rel = path
            .strip_prefix(ws.root())
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if entry.file_type()?.is_dir() {
            let name = entry.file_name();
            if SKIPPED_DIRS.iter().any(|s| name == *s) {
                continue;
            }
            out.push(format!("{rel}/"));
            if recursive && out.len() <= MAX_ENTRIES {
                walk(&path, ws, recursive, out)?;
            }
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

pub struct WriteFile;

impl Tool for WriteFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".into(),
            description: "Create or overwrite a file in the workspace with the full given content. Parent directories are created. Requires human approval; the human sees a diff.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative file path"},
                    "content": {"type": "string", "description": "The complete new file content"}
                },
                "required": ["path", "content"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Write
    }

    fn preview(&self, input: &Value, ws: &Workspace) -> Result<String> {
        let rel = str_arg(input, "path")?;
        let new = str_arg(input, "content")?;
        let path = ws.resolve_writable(rel)?;
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        let header = if path.exists() { "modify" } else { "create" };
        let diff = TextDiff::from_lines(&old, new)
            .unified_diff()
            .context_radius(3)
            .header(&format!("a/{rel}"), &format!("b/{rel}"))
            .to_string();
        Ok(format!("{header} {rel}\n{diff}"))
    }

    fn warnings(&self, input: &Value) -> Vec<String> {
        match input["path"].as_str() {
            Some(p) if is_permission_critical(p) => {
                vec!["this change modifies Airth's security boundaries (permissions, isolation or sandbox)".into()]
            }
            _ => Vec::new(),
        }
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let path = ws.resolve_writable(str_arg(input, "path")?)?;
        let content = str_arg(input, "content")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)
            .with_context(|| format!("cannot write {}", path.display()))?;
        Ok(format!(
            "wrote {} bytes to {}",
            content.len(),
            str_arg(input, "path")?
        ))
    }
}

pub struct EditFile;

impl EditFile {
    /// The file and its content before and after the edit.
    fn edit(input: &Value, ws: &Workspace) -> Result<(std::path::PathBuf, String, String)> {
        let rel = str_arg(input, "path")?;
        let old_text = str_arg(input, "old_text")?;
        let new_text = str_arg(input, "new_text")?;
        let replace_all = input["replace_all"].as_bool().unwrap_or(false);
        if old_text.is_empty() {
            anyhow::bail!("old_text is empty; use write_file to create a file");
        }
        let path = ws.resolve_writable(rel)?;
        let old = std::fs::read_to_string(&path).with_context(|| format!("cannot read {rel}"))?;
        let new = match old.matches(old_text).count() {
            0 => anyhow::bail!(
                "old_text was not found in {rel}; read the file again and copy the text exactly, whitespace included"
            ),
            1 => old.replacen(old_text, new_text, 1),
            n if replace_all => {
                let _ = n;
                old.replace(old_text, new_text)
            }
            n => anyhow::bail!(
                "old_text appears {n} times in {rel}; include more surrounding lines to pick one, or set replace_all"
            ),
        };
        Ok((path, old, new))
    }
}

impl Tool for EditFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "edit_file".into(),
            description: "Change part of an existing file: replace old_text (which must appear exactly once, unless replace_all is set) with new_text. Prefer it to write_file for changes to existing files: it is cheaper and safer. Requires human approval; the human sees a diff.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative file path"},
                    "old_text": {"type": "string", "description": "The exact text to replace, with enough context to be unique"},
                    "new_text": {"type": "string", "description": "The replacement"},
                    "replace_all": {"type": "boolean", "description": "Replace every occurrence, default false"}
                },
                "required": ["path", "old_text", "new_text"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Write
    }

    fn preview(&self, input: &Value, ws: &Workspace) -> Result<String> {
        let rel = str_arg(input, "path")?;
        let (_, old, new) = Self::edit(input, ws)?;
        let diff = TextDiff::from_lines(&old, &new)
            .unified_diff()
            .context_radius(3)
            .header(&format!("a/{rel}"), &format!("b/{rel}"))
            .to_string();
        Ok(format!("modify {rel}\n{diff}"))
    }

    fn warnings(&self, input: &Value) -> Vec<String> {
        WriteFile.warnings(input)
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let (path, _, new) = Self::edit(input, ws)?;
        std::fs::write(&path, new).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(format!("edited {}", str_arg(input, "path")?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_range_of_lines() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        std::fs::write(dir.path().join("f.txt"), "a\nb\nc\nd\n").unwrap();
        let read = |input: Value| {
            ReadFile
                .execute(&input, &ws, &CancelToken::default())
                .unwrap()
        };
        assert_eq!(read(json!({"path": "f.txt"})), "a\nb\nc\nd\n");
        assert_eq!(
            read(json!({"path": "f.txt", "offset": 2, "limit": 2})),
            "[lines 2-3 of 4]\nb\nc"
        );
        assert_eq!(
            read(json!({"path": "f.txt", "offset": 4})),
            "[lines 4-4 of 4]\nd"
        );
        let big = "line\n".repeat(MAX_READ / 5 + 100);
        std::fs::write(dir.path().join("big.txt"), &big).unwrap();
        let out = read(json!({"path": "big.txt"}));
        assert!(
            out.ends_with(&format!("read the rest with offset={}]", MAX_READ / 5 + 1)),
            "{}",
            &out[out.len() - 120..]
        );
    }

    #[test]
    fn write_then_read_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let input = json!({"path": "a/b.txt", "content": "hello\n"});
        assert!(WriteFile.preview(&input, &ws).unwrap().contains("+hello"));
        WriteFile
            .execute(&input, &ws, &CancelToken::default())
            .unwrap();
        assert_eq!(
            ReadFile
                .execute(&json!({"path": "a/b.txt"}), &ws, &CancelToken::default())
                .unwrap(),
            "hello\n"
        );
        let ws = ws.with_rule(dir.path().join("a"), Access::None);
        let listing = ListDirectory
            .execute(&json!({"recursive": true}), &ws, &CancelToken::default())
            .unwrap();
        assert_eq!(listing, "");
        let ws = Workspace::new(dir.path()).unwrap();
        let listing = ListDirectory
            .execute(&json!({"recursive": true}), &ws, &CancelToken::default())
            .unwrap();
        assert_eq!(listing, "a/\na/b.txt");
    }

    #[test]
    fn flags_permission_layer_edits() {
        assert!(
            !WriteFile
                .warnings(&json!({"path": "src/permissions.rs"}))
                .is_empty()
        );
        assert!(
            WriteFile
                .warnings(&json!({"path": "src/main.rs"}))
                .is_empty()
        );
    }

    #[test]
    fn edits_unique_text_only() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        std::fs::write(dir.path().join("f.txt"), "a\nb\na\n").unwrap();
        let c = CancelToken::default();
        let edit = |old: &str, all: bool| json!({"path": "f.txt", "old_text": old, "new_text": "z", "replace_all": all});
        assert!(EditFile.execute(&edit("a", false), &ws, &c).is_err());
        assert!(EditFile.execute(&edit("q", false), &ws, &c).is_err());
        assert!(EditFile.execute(&edit("", false), &ws, &c).is_err());
        assert!(
            EditFile
                .preview(&edit("b", false), &ws)
                .unwrap()
                .contains("+z")
        );
        EditFile.execute(&edit("b", false), &ws, &c).unwrap();
        EditFile.execute(&edit("a", true), &ws, &c).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
            "z\nz\nz\n"
        );
        assert!(
            !EditFile
                .warnings(&json!({"path": "src/runtime.rs"}))
                .is_empty()
        );
    }
}
