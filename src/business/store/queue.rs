//! The queue: what waits for the founder (decisions 45 and 47). An item is
//! `pending` until the founder approves, edits or rejects it. Every answer
//! is one transaction that also logs it (append-only), records the
//! founder's time with the item and the channel it came from, and sets the
//! outcome of the model call behind it. Answering twice the same way
//! changes nothing; an answer made on an older read is refused.

use super::Store;
use super::time::MAX_ANSWER_SECS;
use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: i64,
    pub created_at: String,
    pub kind: String,
    pub title: String,
    pub agent: Option<String>,
    pub routine: Option<String>,
    pub run: Option<String>,
    pub action: Option<String>,
    pub args: Value,
    pub original_args: Value,
    pub source: Value,
    pub model_call: Option<i64>,
    pub execute_at: Option<String>,
    pub status: String,
    pub revision: i64,
}

/// What the founder decides.
pub enum Answer {
    Approve,
    /// A new text for the field the item says may be edited.
    Edit(String),
    Reject(String),
}

const COLUMNS: &str = "id, created_at, kind, title, agent, routine, run, action, args, \
    original_args, source, model_call, execute_at, status, revision";

fn read(r: &rusqlite::Row) -> rusqlite::Result<Item> {
    let json = |i: usize| -> rusqlite::Result<Value> {
        let text: String = r.get(i)?;
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    };
    Ok(Item {
        id: r.get(0)?,
        created_at: r.get(1)?,
        kind: r.get(2)?,
        title: r.get(3)?,
        agent: r.get(4)?,
        routine: r.get(5)?,
        run: r.get(6)?,
        action: r.get(7)?,
        args: json(8)?,
        original_args: json(9)?,
        source: json(10)?,
        model_call: r.get(11)?,
        execute_at: r.get(12)?,
        status: r.get(13)?,
        revision: r.get(14)?,
    })
}

fn get(tx: &Transaction, id: i64) -> Result<Item> {
    tx.query_row(
        &format!("SELECT {COLUMNS} FROM queue WHERE id = ?1"),
        params![id],
        read,
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("no item {id} in the queue"))
}

fn log(
    tx: &Transaction,
    id: i64,
    action: &str,
    by: &str,
    channel: Option<&str>,
    detail: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO queue_log (item, at, action, by, channel, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, Utc::now().to_rfc3339(), action, by, channel, detail],
    )?;
    Ok(())
}

/// `value` with `text` at the dotted `path`.
fn set_at(value: &mut Value, path: &str, text: &str) -> Result<()> {
    let mut at = value;
    for key in path.split('.') {
        at = at
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("nothing to edit at {path}"))?;
    }
    *at = Value::String(text.to_string());
    Ok(())
}

impl Store {
    /// The items with one of `statuses`, oldest first.
    pub fn queue(&self, statuses: &[&str]) -> Result<Vec<Item>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLUMNS} FROM queue ORDER BY id"))?;
        let all = stmt
            .query_map([], read)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(all
            .into_iter()
            .filter(|i| statuses.contains(&i.status.as_str()))
            .collect())
    }

    /// The founder answers item `id`, from `channel`, after `seconds`.
    pub fn answer(
        &mut self,
        id: i64,
        revision: Option<i64>,
        answer: Answer,
        channel: &str,
        seconds: u64,
    ) -> Result<Item> {
        self.write(|tx| {
            let item = get(tx, id)?;
            let done = matches!(
                (&answer, item.status.as_str()),
                (Answer::Approve, "approved" | "done") | (Answer::Reject(_), "rejected")
            );
            if done {
                return Ok(item);
            }
            if item.status != "pending" {
                bail!(
                    "item {id} is {}: it cannot be answered any more",
                    item.status
                );
            }
            if revision.is_some_and(|r| r != item.revision) {
                bail!("item {id} changed since you read it: reload it");
            }
            let mut args = item.args.clone();
            let (next, event, detail) = match &answer {
                Answer::Approve => ("approved", "approved", String::new()),
                Answer::Reject(why) => ("rejected", "rejected", why.clone()),
                Answer::Edit(text) => {
                    let Some(path) = item.source["editable"].as_str() else {
                        bail!("item {id} has nothing the founder may edit");
                    };
                    if text.trim().is_empty() {
                        bail!("an edit needs text");
                    }
                    set_at(&mut args, path, text)?;
                    ("pending", "edited", String::new())
                }
            };
            tx.execute(
                "UPDATE queue SET status = ?2, args = ?3, revision = revision + 1 WHERE id = ?1",
                params![id, next, args.to_string()],
            )?;
            log(tx, id, event, "founder", Some(channel), &detail)?;
            tx.execute(
                "INSERT INTO owner_time (at, activity, seconds, note, item, channel)
                 VALUES (?1, 'answer', ?2, ?3, ?4, ?5)",
                params![
                    Utc::now().to_rfc3339(),
                    seconds.min(MAX_ANSWER_SECS) as i64,
                    format!("{event} {}", item.title),
                    id,
                    channel
                ],
            )?;
            // The model call's outcome, for evaluating its prompt.
            let outcome = match next {
                "approved" if args != item.original_args => Some("edited"),
                "approved" | "rejected" => Some(next),
                _ => None,
            };
            if let (Some(outcome), Some(call)) = (outcome, item.model_call) {
                tx.execute(
                    "UPDATE model_calls SET outcome = ?2 WHERE id = ?1",
                    params![call, outcome],
                )?;
            }
            get(tx, id)
        })
    }
}

#[cfg(test)]
mod tests;
