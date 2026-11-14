//! The mail a role's mailbox received (decision 70): kept once, as
//! untrusted data, for routines to read through a Read block.

use super::Store;
use crate::mail::Mail;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;

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

    /// Keep a message; false when it was kept already.
    pub fn keep_mail(&mut self, m: &NewMail) -> Result<bool> {
        let mail = m.mail;
        self.write(|tx| {
            Ok(tx.execute(
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
            )? == 1)
        })
    }

    /// The last `limit` messages kept from `mailbox`, oldest first.
    pub fn mails(&self, mailbox: &str, limit: usize) -> Result<Vec<Kept>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, fetched_at, message_id, thread, sender, recipients, subject,
                    sent_at, text, attachments
             FROM mail_messages WHERE mailbox = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let list = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
        let mut out = stmt
            .query_map(params![mailbox, limit as i64], |r| {
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
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.reverse();
        Ok(out)
    }
}
