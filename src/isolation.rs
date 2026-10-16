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

/// How an isolated realm's changes reach the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// The owner adopts each change into their checkout.
    #[default]
    Adopt,
    /// Each change becomes a pull request on GitHub; the owner merges it there.
    PullRequest,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::Adopt => "adopt",
            Delivery::PullRequest => "pull_request",
        }
    }

    pub fn parse(s: &str) -> Result<Delivery> {
        Ok(match s {
            "adopt" | "local" => Delivery::Adopt,
            "pull_request" | "pr" => Delivery::PullRequest,
            other => bail!("unknown delivery '{other}' (adopt, pr)"),
        })
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
    /// Files that conflicted the last time the base branch was merged in;
    /// empty once the conflict is resolved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    /// The base commit being merged in while the chat resolves a conflict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merging: Option<String>,
    /// The pull request the change was opened as, in a realm that delivers
    /// changes as pull requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    /// The last commit pushed to it.
    pub pushed: String,
}

/// What merging the base branch into a change did.
#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    /// The change already contains the base branch.
    UpToDate,
    /// The base branch was merged in cleanly: the change has a new head.
    Updated,
    /// Merging conflicts in these files; nothing was changed.
    Conflict(Vec<String>),
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
pub fn current_branch(repo: &Path) -> Option<String> {
    git(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty())
}

/// Create a worktree for run `id`, branched from the repository's HEAD.
#[cfg(test)]
pub fn create(repo: &Path, worktrees_dir: &Path, id: &str) -> Result<Change> {
    create_from(repo, worktrees_dir, id, "HEAD")
}

/// Create a worktree for run `id`, branched from `start` (a commit or ref).
pub fn create_from(repo: &Path, worktrees_dir: &Path, id: &str, start: &str) -> Result<Change> {
    let base = git(
        repo,
        &["rev-parse", "--verify", &format!("{start}^{{commit}}")],
    )
    .context("the repository has no commits yet")?;
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
        conflicts: Vec::new(),
        merging: None,
        pull_request: None,
    })
}

/// Commit everything the run changed in its worktree. Returns whether there
/// was anything to commit.
pub fn commit(change: &mut Change, message: &str) -> Result<bool> {
    if change.status != ChangeStatus::Open || !change.path.exists() {
        return Ok(false);
    }
    if merge_in_progress(&change.path) {
        return finish_merge(change).map(|()| true);
    }
    if change.merging.take().is_some() {
        // The merge was abandoned: the conflict still stands.
        refresh(change)?;
    }
    let dirty = !git(&change.path, &["status", "--porcelain"])?.is_empty();
    if dirty {
        git(&change.path, &["add", "-A"])?;
        git_as_airth(
            &change.path,
            &["commit", "-q", "--no-verify", "-m", message],
        )?;
    }
    refresh(change)?;
    Ok(dirty)
}

/// Run git in `dir` as Airth: its own identity, no signing.
fn git_as_airth(dir: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec![
        "-c",
        "user.name=Airth",
        "-c",
        "user.email=airth@localhost",
        "-c",
        "commit.gpgsign=false",
    ];
    all.extend_from_slice(args);
    git(dir, &all)
}

/// Read the change's head and the files it changes against its base.
fn refresh(change: &mut Change) -> Result<()> {
    change.head = git(&change.path, &["rev-parse", "HEAD"])?;
    change.files = git(
        &change.path,
        &["diff", "--name-status", &change.base, &change.head],
    )?
    .lines()
    .map(String::from)
    .collect();
    Ok(())
}

fn merge_in_progress(dir: &Path) -> bool {
    git(dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok()
}

/// Files git reports as conflicted (unmerged) in `dir`.
fn conflicted(dir: &Path) -> Result<Vec<String>> {
    Ok(git(dir, &["diff", "--name-only", "--diff-filter=U"])?
        .lines()
        .map(String::from)
        .collect())
}

/// Files among `files` that still hold conflict markers.
fn with_markers(dir: &Path, files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|f| {
            std::fs::read_to_string(dir.join(f)).is_ok_and(|text| {
                text.lines()
                    .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> "))
            })
        })
        .cloned()
        .collect()
}

/// Ensure the change is open and its worktree exists.
fn ensure_open(change: &Change) -> Result<()> {
    if change.status != ChangeStatus::Open {
        bail!("this change was already {}", change.status.as_str());
    }
    if !change.path.exists() {
        bail!("this change's working copy is gone");
    }
    Ok(())
}

/// The commit `target` (a branch, remote branch or commit) points to.
fn tip(change: &Change, target: &str) -> Result<String> {
    git(
        &change.path,
        &["rev-parse", "--verify", &format!("{target}^{{commit}}")],
    )
    .with_context(|| format!("cannot find '{target}'"))
}

/// Whether the change already contains `target`.
pub fn contains(change: &Change, target: &str) -> Result<bool> {
    let tip = tip(change, target)?;
    Ok(git(
        &change.path,
        &["merge-base", "--is-ancestor", &tip, &change.head],
    )
    .is_ok())
}

/// Merge `target` (the branch the chat started from, as it is now) into the
/// change, so the change is checked and delivered against the current base.
/// On a conflict nothing is changed and the conflicting files are recorded.
pub fn update(change: &mut Change, target: &str) -> Result<Update> {
    ensure_open(change)?;
    if change.merging.is_some() {
        bail!("this chat is resolving a conflict with its base branch");
    }
    let tip = tip(change, target)?;
    if git(&change.path, &["merge-base", "--is-ancestor", &tip, "HEAD"]).is_ok() {
        return Ok(Update::UpToDate);
    }
    if !git(&change.path, &["status", "--porcelain"])?.is_empty() {
        bail!("this chat's working copy has edits that are not committed yet");
    }
    let message = format!("Update from {}", short_ref(target));
    match git_as_airth(
        &change.path,
        &["merge", "--no-ff", "--no-verify", "-m", &message, &tip],
    ) {
        Ok(_) => {
            change.base = tip;
            change.conflicts.clear();
            refresh(change)?;
            Ok(Update::Updated)
        }
        Err(e) => {
            let files = conflicted(&change.path).unwrap_or_default();
            let _ = git(&change.path, &["merge", "--abort"]);
            if files.is_empty() {
                return Err(e);
            }
            change.conflicts = files.clone();
            Ok(Update::Conflict(files))
        }
    }
}

/// Start merging `target` into the change and leave the conflicts in its
/// working copy for the chat to resolve; its next commit finishes the merge.
/// Returns the conflicting files (none when it merged cleanly after all).
pub fn start_resolving(change: &mut Change, target: &str) -> Result<Vec<String>> {
    if change.merging.is_some() && merge_in_progress(&change.path) {
        return Ok(change.conflicts.clone());
    }
    change.merging = None;
    match update(change, target)? {
        Update::UpToDate | Update::Updated => Ok(Vec::new()),
        Update::Conflict(_) => {
            let tip = tip(change, target)?;
            let message = format!("Update from {}", short_ref(target));
            // Fails by design: the conflicts stay in the working copy.
            let _ = git_as_airth(
                &change.path,
                &["merge", "--no-ff", "--no-verify", "-m", &message, &tip],
            );
            if !merge_in_progress(&change.path) {
                bail!("could not start merging '{target}'");
            }
            change.conflicts = conflicted(&change.path)?;
            change.merging = Some(tip);
            Ok(change.conflicts.clone())
        }
    }
}

/// Commit the merge the chat resolved, unless conflict markers remain.
fn finish_merge(change: &mut Change) -> Result<()> {
    // Git keeps a file "unmerged" until it is added, resolved or not: look
    // for markers instead.
    let mut files = conflicted(&change.path)?;
    files.extend(change.conflicts.iter().cloned());
    files.sort();
    files.dedup();
    let left = with_markers(&change.path, &files);
    if !left.is_empty() {
        bail!(
            "the conflict with the base branch is not resolved yet: markers remain in {}",
            left.join(", ")
        );
    }
    git(&change.path, &["add", "-A"])?;
    git_as_airth(&change.path, &["commit", "-q", "--no-verify", "--no-edit"])?;
    if let Some(tip) = change.merging.take() {
        change.base = tip;
    }
    change.conflicts.clear();
    refresh(change)
}

/// `refs/remotes/origin/main` → `origin/main`, `refs/heads/main` → `main`.
fn short_ref(target: &str) -> &str {
    target
        .strip_prefix("refs/heads/")
        .or_else(|| target.strip_prefix("refs/remotes/"))
        .unwrap_or(target)
}

/// Tracked files with edits in the owner's checkout that the change also
/// touches: merging would have to overwrite them.
pub fn blocking_edits(change: &Change) -> Result<Vec<String>> {
    let edited: Vec<String> = git(
        &change.repo,
        &["status", "--porcelain", "--untracked-files=no"],
    )?
    .lines()
    .filter_map(|l| l.get(3..))
    .map(|p| p.rsplit(" -> ").next().unwrap_or(p).to_string())
    .collect();
    let touched: Vec<&str> = change
        .files
        .iter()
        .flat_map(|f| f.split('\t').skip(1))
        .collect();
    Ok(edited
        .into_iter()
        .filter(|e| touched.contains(&e.as_str()))
        .collect())
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
    if let Some(base) = &change.base_branch
        && change.path.exists()
        && !contains(change, &format!("refs/heads/{base}"))?
    {
        bail!("'{base}' changed since this chat last updated: update the chat first");
    }
    let blocking = blocking_edits(change)?;
    if !blocking.is_empty() {
        bail!(
            "your checkout has edits that are not committed in {}: commit or stash them, then adopt again",
            blocking.join(", ")
        );
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

/// Fast-forward the owner's checkout to `target` when it is on `branch` and
/// can move without touching their edits (best effort: never merges).
pub fn fast_forward(repo: &Path, branch: &str, target: &str) {
    if current_branch(repo).as_deref() == Some(branch) {
        let _ = git(repo, &["merge", "--ff-only", "--quiet", target]);
    }
}

/// Record that the change was merged elsewhere (its pull request), and
/// remove its worktree and branch.
pub fn merged_elsewhere(change: &mut Change) {
    change.status = ChangeStatus::Adopted;
    remove(change);
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

    /// Commit `text` to `file` on the owner's checkout (the base branch moves on).
    fn owner_commits(repo: &Path, file: &str, text: &str) {
        std::fs::write(repo.join(file), text).unwrap();
        git(repo, &["add", "."]).unwrap();
        git_as_airth(repo, &["commit", "-q", "-m", "owner"]).unwrap();
    }

    #[test]
    fn a_change_is_updated_with_its_base_branch() {
        let repo = repo();
        let wt = tempfile::tempdir().unwrap();
        let mut change = create(repo.path(), wt.path(), "u1").unwrap();
        std::fs::write(change.path.join("b.txt"), "mine\n").unwrap();
        commit(&mut change, "run u1").unwrap();
        assert_eq!(
            update(&mut change, "refs/heads/main").unwrap(),
            Update::UpToDate
        );

        owner_commits(repo.path(), "c.txt", "theirs\n");
        assert!(!contains(&change, "refs/heads/main").unwrap());
        // Adopting a change that misses the base's new commits is refused.
        let err = adopt(&mut change, "adopt").unwrap_err().to_string();
        assert!(err.contains("update the chat first"), "{err}");

        assert_eq!(
            update(&mut change, "refs/heads/main").unwrap(),
            Update::Updated
        );
        assert!(change.path.join("c.txt").exists());
        // The change still shows only what the chat did.
        assert_eq!(change.files, vec!["A\tb.txt"]);
        assert!(contains(&change, "refs/heads/main").unwrap());
        adopt(&mut change, "adopt").unwrap();
        assert_eq!(change.status, ChangeStatus::Adopted);
    }

    #[test]
    fn a_conflict_is_recorded_then_resolved_by_the_chat() {
        let repo = repo();
        let wt = tempfile::tempdir().unwrap();
        let mut change = create(repo.path(), wt.path(), "u2").unwrap();
        std::fs::write(change.path.join("a.txt"), "mine\n").unwrap();
        commit(&mut change, "run u2").unwrap();
        let head = change.head.clone();
        owner_commits(repo.path(), "a.txt", "theirs\n");

        let target = "refs/heads/main";
        assert_eq!(
            update(&mut change, target).unwrap(),
            Update::Conflict(vec!["a.txt".into()])
        );
        // Nothing changed: no merge left behind.
        assert_eq!(change.head, head);
        assert!(!merge_in_progress(&change.path));

        assert_eq!(start_resolving(&mut change, target).unwrap(), vec!["a.txt"]);
        assert!(merge_in_progress(&change.path));
        assert!(update(&mut change, target).is_err());
        // Markers left: not committed.
        assert!(commit(&mut change, "x").is_err());
        assert!(merge_in_progress(&change.path));

        std::fs::write(change.path.join("a.txt"), "mine and theirs\n").unwrap();
        assert!(commit(&mut change, "x").unwrap());
        assert!(!merge_in_progress(&change.path));
        assert!(change.conflicts.is_empty() && change.merging.is_none());
        assert!(contains(&change, target).unwrap());
        assert_eq!(change.files, vec!["M\ta.txt"]);
        assert!(diff(&change).unwrap().contains("+mine and theirs"));
    }

    #[test]
    fn owner_edits_to_the_same_files_block_adoption() {
        let repo = repo();
        let wt = tempfile::tempdir().unwrap();
        git(repo.path(), &["config", "user.name", "t"]).unwrap();
        git(repo.path(), &["config", "user.email", "t@t"]).unwrap();
        let mut change = create(repo.path(), wt.path(), "u3").unwrap();
        std::fs::write(change.path.join("a.txt"), "mine\n").unwrap();
        commit(&mut change, "run u3").unwrap();
        std::fs::write(repo.path().join("a.txt"), "unsaved\n").unwrap();
        assert_eq!(blocking_edits(&change).unwrap(), vec!["a.txt"]);
        let err = adopt(&mut change, "adopt").unwrap_err().to_string();
        assert!(err.contains("a.txt"), "{err}");
        assert_eq!(change.status, ChangeStatus::Open);
        git(repo.path(), &["checkout", "--", "a.txt"]).unwrap();
        adopt(&mut change, "adopt").unwrap();
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
