//! Autonomy (decision 45): each agent's level per action and the history of
//! every change, the journal of actions taken, and the queue where what waits
//! for the founder goes.

use super::Store;
use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{OptionalExtension, params};

/// A new action's level before the founder sets one.
pub const DEFAULT_LEVEL: u8 = 1;

/// One action about to be taken.
pub struct NewAction<'a> {
    /// Unique: the same key never acts twice (decision 49).
    pub key: &'a str,
    pub action: &'a str,
    pub connector: &'a str,
    pub agent: Option<&'a str>,
    pub routine: Option<&'a str>,
    pub run: Option<&'a str>,
    pub level: u8,
    /// The founder approved this very action.
    pub approved: bool,
    pub args: &'a str,
    /// Actions of this connector taken without approval in 24 hours.
    pub daily_limit: u32,
}

#[derive(Debug, PartialEq)]
pub enum Begin {
    /// Recorded in flight; the caller acts, then finishes it.
    Started(i64),
    /// An action under this key exists already, with this status.
    Duplicate(String),
    /// The connector's daily limit is reached: it must not act alone.
    LimitReached,
}

/// Something for the founder: an approval, a prepared or delayed action.
pub struct NewItem<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub agent: Option<&'a str>,
    pub routine: Option<&'a str>,
    pub run: Option<&'a str>,
    pub action: Option<&'a str>,
    pub args: &'a str,
    pub execute_at: Option<DateTime<Utc>>,
}

impl Store {
    /// The agent's level for `action`, never above `ceiling`.
    pub fn level(&self, agent: &str, action: &str, ceiling: u8) -> Result<u8> {
        let set: Option<u8> = self
            .conn
            .query_row(
                "SELECT level FROM autonomy WHERE agent = ?1 AND action = ?2",
                params![agent, action],
                |r| r.get(0),
            )
            .optional()?;
        Ok(set.unwrap_or(DEFAULT_LEVEL).min(ceiling))
    }

    /// Set a level, as the founder (`by`) or an incident does. A level above
    /// the ceiling is refused, never stored.
    pub fn set_level(
        &mut self,
        agent: &str,
        action: &str,
        level: u8,
        ceiling: u8,
        by: &str,
        reason: &str,
    ) -> Result<()> {
        if level > ceiling {
            bail!("{action} cannot go above level {ceiling}");
        }
        self.write(|tx| {
            let from: Option<u8> = tx
                .query_row(
                    "SELECT level FROM autonomy WHERE agent = ?1 AND action = ?2",
                    params![agent, action],
                    |r| r.get(0),
                )
                .optional()?;
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO autonomy (agent, action, level, set_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (agent, action) DO UPDATE SET level = ?3, set_at = ?4, revision = revision + 1",
                params![agent, action, level, now],
            )?;
            tx.execute(
                "INSERT INTO autonomy_log (at, agent, action, from_level, to_level, by, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![now, agent, action, from, level, by, reason],
            )?;
            Ok(())
        })
    }

    /// Record an action before it is taken. The key check and the daily
    /// count happen in the same transaction as the insert, so runs acting at
    /// the same moment can neither act twice nor pass the limit together.
    pub fn begin_action(&mut self, a: &NewAction) -> Result<Begin> {
        let since = (Utc::now() - Duration::hours(24)).to_rfc3339();
        self.write(|tx| {
            let existing: Option<String> = tx
                .query_row("SELECT status FROM actions WHERE key = ?1", params![a.key], |r| r.get(0))
                .optional()?;
            if let Some(status) = existing {
                return Ok(Begin::Duplicate(status));
            }
            if !a.approved && a.level >= 2 {
                let alone: u32 = tx.query_row(
                    "SELECT COUNT(*) FROM actions WHERE connector = ?1 AND started_at >= ?2
                     AND approved = 0 AND status != 'failed'",
                    params![a.connector, since],
                    |r| r.get(0),
                )?;
                if alone >= a.daily_limit {
                    return Ok(Begin::LimitReached);
                }
            }
            tx.execute(
                "INSERT INTO actions (key, action, connector, agent, routine, run, level, approved, args, status, attempts, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'in_flight', 1, ?10)",
                params![a.key, a.action, a.connector, a.agent, a.routine, a.run, a.level, a.approved, a.args, Utc::now().to_rfc3339()],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO action_log (action, at, status) VALUES (?1, ?2, 'in_flight')",
                params![id, Utc::now().to_rfc3339()],
            )?;
            Ok(Begin::Started(id))
        })
    }

    /// What became of an action: `confirmed`, `failed` or `unknown`.
    pub fn finish_action(&mut self, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        self.write(|tx| {
            let now = Utc::now().to_rfc3339();
            let changed = tx.execute(
                "UPDATE actions SET status = ?2, error = ?3, finished_at = ?4, revision = revision + 1
                 WHERE id = ?1 AND status = 'in_flight'",
                params![id, status, error, now],
            )?;
            if changed == 0 {
                bail!("action {id} is not in flight");
            }
            tx.execute(
                "INSERT INTO action_log (action, at, status, detail) VALUES (?1, ?2, ?3, ?4)",
                params![id, now, status, error.unwrap_or_default()],
            )?;
            Ok(())
        })
    }

    /// Put something in the queue for the founder; its id.
    pub fn propose(&mut self, item: &NewItem) -> Result<i64> {
        self.write(|tx| {
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO queue (created_at, kind, title, agent, routine, run, action, args, original_args, execute_at, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, 'pending')",
                params![now, item.kind, item.title, item.agent, item.routine, item.run, item.action, item.args,
                    item.execute_at.map(|t| t.to_rfc3339())],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO queue_log (item, at, action, by) VALUES (?1, ?2, 'proposed', 'airth')",
                params![id, now],
            )?;
            Ok(id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;

    fn action<'a>(key: &'a str, level: u8, approved: bool) -> NewAction<'a> {
        NewAction {
            key,
            action: "typefully.create_draft",
            connector: "typefully",
            agent: Some("writer"),
            routine: None,
            run: None,
            level,
            approved,
            args: "{}",
            daily_limit: 3,
        }
    }

    #[test]
    fn levels_start_at_one_never_pass_the_ceiling_and_keep_their_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join(FILE)).unwrap();
        assert_eq!(s.level("writer", "blog.publish", 1).unwrap(), 1);
        assert_eq!(s.level("writer", "x.reply", 0).unwrap(), 0);
        assert!(
            s.set_level("writer", "blog.publish", 2, 1, "founder", "")
                .is_err()
        );
        s.set_level(
            "writer",
            "typefully.create_draft",
            3,
            3,
            "founder",
            "trusted",
        )
        .unwrap();
        assert_eq!(s.level("writer", "typefully.create_draft", 3).unwrap(), 3);
        // A lower ceiling applies at once, without rewriting the level.
        assert_eq!(s.level("writer", "typefully.create_draft", 1).unwrap(), 1);
        s.set_level(
            "writer",
            "typefully.create_draft",
            2,
            3,
            "incident",
            "failed",
        )
        .unwrap();
        let history: Vec<(Option<u8>, u8, String)> = s
            .conn
            .prepare("SELECT from_level, to_level, by FROM autonomy_log ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            history,
            vec![(None, 3, "founder".into()), (Some(3), 2, "incident".into())]
        );
    }

    #[test]
    fn an_action_runs_once_per_key_and_is_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join(FILE)).unwrap();
        let Begin::Started(id) = s.begin_action(&action("r/link/send", 3, false)).unwrap() else {
            panic!("not started");
        };
        assert_eq!(
            s.begin_action(&action("r/link/send", 3, false)).unwrap(),
            Begin::Duplicate("in_flight".into())
        );
        s.finish_action(id, "confirmed", None).unwrap();
        assert!(s.finish_action(id, "failed", None).is_err());
        assert_eq!(
            s.begin_action(&action("r/link/send", 3, true)).unwrap(),
            Begin::Duplicate("confirmed".into())
        );
        let item = NewItem {
            kind: "approval",
            title: "Publish",
            agent: Some("writer"),
            routine: None,
            run: None,
            action: Some("blog.publish"),
            args: "{\"id\": 1}",
            execute_at: None,
        };
        assert!(s.propose(&item).unwrap() > 0);
    }

    /// Decision 45: the daily limit holds when runs act at the same moment.
    #[test]
    fn the_daily_limit_holds_under_concurrent_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        Store::open(&path).unwrap();
        let started: Vec<bool> = (0..10)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut s = Store::open(&path).unwrap();
                    let key = format!("k{i}");
                    matches!(
                        s.begin_action(&action(&key, 3, false)).unwrap(),
                        Begin::Started(_)
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect();
        assert_eq!(started.iter().filter(|s| **s).count(), 3);
        // What the founder approved, or a level that asks, is not counted.
        let mut s = Store::open(&path).unwrap();
        assert!(matches!(
            s.begin_action(&action("a", 3, true)).unwrap(),
            Begin::Started(_)
        ));
        assert!(matches!(
            s.begin_action(&action("b", 1, false)).unwrap(),
            Begin::Started(_)
        ));
    }
}
