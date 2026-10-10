use super::{Tool, Workspace, str_arg, truncate};
use crate::models::ToolSpec;
use crate::permissions::Capability;
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

    fn execute(&self, input: &Value, ws: &Workspace) -> Result<String> {
        let command = str_arg(input, "command")?;
        let timeout = Duration::from_secs(
            input["timeout_secs"]
                .as_u64()
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .min(MAX_TIMEOUT_SECS),
        );
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(ws.root())
            .stdin(Stdio::null())
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
            if start.elapsed() > timeout {
                // Kill before reaping, so the group id is still ours.
                kill_process_group(&mut child);
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let out = out_t.join().unwrap_or_default();
        let err = err_t.join().unwrap_or_default();

        let code = match status {
            Some(s) => s
                .code()
                .map_or("killed by signal".into(), |c| c.to_string()),
            None => format!("timed out after {}s", timeout.as_secs()),
        };
        Ok(format!(
            "exit code: {code}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            truncate(out, MAX_OUTPUT),
            truncate(err, MAX_OUTPUT)
        ))
    }
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

    #[test]
    fn runs_in_workspace_and_reports_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        std::fs::write(dir.path().join("marker"), "").unwrap();
        let out = ExecuteCommand
            .execute(&json!({"command": "ls; echo oops >&2; exit 3"}), &ws)
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
            .execute(&json!({"command": "sleep 5", "timeout_secs": 0}), &ws)
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
    fn background_processes_do_not_block_the_result() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let start = Instant::now();
        let out = ExecuteCommand
            .execute(&json!({"command": "sleep 10 & echo started"}), &ws)
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
