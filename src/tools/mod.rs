//! Tools are capabilities the agent can invoke. Each declares the capability it
//! needs; the runtime checks it with the permission layer before `execute`.

mod fs;
mod memory;
mod shell;

use crate::cancel::CancelToken;
use crate::memory::MemoryStore;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Context, Result, bail};
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

/// The tools of a run in a realm: the defaults plus the realm's memory.
pub fn realm_tools(memory: MemoryStore, source: String) -> Vec<Box<dyn Tool>> {
    let mut tools = default_tools();
    tools.push(Box::new(memory::Remember { memory, source }));
    tools
}

pub use shell::run_shell;

/// How much of the filesystem a path grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    None,
    Read,
    ReadWrite,
}

/// Where the agent operates, and what it may touch from there.
///
/// Access is decided by rules on path prefixes; the longest matching prefix
/// wins and anything no rule covers is off-limits. The root is read-write.
/// Realm isolation adds `None` rules for the Airth home and other realms, so a
/// realm whose root contains them (say, the home directory) still cannot reach
/// them. File tools check these rules; commands get the same rules from the
/// sandbox where one is available (see `sandbox.rs`).
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    rules: Vec<(PathBuf, Access)>,
    /// Extra environment for commands (the realm's credentials).
    env: Vec<(String, String)>,
    /// Values never shown to the model: replaced in every tool result.
    secrets: Vec<(String, String)>,
    sandboxed: bool,
}

impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("workspace {} does not exist", root.as_ref().display()))?;
        Ok(Self {
            rules: vec![(root.clone(), Access::ReadWrite)],
            root,
            env: Vec::new(),
            secrets: Vec::new(),
            sandboxed: false,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Grant (or remove, with `Access::None`) access under `path`.
    pub fn with_rule(mut self, path: impl AsRef<Path>, access: Access) -> Self {
        let path = path.as_ref();
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.rules.retain(|(p, _)| *p != path || p == &self.root);
        if path != self.root {
            self.rules.push((path, access));
        }
        self
    }

    /// Environment variables for commands, and values to redact from results.
    pub fn with_secrets(
        mut self,
        env: Vec<(String, String)>,
        redact: Vec<(String, String)>,
    ) -> Self {
        self.env = env;
        self.secrets = redact.into_iter().filter(|(_, v)| v.len() >= 4).collect();
        // Longest first, so a secret containing another is replaced whole.
        self.secrets
            .sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        self
    }

    /// Run commands inside the OS sandbox (when the platform has one).
    pub fn with_sandbox(mut self, sandboxed: bool) -> Self {
        self.sandboxed = sandboxed;
        self
    }

    pub fn rules(&self) -> &[(PathBuf, Access)] {
        &self.rules
    }

    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    pub fn sandboxed(&self) -> bool {
        self.sandboxed
    }

    /// The access granted at an absolute, symlink-free path.
    pub fn access(&self, path: &Path) -> Access {
        self.rules
            .iter()
            .filter(|(p, _)| path.starts_with(p))
            .max_by_key(|(p, _)| p.components().count())
            .map_or(Access::None, |(_, a)| *a)
    }

    /// Resolve a path the agent wants to read.
    pub fn resolve(&self, path: &str) -> Result<PathBuf> {
        self.resolve_for(path, Access::Read)
    }

    /// Resolve a path the agent wants to write.
    pub fn resolve_writable(&self, path: &str) -> Result<PathBuf> {
        self.resolve_for(path, Access::ReadWrite)
    }

    /// Workspace-relative or absolute path → real path, checked against the
    /// rules. Symlinks are followed before checking, so a link inside the
    /// workspace cannot lead outside it.
    fn resolve_for(&self, path: &str, needed: Access) -> Result<PathBuf> {
        let mut out = if Path::new(path).is_absolute() {
            PathBuf::new()
        } else {
            self.root.clone()
        };
        for component in Path::new(path).components() {
            match component {
                Component::Normal(c) => out.push(c),
                Component::RootDir | Component::Prefix(_) => out.push(component),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        bail!("path '{path}' is outside the workspace");
                    }
                }
            }
        }
        let real = real_path(&out);
        match self.access(&real) {
            Access::None if real.starts_with(&self.root) => {
                bail!("path '{path}' is off-limits (it belongs to Airth or another realm)")
            }
            Access::None => bail!("path '{path}' is outside the workspace"),
            Access::Read if needed == Access::ReadWrite => {
                bail!("path '{path}' is read-only for this run")
            }
            _ => Ok(real),
        }
    }

    /// Replace every secret value with a placeholder naming it.
    pub fn redact(&self, text: &str) -> String {
        let mut text = text.to_string();
        for (name, value) in &self.secrets {
            if text.contains(value.as_str()) {
                text = text.replace(value.as_str(), &format!("[secret:{name}]"));
            }
        }
        text
    }
}

/// Canonicalize the longest existing ancestor and append the rest, so paths
/// that do not exist yet (files about to be written) are checked too.
fn real_path(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (
            existing.file_name().map(|n| n.to_os_string()),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
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
    fn symlinks_cannot_lead_outside() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "s").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        assert!(ws.resolve("link/secret").is_err());
        assert!(ws.resolve_writable("link/new").is_err());
    }

    #[test]
    fn longest_rule_wins() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["home/realm/run", "other", "repo"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        // A realm rooted at a directory that also holds Airth's data and another realm.
        let ws = Workspace::new(dir.path())
            .unwrap()
            .with_rule(dir.path().join("home"), Access::None)
            .with_rule(dir.path().join("other"), Access::None)
            .with_rule(dir.path().join("home/realm/run"), Access::ReadWrite)
            .with_rule(dir.path().join("repo"), Access::Read);
        assert!(ws.resolve("notes.txt").is_ok());
        assert!(ws.resolve("home/config.json").is_err());
        assert!(ws.resolve("other/x").is_err());
        assert!(ws.resolve_writable("home/realm/run/x").is_ok());
        assert!(ws.resolve("repo/x").is_ok());
        assert!(ws.resolve_writable("repo/x").is_err());
    }

    #[test]
    fn redacts_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap().with_secrets(
            vec![],
            vec![
                ("TOKEN".into(), "abcd1234".into()),
                ("SHORT".into(), "ab".into()),
            ],
        );
        assert_eq!(ws.redact("x abcd1234 ab"), "x [secret:TOKEN] ab");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "é".repeat(10);
        let t = truncate(s, 5);
        assert!(t.starts_with("éé"));
        assert!(t.contains("truncated"));
    }
}
