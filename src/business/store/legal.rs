//! The legal calendar's records (decision 66): whether a deadline's item
//! was proposed already, by its key, and the filings the founder recorded,
//! which are evidence.

use super::Store;
use super::queue::{get, log};
use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::params;

impl Store {
    /// Whether an item for this deadline and reminder was ever proposed: a
    /// reminder comes once, whatever became of it.
    pub fn deadline_proposed(&self, key: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM queue WHERE kind = 'deadline' AND json_extract(source, '$.key') = ?1)",
            [key],
            |r| r.get(0),
        )?)
    }

    /// The founder filed what deadline item `id` is about: recorded once,
    /// and the other items still pending for that obligation and period
    /// close, so nothing more comes for it.
    pub fn record_filing(&mut self, id: i64) -> Result<()> {
        self.write(|tx| {
            let item = get(tx, id)?;
            let (Some(rule), Some(period_end), "deadline") = (
                item.source["rule"].as_str(),
                item.source["period_end"].as_str(),
                item.kind.as_str(),
            ) else {
                bail!("item {id} is not a legal deadline");
            };
            tx.execute(
                "INSERT OR IGNORE INTO legal_filings (rule, period_end, filed_at, item) VALUES (?1, ?2, ?3, ?4)",
                params![rule, period_end, Utc::now().to_rfc3339(), id],
            )?;
            let others: Vec<i64> = tx
                .prepare(
                    "SELECT id FROM queue WHERE kind = 'deadline' AND status = 'pending' AND id != ?1
                     AND json_extract(source, '$.rule') = ?2 AND json_extract(source, '$.period_end') = ?3",
                )?
                .query_map(params![id, rule, period_end], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for other in others {
                tx.execute(
                    "UPDATE queue SET status = 'done', revision = revision + 1 WHERE id = ?1",
                    [other],
                )?;
                log(tx, other, "done", "airth", None, "filed: answered on another reminder")?;
            }
            Ok(())
        })
    }

    /// What the founder filed: each obligation and the end of its period.
    pub fn filings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT rule, period_end FROM legal_filings ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
