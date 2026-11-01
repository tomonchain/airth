//! OS sandbox for commands, so realm isolation does not depend on the agent
//! choosing to respect it.
//!
//! File tools check a workspace's access rules themselves; a shell command can
//! open any path, so on macOS it runs under `sandbox-exec` with the same rules:
//! no access to the Airth home or other realms, read-only where the run may
//! only read, no writes anywhere the workspace does not allow (a few tool
//! caches aside), and none of the owner's credentials (decision 48). Elsewhere commands are not confined (yet) and `airth doctor` says
//! so; every command still needs approval unless the realm's policy says
//! otherwise.

use crate::tools::Access;
use std::path::{Path, PathBuf};

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Caches under the owner's home that the tools runs use must write: Cargo's
/// downloads and npm's (where `npx` puts the MCP servers it starts). Never a
/// folder on the owner's `PATH`, such as `~/.cargo/bin`.
const CACHES: &[&str] = &[
    ".cargo/registry",
    ".cargo/git",
    ".cargo/.package-cache",
    ".cargo/.package-cache-mutate",
    ".cargo/.global-cache",
    ".npm",
];

/// The owner's credentials under their home, which no command may read.
const CREDENTIALS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".config/gcloud",
    ".kube",
    ".docker",
    ".config/gh",
    ".netrc",
    ".git-credentials",
    ".npmrc",
    ".pypirc",
    ".cargo/credentials",
    ".cargo/credentials.toml",
    "Library/Keychains",
];

/// The system services that answer keychain queries. Certificates are
/// checked by another one, so HTTPS still works without them.
const KEYCHAIN_SERVICES: &[&str] = &["com.apple.SecurityServer", "com.apple.securityd.xpc"];

/// What holds for every command before the workspace's own rules: nothing
/// written but `/dev` and the tool caches, the owner's credentials unread.
fn baseline(home: Option<&Path>) -> Vec<(PathBuf, Access)> {
    let mut rules = vec![
        (PathBuf::from("/"), Access::Read),
        (PathBuf::from("/dev"), Access::ReadWrite),
    ];
    if let Some(home) = home {
        rules.extend(CACHES.iter().map(|c| (home.join(c), Access::ReadWrite)));
        rules.extend(CREDENTIALS.iter().map(|c| (home.join(c), Access::None)));
    }
    rules
}

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

/// A sandbox profile enforcing `rules` on file contents, and cutting
/// commands off the network when `network` is false (local sockets, DNS
/// included, still work). Processes and metadata are left as is.
///
/// In this profile language later rules win, so rules are emitted from the
/// shortest path to the longest: the most specific rule decides, as it does
/// for the file tools. Only reading file *contents* is denied, not metadata,
/// so a command can still traverse a denied directory to reach an allowed one
/// inside it. Each `pinned` path cannot be renamed or removed itself, whatever
/// the rules on it, so its protected contents cannot be moved out of reach.
///
/// The baseline comes first: writes are denied everywhere the rules do not
/// allow them, and `home`'s credentials are never read. Sorting is stable, so
/// at equal depth the workspace's rule, which comes after, decides.
pub fn profile(
    rules: &[(PathBuf, Access)],
    pinned: &[PathBuf],
    network: bool,
    home: Option<&Path>,
) -> String {
    let base = baseline(home);
    let mut rules: Vec<&(PathBuf, Access)> = base.iter().chain(rules).collect();
    rules.sort_by_key(|(p, _)| p.components().count());
    let mut out = String::from("(version 1)\n(allow default)\n");
    if !network {
        out.push_str("(deny network-outbound (remote ip))\n");
    }
    out.push_str("(deny mach-lookup");
    for service in KEYCHAIN_SERVICES {
        out.push_str(&format!(" (global-name \"{service}\")"));
    }
    out.push_str(")\n");
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
    for path in pinned {
        out.push_str(&format!("(deny file-write* (literal {}))\n", quote(path)));
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
        let p = profile(&rules, &[], true, None);
        assert!(!p.contains("network"));
        assert!(profile(&rules, &[], false, None).contains("(deny network-outbound (remote ip))"));
        let deny = p.find("\"/home/me/.airth\"").unwrap();
        let allow = p.find("worktrees/r1").unwrap();
        assert!(deny < allow, "{p}");
        assert!(p.contains("(deny file-write* (subpath \"/home/me/repo\"))"));
    }

    #[test]
    fn pinned_paths_come_after_every_rule() {
        let rules = vec![(PathBuf::from("/r"), Access::ReadWrite)];
        let p = profile(&rules, &[PathBuf::from("/r/.git")], true, None);
        let pin = p.find("(deny file-write* (literal \"/r/.git\"))").unwrap();
        assert!(p.find("(subpath \"/r\")").unwrap() < pin, "{p}");
    }

    /// Writes only where allowed, credentials never read, and a workspace
    /// at the home's root still loses to the credentials inside it.
    #[test]
    fn commands_write_only_where_allowed_and_never_read_credentials() {
        if !available() {
            return;
        }
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        for dir in [".ssh", ".npm", "work", "Library"] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        std::fs::write(home.join(".ssh/id"), "private key").unwrap();
        let run = |rules: &[(PathBuf, Access)], cmd: &str| {
            let out = std::process::Command::new(SANDBOX_EXEC)
                .arg("-p")
                .arg(profile(rules, &[], true, Some(&home)))
                .args(["sh", "-c", cmd])
                .current_dir(&home)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        let work = vec![(home.join("work"), Access::ReadWrite)];
        let out = run(
            &work,
            "echo w > work/a; echo c > .npm/c; echo x > .zshrc; echo x > Library/x; cat .ssh/id; echo n > /dev/null && echo done",
        );
        assert!(out.contains("done"), "{out}");
        assert!(!out.contains("private key"), "{out}");
        assert!(home.join("work/a").exists() && home.join(".npm/c").exists());
        assert!(!home.join(".zshrc").exists());
        assert!(!home.join("Library/x").exists());
        // Even a realm whose folder is the home cannot read the keys.
        let out = run(
            &[(home.clone(), Access::ReadWrite)],
            "cat .ssh/id; echo y > b",
        );
        assert!(!out.contains("private key"), "{out}");
        assert!(home.join("b").exists());
    }

    /// Decision 50: in the owner's home, a command reads only the realm and
    /// the developer tools it uses; personal folders, browser profiles and
    /// mail stay closed.
    #[test]
    fn commands_read_only_the_realm_and_dev_tools_in_the_home() {
        if !available() {
            return;
        }
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        let files = [
            ("Documents/taxes.txt", "personal"),
            ("Library/Mail/V10/inbox.emlx", "personal"),
            ("Library/Application Support/Google/Chrome/Default/Cookies", "personal"),
            (".zsh_history", "personal"),
            ("realm/notes.md", "realm"),
            (".cargo/config.toml", "tool"),
            (".rustup/settings.toml", "tool"),
            (".gitconfig", "tool"),
            (".config/git/ignore", "tool"),
            (".npm/_cacache/index", "tool"),
        ];
        for (path, text) in files {
            std::fs::create_dir_all(home.join(path).parent().unwrap()).unwrap();
            std::fs::write(home.join(path), text).unwrap();
        }
        let rules = vec![(home.join("realm"), Access::ReadWrite)];
        for (path, text) in files {
            let out = std::process::Command::new(SANDBOX_EXEC)
                .arg("-p")
                .arg(profile(&rules, &[], true, Some(&home)))
                .args(["cat", path])
                .current_dir(&home)
                .output()
                .unwrap();
            let read = String::from_utf8_lossy(&out.stdout) == text;
            assert_eq!(read, text != "personal", "{path}: read {read}");
        }
    }

    #[test]
    fn keychain_services_are_out_of_reach() {
        let p = profile(&[], &[], true, None);
        assert!(
            p.contains("(deny mach-lookup (global-name \"com.apple.SecurityServer\")"),
            "{p}"
        );
        assert!(p.contains("(deny file-write* (subpath \"/\"))"), "{p}");
    }

    #[test]
    fn quotes_paths() {
        assert_eq!(quote(Path::new("/a\"b")), "\"/a\\\"b\"");
    }
}
