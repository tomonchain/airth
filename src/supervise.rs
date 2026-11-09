//! Keeping the service's background loops alive (decision 108). The
//! scheduler, the Telegram bot, the updater, the pull-request follower and
//! the backup each run in a thread of their own while the app answers. A
//! panic in one of them used to end that thread for good with the app still
//! answering, so nothing ran and nothing said so. Now the loop starts again
//! after a pause, and the founder is told.

use crate::notify::{Inbox, Kind};
use std::collections::HashMap;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long a loop waits before starting again after a panic.
const PAUSE: Duration = Duration::from_secs(60);
/// The founder hears of a loop's panics at most this often.
const TELL_EVERY: Duration = Duration::from_secs(3600);

/// Run `body` (a loop that never returns) and start it again whenever it
/// panics or returns, for as long as the process lives.
pub fn supervise(name: &'static str, inbox: Inbox, body: impl Fn()) {
    let mut told: Option<Instant> = None;
    loop {
        let said = match run_once(&body) {
            Some(panic) => format!("it stopped on an error: {panic}"),
            None => "it stopped".to_string(),
        };
        eprintln!("{name}: {said}; starting it again in {} s", PAUSE.as_secs());
        if told.is_none_or(|at| at.elapsed() >= TELL_EVERY) {
            let _ = inbox.push(
                Kind::Failed,
                "airth",
                None,
                &format!("Airth's {name} stopped, and started again"),
                &format!("{said}. Airth started it again; if this comes back, the log says more (serve.log)."),
            );
            told = Some(Instant::now());
        }
        std::thread::sleep(PAUSE);
    }
}

/// Run `body` once; what it panicked with, if it did.
fn run_once(body: &impl Fn()) -> Option<String> {
    catch_unwind(AssertUnwindSafe(body)).err().map(|p| {
        p.downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".to_string())
    })
}

/// The last error said for each key, so an error that comes back every tick
/// is said once, then again only when it changes.
static SAID: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Whether `error` under `key` is news: a different error from the last one
/// said, or the first. `None` (it worked) forgets the key.
fn news(seen: &mut HashMap<String, String>, key: &str, error: Option<&str>) -> bool {
    match error {
        None => {
            seen.remove(key);
            false
        }
        Some(e) if seen.get(key).map(String::as_str) == Some(e) => false,
        Some(e) => {
            seen.insert(key.to_string(), e.to_string());
            true
        }
    }
}

/// One business's share of a loop's work went wrong: say so once, in the log
/// and to the founder, and let the loop go on with the other businesses.
/// `key` names the work and the business; `what` is said to the founder.
pub fn report(inbox: &Inbox, key: &str, realm: &str, what: &str, outcome: anyhow::Result<()>) {
    let error = outcome.err().map(|e| format!("{e:#}"));
    if !news(
        SAID.locked().get_or_insert_with(HashMap::new),
        key,
        error.as_deref(),
    ) {
        return;
    }
    let error = error.unwrap_or_default();
    eprintln!("{key}: {error}");
    let _ = inbox.push(
        Kind::Failed,
        realm,
        None,
        &format!("{what} stopped for this business"),
        &format!("{error}\nThe other businesses go on. Airth tries again every few seconds and says nothing more until the error changes or clears."),
    );
}

/// Run a command and wait for it at most `limit`, then kill it: a `git
/// fetch` on a stalled connection, or a test that hangs, would otherwise
/// hold a loop for good (decision 108). Its output is read as it comes, so a
/// chatty command cannot block on a full pipe.
pub fn output_within(cmd: &mut Command, limit: Duration) -> std::io::Result<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut out);
            }
            out
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("stopped after {} s without finishing", limit.as_secs()),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Take a lock even if a thread panicked while holding it: what it guards
/// is plain data each holder leaves whole between writes, and a poisoned
/// lock would otherwise make every later holder panic in turn, so one
/// panic would stop every loop that shares it.
pub trait Locked<T> {
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> Locked<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_caught_and_said() {
        let said = run_once(&|| panic!("index out of bounds"));
        assert_eq!(said.as_deref(), Some("index out of bounds"));
        let n = 3;
        let said = run_once(&|| panic!("{n} items"));
        assert_eq!(said.as_deref(), Some("3 items"));
        assert_eq!(run_once(&|| {}), None);
    }

    #[test]
    fn an_error_is_said_once_until_it_changes_or_clears() {
        let mut seen = HashMap::new();
        assert!(news(&mut seen, "routines/acme", Some("bad mcp.json")));
        assert!(!news(&mut seen, "routines/acme", Some("bad mcp.json")));
        assert!(news(&mut seen, "routines/other", Some("bad mcp.json")));
        assert!(news(&mut seen, "routines/acme", Some("no such folder")));
        assert!(!news(&mut seen, "routines/acme", None));
        assert!(news(&mut seen, "routines/acme", Some("no such folder")));
    }

    #[test]
    fn a_command_that_takes_too_long_is_stopped() {
        let started = Instant::now();
        let err =
            output_within(Command::new("sleep").arg("5"), Duration::from_millis(300)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(3));
        let out = output_within(Command::new("echo").arg("hi"), Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
    }

    #[test]
    fn a_lock_poisoned_by_a_panic_can_still_be_taken() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let held = m.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("while holding the lock");
        })
        .join();
        assert!(m.lock().is_err(), "the lock is poisoned");
        *m.locked() += 1;
        assert_eq!(*m.locked(), 2);
    }
}
