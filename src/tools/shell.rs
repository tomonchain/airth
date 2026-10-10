use super::{Tool, Workspace, str_arg, truncate};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use crate::sandbox;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECS: u64 = 300;
const MAX_TIMEOUT_SECS: u64 = 1800;
const MAX_OUTPUT: usize = 30_000;

pub struct ExecuteCommand;

impl Tool for ExecuteCommand {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "execute_command".into(),
            description: "Run a shell command (sh -c) in the workspace root, e.g. `cargo check`, `cargo test`, `git diff`. Returns exit code, stdout and stderr. Requires human approval. No interactive input is available, and background processes do not outlive the command.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The shell command to run"},
                    "timeout_secs": {"type": "integer", "description": "Kill the command after this many seconds (default 300, max 1800)"}
                },
                "required": ["command"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Execute
    }

    fn preview(&self, input: &Value, ws: &Workspace) -> Result<String> {
        Ok(format!(
            "$ {}\n(in {})",
            str_arg(input, "command")?,
            ws.root().display()
        ))
    }

    fn execute(&self, input: &Value, ws: &Workspace, cancel: &CancelToken) -> Result<String> {
        let timeout = Duration::from_secs(
            input["timeout_secs"]
                .as_u64()
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .min(MAX_TIMEOUT_SECS),
        );
        let out = run_shell(str_arg(input, "command")?, ws, cancel, timeout)?;
        if out.stopped {
            anyhow::bail!("stopped by user\n{}", out.streams());
        }
        Ok(format!(
            "exit code: {}\n{}",
            out.code_text(timeout),
            out.streams()
        ))
    }
}

/// Variables passed through from Airth's own environment. Everything else
/// (API keys, tokens the owner exported for other purposes) is withheld;
/// a realm's credentials are added explicitly from its secrets.
const PASSED_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TMPDIR",
    "TZ",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
];

pub struct ShellOutput {
    /// None if the command was killed (timeout or stop).
    pub code: Option<i32>,
    pub signaled: bool,
    pub stopped: bool,
    pub stdout: String,
    pub stderr: String,
}

impl ShellOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    fn code_text(&self, timeout: Duration) -> String {
        match (self.code, self.signaled) {
            (Some(c), _) => c.to_string(),
            (None, true) => "killed by signal".into(),
            (None, false) => format!("timed out after {}s", timeout.as_secs()),
        }
    }

    pub fn streams(&self) -> String {
        format!(
            "--- stdout ---\n{}\n--- stderr ---\n{}",
            truncate(self.stdout.clone(), MAX_OUTPUT),
            truncate(self.stderr.clone(), MAX_OUTPUT)
        )
    }
}

/// Run `sh -c command` in the workspace root, with the workspace's environment
/// and sandbox. Used by `execute_command` and by the runtime's own checks.
pub fn run_shell(
    command: &str,
    ws: &Workspace,
    cancel: &CancelToken,
    timeout: Duration,
) -> Result<ShellOutput> {
    run_shell_with_input(command, ws, cancel, timeout, None)
}

/// `run_shell`, with `input` written to the command's stdin (installed skills
/// read their arguments there).
pub fn run_shell_with_input(
    command: &str,
    ws: &Workspace,
    cancel: &CancelToken,
    timeout: Duration,
    input: Option<&str>,
) -> Result<ShellOutput> {
    let mut cmd = if ws.sandboxed() && sandbox::available() {
        let mut c = Command::new(sandbox::program());
        c.arg("-p").arg(sandbox::profile(ws.rules())).arg("sh");
        c
    } else {
        Command::new("sh")
    };
    cmd.arg("-c")
        .arg(command)
        .current_dir(ws.root())
        .env_clear()
        .envs(
            PASSED_ENV
                .iter()
                .filter_map(|k| Some((k, std::env::var_os(k)?))),
        )
        .envs(ws.env().iter().map(|(k, v)| (k, v)))
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        // Own process group, so the command and everything it spawns can be
        // killed together (see `kill_process_group`).
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().context("failed to spawn sh")?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let text = text.to_string();
        // On a thread: a command that does not read its input must not block us.
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        });
    }

    // Drain pipes on threads so a chatty command cannot deadlock on a full pipe.
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            // Leftover background processes would hold the pipes open and
            // block the readers indefinitely.
            kill_process_group(&mut child);
            break Some(status);
        }
        if start.elapsed() > timeout || cancel.is_cancelled() {
            // Kill before reaping, so the group id is still ours.
            kill_process_group(&mut child);
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(ShellOutput {
        code: status.and_then(|s| s.code()),
        signaled: status.is_some_and(|s| s.code().is_none()),
        stopped: status.is_none() && cancel.is_cancelled(),
        stdout: out_t.join().unwrap_or_default(),
        stderr: err_t.join().unwrap_or_default(),
    })
}

/// Kill the command and every process it spawned. Killing only `sh` would leave
/// its children running with our output pipes open, so a timeout would not end
/// the call.
fn kill_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        // The child leads its own group (`process_group(0)`), so its pid is the
        // group id; a negative pid targets the whole group. Errors (e.g. ESRCH
        // when nothing is left) are expected and ignored.
        // SAFETY: kill(2) has no memory-safety preconditions.
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Access;

    #[test]
    fn runs_in_workspace_and_reports_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        std::fs::write(dir.path().join("marker"), "").unwrap();
        let out = ExecuteCommand
            .execute(
                &json!({"command": "ls; echo oops >&2; exit 3"}),
                &ws,
                &CancelToken::default(),
            )
            .unwrap();
        assert!(out.starts_with("exit code: 3"));
        assert!(out.contains("marker"));
        assert!(out.contains("oops"));
    }

    #[test]
    fn enforces_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let out = ExecuteCommand
            .execute(
                &json!({"command": "sleep 5", "timeout_secs": 0}),
                &ws,
                &CancelToken::default(),
            )
            .unwrap();
        assert!(out.contains("timed out"));
    }

    #[test]
    fn timeout_also_kills_grandchildren() {
        // `sleep` is a child of `sh` here (not exec'd), and inherits the output pipes.
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let start = Instant::now();
        let out = ExecuteCommand
            .execute(
                &json!({"command": "sleep 10; echo finished", "timeout_secs": 1}),
                &ws,
                &CancelToken::default(),
            )
            .unwrap();
        assert!(out.contains("timed out"), "{out}");
        assert!(!out.contains("finished"), "{out}");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn cancel_stops_a_running_command() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let cancel = CancelToken::default();
        let c = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            c.cancel();
        });
        let start = Instant::now();
        let out = ExecuteCommand
            .execute(&json!({"command": "sleep 10"}), &ws, &cancel)
            .unwrap_err()
            .to_string();
        assert!(out.contains("stopped by user"), "{out}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn commands_do_not_inherit_secrets_but_get_the_realm_ones() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: tests in this module do not read this variable concurrently.
        unsafe { std::env::set_var("AIRTH_TEST_LEAK", "leaked") };
        let ws = Workspace::new(dir.path())
            .unwrap()
            .with_secrets(vec![("REALM_TOKEN".into(), "mine".into())], vec![]);
        let out = ExecuteCommand
            .execute(
                &json!({"command": "echo \"[$AIRTH_TEST_LEAK] [$REALM_TOKEN]\""}),
                &ws,
                &CancelToken::default(),
            )
            .unwrap();
        assert!(out.contains("[] [mine]"), "{out}");
    }

    #[test]
    fn sandbox_blocks_denied_paths() {
        if !sandbox::available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("private"), "other realm").unwrap();
        let ws = Workspace::new(dir.path())
            .unwrap()
            .with_rule(other.path(), Access::None)
            .with_sandbox(true);
        let target = other.path().canonicalize().unwrap();
        let cmd = format!(
            "echo ok > mine; cat mine; cat {0}/private; echo x > {0}/new",
            target.display()
        );
        let out = ExecuteCommand
            .execute(&json!({ "command": cmd }), &ws, &CancelToken::default())
            .unwrap();
        assert!(out.contains("ok"), "{out}");
        assert!(!out.contains("other realm"), "{out}");
        assert!(!other.path().join("new").exists());
    }

    #[test]
    fn background_processes_do_not_block_the_result() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let start = Instant::now();
        let out = ExecuteCommand
            .execute(
                &json!({"command": "sleep 10 & echo started"}),
                &ws,
                &CancelToken::default(),
            )
            .unwrap();
        assert!(out.starts_with("exit code: 0"), "{out}");
        assert!(out.contains("started"), "{out}");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }
}
