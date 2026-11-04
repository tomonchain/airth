//! Records which commit and repository the binary is built from, so a running
//! Airth can tell when its source has a newer version (see src/update.rs), and
//! joins the app's scripts and styles into the one app.js and app.css it serves,
//! and the new interface's into next.js and next.css, and embeds the library
//! and the interface built in web/dist.

use std::path::PathBuf;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Join the files of `dir` with this extension, in name order, into `OUT_DIR/name`.
fn bundle(dir: &str, ext: &str, name: &str) {
    println!("cargo:rerun-if-changed={dir}");
    let mut parts: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    parts.sort();
    let mut text = String::new();
    for part in parts {
        text.push_str(&std::fs::read_to_string(&part).unwrap());
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join(name);
    std::fs::write(out, text).unwrap();
}

/// Every file of `library/` (decision 74), as `(path, text)` pairs in
/// `OUT_DIR/library.rs`, so Airth ships its library inside the binary.
fn library() {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    println!("cargo:rerun-if-changed=library");
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("library");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut code = String::from("pub static FILES: &[(&str, &str)] = &[\n");
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        code.push_str(&format!(
            "    ({rel:?}, include_str!({:?})),\n",
            file.display().to_string()
        ));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("library.rs");
    std::fs::write(out, code).unwrap();
}

/// Every file of `web/dist`, the interface built by `npm run build` in web/
/// (decision 73), as `(path, bytes)` pairs in `OUT_DIR/web.rs`. The build is
/// committed, so `cargo build` needs no Node.
fn web() {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| {
            panic!(
                "cannot read {}: {e} (run `npm run build` in web/)",
                dir.display()
            )
        }) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    println!("cargo:rerun-if-changed=web/dist");
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("web/dist");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut code = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        code.push_str(&format!(
            "    ({rel:?}, include_bytes!({:?})),\n",
            file.display().to_string()
        ));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("web.rs");
    std::fs::write(out, code).unwrap();
}

fn main() {
    library();
    web();
    bundle("src/ui/js", "js", "app.js");
    bundle("src/ui/css", "css", "app.css");
    // The new interface, next to the current one (decision 57).
    bundle("src/ui/next/js", "js", "next.js");
    bundle("src/ui/next/css", "css", "next.css");
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
