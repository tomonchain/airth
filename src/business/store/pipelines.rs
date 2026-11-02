//! Pipelines' state: when each runs next, each run's heartbeat and what it
//! kept, and every model call a pipeline makes (decisions 31 and 34).

use super::Store;
#[cfg(test)]
use super::status::RunRow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use std::collections::HashSet;

/// A scheduled pipeline whose time has come, claimed for this run.
#[derive(Debug, PartialEq)]
pub struct Claimed {
    /// The time it was due.
    pub due_at: DateTime<Utc>,
    /// When the run before this one started, and whether it left a heartbeat.
    pub last_start: Option<(DateTime<Utc>, bool)>,
}

/// One run's heartbeat, as it is recorded.
pub struct Heartbeat<'a> {
    pub pipeline: &'a str,
    /// When its schedule said it should run; None for a run by hand.
    pub due: Option<DateTime<Utc>>,
    pub started: DateTime<Utc>,
    pub finished: DateTime<Utc>,
    pub items_read: usize,
    pub items_kept: usize,
    pub report: Option<&'a str>,
    pub notes: &'a str,
    pub error: Option<&'a str>,
}

/// A model call, as it is recorded.
pub struct NewCall<'a> {
    pub pipeline: &'a str,
    pub step: usize,
    pub prompt: &'a str,
    pub prompt_version: &'a str,
    pub model: &'a str,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: Option<f64>,
    pub latency_ms: i64,
    pub error: Option<&'a str>,
    pub answer: &'a str,
    /// Made by a dry run: kept apart from statistics and evidence.
    pub dry_run: bool,
}

/// A recorded model call, as tests read it back.
#[cfg(test)]
#[derive(Debug, PartialEq)]
pub struct ModelCall {
    pub id: i64,
    pub pipeline: String,
    pub prompt_version: String,
    pub model: String,
    pub input_tokens: i64,
    pub cost_usd: Option<f64>,
    pub valid: bool,
    pub error: Option<String>,
    pub answer: Option<String>,
    pub dry_run: bool,
    pub outcome: Option<String>,
}

fn parse(at: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(at)?.with_timezone(&Utc))
}

impl Store {
    /// Claim `pipeline` if its time has come by `now`, and set its next time
    /// to `next`, in one transaction, so two ticks never both start it. A
    /// pipeline seen for the first time is armed for `next`, not run.
    pub fn claim_schedule(
        &mut self,
        pipeline: &str,
        next: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<Claimed>> {
        self.write(|tx| {
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
                    params![pipeline, next.to_rfc3339()],
                )?;
                return Ok(None);
            };
            let due_at = parse(&next_at)?;
            if due_at > now {
                return Ok(None);
            }
            let last_start = match started_at {
                Some(at) => {
                    let at = parse(&at)?;
                    let beats: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM pipeline_runs WHERE pipeline = ?1 AND started_at >= ?2",
                        params![pipeline, at.to_rfc3339()],
                        |r| r.get(0),
                    )?;
                    Some((at, beats > 0))
                }
                None => None,
            };
            tx.execute(
                "UPDATE pipeline_schedule SET next_at = ?2, started_at = ?3 WHERE pipeline = ?1",
                params![pipeline, next.to_rfc3339(), now.to_rfc3339()],
            )?;
            Ok(Some(Claimed { due_at, last_start }))
        })
    }

    /// Which of `keys` an earlier run of `pipeline` already kept.
    pub fn seen(&self, pipeline: &str, keys: &[String]) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM pipeline_seen WHERE pipeline = ?1 AND key = ?2")?;
        let mut out = HashSet::new();
        for key in keys {
            if stmt.exists(params![pipeline, key])? {
                out.insert(key.clone());
            }
        }
        Ok(out)
    }

    /// Remember `keys` as kept by `pipeline`, so later runs skip them.
    pub fn mark_seen(&mut self, pipeline: &str, keys: &[String]) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.write(|tx| {
            for key in keys {
                tx.execute(
                    "INSERT OR IGNORE INTO pipeline_seen (pipeline, key, first_seen) VALUES (?1, ?2, ?3)",
                    params![pipeline, key, now],
                )?;
            }
            Ok(())
        })
    }

    /// One run's heartbeat, and what it saw, in one transaction: a run that
    /// fails to record leaves its items to the next one.
    pub fn record_run(&mut self, beat: &Heartbeat, seen: &[String]) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "INSERT INTO pipeline_runs (pipeline, started_at, finished_at, ok, items_read, items_kept, report, notes, error, due_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    beat.pipeline,
                    beat.started.to_rfc3339(),
                    beat.finished.to_rfc3339(),
                    beat.error.is_none(),
                    beat.items_read as i64,
                    beat.items_kept as i64,
                    beat.report,
                    beat.notes,
                    beat.error,
                    beat.due.map(|d| d.to_rfc3339()),
                ],
            )?;
            for key in seen {
                tx.execute(
                    "INSERT OR IGNORE INTO pipeline_seen (pipeline, key, first_seen) VALUES (?1, ?2, ?3)",
                    params![beat.pipeline, key, beat.finished.to_rfc3339()],
                )?;
            }
            Ok(())
        })
    }

    /// Record a model call; its id.
    pub fn record_call(&mut self, call: &NewCall) -> Result<i64> {
        self.write(|tx| {
            tx.execute(
                "INSERT INTO model_calls (at, pipeline, step, prompt, prompt_version, model, input_tokens, output_tokens,
                    cache_read_tokens, cache_write_tokens, cost_usd, latency_ms, valid, error, answer, dry_run)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    Utc::now().to_rfc3339(),
                    call.pipeline,
                    call.step as i64,
                    call.prompt,
                    call.prompt_version,
                    call.model,
                    call.input_tokens as i64,
                    call.output_tokens as i64,
                    call.cache_read_tokens as i64,
                    call.cache_write_tokens as i64,
                    call.cost_usd,
                    call.latency_ms,
                    call.error.is_none(),
                    call.error,
                    call.answer,
                    call.dry_run,
                ],
            )?;
            Ok(tx.last_insert_rowid())
        })
    }

    /// Every pipeline's heartbeats, oldest first, with the pipeline's name.
    #[cfg(test)]
    pub fn heartbeats(&self) -> Result<Vec<(String, RunRow)>> {
        let mut stmt = self.conn.prepare(
            "SELECT pipeline, started_at, finished_at, ok, items_read, items_kept, report, notes, error, due_at
             FROM pipeline_runs ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                RunRow {
                    started_at: r.get(1)?,
                    finished_at: r.get(2)?,
                    ok: r.get(3)?,
                    items_read: r.get(4)?,
                    items_kept: r.get(5)?,
                    report: r.get(6)?,
                    notes: r.get(7)?,
                    error: r.get(8)?,
                    due_at: r.get(9)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every model call, oldest first.
    #[cfg(test)]
    pub fn model_calls(&self) -> Result<Vec<ModelCall>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, pipeline, prompt_version, model, input_tokens, cost_usd, valid, error, answer, dry_run, outcome
             FROM model_calls ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ModelCall {
                id: r.get(0)?,
                pipeline: r.get(1)?,
                prompt_version: r.get(2)?,
                model: r.get(3)?,
                input_tokens: r.get(4)?,
                cost_usd: r.get(5)?,
                valid: r.get(6)?,
                error: r.get(7)?,
                answer: r.get(8)?,
                dry_run: r.get(9)?,
                outcome: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
