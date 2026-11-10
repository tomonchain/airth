//! Each business folder follows its remote (decision 114). The founder
//! changes a business's config (prompts, routines, agents) on GitHub and
//! merges it there; Airth takes it into the folder on the server by itself,
//! instead of the founder pulling by hand. It never overwrites what the
//! server holds: uncommitted edits stop it, and a conflict is undone. Either
//! way the founder is told once.

use crate::realm::Realm;
use crate::supervise::output_within;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often a business folder asks its remote.
const EVERY: Duration = Duration::from_secs(600);
/// A fetch that hangs longer is stopped.
const LIMIT: Duration = Duration::from_secs(120);

/// What following the remote did.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// No remote branch to follow, or nothing new on it.
    Unchanged,
    /// The folder took these commits from the remote (their subjects, newest first).
    Updated(Vec<String>),
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = output_within(
        Command::new("git")
            // A merge commit is Airth's, whatever identity the server has (or lacks).
            .env("GIT_AUTHOR_NAME", "Airth")
            .env("GIT_AUTHOR_EMAIL", "airth@localhost")
            .env("GIT_COMMITTER_NAME", "Airth")
            .env("GIT_COMMITTER_EMAIL", "airth@localhost")
            .arg("-C")
            .arg(dir)
            // Nothing the folder holds runs: no hooks.
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(args)
            // Never wait for a password or a host to be confirmed.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes"),
        LIMIT,
    )
    .context("git did not finish")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// Bring the folder's current branch up to its upstream on the remote.
pub fn follow(dir: &Path) -> Result<Outcome> {
    if !dir.join(".git").exists() {
        return Ok(Outcome::Unchanged);
    }
    // A branch that follows nothing is the founder's to manage.
    let Ok(upstream) = git(
        dir,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    ) else {
        return Ok(Outcome::Unchanged);
    };
    let remote = upstream.split('/').next().unwrap_or("origin").to_string();
    git(dir, &["fetch", "--quiet", &remote])?;
    let new = git(dir, &["log", "--format=%s", "HEAD..@{u}"])?;
    if new.is_empty() {
        return Ok(Outcome::Unchanged);
    }
    let edits = git(dir, &["status", "--porcelain", "--untracked-files=no"])?;
    if !edits.is_empty() {
        bail!(
            "the folder has edits that are not committed ({}): commit or undo them, and Airth takes the new version",
            edits
                .lines()
                .map(|l| l.get(3..).unwrap_or(l))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let ahead = git(dir, &["rev-list", "--count", "@{u}..HEAD"])?;
    if ahead == "0" {
        git(dir, &["merge", "--ff-only", "--quiet", "@{u}"])?;
    } else if let Err(e) = git(
        dir,
        &[
            "merge",
            "--no-edit",
            "--quiet",
            "-m",
            "Take the business's changes from its remote",
            "@{u}",
        ],
    ) {
        // Whatever the merge left half done is undone: the folder stays as it was.
        let _ = git(dir, &["merge", "--abort"]);
        bail!(
            "the remote's changes conflict with commits made on this server: merge them by hand ({e:#})"
        );
    }
    Ok(Outcome::Updated(new.lines().map(String::from).collect()))
}

/// The loop the service runs: each business folder follows its remote,
/// for as long as the service lives.
pub fn run(home: crate::home::Home) {
    loop {
        match home.realms() {
            Ok(realms) => keep_up(&realms, &home.inbox()),
            Err(e) => eprintln!("business folder sync: {e:#}"),
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// Every ten minutes, each business folder follows its remote. What it took
/// is said to the founder; what stopped it is said once, until it clears.
pub fn keep_up(realms: &[Realm], inbox: &crate::notify::Inbox) {
    use crate::supervise::Locked;
    for realm in realms {
        let Some(root) = realm.config.root.clone() else {
            continue;
        };
        {
            let mut last = LAST.locked();
            let last = last.get_or_insert_with(HashMap::new);
            if last
                .get(realm.name())
                .is_some_and(|at| at.elapsed() < EVERY)
            {
                continue;
            }
            last.insert(realm.name().to_string(), Instant::now());
        }
        let outcome = follow(&root);
        if let Ok(Outcome::Updated(subjects)) = &outcome {
            let shown: Vec<String> = subjects.iter().take(5).map(|s| format!("· {s}")).collect();
            let more = subjects.len().saturating_sub(5);
            let _ = inbox.push(
                crate::notify::Kind::Completed,
                realm.name(),
                None,
                &format!(
                    "The business folder took {} change{} from GitHub",
                    subjects.len(),
                    if subjects.len() == 1 { "" } else { "s" }
                ),
                &format!(
                    "{}{}",
                    shown.join("\n"),
                    if more > 0 {
                        format!("\nand {more} more")
                    } else {
                        String::new()
                    }
                ),
            );
            eprintln!(
                "{}: took {} change(s) from its remote",
                realm.name(),
                subjects.len()
            );
        }
        crate::supervise::report(
            inbox,
            &format!("sync/{}", realm.name()),
            realm.name(),
            "Following the business folder's remote",
            outcome.map(|_| ()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(dir: &Path, file: &str, text: &str, message: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        run(dir, &["add", file]);
        run(dir, &["commit", "-q", "-m", message]);
    }

    /// A remote, the founder's checkout of it, and the server's folder.
    fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let remote = t.path().join("remote.git");
        let founder = t.path().join("founder");
        let server = t.path().join("server");
        std::fs::create_dir_all(&remote).unwrap();
        run(&remote, &["init", "-q", "--bare"]);
        std::fs::create_dir_all(&founder).unwrap();
        run(&founder, &["init", "-q"]);
        commit(&founder, "reply.md", "dry\n", "First voice");
        run(
            &founder,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run(&founder, &["push", "-q", "-u", "origin", "main"]);
        run(
            t.path(),
            &["clone", "-q", remote.to_str().unwrap(), "server"],
        );
        (t, founder, server)
    }

    #[test]
    fn a_change_merged_on_the_remote_reaches_the_folder() {
        let (_t, founder, server) = setup();
        assert_eq!(follow(&server).unwrap(), Outcome::Unchanged);
        commit(&founder, "reply.md", "plain words\n", "Plain words on X");
        run(&founder, &["push", "-q"]);
        assert_eq!(
            follow(&server).unwrap(),
            Outcome::Updated(vec!["Plain words on X".into()])
        );
        assert_eq!(
            std::fs::read_to_string(server.join("reply.md")).unwrap(),
            "plain words\n"
        );
        assert_eq!(follow(&server).unwrap(), Outcome::Unchanged);
    }

    #[test]
    fn edits_not_committed_on_the_server_are_never_overwritten() {
        let (_t, founder, server) = setup();
        commit(&founder, "reply.md", "plain words\n", "Plain words on X");
        run(&founder, &["push", "-q"]);
        std::fs::write(server.join("reply.md"), "edited on the server\n").unwrap();
        let err = follow(&server).unwrap_err().to_string();
        assert!(err.contains("reply.md"), "{err}");
        assert_eq!(
            std::fs::read_to_string(server.join("reply.md")).unwrap(),
            "edited on the server\n"
        );
    }

    #[test]
    fn commits_made_on_the_server_are_kept_and_a_conflict_undone() {
        let (_t, founder, server) = setup();
        // An adopted change on the server, and another file changed on the remote: both kept.
        commit(&server, "agent.md", "writer\n", "Adopt a run");
        commit(&founder, "post.md", "hook first\n", "Hook first");
        run(&founder, &["push", "-q"]);
        assert_eq!(
            follow(&server).unwrap(),
            Outcome::Updated(vec!["Hook first".into()])
        );
        assert!(server.join("agent.md").exists() && server.join("post.md").exists());
        // The same file changed on both sides: the merge is undone.
        commit(&server, "reply.md", "server\n", "Server voice");
        commit(&founder, "reply.md", "remote\n", "Remote voice");
        run(&founder, &["push", "-q"]);
        let err = follow(&server).unwrap_err().to_string();
        assert!(err.contains("conflict"), "{err}");
        assert_eq!(
            std::fs::read_to_string(server.join("reply.md")).unwrap(),
            "server\n"
        );
        assert!(!server.join(".git/MERGE_HEAD").exists());
    }

    #[test]
    fn a_folder_without_git_or_upstream_is_left_alone() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(follow(t.path()).unwrap(), Outcome::Unchanged);
        run(t.path(), &["init", "-q"]);
        commit(t.path(), "a.md", "a\n", "A");
        assert_eq!(follow(t.path()).unwrap(), Outcome::Unchanged);
    }
}
