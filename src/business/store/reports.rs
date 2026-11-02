//! Reports (decision 59): what an `act airth.report` gathers in one run and
//! block becomes one item of the queue, of kind `report`, which the founder
//! marks Seen. Each entry is journaled under its own act's key in the same
//! transaction as the item, so a retried run adds nothing twice and an entry
//! is never journaled without its report.

use super::Store;
use super::queue::log;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How long a report waits to be seen before it leaves Needs you.
pub const KEPT_DAYS: i64 = 7;

pub struct NewReport<'a> {
    pub title: &'a str,
    pub agent: Option<&'a str>,
    pub routine: &'a str,
    pub run: &'a str,
    /// Each entry's act key and fields, in the order the items came.
    pub entries: &'a [(String, BTreeMap<String, String>)],
}

/// One entry as the founder reads it: a lone field is its value, several
/// are one per line.
fn entry_text(fields: &BTreeMap<String, String>) -> String {
    match fields.values().collect::<Vec<_>>().as_slice() {
        [one] => one.to_string(),
        _ => fields
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

impl Store {
    /// The report of these entries, without those already reported; None
    /// when every one was.
    pub fn add_report(&mut self, r: &NewReport) -> Result<Option<i64>> {
        self.write(|tx| {
            let now = Utc::now().to_rfc3339();
            let mut new = Vec::new();
            for (key, fields) in r.entries {
                let added = tx.execute(
                    "INSERT OR IGNORE INTO actions (key, action, connector, agent, routine, run, level, approved, args, status, attempts, started_at, finished_at)
                     VALUES (?1, 'airth.report', 'airth', ?2, ?3, ?4, 3, 0, ?5, 'confirmed', 1, ?6, ?6)",
                    params![key, r.agent, r.routine, r.run, json!(fields).to_string(), now],
                )?;
                if added == 1 {
                    let action = tx.last_insert_rowid();
                    tx.execute(
                        "INSERT INTO action_log (action, at, status) VALUES (?1, ?2, 'confirmed')",
                        params![action, now],
                    )?;
                    new.push((action, fields));
                }
            }
            if new.is_empty() {
                return Ok(None);
            }
            let text = new
                .iter()
                .map(|(_, f)| entry_text(f))
                .collect::<Vec<_>>()
                .join("\n\n");
            let args = json!({
                "text": text,
                "entries": new.iter().map(|(_, f)| json!(f)).collect::<Vec<Value>>(),
            })
            .to_string();
            tx.execute(
                "INSERT INTO queue (created_at, kind, title, agent, routine, run, args, original_args, source, status)
                 VALUES (?1, 'report', ?2, ?3, ?4, ?5, ?6, ?6, ?7, 'pending')",
                params![now, r.title, r.agent, r.routine, r.run, args, json!({"show": "text"}).to_string()],
            )?;
            let id = tx.last_insert_rowid();
            log(tx, id, "proposed", "airth", None, "")?;
            for (action, _) in &new {
                tx.execute(
                    "UPDATE actions SET item = ?2 WHERE id = ?1",
                    params![action, id],
                )?;
            }
            Ok(Some(id))
        })
    }

    /// Reports created before `before` and still not seen leave Needs you,
    /// marked expired; how many.
    pub fn expire_reports(&mut self, before: DateTime<Utc>) -> Result<usize> {
        self.write(|tx| {
            let ids: Vec<i64> = tx
                .prepare(
                    "SELECT id FROM queue WHERE kind = 'report' AND status = 'pending' AND created_at < ?1",
                )?
                .query_map(params![before.to_rfc3339()], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for id in &ids {
                tx.execute(
                    "UPDATE queue SET status = 'expired', revision = revision + 1 WHERE id = ?1",
                    params![id],
                )?;
                log(tx, *id, "expired", "airth", None, "not seen within a week")?;
            }
            Ok(ids.len())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;

    fn entry(key: &str, line: &str) -> (String, BTreeMap<String, String>) {
        let fields = BTreeMap::from([("line".to_string(), line.to_string())]);
        (key.to_string(), fields)
    }

    /// Decision 59: one report for a run's entries, each entry journaled
    /// once under its act's key: a retried run adds only what is new, and
    /// nothing at all when everything was reported.
    #[test]
    fn a_report_gathers_its_entries_and_never_reports_one_twice() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        fn report(entries: &[(String, BTreeMap<String, String>)]) -> NewReport<'_> {
            NewReport {
                title: "Daily watch",
                agent: Some("watcher"),
                routine: "watch",
                run: "r1",
                entries,
            }
        }
        let first = [
            entry("watch/a/report", "ETH left"),
            entry("watch/b/report", "BTC up"),
        ];
        let id = store.add_report(&report(&first)).unwrap().unwrap();
        let items = store.queue(&["pending"]).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);
        assert_eq!(items[0].kind, "report");
        assert_eq!(items[0].args["text"], "ETH left\n\nBTC up");
        assert_eq!(items[0].source["show"], "text");
        assert_eq!(store.add_report(&report(&first)).unwrap(), None);
        let again = [
            entry("watch/b/report", "BTC up, drafted again"),
            entry("watch/c/report", "SOL"),
        ];
        let second = store.add_report(&report(&again)).unwrap().unwrap();
        let items = store.queue(&["pending"]).unwrap();
        let second = items.iter().find(|i| i.id == second).unwrap();
        assert_eq!(second.args["text"], "SOL");
        let journal = store.journal().unwrap();
        assert_eq!(journal.len(), 3);
        assert!(
            journal
                .iter()
                .all(|(_, status, approved)| status == "confirmed" && !approved)
        );
    }

    /// Several fields read one per line; a report not seen in a week
    /// leaves Needs you, logged.
    #[test]
    fn a_report_reads_field_by_field_and_expires_unseen() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let fields = BTreeMap::from([
            ("line".to_string(), "ETH left".to_string()),
            ("why".to_string(), "flows".to_string()),
        ]);
        let entries = [("w/a/r".to_string(), fields)];
        let id = store
            .add_report(&NewReport {
                title: "Watch",
                agent: None,
                routine: "w",
                run: "r1",
                entries: &entries,
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            store.queue(&["pending"]).unwrap()[0].args["text"],
            "line: ETH left\nwhy: flows"
        );
        assert_eq!(
            store
                .expire_reports(Utc::now() - chrono::Duration::days(1))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .expire_reports(Utc::now() + chrono::Duration::seconds(1))
                .unwrap(),
            1
        );
        assert!(store.queue(&["pending"]).unwrap().is_empty());
        let log = store.queue_log(id).unwrap();
        assert!(
            log.iter()
                .any(|(action, by, _)| action == "expired" && by == "airth"),
            "{log:?}"
        );
    }
}
