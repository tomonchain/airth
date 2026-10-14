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
//!
//! A release build (the signed app published on GitHub, built with
//! `AIRTH_CHANNEL=release`) updates from published releases instead: it reads
//! a small manifest (`latest.json`: version, notes, archive URL, SHA-256 and
//! an Ed25519 signature), downloads the archive, checks its hash and the
//! signature against the public key built into Airth, and swaps the whole
//! app bundle, keeping the previous one for `airth update --rollback`.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

pub const COMMIT: Option<&str> = option_env!("AIRTH_COMMIT");
const SOURCE: Option<&str> = option_env!("AIRTH_SOURCE");
const DIRTY: Option<&str> = option_env!("AIRTH_DIRTY");

/// Where a release build finds the newest release.
const MANIFEST_URL: &str = match option_env!("AIRTH_UPDATE_URL") {
    Some(url) => url,
    None => "https://github.com/tomonchain/airth/releases/latest/download/latest.json",
};

/// The Ed25519 public key release archives are signed with (hex). Its
/// private half is the `AIRTH_UPDATE_KEY` secret of the release workflow.
pub const UPDATE_KEY: &str = "32b71dd06c785d6300c7ccad7562ab3c4b881a98c71c29afef71c80b61db809f";

/// Built for release (`AIRTH_CHANNEL=release`): updates come from published
/// releases, not from a git checkout.
pub fn from_releases() -> bool {
    option_env!("AIRTH_CHANNEL") == Some("release")
}

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
    /// A signed release (verified on download), not a commit to build.
    pub release: bool,
    #[serde(skip)]
    manifest: Option<Manifest>,
}

/// `latest.json`, published with each release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: Vec<String>,
    /// The app archive (a zip of Airth.app).
    pub url: String,
    pub sha256: String,
    /// Ed25519 over `signed_message(version, sha256)`, hex.
    pub signature: String,
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

/// Whether a newer version exists: in published releases for a release
/// build, otherwise in `source`. `fetch` also asks the network.
pub fn check(fetch: bool) -> Result<Option<Available>> {
    if from_releases() {
        check_releases(fetch)
    } else {
        check_source(fetch)
    }
}

fn check_source(fetch: bool) -> Result<Option<Available>> {
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
        release: false,
        manifest: None,
    }))
}

/// Get `available` ready to install in `work` (in Airth's home), reporting
/// each step: a release is downloaded and verified, a commit is built and
/// tested. Returns what `install` takes.
pub fn prepare(available: &Available, work: &Path, step: &dyn Fn(&str)) -> Result<PathBuf> {
    match &available.manifest {
        Some(manifest) => prepare_release(manifest, work, step),
        None => prepare_source(available, work, step),
    }
}

fn prepare_source(available: &Available, work: &Path, step: &dyn Fn(&str)) -> Result<PathBuf> {
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
/// `airth.previous` (a staged app bundle replaces the whole running bundle).
/// Returns the installed binary.
pub fn install(staged: &Path) -> Result<PathBuf> {
    if staged.extension().is_some_and(|e| e == "app") {
        return install_bundle(staged);
    }
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

/// Put back the binary (or, for a release, the app) the last update replaced.
pub fn rollback() -> Result<PathBuf> {
    if from_releases() {
        return rollback_bundle();
    }
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

// ---- Releases ----

/// What the release signature covers: the version and the archive's hash,
/// so an old (signed) archive cannot be passed off as a newer version.
pub fn signed_message(version: &str, sha256: &str) -> String {
    format!("airth-release {version} {sha256}")
}

/// Check a release signature against the public key built into Airth.
pub fn verify_signature(version: &str, sha256: &str, signature: &str) -> Result<()> {
    verify_with(UPDATE_KEY, version, sha256, signature)
}

fn verify_with(public_key: &str, version: &str, sha256: &str, signature: &str) -> Result<()> {
    use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
    let key = from_hex(public_key).context("the built-in update key is invalid")?;
    let sig = from_hex(signature).context("the release signature is not valid hex")?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(signed_message(version, sha256).as_bytes(), &sig)
        .map_err(|_| anyhow::anyhow!("the release signature does not match Airth's update key"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, bytes).as_ref())
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// "1.10.0" > "1.9.2"; anything after a "-" (pre-release) is ignored.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> Option<Vec<u64>> {
        let v = v.trim().trim_start_matches('v');
        v.split('-')
            .next()?
            .split('.')
            .map(|p| p.parse().ok())
            .collect()
    }
    match (parts(candidate), parts(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn http() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(600))
        .build()?)
}

/// The last answer from the release manifest: asked only when `fetch`.
static LAST_RELEASE: Mutex<Option<Option<Available>>> = Mutex::new(None);

fn check_releases(fetch: bool) -> Result<Option<Available>> {
    if !fetch && let Some(last) = LAST_RELEASE.lock().unwrap().clone() {
        return Ok(last);
    }
    let url = std::env::var("AIRTH_UPDATE_URL").unwrap_or_else(|_| MANIFEST_URL.to_string());
    let resp = http()?
        .get(&url)
        .send()
        .context("cannot reach the release server")?;
    let found = if resp.status() == reqwest::StatusCode::NOT_FOUND {
        None // nothing published yet
    } else {
        let manifest: Manifest = resp
            .error_for_status()
            .context("the release server refused")?
            .json()
            .context("the release manifest is not valid")?;
        release_available(manifest, env!("CARGO_PKG_VERSION"))
    };
    *LAST_RELEASE.lock().unwrap() = Some(found.clone());
    Ok(found)
}

fn release_available(manifest: Manifest, current: &str) -> Option<Available> {
    if !is_newer(&manifest.version, current) {
        return None;
    }
    Some(Available {
        commit: manifest.version.clone(),
        short: format!("v{}", manifest.version),
        from: "releases".into(),
        count: manifest.notes.len(),
        changes: manifest.notes.iter().take(20).cloned().collect(),
        release: true,
        manifest: Some(manifest),
    })
}

/// Download the release archive, check it, unpack it: the new Airth.app.
fn prepare_release(manifest: &Manifest, work: &Path, step: &dyn Fn(&str)) -> Result<PathBuf> {
    const MAX: u64 = 400 << 20;
    // Check the signature first: it says the manifest is genuine.
    verify_signature(&manifest.version, &manifest.sha256, &manifest.signature)?;
    // (Plain http only from this Mac, to try a release before publishing it.)
    if !manifest.url.starts_with("https://") && !manifest.url.starts_with("http://127.0.0.1:") {
        bail!("the release archive is not served over https");
    }
    std::fs::create_dir_all(work)?;
    step("downloading");
    let resp = http()?
        .get(&manifest.url)
        .send()
        .context("cannot download the release")?
        .error_for_status()
        .context("the release download failed")?;
    if resp.content_length().is_some_and(|n| n > MAX) {
        bail!("the release archive is too large");
    }
    let bytes = resp.bytes().context("the release download was cut off")?;
    step("verifying");
    if !sha256_hex(&bytes).eq_ignore_ascii_case(manifest.sha256.trim()) {
        bail!("the downloaded archive does not match the release (SHA-256 differs)");
    }
    let zip = work.join(format!("Airth-{}.zip", manifest.version));
    std::fs::write(&zip, &bytes)?;
    step("unpacking");
    let out = work.join(format!("Airth-{}", manifest.version));
    let _ = std::fs::remove_dir_all(&out);
    let unpacked = Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&zip)
        .arg(&out)
        .output()
        .context("cannot run ditto")?;
    let _ = std::fs::remove_file(&zip);
    if !unpacked.status.success() {
        bail!(
            "cannot unpack the release: {}",
            String::from_utf8_lossy(&unpacked.stderr).trim()
        );
    }
    let app = out.join("Airth.app");
    if !app.join("Contents/MacOS/airth").is_file() {
        bail!("the release archive does not hold Airth.app");
    }
    let signed = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&app)
        .output()
        .context("cannot run codesign")?;
    if !signed.status.success() {
        bail!(
            "the new app's code signature is broken: {}",
            String::from_utf8_lossy(&signed.stderr).trim()
        );
    }
    Ok(app)
}

/// The app bundle the running binary belongs to.
fn current_bundle() -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let path = exe.to_string_lossy();
    match path.find(".app/Contents/MacOS/") {
        Some(i) => Ok(PathBuf::from(&path[..i + 4])),
        None => bail!("this Airth does not run from Airth.app, so it cannot replace it"),
    }
}

/// The replaced app, kept next to it (hidden).
fn previous_bundle(app: &Path) -> PathBuf {
    app.with_file_name(".Airth-previous.app")
}

/// Swap `new` into `app`'s place, keeping what was there as `keep`. A failed
/// swap puts things back.
fn swap_bundle(app: &Path, new: &Path, keep: &Path) -> Result<()> {
    let next = app.with_file_name(".Airth-next.app");
    let _ = std::fs::remove_dir_all(&next);
    if std::fs::rename(new, &next).is_err() {
        // Another volume: copy, keeping signatures and attributes.
        let out = Command::new("/usr/bin/ditto")
            .arg(new)
            .arg(&next)
            .output()?;
        if !out.status.success() {
            bail!(
                "cannot copy the new app next to {}: {}",
                app.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
    let _ = std::fs::remove_dir_all(keep);
    std::fs::rename(app, keep)
        .with_context(|| format!("cannot replace {} (is it writable?)", app.display()))?;
    if let Err(e) = std::fs::rename(&next, app) {
        let _ = std::fs::rename(keep, app);
        return Err(e).context("cannot put the new app in place");
    }
    Ok(())
}

fn install_bundle(staged: &Path) -> Result<PathBuf> {
    let app = current_bundle()?;
    swap_bundle(&app, staged, &previous_bundle(&app))?;
    Ok(app.join("Contents/MacOS/airth"))
}

fn rollback_bundle() -> Result<PathBuf> {
    let app = current_bundle()?;
    let previous = previous_bundle(&app);
    if !previous.exists() {
        bail!("there is no previous version to go back to");
    }
    let replaced = app.with_file_name(".Airth-replaced.app");
    swap_bundle(&app, &previous, &replaced)?;
    let _ = std::fs::remove_dir_all(&replaced);
    Ok(app.join("Contents/MacOS/airth"))
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

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v1.10.0", "1.9.2"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.2.0-beta", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 127, 128, 255];
        assert_eq!(from_hex(&to_hex(&bytes)).unwrap(), bytes);
        assert!(from_hex("abc").is_none());
        assert!(from_hex("zz").is_none());
    }

    #[test]
    fn a_release_is_offered_only_when_newer() {
        let manifest = |v: &str| Manifest {
            version: v.into(),
            notes: vec!["Faster replies".into()],
            url: "https://example.com/Airth.zip".into(),
            sha256: "00".into(),
            signature: "00".into(),
        };
        assert!(release_available(manifest("0.1.0"), "0.1.0").is_none());
        let a = release_available(manifest("0.2.0"), "0.1.0").unwrap();
        assert_eq!(a.short, "v0.2.0");
        assert_eq!(a.changes, ["Faster replies"]);
        assert!(a.release);
    }

    #[test]
    fn a_signature_covers_version_and_archive() {
        use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair};
        let rng = aws_lc_rs::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public = to_hex(pair.public_key().as_ref());
        let sha = sha256_hex(b"the app");
        let sig = to_hex(pair.sign(signed_message("0.2.0", &sha).as_bytes()).as_ref());
        assert!(verify_with(&public, "0.2.0", &sha, &sig).is_ok());
        // Another version, another archive, another key: refused.
        assert!(verify_with(&public, "0.3.0", &sha, &sig).is_err());
        assert!(verify_with(&public, "0.2.0", &sha256_hex(b"other"), &sig).is_err());
        let other =
            Ed25519KeyPair::from_pkcs8(Ed25519KeyPair::generate_pkcs8(&rng).unwrap().as_ref())
                .unwrap();
        assert!(verify_with(&to_hex(other.public_key().as_ref()), "0.2.0", &sha, &sig).is_err());
    }

    #[test]
    fn swapping_a_bundle_keeps_the_previous_one() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Airth.app");
        let new = dir.path().join("staged/Airth.app");
        let keep = previous_bundle(&app);
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("v"), "1").unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("v"), "2").unwrap();
        swap_bundle(&app, &new, &keep).unwrap();
        assert_eq!(std::fs::read_to_string(app.join("v")).unwrap(), "2");
        assert_eq!(std::fs::read_to_string(keep.join("v")).unwrap(), "1");
        assert!(!new.exists());
    }
}
