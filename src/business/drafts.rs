//! The draft queue (pilot step 3): what a pipeline drafted waits here for
//! the owner. A draft is `pending` until the owner approves, edits or
//! rejects it; an approved draft becomes `published` once it went out.
//! Every move is one transaction that also logs it (append-only), records
//! the owner's time, and sets the outcome of the model call that drafted
//! it. Moves are idempotent: approving twice approves once, and nothing is
//! published twice.

use super::store::Store;
use super::time::MAX_ANSWER_SECS;
use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Draft {
    pub id: i64,
    pub created_at: String,
    pub pipeline: String,
    /// What the business calls it: post, thread, reply…
    pub kind: String,
    pub text: String,
    /// As drafted, before the owner's edits.
    pub original: String,
    /// The item it was drafted from.
    pub source: Value,
    pub model_call: Option<i64>,
    pub status: String,
    /// Bumped by every move: a move made on an older read is refused.
    pub revision: i64,
}

/// A new draft, from a pipeline.
pub struct New<'a> {
    pub pipeline: &'a str,
    pub kind: &'a str,
    pub text: &'a str,
    pub source: &'a Value,
    pub model_call: Option<i64>,
}

/// What the owner decides, and how long it took them.
pub enum Move {
    Approve,
    Edit(String),
    Reject(String),
    /// It went out (by the owner, or through an integration).
    Published,
}

const COLUMNS: &str =
    "id, created_at, pipeline, kind, text, original, source, model_call, status, revision";

fn read(r: &rusqlite::Row) -> rusqlite::Result<Draft> {
    let source: String = r.get(6)?;
    Ok(Draft {
        id: r.get(0)?,
        created_at: r.get(1)?,
        pipeline: r.get(2)?,
        kind: r.get(3)?,
        text: r.get(4)?,
        original: r.get(5)?,
        source: serde_json::from_str(&source).unwrap_or(Value::Null),
        model_call: r.get(7)?,
        status: r.get(8)?,
        revision: r.get(9)?,
    })
}

fn get(tx: &Transaction, id: i64) -> Result<Draft> {
    tx.query_row(
        &format!("SELECT {COLUMNS} FROM drafts WHERE id = ?1"),
        params![id],
        read,
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("no draft {id}"))
}

impl Store {
    pub fn add_draft(&mut self, new: &New) -> Result<i64> {
        if new.text.trim().is_empty() {
            bail!("a draft needs text");
        }
        self.write(|tx| {
            tx.execute(
                "INSERT INTO drafts (created_at, pipeline, kind, text, original, source, model_call, status)
                 VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, 'pending')",
                params![
                    Utc::now().to_rfc3339(),
                    new.pipeline,
                    new.kind,
                    new.text,
                    new.source.to_string(),
                    new.model_call
                ],
            )?;
            let id = tx.last_insert_rowid();
            log(tx, id, "drafted", "")?;
            Ok(id)
        })
    }

    /// Drafts with one of `statuses` (all when empty), oldest first.
    pub fn drafts(&self, statuses: &[&str]) -> Result<Vec<Draft>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLUMNS} FROM drafts ORDER BY id"))?;
        let all = stmt
            .query_map([], read)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(all
            .into_iter()
            .filter(|d| statuses.is_empty() || statuses.contains(&d.status.as_str()))
            .collect())
    }

    /// Apply the owner's move to draft `id`, read at `revision` (None: the
    /// latest). `seconds` is how long the owner spent on it.
    pub fn move_draft(
        &mut self,
        id: i64,
        revision: Option<i64>,
        action: Move,
        seconds: u64,
    ) -> Result<Draft> {
        self.write(|tx| {
            let draft = get(tx, id)?;
            let status = draft.status.as_str();
            // Asking again for what is already done changes nothing.
            let done = matches!(
                (&action, status),
                (Move::Approve, "approved" | "published")
                    | (Move::Reject(_), "rejected")
                    | (Move::Published, "published")
            );
            if done {
                return Ok(draft);
            }
            if revision.is_some_and(|r| r != draft.revision) {
                bail!("draft {id} changed since you read it: reload it");
            }
            let (next, text, event, detail) = match (&action, status) {
                (Move::Approve, "pending") => ("approved", &draft.text, "approved", String::new()),
                (Move::Edit(text), "pending") => {
                    if text.trim().is_empty() {
                        bail!("a draft needs text");
                    }
                    ("pending", text, "edited", String::new())
                }
                (Move::Reject(why), "pending") => ("rejected", &draft.text, "rejected", why.clone()),
                (Move::Published, "approved") => ("published", &draft.text, "published", String::new()),
                _ => bail!("draft {id} is {status}: it cannot be {}", verb(&action)),
            };
            tx.execute(
                "UPDATE drafts SET status = ?2, text = ?3, revision = revision + 1 WHERE id = ?1",
                params![id, next, text],
            )?;
            log(tx, id, event, &detail)?;
            if !matches!(action, Move::Published) {
                tx.execute(
                    "INSERT INTO owner_time (at, activity, seconds, note) VALUES (?1, 'draft', ?2, ?3)",
                    params![
                        Utc::now().to_rfc3339(),
                        seconds.min(MAX_ANSWER_SECS) as i64,
                        format!("{event} draft {id}")
                    ],
                )?;
            }
            // The model call's outcome, for evaluating its prompt.
            let outcome = match next {
                "approved" if *text != draft.original => Some("edited"),
                "approved" | "rejected" => Some(next),
                _ => None,
            };
            if let (Some(outcome), Some(call)) = (outcome, draft.model_call) {
                tx.execute(
                    "UPDATE model_calls SET outcome = ?2 WHERE id = ?1",
                    params![call, outcome],
                )?;
            }
            get(tx, id)
        })
    }
}

fn verb(action: &Move) -> &'static str {
    match action {
        Move::Approve => "approved",
        Move::Edit(_) => "edited",
        Move::Reject(_) => "rejected",
        Move::Published => "published",
    }
}

fn log(tx: &Transaction, draft: i64, action: &str, detail: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO draft_log (draft, at, action, detail) VALUES (?1, ?2, ?3, ?4)",
        params![draft, Utc::now().to_rfc3339(), action, detail],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;
    use serde_json::json;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join(FILE)).unwrap();
        (dir, store)
    }

    fn add(store: &mut Store, call: Option<i64>) -> i64 {
        store
            .add_draft(&New {
                pipeline: "production",
                kind: "post",
                text: "ETH left the queue.",
                source: &json!({"link": "https://e.com/1"}),
                model_call: call,
            })
            .unwrap()
    }

    fn call(store: &mut Store) -> i64 {
        store
            .write(|tx| {
                tx.execute(
                    "INSERT INTO model_calls (at, pipeline, step, prompt, prompt_version, model, input_tokens,
                        output_tokens, cache_read_tokens, cache_write_tokens, latency_ms, valid)
                     VALUES ('t', 'production', 2, 'p.md', 'v1', 'm', 0, 0, 0, 0, 0, 1)",
                    [],
                )?;
                Ok(tx.last_insert_rowid())
            })
            .unwrap()
    }

    fn outcome(store: &Store, call: i64) -> Option<String> {
        store
            .conn
            .query_row(
                "SELECT outcome FROM model_calls WHERE id = ?1",
                [call],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn logged(store: &Store, id: i64) -> Vec<String> {
        let mut stmt = store
            .conn
            .prepare("SELECT action FROM draft_log WHERE draft = ?1 ORDER BY id")
            .unwrap();
        stmt.query_map([id], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn approve_then_publish_once() {
        let (_dir, mut store) = store();
        let c = call(&mut store);
        let id = add(&mut store, Some(c));
        assert_eq!(store.drafts(&["pending"]).unwrap().len(), 1);
        let d = store.move_draft(id, Some(1), Move::Approve, 30).unwrap();
        assert_eq!((d.status.as_str(), d.revision), ("approved", 2));
        assert_eq!(outcome(&store, c).as_deref(), Some("approved"));
        // A retried approval, or one from an older read, changes nothing.
        let again = store.move_draft(id, Some(1), Move::Approve, 30).unwrap();
        assert_eq!(again.revision, 2);
        store.move_draft(id, None, Move::Published, 0).unwrap();
        let twice = store.move_draft(id, None, Move::Published, 0).unwrap();
        assert_eq!((twice.status.as_str(), twice.revision), ("published", 3));
        assert_eq!(logged(&store, id), vec!["drafted", "approved", "published"]);
        // Too late to reject.
        assert!(
            store
                .move_draft(id, None, Move::Reject("no".into()), 5)
                .is_err()
        );
    }

    #[test]
    fn an_edited_draft_counts_as_edited_and_keeps_its_original() {
        let (_dir, mut store) = store();
        let c = call(&mut store);
        let id = add(&mut store, Some(c));
        let d = store
            .move_draft(
                id,
                Some(1),
                Move::Edit("ETH left the staking queue.".into()),
                90,
            )
            .unwrap();
        assert_eq!(d.status, "pending");
        assert_eq!(d.original, "ETH left the queue.");
        // The edit bumped the revision: a move made on the first read is refused.
        let stale = store.move_draft(id, Some(1), Move::Approve, 5).unwrap_err();
        assert!(format!("{stale:#}").contains("changed since you read it"));
        store.move_draft(id, Some(2), Move::Approve, 5).unwrap();
        assert_eq!(outcome(&store, c).as_deref(), Some("edited"));
        assert!(
            store
                .move_draft(id, None, Move::Edit("late".into()), 5)
                .is_err()
        );
        assert!(
            store
                .move_draft(id, Some(3), Move::Edit(" ".into()), 5)
                .is_err()
        );
    }

    #[test]
    fn rejecting_is_final_and_every_decision_costs_owner_time() {
        let (_dir, mut store) = store();
        let c = call(&mut store);
        let id = add(&mut store, Some(c));
        store
            .move_draft(id, None, Move::Reject("off topic".into()), 4000)
            .unwrap();
        store
            .move_draft(id, None, Move::Reject("again".into()), 10)
            .unwrap();
        assert_eq!(outcome(&store, c).as_deref(), Some("rejected"));
        assert!(store.move_draft(id, None, Move::Approve, 5).is_err());
        assert!(store.move_draft(id, None, Move::Published, 0).is_err());
        let time = store
            .time_since(chrono::Utc::now() - chrono::Duration::hours(1))
            .unwrap();
        assert_eq!(time.len(), 1, "a repeated reject is not counted twice");
        assert_eq!((time[0].activity.as_str(), time[0].seconds), ("draft", 600));
        assert!(store.move_draft(99, None, Move::Approve, 5).is_err());
        assert!(
            store
                .write(|tx| Ok(tx.execute("DELETE FROM draft_log", [])?))
                .is_err()
        );
    }
}
