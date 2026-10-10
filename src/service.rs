//! `airth service`: keep `airth serve` running in the background, started at
//! login, so objectives run while the owner is away. macOS (launchd) only.

use crate::Ctx;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "local.airth.serve";

#[derive(Subcommand)]
pub enum ServiceCmd {
    /// Start Airth at login and keep it running
    Install,
    /// Stop it and remove it from login items
    Uninstall,
    /// Whether it is installed and running
    Status,
}

fn plist_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn domain() -> String {
    // SAFETY: getuid cannot fail.
    format!("gui/{}", unsafe { libc::getuid() })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn plist_text(exe: &str, home: &str, path: &str, log: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{exe}</string><string>serve</string></array>
  <key>EnvironmentVariables</key><dict>
    <key>AIRTH_HOME</key><string>{home}</string>
    <key>PATH</key><string>{path}</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        exe = escape(exe),
        home = escape(home),
        path = escape(path),
        log = escape(log),
    )
}

pub fn run(ctx: &Ctx, command: ServiceCmd) -> Result<bool> {
    if !cfg!(target_os = "macos") {
        bail!(
            "`airth service` supports macOS only; elsewhere, run `airth serve` under your init system (e.g. a systemd user unit)"
        );
    }
    let plist = plist_path()?;
    match command {
        ServiceCmd::Install => {
            let exe = std::env::current_exe()?.canonicalize()?;
            let home = ctx.home().dir();
            let logs = home.join("logs");
            std::fs::create_dir_all(&logs)?;
            let path = std::env::var("PATH").unwrap_or_default();
            let text = plist_text(
                &exe.display().to_string(),
                &home.display().to_string(),
                &path,
                &logs.join("serve.log").display().to_string(),
            );
            if let Some(dir) = plist.parent() {
                std::fs::create_dir_all(dir)?;
            }
            // Replace a previous installation.
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{}/{LABEL}", domain())])
                .output();
            std::fs::write(&plist, text)?;
            let out = Command::new("launchctl")
                .args(["bootstrap", &domain()])
                .arg(&plist)
                .output()?;
            if !out.status.success() {
                bail!(
                    "launchctl bootstrap failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            eprintln!("\x1b[32m✓\x1b[0m Airth runs in the background and starts at login");
            eprintln!("  binary {}", exe.display());
            eprintln!("  logs   {}", logs.join("serve.log").display());
            eprintln!("  open the app with `airth ui`");
            let has_key = std::env::var("ANTHROPIC_API_KEY").is_ok()
                || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();
            let shared = ctx.home().provider_settings()?;
            if has_key
                && !shared
                    .iter()
                    .any(|(k, _)| k == "ANTHROPIC_API_KEY" || k == "ANTHROPIC_AUTH_TOKEN")
            {
                eprintln!(
                    "\x1b[33m!\x1b[0m the service does not see your shell's ANTHROPIC_API_KEY. Save it for it: airth secret set ANTHROPIC_API_KEY --all-realms"
                );
            }
            eprintln!(
                "  after rebuilding Airth, run `airth service install` again so the service uses the new binary"
            );
        }
        ServiceCmd::Uninstall => {
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{}/{LABEL}", domain())])
                .output();
            if plist.exists() {
                std::fs::remove_file(&plist)?;
                eprintln!("removed the Airth service");
            } else {
                eprintln!("the Airth service was not installed");
            }
        }
        ServiceCmd::Status => {
            let loaded = Command::new("launchctl")
                .args(["print", &format!("{}/{LABEL}", domain())])
                .output()
                .is_ok_and(|o| o.status.success());
            println!(
                "service  {}",
                match (plist.exists(), loaded) {
                    (true, true) => "installed, loaded",
                    (true, false) => "installed, not loaded",
                    _ => "not installed",
                }
            );
            match crate::ui::running_url(&ctx.hub()) {
                Some(url) => println!("app      running → {url}"),
                None => println!("app      not running"),
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_is_valid() {
        let text = plist_text(
            "/opt/a&b/airth",
            "/Users/me/.airth",
            "/usr/bin:/bin",
            "/tmp/<x>.log",
        );
        assert!(text.contains("/opt/a&amp;b/airth"));
        if cfg!(target_os = "macos") && std::path::Path::new("/usr/bin/plutil").exists() {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("t.plist");
            std::fs::write(&file, text).unwrap();
            let out = Command::new("/usr/bin/plutil")
                .arg("-lint")
                .arg(&file)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }
}
