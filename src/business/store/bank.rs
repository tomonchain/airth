//! The bank statements the founder imports (decision 123): each line kept
//! once, by its id, however often a statement covering it is imported.

use super::Store;
use crate::statement::Line;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

/// What an import changed.
#[derive(Debug, PartialEq)]
pub struct Imported {
    pub added: usize,
    pub known: usize,
}

impl Store {
    /// Keep the statement's lines, oldest first, those already kept left
    /// as they are.
    pub fn import_bank_lines(&mut self, lines: &[Line], now: DateTime<Utc>) -> Result<Imported> {
        self.write(|tx| {
            let mut seq: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0) FROM bank_lines", [], |r| {
                r.get(0)
            })?;
            let mut added = 0;
            for line in lines {
                seq += 1;
                added += tx.execute(
                    "INSERT OR IGNORE INTO bank_lines
                     (id, account, currency, at, seq, kind, description, amount, balance, imported_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        line.id,
                        line.account,
                        line.currency,
                        line.at,
                        seq,
                        line.kind,
                        line.description,
                        line.amount,
                        line.balance,
                        now.to_rfc3339()
                    ],
                )?;
            }
            Ok(Imported {
                added,
                known: lines.len() - added,
            })
        })
    }

    /// Every line kept, in time order.
    pub fn bank_lines(&self) -> Result<Vec<Line>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, account, currency, at, kind, description, amount, balance
             FROM bank_lines ORDER BY at, seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Line {
                id: r.get(0)?,
                account: r.get(1)?,
                currency: r.get(2)?,
                at: r.get(3)?,
                kind: r.get(4)?,
                description: r.get(5)?,
                amount: r.get(6)?,
                balance: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// When a statement was last imported, if ever.
    pub fn bank_imported_at(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT MAX(imported_at) FROM bank_lines", [], |r| r.get(0))
            .optional()?
            .flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, at: &str, amount: f64, balance: f64) -> Line {
        Line {
            id: id.into(),
            account: "Main".into(),
            currency: "GBP".into(),
            at: at.into(),
            kind: "card_payment".into(),
            description: "OVH".into(),
            amount,
            balance: Some(balance),
        }
    }

    #[test]
    fn a_line_imported_twice_is_kept_once_and_lines_come_back_in_time_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("business.db")).unwrap();
        assert_eq!(store.bank_imported_at().unwrap(), None);
        let now = Utc::now();
        let first = [
            line("a", "2026-10-01T10:00:00", -10.0, 90.0),
            line("b", "2026-10-02T10:00:00", -5.0, 85.0),
        ];
        assert_eq!(
            store.import_bank_lines(&first, now).unwrap(),
            Imported { added: 2, known: 0 }
        );
        // A later statement overlaps the first, and has a line of the same
        // second after the one already kept.
        let second = [
            line("b", "2026-10-02T10:00:00", -5.0, 85.0),
            line("c", "2026-10-02T10:00:00", -1.0, 84.0),
            line("d", "2026-09-30T10:00:00", 100.0, 100.0),
        ];
        assert_eq!(
            store.import_bank_lines(&second, now).unwrap(),
            Imported { added: 2, known: 1 }
        );
        let ids: Vec<String> = store
            .bank_lines()
            .unwrap()
            .into_iter()
            .map(|l| l.id)
            .collect();
        assert_eq!(ids, ["d", "a", "b", "c"]);
        assert!(store.bank_imported_at().unwrap().is_some());
    }
}
