//! Failed actions and their next attempt (decision 55). A failure is
//! recorded on the action; one that may be tried again keeps the time of
//! its next attempt, so a restart of Airth does not lose it. Taking it up
//! puts it in flight again, under the same key, one attempt more.

use super::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;

/// A failed action whose next attempt is due.
#[derive(Debug, Clone)]
pub struct Retry {
    pub id: i64,
    pub key: String,
    pub action: String,
    pub agent: Option<String>,
    pub routine: Option<String>,
    pub item: Option<i64>,
    pub args: String,
    /// Attempts made so far.
    pub attempts: u32,
}

impl Store {
    /// The action under `key` failed: tried again at `retry_at`, or never.
    /// Whatever the journal said before, it failed: the runtime records a
    /// call that got an answer as done, even a 5xx. False when no action
    /// has this key.
    pub fn action_failed(
        &mut self,
        key: &str,
        error: &str,
        retry_at: Option<DateTime<Utc>>,
    ) -> Result<bool> {
        self.write(|tx| {
            let now = Utc::now().to_rfc3339();
            let retry_at = retry_at.map(|t| t.to_rfc3339());
            let changed = tx.execute(
                "UPDATE actions SET status = 'failed', error = ?2, retry_at = ?3, finished_at = ?4,
                 revision = revision + 1 WHERE key = ?1",
                params![key, error, retry_at, now],
            )?;
            if changed == 0 {
                return Ok(false);
            }
            let detail = match &retry_at {
                Some(at) => format!("{error}\n\ntried again at {at}"),
                None => error.to_string(),
            };
            tx.execute(
                "INSERT INTO action_log (action, at, status, detail)
                 SELECT id, ?2, 'failed', ?3 FROM actions WHERE key = ?1",
                params![key, now, detail],
            )?;
            Ok(true)
        })
    }

    /// Failed actions whose next attempt is due by `now`.
    pub fn due_retries(&self, now: DateTime<Utc>) -> Result<Vec<Retry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, key, action, agent, routine, item, args, attempts FROM actions
             WHERE status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![now.to_rfc3339()], |r| {
            Ok(Retry {
                id: r.get(0)?,
                key: r.get(1)?,
                action: r.get(2)?,
                agent: r.get(3)?,
                routine: r.get(4)?,
                item: r.get(5)?,
                args: r.get(6)?,
                attempts: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Take up a due retry: in flight again, one attempt more. False when it
    /// was taken up already.
    pub fn take_retry(&mut self, id: i64) -> Result<bool> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE actions SET status = 'in_flight', retry_at = NULL, attempts = attempts + 1,
                 finished_at = NULL, revision = revision + 1
                 WHERE id = ?1 AND status = 'failed' AND retry_at IS NOT NULL",
                params![id],
            )?;
            if changed == 1 {
                tx.execute(
                    "INSERT INTO action_log (action, at, status, detail) VALUES (?1, ?2, 'in_flight', 'tried again')",
                    params![id, Utc::now().to_rfc3339()],
                )?;
            }
            Ok(changed == 1)
        })
    }
}
