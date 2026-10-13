//! Controlled updates of Airth itself.
//!
//! Airth is built from a git checkout (recorded at build time). A newer
//! version is the tip of that checkout's branch, or of its upstream when the
//! upstream is ahead: changes the owner committed or adopted, or pulled. An
//! update is prepared away from the owner's checkout: a detached worktree at
//! the new commit in Airth's home, built and tested there (`cargo build
//! --release`, `cargo test --release`). Only a version whose tests pass is
//! installed, and only when the owner asks: the binary is swapped atomically,
//! the previous one kept for `airth update --rollback`, and Airth restarts.
//!
//! Nothing a run can do reaches this: the worktree and build live in the home,
//! and the owner triggers installation.

use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const COMMIT: Option<&str> = option_env!("AIRTH_COMMIT");
const SOURCE: Option<&str> = option_env!("AIRTH_SOURCE");
const DIRTY: Option<&str> = option_env!("AIRTH_DIRTY");

/// "0.1.0 (c80f74b)", "+" when built from uncommitted changes.
pub fn version() -> String {
    match COMMIT {
        Some(c) => format!(
            "{} ({}{})",
            env!("CARGO_PKG_VERSION"),
            &c[..c.len().min(7)],
            if DIRTY == Some("1") { "+" } else { "" }
        ),
        None => env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// A newer version of Airth in its source repository.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Available {
    pub commit: String,
    pub short: String,
    /// Where it comes from: the branch, or its upstream.
    pub from: String,
    /// Subjects of the new commits, newest first (at most 20).
    pub changes: Vec<String>,
    pub count: usize,
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn source() -> Option<PathBuf> {
    SOURCE
        .map(PathBuf::from)
        .filter(|p| p.join(".git").exists())
}

/// Whether `source` has a version newer than the running one. `fetch` also
/// asks the upstream (network).
pub fn check(fetch: bool) -> Result<Option<Available>> {
    let (Some(current), Some(repo)) = (COMMIT, source()) else {
        bail!("this Airth was not built from a git checkout, so it cannot update itself");
    };
    let branch = git(&repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .context("the source checkout is not on a branch")?;
    let mut target = (git(&repo, &["rev-parse", &branch])?, branch.clone());
    if let Ok(upstream) = git(
        &repo,
        &[
            "rev-parse",
            "--abbrev-ref",
            &format!("{branch}@{{upstream}}"),
        ],
    ) {
        if fetch {
            let remote = upstream.split('/').next().unwrap_or("origin");
            let _ = git(&repo, &["fetch", "--quiet", remote]);
        }
        if let Ok(up) = git(&repo, &["rev-parse", &upstream])
            && up != target.0
            && git(&repo, &["merge-base", "--is-ancestor", &target.0, &up]).is_ok()
        {
            target = (up, upstream);
        }
    }
    if target.0 == current {
        return Ok(None);
    }
    // Only forward: a branch reset to an older commit is not an update.
    if git(&repo, &["merge-base", "--is-ancestor", current, &target.0]).is_err() {
        return Ok(None);
    }
    let log = git(
        &repo,
        &["log", "--format=%s", &format!("{current}..{}", target.0)],
    )?;
    let changes: Vec<String> = log.lines().map(String::from).collect();
    Ok(Some(Available {
        short: target.0[..7].to_string(),
        commit: target.0,
        from: target.1,
        count: changes.len(),
        changes: changes.into_iter().take(20).collect(),
    }))
}

/// Build and test `available` in `work` (in Airth's home), reporting each
/// step. Returns the new binary, ready to install.
pub fn prepare(available: &Available, work: &Path, step: &dyn Fn(&str)) -> Result<PathBuf> {
    let repo = source().context("the source checkout is gone")?;
    let src = work.join("src");
    let target = work.join("target");
    std::fs::create_dir_all(work)?;
    step("checking out");
    let _ = git(
        &repo,
        &["worktree", "remove", "--force", &src.display().to_string()],
    );
    let _ = std::fs::remove_dir_all(&src);
    let _ = git(&repo, &["worktree", "prune"]);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--detach",
            "--force",
            &src.display().to_string(),
            &available.commit,
        ],
    )?;
    let result = (|| -> Result<PathBuf> {
        let cargo = |args: &[&str], what: &str| -> Result<()> {
            step(what);
            let out = Command::new("cargo")
                .args(args)
                .current_dir(&src)
                .env("CARGO_TARGET_DIR", &target)
                .output()
                .context("cannot run cargo")?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                let tail: Vec<&str> = err.lines().rev().take(40).collect();
                let stdout = String::from_utf8_lossy(&out.stdout);
                let failures: Vec<&str> = stdout
                    .lines()
                    .filter(|l| l.contains("FAILED") || l.contains("panicked"))
                    .take(20)
                    .collect();
                bail!(
                    "{what} failed:\n{}\n{}",
                    failures.join("\n"),
                    tail.into_iter().rev().collect::<Vec<_>>().join("\n")
                );
            }
            Ok(())
        };
        cargo(&["build", "--release", "--locked"], "building")?;
        cargo(&["test", "--release", "--locked"], "testing")?;
        let built = target.join("release/airth");
        let staged = work.join(format!("airth-{}", available.short));
        std::fs::copy(&built, &staged)?;
        Ok(staged)
    })();
    let _ = git(
        &repo,
        &["worktree", "remove", "--force", &src.display().to_string()],
    );
    result
}

/// Swap the running binary for `staged`, keeping the old one next to it as
/// `airth.previous`. Returns the installed path.
pub fn install(staged: &Path) -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let dir = exe.parent().context("binary has no folder")?;
    let next = dir.join(".airth.next");
    std::fs::copy(staged, &next).with_context(|| format!("cannot write to {}", dir.display()))?;
    std::fs::copy(&exe, previous_path(&exe))?;
    std::fs::rename(&next, &exe)?;
    resign(&exe);
    Ok(exe)
}

/// Where the replaced binary is kept: next to it, or next to Airth.app (a
/// second binary inside the bundle would break its signature).
fn previous_path(exe: &Path) -> PathBuf {
    let path = exe.to_string_lossy();
    match path.find(".app/Contents/MacOS/") {
        Some(i) => Path::new(&path[..i + 4]).with_file_name(".airth-previous"),
        None => exe.with_file_name("airth.previous"),
    }
}

/// A binary inside Airth.app changes the bundle: sign it again, or macOS
/// would consider the app damaged.
fn resign(exe: &Path) {
    let path = exe.to_string_lossy();
    if let Some(i) = path.find(".app/Contents/MacOS/") {
        crate::service::sign(Path::new(&path[..i + 4]));
    }
}

/// Put back the binary that was replaced by the last update.
pub fn rollback() -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let previous = previous_path(&exe);
    if !previous.exists() {
        bail!("there is no previous version to go back to");
    }
    let next = exe.with_file_name(".airth.next");
    std::fs::copy(&previous, &next)?;
    std::fs::rename(&next, &exe)?;
    resign(&exe);
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_the_commit() {
        let v = version();
        assert!(v.starts_with(env!("CARGO_PKG_VERSION")));
        if let Some(c) = COMMIT {
            assert!(v.contains(&c[..7]));
        }
    }
}
