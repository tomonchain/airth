//! Notifications: what Airth tells the owner about work it did on its own
//! (an objective's run finished, failed, or is waiting for approval).
//!
//! They are kept in the home's `notifications.json`, shown in the app's inbox
//! and by `airth inbox`. On macOS a desktop notification is shown too; it
//! names only the realm and the event, never a run's content, since the
//! screen may be visible to others.

use crate::realm::write_atomic;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());
const KEEP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Completed,
    Failed,
    Stopped,
    /// A run waits for the owner.
    Approval,
    /// A change is ready for review.
    Review,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: u64,
    pub kind: Kind,
    pub realm: String,
    #[serde(default)]
    pub run: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub read: bool,
}

#[derive(Debug, Clone)]
pub struct Inbox {
    path: PathBuf,
    desktop: bool,
}

impl Inbox {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            desktop: false,
        }
    }

    /// Also show each notification on the desktop (macOS).
    pub fn with_desktop(mut self, desktop: bool) -> Self {
        self.desktop = desktop;
        self
    }

    pub fn list(&self) -> Result<Vec<Notification>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn unread(&self) -> usize {
        self.list()
            .map_or(0, |l| l.iter().filter(|n| !n.read).count())
    }

    pub fn push(
        &self,
        kind: Kind,
        realm: &str,
        run: Option<&str>,
        title: &str,
        body: &str,
    ) -> Result<Notification> {
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        let n = Notification {
            id: all.iter().map(|n| n.id).max().unwrap_or(0) + 1,
            kind,
            realm: realm.to_string(),
            run: run.map(String::from),
            title: title.to_string(),
            body: body.chars().take(2000).collect(),
            created_at: Utc::now(),
            read: false,
        };
        all.push(n.clone());
        let excess = all.len().saturating_sub(KEEP);
        all.drain(..excess);
        write_atomic(&self.path, &serde_json::to_string_pretty(&all)?)?;
        drop(_guard);
        if self.desktop {
            desktop(&format!("Airth · {realm}"), title);
        }
        Ok(n)
    }

    /// Mark notifications read: those with these ids, or all of them.
    pub fn mark_read(&self, ids: Option<&[u64]>) -> Result<()> {
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        for n in &mut all {
            if ids.is_none_or(|ids| ids.contains(&n.id)) {
                n.read = true;
            }
        }
        write_atomic(&self.path, &serde_json::to_string_pretty(&all)?)
    }
}

/// Best effort; a missing notifier is not an error.
fn desktop(title: &str, message: &str) {
    if !cfg!(target_os = "macos") {
        return;
    }
    // Text goes in as arguments, never into the script source.
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            title,
            message,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_list_and_mark_read() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(dir.path().join("n.json"));
        inbox
            .push(
                Kind::Completed,
                "work",
                Some("r1"),
                "triage: completed",
                "ok",
            )
            .unwrap();
        let second = inbox
            .push(Kind::Failed, "home", None, "backup: failed", "")
            .unwrap();
        assert_eq!(second.id, 2);
        assert_eq!(inbox.unread(), 2);
        inbox.mark_read(Some(&[1])).unwrap();
        assert_eq!(inbox.unread(), 1);
        inbox.mark_read(None).unwrap();
        assert_eq!(inbox.unread(), 0);
    }
}
