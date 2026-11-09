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
    sign_checked(&app)?;
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

/// Ad-hoc sign the bundle, so macOS keeps track of its permissions, then
/// check the signature holds: macOS kills an app whose signature does not
/// match the moment it starts. Elsewhere there is nothing to sign.
pub fn sign_checked(app: &std::path::Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    for args in [
        &["--force", "--deep", "--sign", "-"][..],
        &["--verify", "--deep", "--strict"][..],
    ] {
        let out = Command::new("/usr/bin/codesign")
            .args(args)
            .arg(app)
            .output()
            .context("cannot run codesign")?;
        if !out.status.success() {
            bail!(
                "codesign {} {} failed: {}",
                args[0],
                app.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
    Ok(())
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
    if cfg!(target_os = "linux") {
        return systemd::restart_if_running();
    }
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
    if cfg!(target_os = "linux") {
        return systemd::run(ctx, command);
    }
    if !cfg!(target_os = "macos") {
        bail!(
            "`airth service` supports macOS and Linux only; elsewhere, run `airth serve` under your init system"
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

/// Above this size, the service's log is set aside and started again.
const LOG_LIMIT: u64 = 20 * 1024 * 1024;

/// Keep the service's log from filling the disk (decision 108): above
/// `limit`, its content moves to `serve.log.1` (one generation kept) and the
/// log starts again empty. launchd and systemd keep the log open to append,
/// so emptying it in place is safe: their next line goes at its new end.
pub fn trim_log(log: &std::path::Path, limit: u64) -> Result<bool> {
    let size = match std::fs::metadata(log) {
        Ok(m) => m.len(),
        Err(_) => return Ok(false),
    };
    if size <= limit {
        return Ok(false);
    }
    std::fs::copy(log, log.with_extension("log.1"))?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(log)?
        .set_len(0)?;
    Ok(true)
}

/// The service's housekeeping: the log, at start and every hour.
pub fn keep_log(home: &std::path::Path) {
    let log = home.join("logs/serve.log");
    loop {
        match trim_log(&log, LOG_LIMIT) {
            Ok(true) => eprintln!("log: the previous lines are in serve.log.1"),
            Ok(false) => {}
            Err(e) => eprintln!("log: could not set the log aside: {e:#}"),
        }
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

/// Linux (decision 106): a systemd user service, started at boot and again
/// whenever it stops, as launchd keeps the Mac's alive.
mod systemd {
    use super::*;

    const UNIT: &str = "airth.service";

    fn unit_path() -> Result<PathBuf> {
        let config = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => {
                PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".config")
            }
        };
        Ok(config.join("systemd/user").join(UNIT))
    }

    /// A value in a unit file: quoted, with systemd's `%` specifiers kept
    /// literal.
    fn quote(s: &str) -> String {
        format!(
            "\"{}\"",
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('%', "%%")
        )
    }

    pub(super) fn unit_text(exe: &str, home: &str, path: &str, log: &str) -> String {
        format!(
            "[Unit]\n\
             Description=Airth: the businesses' agents, their routines and the app\n\
             After=network-online.target\n\
             Wants=network-online.target\n\
             \n\
             [Service]\n\
             ExecStart={} serve --wait\n\
             Environment={}\n\
             Environment={}\n\
             Environment={}\n\
             Restart=always\n\
             RestartSec=5\n\
             StandardOutput=append:{log}\n\
             StandardError=append:{log}\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            quote(exe),
            quote(&format!("AIRTH_HOME={home}")),
            quote("AIRTH_SERVICE=1"),
            quote(&format!("PATH={path}")),
            log = log.replace('%', "%%"),
        )
    }

    fn systemctl(args: &[&str]) -> Result<std::process::Output> {
        Command::new("systemctl")
            .arg("--user")
            .args(args)
            .output()
            .context("cannot run systemctl (is this a systemd system?)")
    }

    fn active() -> bool {
        systemctl(&["is-active", "--quiet", UNIT]).is_ok_and(|o| o.status.success())
    }

    pub(super) fn restart_if_running() -> bool {
        if !active() {
            return false;
        }
        let _ = systemctl(&["restart", UNIT]);
        eprintln!("restarted the Airth service");
        true
    }

    pub(super) fn run(ctx: &Ctx, command: ServiceCmd) -> Result<bool> {
        let unit = unit_path()?;
        match command {
            ServiceCmd::Install => {
                let exe = std::env::current_exe()?.canonicalize()?;
                let home = ctx.home().dir();
                let logs = home.join("logs");
                std::fs::create_dir_all(&logs)?;
                let log = logs.join("serve.log");
                let text = unit_text(
                    &exe.display().to_string(),
                    &home.display().to_string(),
                    &std::env::var("PATH").unwrap_or_default(),
                    &log.display().to_string(),
                );
                if let Some(dir) = unit.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&unit, text)?;
                for args in [
                    &["daemon-reload"][..],
                    &["enable", UNIT],
                    &["restart", UNIT],
                ] {
                    let out = systemctl(args)?;
                    if !out.status.success() {
                        bail!(
                            "systemctl --user {} failed: {}",
                            args.join(" "),
                            String::from_utf8_lossy(&out.stderr).trim()
                        );
                    }
                }
                eprintln!(
                    "\x1b[32m✓\x1b[0m Airth runs in the background, and again after a stop or a reboot"
                );
                eprintln!("  binary {}", exe.display());
                eprintln!("  logs   {}", log.display());
                // A user's services stop when they log out, unless systemd
                // keeps their manager running ("lingering").
                let user = std::env::var("USER").unwrap_or_default();
                let lingers = Command::new("loginctl")
                    .args(["show-user", &user, "--property=Linger"])
                    .output()
                    .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("Linger=yes"));
                let enabled = lingers
                    || Command::new("loginctl")
                        .args(["enable-linger", &user])
                        .output()
                        .is_ok_and(|o| o.status.success());
                if !enabled {
                    eprintln!(
                        "\x1b[33m!\x1b[0m to keep Airth running when you log out and after a reboot, run once: sudo loginctl enable-linger {user}"
                    );
                }
            }
            ServiceCmd::Uninstall => {
                let _ = systemctl(&["disable", "--now", UNIT]);
                if unit.exists() {
                    std::fs::remove_file(&unit)?;
                    let _ = systemctl(&["daemon-reload"]);
                    eprintln!("removed the Airth service");
                } else {
                    eprintln!("the Airth service was not installed");
                }
            }
            ServiceCmd::Status => {
                println!(
                    "service  {}",
                    match (unit.exists(), active()) {
                        (true, true) => "installed, running",
                        (true, false) => "installed, not running",
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_over_its_limit_is_set_aside_and_started_again() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("serve.log");
        std::fs::write(&log, "a".repeat(100)).unwrap();
        assert!(!trim_log(&log, 1000).unwrap());
        // Kept open to append, as launchd and systemd keep it.
        let mut held = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        assert!(trim_log(&log, 50).unwrap());
        assert_eq!(
            std::fs::read_to_string(log.with_extension("log.1"))
                .unwrap()
                .len(),
            100
        );
        use std::io::Write;
        writeln!(held, "next").unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "next\n");
        assert!(!trim_log(&dir.path().join("none.log"), 50).unwrap());
    }

    #[test]
    fn the_systemd_unit_quotes_paths_and_restarts_airth() {
        let text = systemd::unit_text(
            "/home/me/my apps/airth",
            "/home/me/.airth",
            "/usr/bin:/bin",
            "/home/me/.airth/logs/100%.log",
        );
        assert!(text.contains("ExecStart=\"/home/me/my apps/airth\" serve --wait\n"));
        assert!(text.contains("Environment=\"AIRTH_HOME=/home/me/.airth\"\n"));
        assert!(text.contains("Environment=\"AIRTH_SERVICE=1\"\n"));
        assert!(text.contains("Restart=always\n"));
        assert!(text.contains("StandardError=append:/home/me/.airth/logs/100%%.log\n"));
        assert!(text.contains("WantedBy=default.target"));
    }

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
