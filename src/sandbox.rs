//! OS sandbox for commands, so realm isolation does not depend on the agent
//! choosing to respect it.
//!
//! File tools check a workspace's access rules themselves; a shell command can
//! open any path, so on macOS it runs under `sandbox-exec` with the same rules:
//! no access to the Airth home or other realms, read-only where the run may
//! only read. Elsewhere commands are not confined (yet) and `airth doctor` says
//! so; every command still needs approval unless the realm's policy says
//! otherwise.

use crate::tools::Access;
use std::path::{Path, PathBuf};

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub fn available() -> bool {
    cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).exists() && can_start()
}

#[cfg(not(test))]
fn can_start() -> bool {
    true
}

/// Tests may run inside another sandbox (a chat running the checks), where
/// `sandbox-exec` is refused: there they count it as absent.
#[cfg(test)]
fn can_start() -> bool {
    static USABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USABLE.get_or_init(|| {
        std::process::Command::new(SANDBOX_EXEC)
            .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

pub fn program() -> &'static str {
    SANDBOX_EXEC
}

/// A sandbox profile enforcing `rules` on file contents. Everything else
/// (network, processes, metadata) is left as is.
///
/// In this profile language later rules win, so rules are emitted from the
/// shortest path to the longest: the most specific rule decides, as it does
/// for the file tools. Only reading file *contents* is denied, not metadata,
/// so a command can still traverse a denied directory to reach an allowed one
/// inside it.
pub fn profile(rules: &[(PathBuf, Access)]) -> String {
    let mut rules: Vec<&(PathBuf, Access)> = rules.iter().collect();
    rules.sort_by_key(|(p, _)| p.components().count());
    let mut out = String::from("(version 1)\n(allow default)\n");
    for (path, access) in rules {
        let p = quote(path);
        out.push_str(&match access {
            Access::None => format!("(deny file-read-data file-write* (subpath {p}))\n"),
            Access::Read => {
                format!("(allow file-read-data (subpath {p}))\n(deny file-write* (subpath {p}))\n")
            }
            Access::ReadWrite => format!("(allow file-read-data file-write* (subpath {p}))\n"),
        });
    }
    out
}

fn quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_specific_rule_comes_last() {
        let rules = vec![
            (
                PathBuf::from("/home/me/.airth/realms/a/worktrees/r1"),
                Access::ReadWrite,
            ),
            (PathBuf::from("/home/me/.airth"), Access::None),
            (PathBuf::from("/home/me/repo"), Access::Read),
        ];
        let p = profile(&rules);
        let deny = p.find("\"/home/me/.airth\"").unwrap();
        let allow = p.find("worktrees/r1").unwrap();
        assert!(deny < allow, "{p}");
        assert!(p.contains("(deny file-write* (subpath \"/home/me/repo\"))"));
    }

    #[test]
    fn quotes_paths() {
        assert_eq!(quote(Path::new("/a\"b")), "\"/a\\\"b\"");
    }
}
