//! The owner-time ledger (pilot step 2): what running the business costs
//! the owner. Airth records each answer the owner gives it, from the moment
//! it asked to the answer; the owner adds what happens outside Airth.

use super::Store;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::params;
use std::time::Duration;

/// An answer counts for at most this long: past it, the owner was away,
/// not working on the question.
pub const MAX_ANSWER_SECS: u64 = 600;

/// An answer to an approval request, recorded by Airth.
pub const ANSWER: &str = "answer";
/// Time the owner reports themselves.
pub const REPORTED: &str = "reported";

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub at: DateTime<Utc>,
    pub activity: String,
    pub seconds: u64,
    pub note: String,
}

/// What an answer cost the owner, from the question to the answer.
pub fn answer_seconds(waited: Duration) -> u64 {
    waited.as_secs().min(MAX_ANSWER_SECS)
}

/// A duration as the owner reads it: "42 s", "12 min", "1 h 40".
pub fn human(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min", secs / 60),
        _ => format!("{} h {:02}", secs / 3600, secs % 3600 / 60),
    }
}

impl Store {
    /// The founder's seconds on each agent's items since `since`, by agent
    /// (None: Airth's own items).
    pub fn owner_seconds_by_agent(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<(Option<String>, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT q.agent, SUM(o.seconds) FROM owner_time o JOIN queue q ON o.item = q.id
             WHERE o.at >= ?1 GROUP BY q.agent",
        )?;
        let rows = stmt.query_map(params![since.to_rfc3339()], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn record_time(&mut self, entry: &Entry) -> Result<()> {
        if entry.activity.trim().is_empty() {
            bail!("an entry needs an activity");
        }
        let seconds = i64::try_from(entry.seconds)?;
        self.write(|tx| {
            tx.execute(
                "INSERT INTO owner_time (at, activity, seconds, note) VALUES (?1, ?2, ?3, ?4)",
                params![entry.at.to_rfc3339(), entry.activity, seconds, entry.note],
            )?;
            Ok(())
        })
    }

    /// Entries from `since` on, oldest first.
    pub fn time_since(&self, since: DateTime<Utc>) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT at, activity, seconds, note FROM owner_time WHERE at >= ?1 ORDER BY at, id",
        )?;
        let rows = stmt.query_map(params![since.to_rfc3339()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (at, activity, seconds, note) = row?;
            out.push(Entry {
                at: DateTime::parse_from_rfc3339(&at)?.with_timezone(&Utc),
                activity,
                seconds: u64::try_from(seconds)?,
                note,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;
    use chrono::TimeZone;

    fn entry(day: u32, activity: &str, seconds: u64) -> Entry {
        Entry {
            at: Utc.with_ymd_and_hms(2026, 10, day, 9, 0, 0).unwrap(),
            activity: activity.into(),
            seconds,
            note: String::new(),
        }
    }

    #[test]
    fn answers_count_up_to_ten_minutes() {
        assert_eq!(answer_seconds(Duration::from_millis(42_900)), 42);
        assert_eq!(answer_seconds(Duration::from_secs(3 * 3600)), 600);
    }

    #[test]
    fn durations_read_plainly() {
        assert_eq!(human(42), "42 s");
        assert_eq!(human(12 * 60 + 5), "12 min");
        assert_eq!(human(3600 + 40 * 60), "1 h 40");
        assert_eq!(human(2 * 3600 + 5 * 60), "2 h 05");
    }

    #[test]
    fn records_and_reads_back_from_a_date() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        store.record_time(&entry(20, ANSWER, 30)).unwrap();
        store.record_time(&entry(28, REPORTED, 900)).unwrap();
        store.record_time(&entry(29, ANSWER, 12)).unwrap();
        assert!(store.record_time(&entry(29, " ", 1)).is_err());
        let since = Utc.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).unwrap();
        let week = store.time_since(since).unwrap();
        assert_eq!(week, vec![entry(28, REPORTED, 900), entry(29, ANSWER, 12)]);
    }

    #[test]
    fn the_ledger_cannot_be_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        store.record_time(&entry(20, ANSWER, 30)).unwrap();
        for sql in [
            "UPDATE owner_time SET seconds = 0",
            "DELETE FROM owner_time",
        ] {
            let err = store.write(|tx| Ok(tx.execute(sql, [])?)).err().unwrap();
            assert!(format!("{err:#}").contains("append-only"), "{sql}");
        }
        let all = store
            .time_since(Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap())
            .unwrap();
        assert_eq!(all, vec![entry(20, ANSWER, 30)]);
    }
}
