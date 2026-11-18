//! The mail a role's mailbox received (decision 70): kept once, as
//! untrusted data, for routines to read through a Read block.

use super::Store;
use crate::mail::Mail;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

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

    /// The founder's view of `mailbox` (decision 152): its last `limit`
    /// mails, newest first, each with where it went. Read only.
    pub fn mail_lines(&self, mailbox: &str, limit: usize) -> Result<Vec<MailLine>> {
        // A task's reply is its latest `mail.send` item; a bank flag's
        // incident names its mail in its source. A reply sent is keyed by
        // its mail (`mail/<id>/<call>`, as the mail task's authority sets).
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.sender, m.subject, COALESCE(m.sent_at, m.fetched_at), m.text,
                    s.outcome, s.agent, s.summary, s.run, q.id, q.status,
                    EXISTS(SELECT 1 FROM mail_sent t WHERE t.key LIKE 'mail/' || m.id || '/%')
             FROM mail_messages m
             LEFT JOIN mail_sorting s ON s.mail = m.id
             LEFT JOIN queue q ON q.id = CASE WHEN s.run IS NOT NULL
                 THEN (SELECT MAX(id) FROM queue WHERE run = s.run AND action = ?3)
                 ELSE (SELECT MAX(id) FROM queue WHERE kind = 'incident' AND json_valid(source)
                       AND json_extract(source, '$.mail') = m.id) END
             WHERE m.mailbox = ?1 ORDER BY m.id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![mailbox, limit as i64, crate::tools::mail::ACTION],
            |r| {
                let subject: String = r.get(2)?;
                let text: String = r.get(4)?;
                let snippet = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let bank = crate::mail::bank_change(&Mail {
                    subject: subject.clone(),
                    text,
                    ..Default::default()
                });
                Ok(MailLine {
                    id: r.get(0)?,
                    sender: r.get(1)?,
                    subject,
                    date: r.get(3)?,
                    snippet: snippet.chars().take(SNIPPET).collect(),
                    bank,
                    outcome: r.get(5)?,
                    agent: r.get(6)?,
                    summary: r.get(7)?,
                    run: r.get(8)?,
                    item: r.get(9)?,
                    item_status: r.get(10)?,
                    replied: r.get(11)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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

/// How much of a mail's text its line shows.
const SNIPPET: usize = 160;

/// A mail as the founder's view lists it: who, what, when, and where it
/// went. `outcome` is its [`Sorting`]'s, None until it is taken. `item` is
/// its line in Needs you, if any: the reply its task drafted, or the bank
/// flag's incident; `replied` once a reply to it was sent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MailLine {
    pub id: i64,
    pub sender: String,
    pub subject: String,
    pub date: String,
    pub snippet: String,
    pub bank: bool,
    pub outcome: Option<String>,
    pub agent: Option<String>,
    pub summary: Option<String>,
    pub run: Option<String>,
    pub item: Option<i64>,
    pub item_status: Option<String>,
    pub replied: bool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;
    use crate::business::store::autonomy::NewItem;

    const BOX: &str = "contact@n.example";

    fn keep(s: &mut Store, uid: u32, from: &str, subject: &str, text: &str) -> i64 {
        let mail = Mail {
            thread: format!("t{uid}"),
            from: from.into(),
            subject: subject.into(),
            text: text.into(),
            ..Default::default()
        };
        let new = NewMail {
            mailbox: BOX,
            agent: "chief",
            uid_validity: 1,
            uid,
            mail: &mail,
            at: Utc::now(),
        };
        s.keep_mail(&new).unwrap().unwrap()
    }

    fn sorted(s: &mut Store, id: i64, outcome: &str, agent: Option<&str>, run: Option<&str>) {
        assert!(s.claim_mail(id).unwrap());
        let sorting = Sorting {
            outcome: outcome.into(),
            agent: agent.map(Into::into),
            summary: agent.map(|_| "asks when the lot ships".into()),
            run: run.map(Into::into),
            ..Default::default()
        };
        s.mail_sorted(id, &sorting).unwrap();
    }

    fn item(
        s: &mut Store,
        kind: &str,
        run: Option<&str>,
        action: Option<&str>,
        source: &str,
    ) -> i64 {
        s.propose(&NewItem {
            kind,
            title: "t",
            agent: None,
            routine: None,
            run,
            action,
            args: "{}",
            execute_at: None,
            source,
            model_call: None,
        })
        .unwrap()
    }

    /// Decision 152: the founder sees each mail, newest first, with where
    /// it went, its reply's state and the bank flag; another mailbox's mail
    /// is not shown, and looking changes nothing.
    #[test]
    fn mail_lines_say_where_each_mail_went() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join(FILE)).unwrap();
        let shipped = keep(
            &mut s,
            1,
            "ann@b.example",
            "Lot 12",
            "When does\n\n  lot 12   ship?",
        );
        let replied = keep(&mut s, 2, "bob@b.example", "Invoice", "Please resend it.");
        let bank = keep(
            &mut s,
            3,
            "eve@b.example",
            "Update",
            "Our new IBAN is below.",
        );
        let waiting = keep(&mut s, 4, "zoe@b.example", "Hello", "Hi");
        let long = keep(&mut s, 5, "max@b.example", "Long", &"word ".repeat(100));
        sorted(&mut s, shipped, "agent", Some("writer"), Some("r1"));
        sorted(&mut s, replied, "agent", Some("support"), Some("r2"));
        sorted(&mut s, bank, "bank", None, None);
        // The reply the first task drafted, drafted again: the latest counts.
        let send = Some(crate::tools::mail::ACTION);
        item(&mut s, "approval", Some("r1"), send, "{}");
        let reply = item(&mut s, "approval", Some("r1"), send, "{}");
        item(&mut s, "approval", Some("r1"), Some("other.act"), "{}");
        let flag = item(
            &mut s,
            "incident",
            None,
            None,
            &format!(r#"{{"mail": {bank}, "bank": true}}"#),
        );
        s.record_sent(&Sent {
            mailbox: BOX,
            agent: "support",
            key: &format!("mail/{replied}/c1"),
            message_id: "<m@n.example>",
            to: &["bob@b.example".to_string()],
            subject: "Re: Invoice",
            thread: Some("t2"),
            server: "250 OK",
        })
        .unwrap();
        // Another mailbox of the business is not this view's.
        let other = Mail {
            thread: "x".into(),
            ..Default::default()
        };
        s.keep_mail(&NewMail {
            mailbox: "support@n.example",
            agent: "support",
            uid_validity: 1,
            uid: 1,
            mail: &other,
            at: Utc::now(),
        })
        .unwrap();

        let lines = s.mail_lines(BOX, 50).unwrap();
        let ids: Vec<i64> = lines.iter().map(|l| l.id).collect();
        assert_eq!(
            ids,
            vec![long, waiting, bank, replied, shipped],
            "newest first"
        );
        let line = |id: i64| lines.iter().find(|l| l.id == id).unwrap();

        let l = line(shipped);
        assert_eq!(l.snippet, "When does lot 12 ship?");
        assert_eq!(l.outcome.as_deref(), Some("agent"));
        assert_eq!(l.agent.as_deref(), Some("writer"));
        assert_eq!(l.summary.as_deref(), Some("asks when the lot ships"));
        assert_eq!(l.run.as_deref(), Some("r1"));
        assert_eq!(
            (l.item, l.item_status.as_deref()),
            (Some(reply), Some("pending"))
        );
        assert!(!l.replied && !l.bank);

        assert!(line(replied).replied);
        assert_eq!(line(replied).item, None);

        let l = line(bank);
        assert!(l.bank);
        assert_eq!(l.outcome.as_deref(), Some("bank"));
        assert_eq!(l.item, Some(flag));

        let l = line(waiting);
        assert_eq!(
            (l.outcome.as_deref(), l.agent.as_deref(), l.item),
            (None, None, None)
        );
        assert_eq!(line(long).snippet.chars().count(), SNIPPET);

        assert_eq!(s.mail_lines(BOX, 2).unwrap().len(), 2);
        assert!(s.mail_lines("nobody@n.example", 50).unwrap().is_empty());
        // Looking takes nothing: the mail not yet passed on still waits.
        assert_eq!(s.mails_to_sort(BOX, 10).unwrap().len(), 2);
    }
}
