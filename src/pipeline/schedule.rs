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

/// A run this much after its time was missed, not merely slow to start.
pub const GRACE_MINUTES: i64 = 15;

#[derive(Debug, PartialEq)]
pub enum Tick {
    /// Not yet.
    Wait,
    /// Run it now. It is claimed: the next tick will not start it again.
    Run {
        /// The time it was due, late or not.
        due: DateTime<Utc>,
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
    let Some(claimed) = store.claim_schedule(pipeline, schedule.next_after(now), now)? else {
        return Ok(Tick::Wait);
    };
    let silent = claimed
        .last_start
        .and_then(|(at, beat)| (!beat).then_some(at));
    let missed =
        (now - claimed.due_at > Duration::minutes(GRACE_MINUTES)).then_some(claimed.due_at);
    Ok(Tick::Run {
        due: claimed.due_at,
        missed,
        silent,
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
        let beat = crate::business::store::pipelines::Heartbeat {
            pipeline: "watch",
            due: None,
            started,
            finished: started,
            items_read: 0,
            items_kept: 0,
            report: None,
            notes: "",
            error: None,
        };
        store.record_run(&beat, &[]).unwrap();
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
        let due = |h| Tick::Run {
            due: at(h, 0),
            missed: None,
            silent: None,
        };
        assert_eq!(tick(&mut store, "watch", &every, at(8, 0)).unwrap(), due(8));
        // Claimed: the next tick does not start it again.
        assert_eq!(
            tick(&mut store, "watch", &every, at(8, 0)).unwrap(),
            Tick::Wait
        );
        heartbeat(&mut store, at(8, 0));
        // Five minutes after its time is slow, not missed.
        assert_eq!(tick(&mut store, "watch", &every, at(9, 5)).unwrap(), due(9));
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
                due: at(8, 0),
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
                due: at(9, 0),
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
