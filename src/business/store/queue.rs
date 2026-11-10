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
    /// The quality flag a check set (decision 58): `note`, `blocking`, or,
    /// once the founder dealt with a blocking one, `lifted` or `overridden`.
    pub flag: Option<String>,
    pub flag_reason: Option<String>,
    /// One line on what the same run dropped (decision 61).
    pub note: Option<String>,
}

/// What the founder decides.
pub enum Answer {
    Approve,
    /// A new text for the field the item says may be edited.
    Edit(String),
    Reject(String),
    /// Approve a blocked item as it is (decision 58).
    Override,
    /// Do again the failed action an incident is about (decision 62);
    /// `checked`: the founder checked nothing was done, which an ambiguous
    /// failure needs first.
    Retry {
        checked: bool,
    },
}

/// The channels the founder answers from, the only ones that may override
/// a blocking flag (decision 58).
pub const FOUNDER_CHANNELS: &[&str] = &["app", "telegram"];

/// Rejection reasons that say nothing about the draft: Telegram's default,
/// a batch let go, none at all.
const SILENT: [&str; 3] = ["", "from Telegram", "let go with its batch"];

/// One thing the founder said about a draft (decision 116).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feedback {
    pub item: i64,
    pub at: String,
    /// `edit`, `reject` or `redo`.
    pub kind: &'static str,
    /// The draft as the agent wrote it.
    pub draft: String,
    /// The founder's words: why rejected, or what to change.
    pub said: String,
    /// The draft as the founder approved it, after their edit.
    pub edited: String,
}

const COLUMNS: &str = "id, created_at, kind, title, agent, routine, run, action, args, \
    original_args, source, model_call, execute_at, status, revision, flag, flag_reason, note";

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
        flag: r.get(15)?,
        flag_reason: r.get(16)?,
        note: r.get(17)?,
    })
}

pub(super) fn get(tx: &Transaction, id: i64) -> Result<Item> {
    tx.query_row(
        &format!("SELECT {COLUMNS} FROM queue WHERE id = ?1"),
        params![id],
        read,
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("no item {id} in the queue"))
}

pub(super) fn log(
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
        // Only the statuses asked for are read, through `queue_by_status`:
        // the history grows, what is pending stays small (decision 110).
        let marks = vec!["?"; statuses.len()].join(", ");
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM queue WHERE status IN ({marks}) ORDER BY id"
        ))?;
        Ok(stmt
            .query_map(rusqlite::params_from_iter(statuses), read)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// What the founder said about routine `routine`'s drafts since `since`,
    /// newest first, at most `limit` (decision 116): each draft they edited
    /// before approving, rejected with a reason, or asked to redo. Bare
    /// rejections, batches let go and expired drafts say nothing, and are
    /// left out, as is what the founder chose to forget.
    pub fn founder_feedback(
        &self,
        routine: &str,
        since: chrono::DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM queue
             WHERE routine = ?1 AND kind = 'approval' AND created_at >= ?2
               AND status IN ('approved', 'done', 'failed', 'rejected')
               AND id NOT IN (SELECT item FROM queue_log WHERE action = 'feedback forgotten')
             ORDER BY id DESC"
        ))?;
        let items = stmt
            .query_map(params![routine, since.to_rfc3339()], read)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::new();
        for item in items {
            let Some(path) = item.source["show"].as_str() else {
                continue;
            };
            let at = |v: &Value| {
                path.split('.')
                    .try_fold(v, |v, k| v.get(k))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let (draft, now) = (at(&item.original_args), at(&item.args));
            let said = if item.status == "rejected" {
                let why: String = self
                    .conn
                    .query_row(
                        "SELECT detail FROM queue_log WHERE item = ?1 AND action = 'rejected' ORDER BY id DESC LIMIT 1",
                        params![item.id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or_default();
                match why.strip_prefix("redone: ") {
                    Some(ask) => Some(("redo", ask.to_string(), String::new())),
                    None if SILENT.contains(&why.trim()) => None,
                    None => Some(("reject", why, String::new())),
                }
            } else if now != draft && !now.is_empty() {
                Some(("edit", String::new(), now))
            } else {
                None
            };
            if let Some((kind, said, edited)) = said {
                out.push(Feedback {
                    item: item.id,
                    at: item.created_at,
                    kind,
                    draft,
                    said,
                    edited,
                });
            }
            if out.len() == limit {
                break;
            }
        }
        Ok(out)
    }

    /// The founder takes back what item `id` taught: it is no longer shown
    /// to the agent (decision 116). The log keeps that it was said.
    pub fn forget_feedback(&mut self, id: i64) -> Result<()> {
        self.write(|tx| {
            get(tx, id)?;
            log(tx, id, "feedback forgotten", "founder", Some("app"), "")
        })
    }

    /// Delayed actions whose cancellation window has passed by `now`, and
    /// that the founder did not cancel (decision 45).
    pub fn due_delayed(&self, now: chrono::DateTime<Utc>) -> Result<Vec<Item>> {
        let now = now.to_rfc3339();
        Ok(self
            .queue(&["pending"])?
            .into_iter()
            .filter(|i| {
                i.kind == "delayed" && i.execute_at.as_deref().is_some_and(|at| at <= now.as_str())
            })
            .collect())
    }

    /// An item nobody answered in time leaves Needs you, marked expired,
    /// unless the founder answered meanwhile. Whether it did.
    pub fn expire_item(&mut self, id: i64, why: &str) -> Result<bool> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE queue SET status = 'expired', revision = revision + 1
                 WHERE id = ?1 AND kind = 'approval' AND status = 'pending'",
                params![id],
            )?;
            if changed == 1 {
                log(tx, id, "expired", "airth", None, why)?;
            }
            Ok(changed == 1)
        })
    }

    /// Airth lets a delayed item go once its window has passed: it becomes
    /// approved, unless the founder answered meanwhile.
    pub fn release_item(&mut self, id: i64) -> Result<bool> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE queue SET status = 'approved', revision = revision + 1
                 WHERE id = ?1 AND kind = 'delayed' AND status = 'pending'",
                params![id],
            )?;
            if changed == 1 {
                log(
                    tx,
                    id,
                    "released",
                    "airth",
                    None,
                    "the cancellation window passed",
                )?;
            }
            Ok(changed == 1)
        })
    }

    /// What became of an approved item, as Airth records it: `done` or
    /// `failed`.
    pub fn close_item(&mut self, id: i64, status: &str, detail: &str) -> Result<()> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE queue SET status = ?2, revision = revision + 1 WHERE id = ?1 AND status = 'approved'",
                params![id, status],
            )?;
            if changed == 0 {
                bail!("item {id} is not approved");
            }
            log(tx, id, status, "airth", None, detail)
        })
    }

    /// An item's log, as tests read it back: action, by, channel.
    #[cfg(test)]
    pub fn queue_log(&self, id: i64) -> Result<Vec<(String, String, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT action, by, channel FROM queue_log WHERE item = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The founder's answers in the ledger, as tests read them back: item
    /// and channel.
    #[cfg(test)]
    pub fn answers(&self) -> Result<Vec<(Option<i64>, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT item, channel FROM owner_time WHERE activity = 'answer' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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
                (Answer::Approve | Answer::Override | Answer::Retry { .. }, "approved" | "done")
                    | (Answer::Reject(_), "rejected")
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
            let blocked = item.flag.as_deref() == Some("blocking");
            let mut flag = item.flag.clone();
            let mut args = item.args.clone();
            let (next, event, detail) = match &answer {
                Answer::Approve if blocked => bail!(
                    "item {id} is flagged: {}. Edit it, or override the flag",
                    item.flag_reason.as_deref().unwrap_or("no reason given")
                ),
                Answer::Approve => ("approved", "approved", String::new()),
                Answer::Override if !blocked => bail!("item {id} has no blocking flag to override"),
                Answer::Override if !FOUNDER_CHANNELS.contains(&channel) => {
                    bail!("a flag is overridden only from the app or Telegram")
                }
                Answer::Override => {
                    flag = Some("overridden".into());
                    let why = item.flag_reason.clone().unwrap_or_default();
                    ("approved", "overridden", why)
                }
                Answer::Retry { .. } if !FOUNDER_CHANNELS.contains(&channel) => {
                    bail!("an action is retried only from the app or Telegram")
                }
                Answer::Retry { checked } => match item.args["retry"].as_str() {
                    Some("clear") => ("approved", "retry", String::new()),
                    Some("ambiguous") if *checked => (
                        "approved",
                        "retry",
                        "the founder checked nothing was done".to_string(),
                    ),
                    Some("ambiguous") => bail!(
                        "item {id}: the action may have been done. Check that nothing was created, then confirm the retry"
                    ),
                    _ => bail!("item {id} offers no retry"),
                },
                Answer::Reject(why) => ("rejected", "rejected", why.clone()),
                Answer::Edit(text) => {
                    let Some(path) = item.source["editable"].as_str() else {
                        bail!("item {id} has nothing the founder may edit");
                    };
                    if text.trim().is_empty() {
                        bail!("an edit needs text");
                    }
                    set_at(&mut args, path, text)?;
                    if blocked {
                        // The text is the founder's now: the block goes, and
                        // the journal says so like an override.
                        flag = Some("lifted".into());
                        let why = item.flag_reason.clone().unwrap_or_default();
                        log(tx, id, "flag lifted by edit", "founder", Some(channel), &why)?;
                    }
                    ("pending", "edited", String::new())
                }
            };
            tx.execute(
                "UPDATE queue SET status = ?2, args = ?3, flag = ?4, revision = revision + 1 WHERE id = ?1",
                params![id, next, args.to_string(), flag],
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
