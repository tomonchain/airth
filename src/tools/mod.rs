//! Tools are capabilities the agent can invoke. Each declares the capability it
//! needs; the runtime checks it with the permission layer before `execute`.

mod fs;
mod shell;

use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Result, bail};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

pub trait Tool {
    fn spec(&self) -> ToolSpec;
    fn capability(&self) -> Capability;
    /// Describe exactly what `execute` would do, for the approval prompt.
    fn preview(&self, input: &Value, ws: &Workspace) -> Result<String>;
    fn warnings(&self, _input: &Value) -> Vec<String> {
        Vec::new()
    }
    /// Long-running tools should poll `cancel` and stop early when it is set.
    fn execute(&self, input: &Value, ws: &Workspace, cancel: &CancelToken) -> Result<String>;
}

pub fn default_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(fs::ReadFile),
        Box::new(fs::ListDirectory),
        Box::new(fs::WriteFile),
        Box::new(shell::ExecuteCommand),
    ]
}

/// The directory the agent operates in. File tools cannot leave it.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            root: root.as_ref().canonicalize()?,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a workspace-relative path, rejecting anything that escapes the root.
    pub fn resolve(&self, path: &str) -> Result<PathBuf> {
        let mut out = self.root.clone();
        for component in Path::new(path).components() {
            match component {
                Component::Normal(c) => out.push(c),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() || !out.starts_with(&self.root) {
                        bail!("path '{path}' escapes the workspace");
                    }
                }
                Component::RootDir | Component::Prefix(_) => {
                    // Absolute paths are accepted only if they point inside the workspace.
                    let abs = Path::new(path);
                    if abs.starts_with(&self.root) {
                        return self.resolve(&abs.strip_prefix(&self.root)?.to_string_lossy());
                    }
                    bail!("path '{path}' is outside the workspace");
                }
            }
        }
        if !out.starts_with(&self.root) {
            bail!("path '{path}' escapes the workspace");
        }
        Ok(out)
    }
}

pub(crate) fn str_arg<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    match input.get(key).and_then(Value::as_str) {
        Some(v) => Ok(v),
        None => bail!("missing string argument '{key}'"),
    }
}

/// Keep tool output to a size that is useful in context.
pub(crate) fn truncate(text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n... [truncated {} of {} bytes]",
        &text[..cut],
        text.len() - cut,
        text.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_stays_inside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        assert_eq!(
            ws.resolve("src/main.rs").unwrap(),
            ws.root().join("src/main.rs")
        );
        assert_eq!(ws.resolve("./a/../b").unwrap(), ws.root().join("b"));
        assert!(ws.resolve("../outside").is_err());
        assert!(ws.resolve("a/../../outside").is_err());
        assert!(ws.resolve("/etc/passwd").is_err());
        let inside = ws.root().join("x.txt");
        assert_eq!(ws.resolve(inside.to_str().unwrap()).unwrap(), inside);
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "é".repeat(10);
        let t = truncate(s, 5);
        assert!(t.starts_with("éé"));
        assert!(t.contains("truncated"));
    }
}
