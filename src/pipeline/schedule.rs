//! When a scheduled pipeline runs, and what the owner must hear about it.
//! The schedule is the one in the pipeline's file; when it next runs, and
//! when it last started, are kept in `business.db`. A schedule seen for the
//! first time is armed for its next time; one missed while Airth was not
//! running fires once, late, and says so. A run that started and left no
//! heartbeat (Airth stopped during it) is reported at the next one.

use crate::business::store::Store;
use crate::objective::Schedule;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{OptionalExtension, params};

/// A run this much after its time was missed, not merely slow to start.
pub const GRACE_MINUTES: i64 = 15;

#[derive(Debug, PartialEq)]
pub enum Tick {
    /// Not yet.
    Wait,
    /// Run it now. It is claimed: the next tick will not start it again.
    Run {
        /// The time it should have run, when it runs late.
        missed: Option<DateTime<Utc>>,
        /// The start of an earlier run that left no heartbeat.
        silent: Option<DateTime<Utc>>,
    },
}

/// Whether `pipeline`, on `schedule`, runs at `now`.
pub fn tick(
    store: &mut Store,
    pipeline: &str,
    schedule: &Schedule,
    now: DateTime<Utc>,
) -> Result<Tick> {
    let next = schedule.next_after(now).to_rfc3339();
    store.write(|tx| {
        let row: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT next_at, started_at FROM pipeline_schedule WHERE pipeline = ?1",
                params![pipeline],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((next_at, started_at)) = row else {
            tx.execute(
                "INSERT INTO pipeline_schedule (pipeline, next_at) VALUES (?1, ?2)",
                params![pipeline, next],
            )?;
            return Ok(Tick::Wait);
        };
        let next_at = DateTime::parse_from_rfc3339(&next_at)?.with_timezone(&Utc);
        if next_at > now {
            return Ok(Tick::Wait);
        }
        let started = started_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?;
        let silent = match started {
            Some(at) => {
                let beats: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM pipeline_runs WHERE pipeline = ?1 AND started_at >= ?2",
                    params![pipeline, at.to_rfc3339()],
                    |r| r.get(0),
                )?;
                (beats == 0).then_some(at)
            }
            None => None,
        };
        tx.execute(
            "UPDATE pipeline_schedule SET next_at = ?2, started_at = ?3 WHERE pipeline = ?1",
            params![pipeline, next, now.to_rfc3339()],
        )?;
        let missed = (now - next_at > Duration::minutes(GRACE_MINUTES)).then_some(next_at);
        Ok(Tick::Run { missed, silent })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, h, m, 0).unwrap()
    }

    fn heartbeat(store: &mut Store, started: DateTime<Utc>) {
        store
            .write(|tx| {
                tx.execute(
                    "INSERT INTO pipeline_runs (pipeline, started_at, finished_at, ok, items_read, items_kept)
                     VALUES ('watch', ?1, ?1, 1, 0, 0)",
                    params![started.to_rfc3339()],
                )?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn arms_then_fires_once_per_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let every = Schedule::parse("every 1h").unwrap();
        // First seen: armed for an hour later, not run now.
        assert_eq!(
            tick(&mut store, "watch", &every, at(7, 0)).unwrap(),
            Tick::Wait
        );
        assert_eq!(
            tick(&mut store, "watch", &every, at(7, 59)).unwrap(),
            Tick::Wait
        );
        let due = Tick::Run {
            missed: None,
            silent: None,
        };
        assert_eq!(tick(&mut store, "watch", &every, at(8, 0)).unwrap(), due);
        // Claimed: the next tick does not start it again.
        assert_eq!(
            tick(&mut store, "watch", &every, at(8, 0)).unwrap(),
            Tick::Wait
        );
        heartbeat(&mut store, at(8, 0));
        assert_eq!(tick(&mut store, "watch", &every, at(9, 5)).unwrap(), due);
    }

    #[test]
    fn a_missed_time_fires_once_late_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let every = Schedule::parse("every 1h").unwrap();
        tick(&mut store, "watch", &every, at(7, 0)).unwrap();
        // Airth was not running from 8:00 to 11:30: one run, late.
        assert_eq!(
            tick(&mut store, "watch", &every, at(11, 30)).unwrap(),
            Tick::Run {
                missed: Some(at(8, 0)),
                silent: None
            }
        );
        assert_eq!(
            tick(&mut store, "watch", &every, at(11, 31)).unwrap(),
            Tick::Wait
        );
    }

    #[test]
    fn a_run_that_left_no_heartbeat_is_reported_at_the_next_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let every = Schedule::parse("every 1h").unwrap();
        tick(&mut store, "watch", &every, at(7, 0)).unwrap();
        tick(&mut store, "watch", &every, at(8, 0)).unwrap();
        // The 8:00 run never recorded itself.
        assert_eq!(
            tick(&mut store, "watch", &every, at(9, 0)).unwrap(),
            Tick::Run {
                missed: None,
                silent: Some(at(8, 0))
            }
        );
        // Another pipeline has its own schedule.
        assert_eq!(
            tick(&mut store, "other", &every, at(9, 0)).unwrap(),
            Tick::Wait
        );
    }
}
