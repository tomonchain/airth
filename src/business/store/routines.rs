//! Each block's execution in a routine run (decision 49, migration 10): one
//! row per state, so a run that waits for the founder and resumes leaves its
//! whole story. Append-only.

use super::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::Serialize;

/// A block that ran, as the engine records it.
pub struct BlockRun<'a> {
    pub routine: &'a str,
    pub run: &'a str,
    pub block: &'a str,
    pub started_at: DateTime<Utc>,
    /// `ok`, `failed` or `waiting` (an approval asked the founder).
    pub outcome: &'a str,
    pub items_in: usize,
    pub items_out: usize,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
    /// The items it dropped, as JSON: each one's title and why (decision 61).
    pub dropped: Option<String>,
}

/// A block's record, as the app shows it.
#[derive(Debug, Serialize)]
pub struct BlockRow {
    pub block: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub outcome: String,
    pub items_in: i64,
    pub items_out: i64,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
}

impl Store {
    pub fn record_block(&mut self, b: &BlockRun) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "INSERT INTO routine_blocks (routine, run, block, started_at, finished_at, outcome, items_in, items_out, cost_usd, error, dropped)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![b.routine, b.run, b.block, b.started_at.to_rfc3339(), Utc::now().to_rfc3339(),
                    b.outcome, b.items_in as i64, b.items_out as i64, b.cost_usd, b.error, b.dropped],
            )?;
            Ok(())
        })
    }

    /// The blocks of `routine`'s run `run`, in the order they ran.
    pub fn run_blocks(&self, routine: &str, run: &str) -> Result<Vec<BlockRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT block, started_at, finished_at, outcome, items_in, items_out, cost_usd, error
             FROM routine_blocks WHERE routine = ?1 AND run = ?2 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![routine, run], |r| {
            Ok(BlockRow {
                block: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                outcome: r.get(3)?,
                items_in: r.get(4)?,
                items_out: r.get(5)?,
                cost_usd: r.get(6)?,
                error: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
