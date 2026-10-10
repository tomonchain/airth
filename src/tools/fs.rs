use super::{Tool, Workspace, str_arg, truncate};
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
            description: "Read a UTF-8 text file from the workspace. Paths are relative to the workspace root.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"path": {"type": "string", "description": "Workspace-relative file path"}},
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
        Ok(truncate(text, MAX_READ))
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
        let path = ws.resolve(rel)?;
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
                vec!["this change modifies Airth's permission layer".into()]
            }
            _ => Vec::new(),
        }
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let path = ws.resolve(str_arg(input, "path")?)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
