//! What the owner reads about the business: each pipeline's recent runs and
//! next time, and the week in numbers. Reads only.

use super::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

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
             FROM model_calls WHERE at >= ?1 GROUP BY pipeline
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

    /// The latest runs of `pipeline`, newest first.
    pub fn pipeline_runs(&self, pipeline: &str, limit: usize) -> Result<Vec<RunRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at, finished_at, ok, items_read, items_kept, report, notes, error
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
            model_calls: one("SELECT COUNT(*) FROM model_calls WHERE at >= ?1")?,
            model_cost: self.conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM model_calls WHERE at >= ?1",
                params![since],
                |r| r.get(0),
            )?,
            unpriced_calls: one(
                "SELECT COUNT(*) FROM model_calls WHERE at >= ?1 AND cost_usd IS NULL",
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
}
