//! The mail a role's mailbox received (decision 70): kept once, as
//! untrusted data, for routines to read through a Read block.

use super::Store;
use crate::mail::Mail;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

/// A message fetched from a mailbox, to keep.
pub struct NewMail<'a> {
    pub mailbox: &'a str,
    pub agent: &'a str,
    pub uid_validity: u32,
    pub uid: u32,
    pub mail: &'a Mail,
    pub at: DateTime<Utc>,
}

/// A message kept, with where it was kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub id: i64,
    pub agent: String,
    pub fetched_at: String,
    pub mail: Mail,
}

impl Store {
    /// The highest UID kept from `mailbox` under `uid_validity`; 0 for none.
    pub fn last_mail_uid(&self, mailbox: &str, uid_validity: u32) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(uid), 0) FROM mail_messages WHERE mailbox = ?1 AND uid_validity = ?2",
            params![mailbox, uid_validity],
            |r| r.get(0),
        )?)
    }

    /// Keep a message; its id, or None when it was kept already.
    pub fn keep_mail(&mut self, m: &NewMail) -> Result<Option<i64>> {
        let mail = m.mail;
        self.write(|tx| {
            let added = tx.execute(
                "INSERT OR IGNORE INTO mail_messages
                 (mailbox, agent, uid_validity, uid, message_id, thread, sender, recipients,
                  subject, sent_at, text, attachments, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    m.mailbox,
                    m.agent,
                    m.uid_validity,
                    m.uid,
                    mail.message_id,
                    mail.thread,
                    mail.from,
                    serde_json::to_string(&mail.to)?,
                    mail.subject,
                    mail.date,
                    mail.text,
                    serde_json::to_string(&mail.attachments)?,
                    m.at.to_rfc3339(),
                ],
            )? == 1;
            Ok(added.then(|| tx.last_insert_rowid()))
        })
    }

    /// The last `limit` messages kept from `mailbox`, oldest first.
    pub fn mails(&self, mailbox: &str, limit: usize) -> Result<Vec<Kept>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {KEPT} FROM mail_messages WHERE mailbox = ?1 ORDER BY id DESC LIMIT ?2"
        ))?;
        let mut out = stmt
            .query_map(params![mailbox, limit as i64], kept)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.reverse();
        Ok(out)
    }

    /// One message kept, by its id.
    pub fn mail(&self, id: i64) -> Result<Option<Kept>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {KEPT} FROM mail_messages WHERE id = ?1"),
                params![id],
                kept,
            )
            .optional()?)
    }

    /// Up to `limit` messages of `mailbox` never passed on (decision 152),
    /// newest first: a backlog, such as a mailbox's first reading, never
    /// holds back the mail that just came. The rest waits for the next pass.
    pub fn mails_to_sort(&self, mailbox: &str, limit: usize) -> Result<Vec<Kept>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {KEPT} FROM mail_messages
             WHERE mailbox = ?1 AND NOT EXISTS (SELECT 1 FROM mail_sorting s WHERE s.mail = mail_messages.id)
             ORDER BY id DESC LIMIT ?2"
        ))?;
        Ok(stmt
            .query_map(params![mailbox, limit as i64], kept)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Take mail `id` to pass it on: true once, false ever after, so a mail
    /// is never sorted nor started twice, even across restarts.
    pub fn claim_mail(&mut self, id: i64) -> Result<bool> {
        self.write(|tx| {
            Ok(tx.execute(
                "INSERT OR IGNORE INTO mail_sorting (mail, outcome, at) VALUES (?1, 'sorting', ?2)",
                params![id, Utc::now().to_rfc3339()],
            )? == 1)
        })
    }

    /// Where mail `id`, claimed, went: its outcome, the agent, the sorting's
    /// summary and model call, and the task's run.
    pub fn mail_sorted(&mut self, id: i64, s: &Sorting) -> Result<()> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE mail_sorting SET outcome = ?2, agent = ?3, summary = ?4, model_call = ?5, run = ?6, at = ?7
                 WHERE mail = ?1 AND outcome = 'sorting'",
                params![id, s.outcome, s.agent, s.summary, s.model_call, s.run, Utc::now().to_rfc3339()],
            )?;
            anyhow::ensure!(changed == 1, "mail {id} was not being passed on");
            Ok(())
        })
    }

    /// How mail `id` was passed on, if it was.
    #[cfg(test)]
    pub fn mail_sorting(&self, id: i64) -> Result<Option<Sorting>> {
        Ok(self
            .conn
            .query_row(
                "SELECT outcome, agent, summary, model_call, run FROM mail_sorting WHERE mail = ?1",
                params![id],
                |r| {
                    Ok(Sorting {
                        outcome: r.get(0)?,
                        agent: r.get(1)?,
                        summary: r.get(2)?,
                        model_call: r.get(3)?,
                        run: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Who drafted each mail sent, by its Message-ID.
    #[cfg(test)]
    pub fn sent_by(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT mailbox, agent, message_id FROM mail_sent ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Record a mail a role sent: who, to whom, in which thread, and what
    /// the server answered. Never changed after.
    pub fn record_sent(&mut self, s: &Sent) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "INSERT INTO mail_sent (mailbox, agent, key, message_id, recipients, subject, thread, server, sent_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    s.mailbox,
                    s.agent,
                    s.key,
                    s.message_id,
                    serde_json::to_string(s.to)?,
                    s.subject,
                    s.thread,
                    s.server,
                    Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })
    }

    /// Whether `mailbox` ever exchanged mail with `address`: written to it,
    /// or received from it (rule 3: a new correspondent is marked).
    pub fn corresponded(&self, mailbox: &str, address: &str) -> Result<bool> {
        let address = address.trim().to_lowercase();
        let wrote: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM mail_sent, json_each(mail_sent.recipients) r
             WHERE mail_sent.mailbox = ?1 AND lower(r.value) = ?2)",
            params![mailbox, address],
            |r| r.get(0),
        )?;
        let heard: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM mail_messages WHERE mailbox = ?1 AND lower(sender) = ?2)",
            params![mailbox, address],
            |r| r.get(0),
        )?;
        Ok(wrote || heard)
    }
}

/// The columns a kept message is read from, for [`kept`].
const KEPT: &str = "id, agent, fetched_at, message_id, thread, sender, recipients, subject, sent_at, text, attachments";

fn kept(r: &rusqlite::Row) -> rusqlite::Result<Kept> {
    let list = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
    Ok(Kept {
        id: r.get(0)?,
        agent: r.get(1)?,
        fetched_at: r.get(2)?,
        mail: Mail {
            message_id: r.get(3)?,
            thread: r.get(4)?,
            from: r.get(5)?,
            to: list(r.get(6)?),
            subject: r.get(7)?,
            date: r.get(8)?,
            text: r.get(9)?,
            attachments: list(r.get(10)?),
        },
    })
}

/// How a mail of the interlocutor's mailbox was passed on (decision 152).
/// `outcome`: `agent` (a task started), `founder`, `bank` (flagged, never
/// sorted), `failed` (to the founder), `old` (dated long before it was
/// read), `before` (kept before decision 152), or `sorting` while it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sorting {
    pub outcome: String,
    pub agent: Option<String>,
    pub summary: Option<String>,
    pub model_call: Option<i64>,
    pub run: Option<String>,
}

/// A mail sent, to record.
pub struct Sent<'a> {
    pub mailbox: &'a str,
    pub agent: &'a str,
    pub key: &'a str,
    pub message_id: &'a str,
    pub to: &'a [String],
    pub subject: &'a str,
    pub thread: Option<&'a str>,
    pub server: &'a str,
}
