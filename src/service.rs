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

/// `~/Applications/Airth.app`: opens the app from Spotlight, Launchpad or the Dock.
fn launcher_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Applications/Airth.app"))
}

/// Build `~/Applications/Airth.app` around `exe`, ad-hoc signed. A release
/// app is already a signed bundle: it is used where it is (rebuilding it
/// would break its signature).
fn install_app(exe: &std::path::Path) -> Result<PathBuf> {
    if crate::update::from_releases()
        && let Some(app) = bundle_of(exe)
    {
        return Ok(app);
    }
    let app = launcher_path()?;
    // Keep the binary if we are it (reinstalling from the app's own binary).
    let staged = std::env::temp_dir().join(format!("airth-{}", std::process::id()));
    std::fs::copy(exe, &staged)?;
    let _ = std::fs::remove_dir_all(&app);
    let result = write_bundle(&app, &staged, DEV_IDENTIFIER);
    let _ = std::fs::remove_file(&staged);
    result?;
    sign(&app);
    Ok(app)
}

/// The Airth.app `exe` sits in, if any.
fn bundle_of(exe: &std::path::Path) -> Option<PathBuf> {
    let path = exe.to_string_lossy();
    path.find(".app/Contents/MacOS/")
        .map(|i| PathBuf::from(&path[..i + 4]))
}

/// The bundle id of an Airth.app built on this Mac (`airth service install`).
const DEV_IDENTIFIER: &str = "local.airth.app";
/// The bundle id of the published app.
pub const RELEASE_IDENTIFIER: &str = "io.github.tomonchain.airth";

/// Write Airth.app at `app` around a copy of `exe` (unsigned): Info.plist
/// (with the reasons macOS shows when Airth asks for the microphone,
/// dictation or to control other apps), the icon, the binary.
pub fn write_bundle(app: &std::path::Path, exe: &std::path::Path, identifier: &str) -> Result<()> {
    let contents = app.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS"))?;
    std::fs::create_dir_all(contents.join("Resources"))?;
    std::fs::copy(exe, contents.join("MacOS/airth"))?;
    std::fs::write(
        contents.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Airth</string>
  <key>CFBundleDisplayName</key><string>Airth</string>
  <key>CFBundleIdentifier</key><string>{identifier}</string>
  <key>CFBundleExecutable</key><string>airth</string>
  <key>CFBundleIconFile</key><string>Airth</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSMicrophoneUsageDescription</key><string>Airth uses the microphone when you dictate a request.</string>
  <key>NSSpeechRecognitionUsageDescription</key><string>Airth turns your dictation into text.</string>
  <key>NSAppleEventsUsageDescription</key><string>Airth works with your apps (Calendar, Reminders, Notes, Mail, Safari) when you approve it.</string>
</dict>
</plist>
"#,
            identifier = escape(identifier),
            version = env!("CARGO_PKG_VERSION")
        ),
    )?;
    write_icon(&contents.join("Resources/Airth.icns"))
}

/// Ad-hoc sign the bundle, so macOS keeps track of its permissions.
pub fn sign(app: &std::path::Path) {
    let _ = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(app)
        .output();
}

fn write_icon(icns: &std::path::Path) -> Result<()> {
    let set = std::env::temp_dir().join(format!("airth-{}.iconset", std::process::id()));
    std::fs::create_dir_all(&set)?;
    for (name, size) in [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ] {
        let px = crate::icon::draw(size, true);
        std::fs::write(set.join(name), crate::icon::png(&px, size))?;
    }
    let out = Command::new("/usr/bin/iconutil")
        .args(["-c", "icns", "-o"])
        .arg(icns)
        .arg(&set)
        .output();
    let _ = std::fs::remove_dir_all(&set);
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => bail!(
            "iconutil failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => bail!("cannot run iconutil: {e}"),
    }
}

/// Restart the login service if it is loaded (to pick up a new binary).
/// True if it was.
pub fn restart_if_running() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let target = format!("{}/{LABEL}", domain());
    let loaded = Command::new("launchctl")
        .args(["print", &target])
        .output()
        .is_ok_and(|o| o.status.success());
    if loaded {
        let _ = Command::new("launchctl")
            .args(["kickstart", "-k", &target])
            .output();
        eprintln!("restarted the Airth service");
    }
    loaded
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
  <key>ProgramArguments</key><array><string>{exe}</string><string>serve</string><string>--wait</string></array>
  <key>EnvironmentVariables</key><dict>
    <key>AIRTH_HOME</key><string>{home}</string>
    <key>AIRTH_SERVICE</key><string>1</string>
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
            let current = std::env::current_exe()?.canonicalize()?;
            let app = install_app(&current)?;
            // The binary inside the app bundle: the one the app, the service
            // and (via a symlink) the `airth` command all run, so an update
            // replaces all three.
            let exe = app.join("Contents/MacOS/airth");
            // The `airth` command becomes a link to the app's binary, so the
            // command, the app and the service are always the same version.
            let invoked = std::env::current_exe()?;
            let cargo_bin = std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".cargo/bin/airth"))
                .unwrap_or_default();
            if invoked == cargo_bin && invoked != exe {
                let _ = std::fs::remove_file(&invoked);
                std::os::unix::fs::symlink(&exe, &invoked)?;
            }
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
            // `bootout` returns before the old service is gone: wait for it,
            // and retry while launchd still holds the label.
            let target = format!("{}/{LABEL}", domain());
            let loaded = || {
                Command::new("launchctl")
                    .args(["print", &target])
                    .output()
                    .is_ok_and(|o| o.status.success())
            };
            for _ in 0..50 {
                if !loaded() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let mut out = Command::new("launchctl")
                .args(["bootstrap", &domain()])
                .arg(&plist)
                .output()?;
            for _ in 0..5 {
                if out.status.success() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                out = Command::new("launchctl")
                    .args(["bootstrap", &domain()])
                    .arg(&plist)
                    .output()?;
            }
            if !out.status.success() {
                bail!(
                    "launchctl bootstrap failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            eprintln!("\x1b[32m✓\x1b[0m Airth runs in the background and starts at login");
            eprintln!("  app    {} (Spotlight: \"Airth\")", app.display());
            eprintln!("  binary {}", exe.display());
            eprintln!("  logs   {}", logs.join("serve.log").display());
            eprintln!("  open the app with `airth ui`");
            // The service does not inherit the shell: model settings exported
            // there (keys, AIRTH_PROVIDER, AIRTH_MODEL...) must be saved for it.
            let shared = ctx.home().provider_settings()?;
            let unseen: Vec<&str> = crate::realm::PROVIDER_KEYS
                .iter()
                .copied()
                .filter(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
                .filter(|k| !shared.iter().any(|(s, _)| s == k))
                .collect();
            if let Some(first) = unseen.first() {
                eprintln!(
                    "\x1b[33m!\x1b[0m the service does not see your shell's {}. Save each for it: airth secret set {first} --all-realms",
                    unseen.join(", ")
                );
            }
            eprintln!(
                "  updates: {}",
                if crate::update::from_releases() {
                    "the app offers each new release (or `airth update`)"
                } else {
                    "the app offers them when your Airth checkout has new commits (or `airth update`)"
                }
            );
        }
        ServiceCmd::Uninstall => {
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{}/{LABEL}", domain())])
                .output();
            // The app this built; a release app is the owner's to remove.
            if !crate::update::from_releases() {
                let _ = std::fs::remove_dir_all(launcher_path()?);
            }
            if plist.exists() {
                std::fs::remove_file(&plist)?;
                eprintln!("removed the Airth service and app");
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
