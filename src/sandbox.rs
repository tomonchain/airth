//! OS sandbox for commands, so realm isolation does not depend on the agent
//! choosing to respect it.
//!
//! File tools check a workspace's access rules themselves; a shell command can
//! open any path, so it runs under the OS's sandbox with the same rules
//! (`sandbox-exec` on macOS, bubblewrap on Linux, decision 67):
//! no access to the Airth home or other realms, read-only where the run may
//! only read, no writes anywhere the workspace does not allow (a few tool
//! caches aside), none of the owner's credentials (decision 48), and nothing
//! else of the owner's home but the developer tools (decision 50). Elsewhere,
//! or with bubblewrap missing, commands are not confined and `airth doctor`
//! says so; every command still needs approval unless the realm's policy
//! says otherwise.

use crate::tools::Access;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
const BWRAP: &str = "/usr/bin/bwrap";

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

/// The developer tools' folders under the owner's home that commands read:
/// toolchains and Cargo's config, and the owner's git settings. The rest of
/// the home is closed (decision 50).
const DEV_TOOLS: &[&str] = &[".cargo", ".rustup", ".gitconfig", ".config/git"];

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
    ".local/share/keyrings",
    ".password-store",
];

/// The system services that answer keychain queries. Certificates are
/// checked by another one, so HTTPS still works without them.
const KEYCHAIN_SERVICES: &[&str] = &["com.apple.SecurityServer", "com.apple.securityd.xpc"];

/// What holds for every command before the workspace's own rules: nothing
/// written but `/dev` and the tool caches; in the owner's home, nothing read
/// but the developer tools, and never the credentials.
fn baseline(home: Option<&Path>) -> Vec<(PathBuf, Access)> {
    let mut rules = vec![
        (PathBuf::from("/"), Access::Read),
        (PathBuf::from("/dev"), Access::ReadWrite),
    ];
    if let Some(home) = home {
        rules.push((home.to_path_buf(), Access::None));
        rules.extend(DEV_TOOLS.iter().map(|t| (home.join(t), Access::Read)));
        rules.extend(CACHES.iter().map(|c| (home.join(c), Access::ReadWrite)));
        rules.extend(CREDENTIALS.iter().map(|c| (home.join(c), Access::None)));
    }
    rules
}

pub fn available() -> bool {
    match () {
        _ if cfg!(target_os = "macos") => Path::new(SANDBOX_EXEC).exists() && can_start(),
        _ if cfg!(target_os = "linux") => Path::new(BWRAP).exists() && can_start(),
        _ => false,
    }
}

/// Whether the sandbox actually starts a confined `true`. Tests may run
/// inside another sandbox (a chat running the checks), where `sandbox-exec`
/// is refused; on Linux, user namespaces may be off (Ubuntu's AppArmor
/// restriction without bubblewrap's profile). Either counts as absent.
fn can_start() -> bool {
    if cfg!(all(target_os = "macos", not(test))) {
        return true;
    }
    static USABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USABLE.get_or_init(|| {
        command(&[], &[], false, None)
            .arg("true")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// The OS's sandbox around a command, enforcing the same rules on each:
/// append the program to run and its arguments.
pub fn command(
    rules: &[(PathBuf, Access)],
    pinned: &[PathBuf],
    network: bool,
    home: Option<&Path>,
) -> Command {
    if cfg!(target_os = "linux") {
        let mut c = Command::new(BWRAP);
        c.args(bwrap_args(rules, pinned, network, home)).arg("--");
        c
    } else {
        let mut c = Command::new(SANDBOX_EXEC);
        c.arg("-p").arg(profile(rules, pinned, network, home));
        c
    }
}

/// Decision 67: on Linux, a command nobody watches runs only under the
/// sandbox. Without it (bubblewrap missing, user namespaces off), it is
/// refused, never run unconfined; macOS keeps its behaviour.
pub fn refuses_unwatched(sandboxed: bool, watched: bool) -> bool {
    refuses(cfg!(target_os = "linux"), sandboxed, watched, || {
        available()
    })
}

fn refuses(linux: bool, sandboxed: bool, watched: bool, confined: impl Fn() -> bool) -> bool {
    linux && sandboxed && !watched && !confined()
}

/// The name `airth doctor` gives the sandbox.
pub fn name() -> &'static str {
    if cfg!(target_os = "linux") {
        "bubblewrap"
    } else {
        "sandbox-exec"
    }
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

/// bubblewrap's arguments for the same rules as `profile` (decision 67):
/// the system read-only, then each rule as a mount, from the shortest path
/// to the longest, so the most specific one decides. Read is a read-only
/// bind, read and write a writable one, none an empty folder (or
/// `/dev/null` over a file), made read-only once the binds inside it are
/// in place. A rule on a path that does not exist yet is an empty
/// read-only folder there, unless it allows writing: nothing can be
/// created where the rule forbids it. Each pinned path is a mount point of
/// its own, which cannot be renamed or removed. Without network the
/// command has its own loopback, not the host's.
fn bwrap_args(
    rules: &[(PathBuf, Access)],
    pinned: &[PathBuf],
    network: bool,
    home: Option<&Path>,
) -> Vec<OsString> {
    let mut base = baseline(home);
    // Sockets of the owner's session (D-Bus, keyring, SSH agent): the
    // Linux side of the keychain services macOS denies.
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        base.push((PathBuf::from(runtime), Access::None));
    }
    // The system and /dev are the fixed start, not mounts of their own.
    base.retain(|(p, _)| p != Path::new("/") && p != Path::new("/dev"));
    let mut all: Vec<(PathBuf, Access)> = base.into_iter().chain(rules.iter().cloned()).collect();
    for pin in pinned {
        let access = all
            .iter()
            .filter(|(p, _)| pin.starts_with(p))
            .max_by_key(|(p, _)| p.components().count())
            .map_or(Access::Read, |(_, a)| *a);
        all.push((pin.clone(), access));
    }
    all.sort_by_key(|(p, _)| p.components().count());
    let mut args: Vec<OsString> = ["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"]
        .iter()
        .chain(["--unshare-pid", "--unshare-ipc", "--unshare-uts"].iter())
        .chain(["--die-with-parent", "--new-session"].iter())
        .map(OsString::from)
        .collect();
    if !network {
        args.push("--unshare-net".into());
    }
    let mut closed = Vec::new();
    for (path, access) in &all {
        let (op, from): (&str, &Path) = match (access, path.exists(), path.is_dir()) {
            (Access::ReadWrite, true, _) => ("--bind", path),
            (Access::ReadWrite, false, _) => continue,
            (Access::Read, true, _) => ("--ro-bind", path),
            (Access::None, true, false) => ("--ro-bind", Path::new("/dev/null")),
            (Access::None | Access::Read, ..) => {
                args.extend(["--tmpfs".into(), path.into()]);
                closed.push(path.clone());
                continue;
            }
        };
        args.extend([op.into(), from.into(), path.into()]);
    }
    for path in closed {
        args.extend(["--remount-ro".into(), path.into()]);
    }
    args
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
            let out = command(rules, &[], true, Some(&home))
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
            (
                "Library/Application Support/Google/Chrome/Default/Cookies",
                "personal",
            ),
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
            let out = command(&rules, &[], true, Some(&home))
                .args(["cat", path])
                .current_dir(&home)
                .output()
                .unwrap();
            let read = String::from_utf8_lossy(&out.stdout) == text;
            assert_eq!(read, text != "personal", "{path}: read {read}");
        }
    }

    /// Decision 67: bubblewrap mounts the rules from the shortest path to
    /// the longest; a closed folder is emptied, then made read-only once
    /// the binds inside it are in place; a closed file is `/dev/null`; a
    /// path not there yet cannot be created where the rule forbids it; a
    /// pinned path is a mount point of its own, before the rules inside it.
    #[test]
    fn bubblewrap_mounts_the_same_rules() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().canonicalize().unwrap();
        for sub in ["airth/w", "repo", "r/.git/hooks"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        std::fs::write(d.join("key"), "k").unwrap();
        let rules = vec![
            (d.join("airth/w"), Access::ReadWrite),
            (d.join("airth"), Access::None),
            (d.join("repo"), Access::Read),
            (d.join("key"), Access::None),
            (d.join("r"), Access::ReadWrite),
            (d.join("r/routines"), Access::Read),
            (d.join("r/.git/hooks"), Access::Read),
        ];
        let args: Vec<String> = bwrap_args(&rules, &[d.join("r/.git")], true, None)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        // Where `op` mounts `path`: a bind names it third, the others second.
        let at = |op: &str, path: &Path| {
            let path = path.to_string_lossy();
            let names = |i: usize| match op.ends_with("bind") {
                true => args.get(i + 2),
                false => args.get(i + 1),
            };
            (0..args.len())
                .find(|&i| args[i] == op && names(i).is_some_and(|p| *p == path))
                .unwrap_or_else(|| panic!("{op} {path} in {args:?}"))
        };
        assert_eq!(&args[..3], ["--ro-bind", "/", "/"]);
        assert!(!args.contains(&"--unshare-net".to_string()));
        assert!(at("--tmpfs", &d.join("airth")) < at("--bind", &d.join("airth/w")));
        assert!(at("--bind", &d.join("airth/w")) < at("--remount-ro", &d.join("airth")));
        at("--ro-bind", &d.join("repo"));
        let key = args
            .iter()
            .position(|a| *a == d.join("key").to_string_lossy())
            .unwrap();
        assert_eq!(args[key - 1], "/dev/null", "a closed file");
        // Not there yet: an empty read-only folder, so it cannot be created.
        at("--tmpfs", &d.join("r/routines"));
        at("--remount-ro", &d.join("r/routines"));
        // Pinned: a writable mount of its own, before its read-only hooks.
        let git = args
            .windows(3)
            .position(|w| w[0] == "--bind" && w[2] == d.join("r/.git").to_string_lossy())
            .expect("the pinned .git is a mount point");
        assert!(git < at("--ro-bind", &d.join("r/.git/hooks")));
        let offline = bwrap_args(&rules, &[], false, None);
        assert!(offline.contains(&OsString::from("--unshare-net")));
    }

    /// Decision 67: only on Linux, only for a sandboxed workspace nobody
    /// watches, and only when the sandbox does not work.
    #[test]
    fn an_unconfined_command_nobody_watches_is_refused_on_linux() {
        let works = || true;
        let broken = || false;
        assert!(refuses(true, true, false, broken));
        assert!(!refuses(true, true, false, works));
        assert!(!refuses(true, true, true, broken), "the founder watches");
        assert!(!refuses(false, true, false, broken), "macOS keeps its way");
        assert!(
            !refuses(true, false, false, broken),
            "a workspace not sandboxed"
        );
    }

    /// A pinned folder cannot be moved aside, and without network nothing
    /// is reached, under the sandbox the OS has.
    #[test]
    fn a_pinned_folder_stays_and_no_network_means_none() {
        if !available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(d.join(".git/hooks")).unwrap();
        let rules = vec![
            (d.clone(), Access::ReadWrite),
            (d.join(".git/hooks"), Access::Read),
        ];
        let out = command(&rules, &[d.join(".git")], false, None)
            .args([
                "sh",
                "-c",
                "mv .git g; echo x > .git/hooks/pre-commit; echo y > mine",
            ])
            .current_dir(&d)
            .output()
            .unwrap();
        assert!(d.join(".git").is_dir() && !d.join("g").exists(), "{out:?}");
        assert!(!d.join(".git/hooks/pre-commit").exists());
        assert!(d.join("mine").exists(), "the rest is writable");
        if cfg!(target_os = "linux") {
            let out = command(&rules, &[], false, None)
                .args(["sh", "-c", "getent hosts example.com || echo offline"])
                .output()
                .unwrap();
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("offline"),
                "{out:?}"
            );
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
