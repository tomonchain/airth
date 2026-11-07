//! What the owner reads about the business: each pipeline's recent runs and
//! next time, and the week in numbers. Reads only.

use super::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::collections::BTreeMap;

/// One run of a pipeline: its heartbeat.
#[derive(Debug, PartialEq, Serialize)]
pub struct RunRow {
    pub started_at: String,
    pub finished_at: String,
    pub ok: bool,
    pub items_read: i64,
    pub items_kept: i64,
    pub report: Option<String>,
    pub notes: String,
    pub error: Option<String>,
    /// When its schedule said it should run; None for a run by hand.
    pub due_at: Option<String>,
}

/// The week in numbers.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Week {
    pub owner_seconds: i64,
    /// Drafts the owner approved, and of those, the ones sent as drafted.
    pub approved: i64,
    pub unedited: i64,
    pub rejected: i64,
    pub model_calls: i64,
    /// Estimated, from list prices; calls to a model without a known price
    /// are counted apart.
    pub model_cost: f64,
    pub unpriced_calls: i64,
}

/// How the routines ran over a period, for the weekly report: the
/// founder's time, the answers, the cost, the actions and the incidents.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct RoutinesWeek {
    /// Every second in the ledger: answers and time the founder reported.
    pub founder_seconds: i64,
    pub approved: i64,
    pub edited: i64,
    pub rejected: i64,
    pub model_calls: i64,
    /// Estimated, from list prices; calls without a known price are left
    /// out and counted in `unpriced`.
    pub cost_usd: f64,
    pub unpriced: i64,
    pub actions_done: i64,
    pub actions_failed: i64,
    pub incidents: i64,
    /// By routine, by name.
    pub routines: Vec<RoutineWeek>,
    /// Scheduled runs that started more than the grace after their time
    /// (the Mac asleep, Airth stopped), oldest first.
    pub late: Vec<LateRun>,
    /// Why items were dropped (decision 61): each reason once, by routine,
    /// with how many times it came.
    pub dropped: Vec<DroppedReason>,
}

/// One week's facts for the beta's proof (decision 92).
#[derive(Debug, PartialEq, Serialize)]
pub struct ProofWeek {
    /// Scheduled routine runs that started in the week.
    pub scheduled: i64,
    /// Those that started past the grace after their time.
    pub late: i64,
    /// Runs that did not happen at their time (each an incident).
    pub missed: i64,
    /// Incidents of the week closed more than 24 hours after they opened,
    /// or still open past that.
    pub slow_incidents: i64,
    pub founder_seconds: i64,
}

/// A reason items were dropped, and how often.
#[derive(Debug, PartialEq, Serialize)]
pub struct DroppedReason {
    pub routine: String,
    pub reason: String,
    pub count: i64,
}

/// A scheduled run that started late.
#[derive(Debug, PartialEq, Serialize)]
pub struct LateRun {
    pub routine: String,
    pub due_at: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct RoutineWeek {
    pub routine: String,
    pub runs: i64,
    pub failed: i64,
    pub cost_usd: f64,
    /// The founder's seconds on the items it queued.
    pub founder_seconds: i64,
    /// Of its runs, those that started late.
    pub late: i64,
    /// Items its runs dropped (decision 61).
    pub dropped: i64,
    /// Its items the founder approved, as they were or overridden.
    pub approved: i64,
}

/// What one pipeline's model calls cost over a period.
#[derive(Debug, PartialEq, Serialize)]
pub struct PipelineCost {
    pub pipeline: String,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Estimated, from list prices; calls without a known price are left
    /// out and counted in `unpriced`.
    pub cost_usd: f64,
    pub unpriced: i64,
}

impl Store {
    /// What each pipeline's model calls cost since `since`, costliest first.
    pub fn pipeline_costs(&self, since: DateTime<Utc>) -> Result<Vec<PipelineCost>> {
        let mut stmt = self.conn.prepare(
            "SELECT pipeline, COUNT(*), SUM(input_tokens), SUM(output_tokens),
                    COALESCE(SUM(cost_usd), 0), SUM(cost_usd IS NULL)
             FROM model_calls WHERE at >= ?1 AND dry_run = 0 GROUP BY pipeline
             ORDER BY 5 DESC, 2 DESC",
        )?;
        let rows = stmt.query_map(params![since.to_rfc3339()], |r| {
            Ok(PipelineCost {
                pipeline: r.get(0)?,
                calls: r.get(1)?,
                input_tokens: r.get(2)?,
                output_tokens: r.get(3)?,
                cost_usd: r.get(4)?,
                unpriced: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What the pipelines' model calls cost each day since `since`, by UTC
    /// date (`YYYY-MM-DD`), oldest first; days without a call are left out.
    pub fn model_cost_by_day(&self, since: DateTime<Utc>) -> Result<Vec<(String, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT substr(at, 1, 10), COALESCE(SUM(cost_usd), 0)
             FROM model_calls WHERE at >= ?1 AND dry_run = 0 GROUP BY 1 ORDER BY 1",
        )?;
        let rows = stmt.query_map(params![since.to_rfc3339()], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The latest runs of `pipeline`, newest first.
    pub fn pipeline_runs(&self, pipeline: &str, limit: usize) -> Result<Vec<RunRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at, finished_at, ok, items_read, items_kept, report, notes, error, due_at
             FROM pipeline_runs WHERE pipeline = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pipeline, limit as i64], |r| {
            Ok(RunRow {
                started_at: r.get(0)?,
                finished_at: r.get(1)?,
                ok: r.get(2)?,
                items_read: r.get(3)?,
                items_kept: r.get(4)?,
                report: r.get(5)?,
                notes: r.get(6)?,
                error: r.get(7)?,
                due_at: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// When a scheduled pipeline runs next, once the scheduler has seen it.
    pub fn next_run(&self, pipeline: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT next_at FROM pipeline_schedule WHERE pipeline = ?1",
                params![pipeline],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// How the routines ran since `since`. Dry runs count nowhere.
    pub fn routines_week(&self, since: DateTime<Utc>) -> Result<RoutinesWeek> {
        let at = since.to_rfc3339();
        let one =
            |sql: &str| -> Result<i64> { Ok(self.conn.query_row(sql, params![at], |r| r.get(0))?) };
        let answered = |event: &str| {
            one(&format!(
                "SELECT COUNT(*) FROM queue_log
                 WHERE by = 'founder' AND action = '{event}' AND at >= ?1"
            ))
        };
        let actions = |status: &str| {
            one(&format!(
                "SELECT COUNT(*) FROM actions WHERE status = '{status}' AND started_at >= ?1"
            ))
        };
        let mut by_name: BTreeMap<String, RoutineWeek> = BTreeMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT substr(pipeline, 9), COUNT(*), SUM(ok = 0) FROM pipeline_runs
             WHERE pipeline LIKE 'routine:%' AND started_at >= ?1 GROUP BY pipeline",
        )?;
        for row in stmt.query_map(params![at], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))
        })? {
            let (name, runs, failed) = row?;
            let r = by_name.entry(name.clone()).or_default();
            (r.routine, r.runs, r.failed) = (name, runs, failed);
        }
        let mut stmt = self.conn.prepare(
            "SELECT q.routine, SUM(o.seconds) FROM owner_time o JOIN queue q ON o.item = q.id
             WHERE o.at >= ?1 AND q.routine IS NOT NULL GROUP BY q.routine",
        )?;
        for row in stmt.query_map(params![at], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))? {
            let (name, seconds) = row?;
            let r = by_name.entry(name.clone()).or_default();
            (r.routine, r.founder_seconds) = (name, seconds);
        }
        let grace = chrono::Duration::minutes(crate::pipeline::schedule::GRACE_MINUTES);
        let mut late = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT substr(pipeline, 9), due_at, started_at FROM pipeline_runs
             WHERE pipeline LIKE 'routine:%' AND due_at IS NOT NULL AND started_at >= ?1 ORDER BY id",
        )?;
        for row in stmt.query_map(params![at], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (routine, due, started) = row?;
            let parse = |t: &str| DateTime::parse_from_rfc3339(t).map(|t| t.with_timezone(&Utc));
            let (Ok(due_at), Ok(started_at)) = (parse(&due), parse(&started)) else {
                continue;
            };
            if started_at - due_at > grace {
                if let Some(r) = by_name.get_mut(&routine) {
                    r.late += 1;
                }
                late.push(LateRun {
                    routine,
                    due_at,
                    started_at,
                });
            }
        }
        let mut stmt = self.conn.prepare(
            "SELECT q.routine, COUNT(*) FROM queue_log l JOIN queue q ON q.id = l.item
             WHERE l.by = 'founder' AND l.action IN ('approved', 'overridden') AND l.at >= ?1
             AND q.kind = 'approval' AND q.routine IS NOT NULL GROUP BY q.routine",
        )?;
        for row in stmt.query_map(params![at], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))? {
            let (name, approved) = row?;
            let r = by_name.entry(name.clone()).or_default();
            (r.routine, r.approved) = (name, approved);
        }
        // Every run, by hand or scheduled, keeps what each block dropped.
        let mut reasons: BTreeMap<(String, String), i64> = BTreeMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT routine, dropped FROM routine_blocks WHERE dropped IS NOT NULL AND started_at >= ?1",
        )?;
        for row in stmt.query_map(params![at], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (routine, list) = row?;
            let list: Vec<serde_json::Value> = serde_json::from_str(&list).unwrap_or_default();
            let r = by_name.entry(routine.clone()).or_default();
            r.routine = routine.clone();
            r.dropped += list.len() as i64;
            for d in list {
                let reason = d["reason"]
                    .as_str()
                    .unwrap_or("no reason recorded")
                    .to_string();
                *reasons.entry((routine.clone(), reason)).or_default() += 1;
            }
        }
        // A routine run by hand leaves no heartbeat, but its calls count.
        for c in self.pipeline_costs(since)? {
            let r = by_name.entry(c.pipeline.clone()).or_default();
            (r.routine, r.cost_usd) = (c.pipeline, c.cost_usd);
        }
        Ok(RoutinesWeek {
            founder_seconds: one(
                "SELECT COALESCE(SUM(seconds), 0) FROM owner_time WHERE at >= ?1",
            )?,
            approved: answered("approved")?,
            edited: answered("edited")?,
            rejected: answered("rejected")?,
            model_calls: one("SELECT COUNT(*) FROM model_calls WHERE at >= ?1 AND dry_run = 0")?,
            cost_usd: self.conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM model_calls WHERE at >= ?1 AND dry_run = 0",
                params![at],
                |r| r.get(0),
            )?,
            unpriced: one(
                "SELECT COUNT(*) FROM model_calls WHERE at >= ?1 AND dry_run = 0 AND cost_usd IS NULL",
            )?,
            actions_done: actions("confirmed")?,
            actions_failed: actions("failed")?,
            incidents: one(
                "SELECT COUNT(*) FROM queue WHERE kind = 'incident' AND created_at >= ?1",
            )?,
            routines: by_name.into_values().collect(),
            late,
            dropped: reasons
                .into_iter()
                .map(|((routine, reason), count)| DroppedReason {
                    routine,
                    reason,
                    count,
                })
                .collect(),
        })
    }

    /// What decides whether `[from, to)` was a stable week (decision 92):
    /// its scheduled runs, those late or missed, the incidents that stayed
    /// open more than a day (judged at `now`), and the founder's time.
    pub fn proof_week(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<ProofWeek> {
        let (a, b) = (from.to_rfc3339(), to.to_rfc3339());
        let grace = chrono::Duration::minutes(crate::pipeline::schedule::GRACE_MINUTES);
        let parse = |t: &str| DateTime::parse_from_rfc3339(t).map(|t| t.with_timezone(&Utc));
        let mut stmt = self.conn.prepare(
            "SELECT due_at, started_at FROM pipeline_runs
             WHERE pipeline LIKE 'routine:%' AND due_at IS NOT NULL
             AND started_at >= ?1 AND started_at < ?2",
        )?;
        let (mut scheduled, mut late) = (0, 0);
        for row in stmt.query_map(params![a, b], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (due, started) = row?;
            scheduled += 1;
            if let (Ok(d), Ok(s)) = (parse(&due), parse(&started))
                && s - d > grace
            {
                late += 1;
            }
        }
        let missed: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM queue WHERE kind = 'incident'
             AND title LIKE '% did not run on time' AND created_at >= ?1 AND created_at < ?2",
            params![a, b],
            |r| r.get(0),
        )?;
        // When each incident of the week was closed, if it was.
        let mut stmt = self.conn.prepare(
            "SELECT q.created_at, (SELECT MIN(l.at) FROM queue_log l WHERE l.item = q.id
               AND l.action IN ('approved', 'rejected', 'cancelled', 'expired', 'done', 'failed'))
             FROM queue q WHERE q.kind = 'incident' AND q.created_at >= ?1 AND q.created_at < ?2",
        )?;
        let mut slow_incidents = 0;
        for row in stmt.query_map(params![a, b], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            let (created, closed) = row?;
            let Ok(created) = parse(&created) else {
                continue;
            };
            let closed = closed.and_then(|c| parse(&c).ok()).unwrap_or(now);
            if closed - created > chrono::Duration::hours(24) {
                slow_incidents += 1;
            }
        }
        let founder_seconds: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(seconds), 0) FROM owner_time WHERE at >= ?1 AND at < ?2",
            params![a, b],
            |r| r.get(0),
        )?;
        Ok(ProofWeek {
            scheduled,
            late,
            missed,
            slow_incidents,
            founder_seconds,
        })
    }

    pub fn week(&self, since: DateTime<Utc>) -> Result<Week> {
        let since = since.to_rfc3339();
        let one = |sql: &str| -> Result<i64> {
            Ok(self.conn.query_row(sql, params![since], |r| r.get(0))?)
        };
        let decided = |action: &str, unedited: bool| {
            one(&format!(
                "SELECT COUNT(*) FROM draft_log l JOIN drafts d ON d.id = l.draft
                 WHERE l.action = '{action}' AND l.at >= ?1{}",
                if unedited {
                    " AND d.text = d.original"
                } else {
                    ""
                }
            ))
        };
        Ok(Week {
            owner_seconds: one("SELECT COALESCE(SUM(seconds), 0) FROM owner_time WHERE at >= ?1")?,
            approved: decided("approved", false)?,
            unedited: decided("approved", true)?,
            rejected: decided("rejected", false)?,
            model_calls: one("SELECT COUNT(*) FROM model_calls WHERE at >= ?1 AND dry_run = 0")?,
            model_cost: self.conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM model_calls WHERE at >= ?1 AND dry_run = 0",
                params![since],
                |r| r.get(0),
            )?,
            unpriced_calls: one(
                "SELECT COUNT(*) FROM model_calls WHERE at >= ?1 AND dry_run = 0 AND cost_usd IS NULL",
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::drafts::{Move, New};
    use crate::business::store::FILE;
    use serde_json::json;

    /// Decision 92: a week is stable when its scheduled routines ran on
    /// time, no incident stayed open over a day and the founder's time was
    /// at most half the baseline; the proof counts them in a row.
    #[test]
    fn stable_weeks_are_judged_and_counted_in_a_row() {
        use crate::routine::week::proof;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let at = |t: &str| DateTime::parse_from_rfc3339(t).unwrap().with_timezone(&Utc);
        // A Wednesday: the complete weeks before it start on 2 and 26 Oct.
        let now = at("2026-11-11T12:00:00Z");
        let baseline = Some(2100); // 35 h: half is 17.5 h, 63,000 s.
        let sql = |store: &mut Store, sql: &str| {
            store
                .write(|tx| {
                    tx.execute_batch(sql)?;
                    Ok(())
                })
                .unwrap()
        };
        sql(
            &mut store,
            "INSERT INTO pipeline_runs (pipeline, due_at, started_at, finished_at, ok, items_read, items_kept)
             VALUES ('routine:watch', '2026-10-27T08:00:00+00:00', '2026-10-27T08:01:00+00:00', 'x', 1, 1, 1),
                    ('routine:watch', '2026-11-03T08:00:00+00:00', '2026-11-03T08:01:00+00:00', 'x', 1, 1, 1);
             INSERT INTO owner_time (at, activity, seconds) VALUES
                    ('2026-10-28T10:00:00+00:00', 'answer', 70000),
                    ('2026-11-04T10:00:00+00:00', 'answer', 3600);
             INSERT INTO queue (id, created_at, kind, title, status) VALUES
                    (1, '2026-11-04T10:00:00+00:00', 'incident', 'Typefully refused a request', 'done');
             INSERT INTO queue_log (item, at, action, by) VALUES
                    (1, '2026-11-04T12:00:00+00:00', 'done', 'founder');",
        );
        let p = proof(&store, baseline, now).unwrap();
        assert_eq!((p.in_a_row, p.of), (1, 4), "{}", p.sentence);
        assert_eq!(p.last_week, "2026-W45");
        assert!(p.last_stable);
        let before = store
            .proof_week(at("2026-10-26T00:00:00Z"), at("2026-11-02T00:00:00Z"), now)
            .unwrap();
        let why = crate::routine::week::judge(&before, baseline);
        assert_eq!(why.len(), 1);
        assert!(why[0].contains("over half the baseline"), "{why:?}");

        // No baseline: nothing can be proved.
        assert!(!proof(&store, None, now).unwrap().last_stable);
        // A run started late, and an incident still open after a day.
        sql(
            &mut store,
            "INSERT INTO pipeline_runs (pipeline, due_at, started_at, finished_at, ok, items_read, items_kept)
             VALUES ('routine:watch', '2026-11-05T08:00:00+00:00', '2026-11-05T09:30:00+00:00', 'x', 1, 1, 1);
             INSERT INTO queue (id, created_at, kind, title, status) VALUES
                    (2, '2026-11-06T10:00:00+00:00', 'incident', 'watch did not run on time', 'pending');",
        );
        let p = proof(&store, baseline, now).unwrap();
        assert_eq!(p.in_a_row, 0);
        assert!(!p.last_stable);
        assert_eq!(
            p.why,
            vec!["2 runs late or missed", "1 incident open over 24 h"],
            "{}",
            p.sentence
        );
        assert!(
            p.sentence
                .starts_with("0 stable weeks in a row, of 4: week 2026-W45 had")
        );
    }

    #[test]
    fn counts_the_week() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let since = Utc::now() - chrono::Duration::days(7);
        assert_eq!(store.week(since).unwrap(), Week::default());
        let mut add = |text: &str| {
            store
                .add_draft(&New {
                    pipeline: "p",
                    kind: "post",
                    text,
                    source: &json!({}),
                    model_call: None,
                })
                .unwrap()
        };
        let (a, b, c) = (add("one"), add("two"), add("three"));
        store.move_draft(a, None, Move::Approve, 20).unwrap();
        store
            .move_draft(b, None, Move::Edit("two, better".into()), 40)
            .unwrap();
        store.move_draft(b, None, Move::Approve, 5).unwrap();
        store
            .move_draft(c, None, Move::Reject("no".into()), 5)
            .unwrap();
        store
            .write(|tx| {
                tx.execute_batch(
                    "INSERT INTO model_calls (at, pipeline, step, prompt, prompt_version, model, input_tokens,
                        output_tokens, cache_read_tokens, cache_write_tokens, cost_usd, latency_ms, valid)
                     VALUES ('9999', 'p', 1, 'x', 'v', 'm', 0, 0, 0, 0, 0.25, 0, 1),
                            ('9999', 'p', 1, 'x', 'v', 'gpt-x', 0, 0, 0, 0, NULL, 0, 1);
                     INSERT INTO pipeline_runs (pipeline, started_at, finished_at, ok, items_read, items_kept)
                     VALUES ('p', 'a', 'a', 1, 3, 1), ('p', 'b', 'b', 0, 0, 0);
                     INSERT INTO pipeline_schedule (pipeline, next_at) VALUES ('p', 'tomorrow');",
                )?;
                Ok(())
            })
            .unwrap();
        let week = store.week(since).unwrap();
        assert_eq!(
            week,
            Week {
                owner_seconds: 70,
                approved: 2,
                unedited: 1,
                rejected: 1,
                model_calls: 2,
                model_cost: 0.25,
                unpriced_calls: 1,
            }
        );
        let costs = store.pipeline_costs(since).unwrap();
        assert_eq!(
            costs,
            vec![PipelineCost {
                pipeline: "p".into(),
                calls: 2,
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.25,
                unpriced: 1,
            }]
        );
        let runs = store.pipeline_runs("p", 10).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].started_at, "b", "newest first");
        assert_eq!(store.next_run("p").unwrap().as_deref(), Some("tomorrow"));
        assert_eq!(store.next_run("other").unwrap(), None);
    }

    /// The Finance page's chart: the pipelines' model calls by day, dry runs
    /// and what is before the window left out.
    #[test]
    fn model_costs_add_up_by_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        store
            .write(|tx| {
                tx.execute_batch(
                    "INSERT INTO model_calls (at, pipeline, step, prompt, prompt_version, model, input_tokens,
                        output_tokens, cache_read_tokens, cache_write_tokens, cost_usd, latency_ms, valid, dry_run)
                     VALUES ('2026-11-03T07:00:00+00:00', 'p', 1, 'x', 'v', 'm', 0, 0, 0, 0, 0.25, 0, 1, 0),
                            ('2026-11-03T08:00:00+00:00', 'p', 1, 'x', 'v', 'm', 0, 0, 0, 0, 0.5, 0, 1, 0),
                            ('2026-11-03T09:00:00+00:00', 'p', 1, 'x', 'v', 'm', 0, 0, 0, 0, 9.0, 0, 1, 1),
                            ('2026-11-04T07:00:00+00:00', 'p', 1, 'x', 'v', 'gpt-x', 0, 0, 0, 0, NULL, 0, 1, 0),
                            ('2026-10-01T07:00:00+00:00', 'p', 1, 'x', 'v', 'm', 0, 0, 0, 0, 3.0, 0, 1, 0);",
                )?;
                Ok(())
            })
            .unwrap();
        let since = "2026-11-01T00:00:00Z".parse().unwrap();
        assert_eq!(
            store.model_cost_by_day(since).unwrap(),
            vec![
                ("2026-11-03".to_string(), 0.75),
                ("2026-11-04".to_string(), 0.0)
            ]
        );
    }

    /// The weekly report's figures: answers from the queue's log, time from
    /// the ledger, runs, costs and actions by routine; dry runs and what is
    /// older than the week count nowhere.
    #[test]
    fn counts_how_the_routines_ran() {
        use crate::business::store::autonomy::{NewAction, NewItem};
        use crate::business::store::queue::Answer;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let since = Utc::now() - chrono::Duration::days(7);
        assert_eq!(store.routines_week(since).unwrap(), RoutinesWeek::default());
        let mut item = |kind: &str, routine: Option<&str>| {
            store
                .propose(&NewItem {
                    kind,
                    title: "t",
                    agent: None,
                    routine,
                    run: None,
                    action: None,
                    args: r#"{"answer": {"draft": "x"}}"#,
                    execute_at: None,
                    source: r#"{"editable": "answer.draft"}"#,
                    model_call: None,
                })
                .unwrap()
        };
        let (a, b, c) = (
            item("approval", Some("posts")),
            item("approval", Some("posts")),
            item("approval", Some("watch")),
        );
        item("incident", None);
        store.answer(a, None, Answer::Approve, "app", 30).unwrap();
        store
            .answer(b, None, Answer::Edit("y".into()), "telegram", 60)
            .unwrap();
        store
            .answer(b, None, Answer::Approve, "telegram", 10)
            .unwrap();
        store
            .answer(c, None, Answer::Reject("no".into()), "app", 5)
            .unwrap();
        for (key, status) in [("k1", "confirmed"), ("k2", "failed"), ("k3", "confirmed")] {
            let begin = store
                .begin_action(&NewAction {
                    key,
                    action: "typefully.create_draft",
                    connector: "typefully",
                    agent: None,
                    routine: Some("posts"),
                    run: None,
                    item: None,
                    level: 3,
                    approved: true,
                    args: "{}",
                    daily_limit: 20,
                })
                .unwrap();
            let crate::business::store::autonomy::Begin::Started(id) = begin else {
                panic!("{key} did not start");
            };
            store.finish_action(id, status, None).unwrap();
        }
        let old = "2020-01-01T00:00:00+00:00";
        store
            .write(|tx| {
                tx.execute_batch(&format!(
                    "INSERT INTO model_calls (at, pipeline, step, prompt, prompt_version, model, input_tokens,
                        output_tokens, cache_read_tokens, cache_write_tokens, cost_usd, latency_ms, valid, dry_run)
                     VALUES ('9999', 'posts', 1, 'x', 'v', 'm', 0, 0, 0, 0, 0.25, 0, 1, 0),
                            ('9999', 'posts', 1, 'x', 'v', 'm', 0, 0, 0, 0, 0.5, 0, 1, 1),
                            ('9999', 'by-hand', 1, 'x', 'v', 'm', 0, 0, 0, 0, NULL, 0, 1, 0),
                            ('{old}', 'posts', 1, 'x', 'v', 'm', 0, 0, 0, 0, 9, 0, 1, 0);
                     INSERT INTO pipeline_runs (pipeline, started_at, finished_at, ok, items_read, items_kept)
                     VALUES ('routine:posts', '9999', '9999', 1, 0, 0),
                            ('routine:posts', '9999', '9999', 0, 0, 0),
                            ('routine:posts', '{old}', '{old}', 1, 0, 0),
                            ('daily-posts', '9999', '9999', 1, 0, 0);
                     INSERT INTO owner_time (at, activity, seconds) VALUES ('9999', 'reported', 120),
                            ('{old}', 'reported', 999);
                     INSERT INTO routine_blocks (routine, run, block, started_at, outcome, dropped)
                     VALUES ('posts', 'r1', 'draft', '9999', 'ok', '[{{\"item\": \"Hack\", \"reason\": \"too long\"}}]'),
                            ('posts', 'r2', 'draft', '9999', 'ok', '[{{\"item\": \"A\", \"reason\": \"too long\"}}, {{\"item\": \"B\", \"reason\": \"not JSON\"}}]'),
                            ('posts', 'r3', 'draft', '9999', 'ok', NULL),
                            ('posts', 'r0', 'draft', '{old}', 'ok', '[{{\"item\": \"Old\", \"reason\": \"too long\"}}]');"
                ))?;
                Ok(())
            })
            .unwrap();
        let week = store.routines_week(since).unwrap();
        let routine =
            |routine: &str, runs, failed, cost_usd, founder_seconds, dropped, approved| {
                RoutineWeek {
                    routine: routine.into(),
                    runs,
                    failed,
                    cost_usd,
                    founder_seconds,
                    late: 0,
                    dropped,
                    approved,
                }
            };
        assert_eq!(
            week,
            RoutinesWeek {
                founder_seconds: 225,
                approved: 2,
                edited: 1,
                rejected: 1,
                model_calls: 2,
                cost_usd: 0.25,
                unpriced: 1,
                actions_done: 2,
                actions_failed: 1,
                incidents: 1,
                routines: vec![
                    routine("by-hand", 0, 0, 0.0, 0, 0, 0),
                    // Approved as it was, and after an edit.
                    routine("posts", 2, 1, 0.25, 100, 3, 2),
                    routine("watch", 0, 0, 0.0, 5, 0, 0),
                ],
                late: vec![],
                // Decision 61: each reason once, with how often.
                dropped: vec![
                    DroppedReason {
                        routine: "posts".into(),
                        reason: "not JSON".into(),
                        count: 1
                    },
                    DroppedReason {
                        routine: "posts".into(),
                        reason: "too long".into(),
                        count: 2
                    },
                ],
            }
        );
    }

    /// Milestone 6's proof: a scheduled run that started past the grace
    /// after its time is counted late, apart; one within it, or run by
    /// hand, is not.
    #[test]
    fn a_run_that_started_late_is_told_apart() {
        use crate::business::store::pipelines::Heartbeat;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let now = Utc::now();
        let mut beat = |routine: &str, due: Option<i64>, started_ago: i64| {
            let started = now - chrono::Duration::minutes(started_ago);
            store
                .record_run(
                    &Heartbeat {
                        pipeline: &format!("routine:{routine}"),
                        due: due.map(|ago| now - chrono::Duration::minutes(ago)),
                        started,
                        finished: started,
                        items_read: 0,
                        items_kept: 0,
                        report: None,
                        notes: "",
                        error: None,
                    },
                    &[],
                )
                .unwrap();
        };
        // Due 3 hours before it started: the Mac was asleep.
        beat("posts", Some(300), 120);
        // Five minutes after its time: slow, not late.
        beat("posts", Some(65), 60);
        // By hand: no time to be late for.
        beat("watch", None, 30);
        let week = store
            .routines_week(now - chrono::Duration::days(7))
            .unwrap();
        assert_eq!(week.late.len(), 1, "{:?}", week.late);
        assert_eq!(week.late[0].routine, "posts");
        assert_eq!(
            (week.late[0].started_at - week.late[0].due_at).num_minutes(),
            180
        );
        let posts = week.routines.iter().find(|r| r.routine == "posts").unwrap();
        assert_eq!((posts.runs, posts.late), (2, 1));
        assert_eq!(
            week.routines
                .iter()
                .find(|r| r.routine == "watch")
                .unwrap()
                .late,
            0
        );
    }
}
