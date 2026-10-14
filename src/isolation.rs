//! Isolated working copies: in a realm whose root is a git repository, each
//! run works in its own git worktree on its own branch, never in the owner's
//! checkout. When the run ends the runtime commits what changed, runs the
//! realm's checks, and the owner reviews the diff and adopts (merges) or
//! discards it.
//!
//! Git is driven by the runtime here, outside the agent's sandbox, so hooks
//! are disabled: a hook path inside the repository is content the agent may
//! have written.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Isolation {
    /// Runs work directly in the realm's root.
    #[default]
    None,
    /// Runs work in a git worktree of the realm's root.
    Worktree,
}

impl Isolation {
    pub fn as_str(self) -> &'static str {
        match self {
            Isolation::None => "none",
            Isolation::Worktree => "worktree",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Open,
    Adopted,
    Discarded,
}

impl ChangeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeStatus::Open => "open",
            ChangeStatus::Adopted => "adopted",
            ChangeStatus::Discarded => "discarded",
        }
    }
}

/// A run's isolated change: where it lives and what became of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    /// The owner's repository (the realm root).
    pub repo: PathBuf,
    /// The worktree the run works in.
    pub path: PathBuf,
    pub branch: String,
    /// The branch of the checkout the chat started from (none when it was detached
    /// or the chat predates this field). Adopting merges back into it.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// The commit the worktree started from.
    pub base: String,
    /// The latest commit on the branch.
    pub head: String,
    pub status: ChangeStatus,
    /// Changed files against `base` (git name-status lines).
    #[serde(default)]
    pub files: Vec<String>,
    /// The latest run of the realm's checks.
    #[serde(default)]
    pub checks: Option<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRun {
    /// The commit that was checked.
    pub commit: String,
    pub started_at: DateTime<Utc>,
    pub finished: bool,
    pub results: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub command: String,
    pub ok: bool,
    pub output: String,
    pub duration_ms: u64,
}

impl Change {
    pub fn has_changes(&self) -> bool {
        self.base != self.head
    }

    /// The latest checks ran on the current head, finished, and all passed.
    pub fn checks_passed(&self) -> bool {
        self.checks
            .as_ref()
            .is_some_and(|c| c.finished && c.commit == self.head && c.results.iter().all(|r| r.ok))
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// Whether `dir` is the top level of a git repository with at least one commit.
pub fn is_repo_root(dir: &Path) -> bool {
    let Ok(top) = git(dir, &["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    let same = Path::new(&top).canonicalize().ok() == dir.canonicalize().ok();
    same && git(dir, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok()
}

/// The repository's shared git directory (where objects and worktree metadata live).
pub fn common_dir(repo: &Path) -> Result<PathBuf> {
    let dir = git(repo, &["rev-parse", "--git-common-dir"])?;
    let dir = Path::new(&dir);
    Ok(if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        repo.join(dir)
    })
}

/// The branch checked out in `repo`, if any (none when HEAD is detached).
fn current_branch(repo: &Path) -> Option<String> {
    git(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty())
}

/// Create a worktree for run `id`, branched from the repository's HEAD.
pub fn create(repo: &Path, worktrees_dir: &Path, id: &str) -> Result<Change> {
    let base = git(repo, &["rev-parse", "HEAD"]).context("the repository has no commits yet")?;
    let base_branch = current_branch(repo);
    let branch = format!("airth/{id}");
    std::fs::create_dir_all(worktrees_dir)?;
    let path = worktrees_dir.join(id);
    git(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            path.to_str().context("non UTF-8 path")?,
            &base,
        ],
    )?;
    Ok(Change {
        repo: repo.to_path_buf(),
        path: path.canonicalize()?,
        branch,
        base_branch,
        head: base.clone(),
        base,
        status: ChangeStatus::Open,
        files: Vec::new(),
        checks: None,
    })
}

/// Commit everything the run changed in its worktree. Returns whether there
/// was anything to commit.
pub fn commit(change: &mut Change, message: &str) -> Result<bool> {
    if change.status != ChangeStatus::Open || !change.path.exists() {
        return Ok(false);
    }
    let dirty = !git(&change.path, &["status", "--porcelain"])?.is_empty();
    if dirty {
        git(&change.path, &["add", "-A"])?;
        git(
            &change.path,
            &[
                "-c",
                "user.name=Airth",
                "-c",
                "user.email=airth@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--no-verify",
                "-m",
                message,
            ],
        )?;
    }
    change.head = git(&change.path, &["rev-parse", "HEAD"])?;
    change.files = git(
        &change.path,
        &["diff", "--name-status", &change.base, &change.head],
    )?
    .lines()
    .map(String::from)
    .collect();
    Ok(dirty)
}

/// The full diff of the change against its base.
pub fn diff(change: &Change) -> Result<String> {
    let dir = if change.path.exists() {
        &change.path
    } else {
        &change.repo
    };
    git(dir, &["diff", "--no-color", &change.base, &change.head])
}

/// Merge the change into the repository's current branch and remove its worktree.
pub fn adopt(change: &mut Change, message: &str) -> Result<()> {
    if change.status != ChangeStatus::Open {
        bail!("this change was already {}", change.status.as_str());
    }
    if !change.has_changes() {
        bail!("there is nothing to adopt: the run changed no files");
    }
    if let Some(base) = &change.base_branch {
        let now = current_branch(&change.repo);
        if now.as_deref() != Some(base.as_str()) {
            bail!(
                "your checkout is on {}, but this chat started from '{base}': switch back to '{base}' to adopt it",
                now.map_or("no branch".to_string(), |b| format!("'{b}'"))
            );
        }
    }
    if let Err(e) = git(
        &change.repo,
        &[
            "merge",
            "--no-ff",
            "--no-verify",
            "-m",
            message,
            &change.head,
        ],
    ) {
        let _ = git(&change.repo, &["merge", "--abort"]);
        bail!("could not merge into your checkout, nothing was changed: {e:#}");
    }
    change.status = ChangeStatus::Adopted;
    remove(change);
    Ok(())
}

/// Throw the change away: remove its worktree and branch.
pub fn discard(change: &mut Change) -> Result<()> {
    if change.status != ChangeStatus::Open {
        bail!("this change was already {}", change.status.as_str());
    }
    change.status = ChangeStatus::Discarded;
    remove(change);
    Ok(())
}

fn remove(change: &Change) {
    if let Some(path) = change.path.to_str() {
        let _ = git(&change.repo, &["worktree", "remove", "--force", path]);
    }
    let _ = git(&change.repo, &["branch", "-D", &change.branch]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A repository with one commit.
    pub(crate) fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]).unwrap();
        std::fs::write(p.join("a.txt"), "one\n").unwrap();
        git(p, &["add", "."]).unwrap();
        git(
            p,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        )
        .unwrap();
        dir
    }

    #[test]
    fn worktree_commit_diff_adopt() {
        let repo = repo();
        let wt = tempfile::tempdir().unwrap();
        assert!(is_repo_root(repo.path()));
        let mut change = create(repo.path(), wt.path(), "r1").unwrap();
        assert!(!change.has_changes());
        std::fs::write(change.path.join("a.txt"), "two\n").unwrap();
        std::fs::write(change.path.join("b.txt"), "new\n").unwrap();
        // The owner's checkout is untouched while the run works.
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "one\n"
        );

        assert!(commit(&mut change, "run r1").unwrap());
        assert!(change.has_changes());
        assert_eq!(change.files, vec!["M\ta.txt", "A\tb.txt"]);
        assert!(diff(&change).unwrap().contains("+two"));
        assert!(!commit(&mut change, "again").unwrap());

        // Owner identity for the merge commit.
        git(repo.path(), &["config", "user.name", "t"]).unwrap();
        git(repo.path(), &["config", "user.email", "t@t"]).unwrap();
        adopt(&mut change, "adopt r1").unwrap();
        assert_eq!(change.status, ChangeStatus::Adopted);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "two\n"
        );
        assert!(!change.path.exists());
        assert!(adopt(&mut change, "again").is_err());
    }

    #[test]
    fn adopt_requires_the_branch_the_chat_started_from() {
        let repo = repo();
        let wt = tempfile::tempdir().unwrap();
        git(repo.path(), &["config", "user.name", "t"]).unwrap();
        git(repo.path(), &["config", "user.email", "t@t"]).unwrap();
        let mut change = create(repo.path(), wt.path(), "r3").unwrap();
        let start = change.base_branch.clone().expect("a branch");
        std::fs::write(change.path.join("a.txt"), "two\n").unwrap();
        commit(&mut change, "run r3").unwrap();
        git(repo.path(), &["checkout", "-q", "-b", "other"]).unwrap();
        let err = adopt(&mut change, "adopt").unwrap_err().to_string();
        assert!(err.contains(&start), "{err}");
        assert_eq!(change.status, ChangeStatus::Open);
        git(repo.path(), &["checkout", "-q", &start]).unwrap();
        adopt(&mut change, "adopt").unwrap();
        assert_eq!(change.status, ChangeStatus::Adopted);
    }

    #[test]
    fn discard_removes_worktree_and_branch() {
        let repo = repo();
        let wt = tempfile::tempdir().unwrap();
        let mut change = create(repo.path(), wt.path(), "r2").unwrap();
        std::fs::write(change.path.join("a.txt"), "two\n").unwrap();
        commit(&mut change, "run r2").unwrap();
        discard(&mut change).unwrap();
        assert!(!change.path.exists());
        assert!(git(repo.path(), &["rev-parse", "--verify", "-q", "airth/r2"]).is_err());
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "one\n"
        );
    }
}
