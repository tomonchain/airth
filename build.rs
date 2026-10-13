//! Records which commit and repository the binary is built from, so a running
//! Airth can tell when its source has a newer version (see src/update.rs).

use std::path::PathBuf;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    if let Some(commit) = git(&["rev-parse", "HEAD"]) {
        println!("cargo:rustc-env=AIRTH_COMMIT={commit}");
    }
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some();
    println!(
        "cargo:rustc-env=AIRTH_DIRTY={}",
        if dirty { "1" } else { "" }
    );
    // The main checkout, also when building from a worktree (as updates do).
    if let Some(common) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        let common = PathBuf::from(common);
        if let Some(repo) = common.parent() {
            println!("cargo:rustc-env=AIRTH_SOURCE={}", repo.display());
        }
    }
    for path in ["HEAD", "index"] {
        if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(p) = git(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            &head_ref,
        ])
    {
        println!("cargo:rerun-if-changed={p}");
    }
}
