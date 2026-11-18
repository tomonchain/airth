//! `send_mail` (decision 70): a routine's `mail.send` act, done through the
//! runtime like any declared action, so the agent's levels, the journal and
//! the founder's approval apply. A mission's model never holds a mailbox;
//! a mail task holds only [`Reply`], the answer to its one mail
//! (decision 152).

use super::{Tool, Workspace};
use crate::business::store::Store;
use crate::business::store::mail::Sent;
use crate::cancel::CancelToken;
use crate::mail::Mailbox;
use crate::mail::send::{Outgoing, compose};
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::PathBuf;

pub const NAME: &str = "send_mail";
pub const ACTION: &str = "mail.send";

/// The business's mailboxes with their passwords, never shown to a model.
#[derive(Clone)]
pub struct SendMail {
    pub mailboxes: Vec<(Mailbox, String)>,
    /// The business's name, for the signature.
    pub business: String,
    /// The business's database, where each sent mail is recorded.
    pub store: PathBuf,
}

impl SendMail {
    fn mailbox(&self, agent: &str) -> Result<&(Mailbox, String)> {
        self.mailboxes
            .iter()
            .find(|(m, _)| m.agent == agent)
            .with_context(|| format!("{agent} has no mailbox with its password set"))
    }
}

/// What to send, from a call's arguments.
pub fn outgoing(input: &Value) -> Outgoing {
    let text = |k: &str| input[k].as_str().unwrap_or_default().to_string();
    let to = match &input["to"] {
        Value::Array(a) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        v => v
            .as_str()
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    };
    Outgoing {
        to,
        subject: text("subject"),
        text: text("text"),
        in_reply_to: Some(text("in_reply_to")).filter(|s| !s.trim().is_empty()),
        role: Some(text("role")).filter(|s| !s.trim().is_empty()),
    }
}

impl Tool for SendMail {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME.into(),
            description: "Send a mail from the agent's mailbox. Airth signs it, copies it to archive@ and keeps it in its thread.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent": {"type": "string"},
                    "to": {"type": "string", "description": "Addresses, comma-separated"},
                    "subject": {"type": "string"},
                    "text": {"type": "string"},
                    "in_reply_to": {"type": "string", "description": "The Message-ID of the mail it answers"},
                    "key": {"type": "string"}
                },
                "required": ["agent", "to", "subject", "text", "key"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Network
    }

    fn action(&self, _: &Value) -> Result<Option<String>> {
        Ok(Some(ACTION.into()))
    }

    /// What the founder reads before approving: the mail as it will go, and
    /// whether it writes to someone new (rule 3).
    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let (mailbox, _) = self.mailbox(input["agent"].as_str().unwrap_or_default())?;
        let out = outgoing(input);
        let store = Store::open(&self.store)?;
        let mut new = Vec::new();
        for to in &out.to {
            if !store.corresponded(&mailbox.address, to)? {
                new.push(to.as_str());
            }
        }
        let warn = if new.is_empty() {
            String::new()
        } else {
            format!(
                "\nNew correspondent, never written to before: {}",
                new.join(", ")
            )
        };
        Ok(format!(
            "send a mail from {} to {}: {}{warn}\n\n{}",
            mailbox.address,
            out.to.join(", "),
            out.subject,
            out.text
        ))
    }

    fn execute(&self, input: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        let agent = input["agent"].as_str().unwrap_or_default();
        let key = input["key"]
            .as_str()
            .context("a mail is sent under a key")?;
        let (mailbox, password) = self.mailbox(agent)?;
        let out = outgoing(input);
        let composed = compose(mailbox, &self.business, &out, key)?;
        let server = crate::mail::send::send(mailbox, password, &composed)?;
        // The agent that drafted it, when not the mailbox's own (decision
        // 152).
        let agent = input["drafted_by"]
            .as_str()
            .filter(|a| !a.is_empty())
            .unwrap_or(&mailbox.agent);
        record(
            &self.store,
            mailbox,
            agent,
            key,
            &composed.message_id,
            &out,
            &server,
        )?;
        // Read as a declared action's answer, as HTTP's is (`SMTP 250 …`).
        Ok(format!("SMTP {server}\nsent to {}", out.to.join(", ")))
    }
}

/// Keep the evidence of a mail sent.
pub fn record(
    store: &std::path::Path,
    mailbox: &Mailbox,
    agent: &str,
    key: &str,
    message_id: &str,
    out: &Outgoing,
    server: &str,
) -> Result<()> {
    Store::open(store)?.record_sent(&Sent {
        mailbox: &mailbox.address,
        agent,
        key,
        message_id,
        to: &out.to,
        subject: &out.subject,
        thread: out.in_reply_to.as_deref(),
        server,
    })
}

/// `send_mail` as a mail task holds it (decision 152): the reply to its
/// one mail, from the mailbox that received it, to its sender, in its
/// thread, signed with the drafting agent's role. The model writes the
/// subject and the text, nothing else. It never sends: the task's call
/// waits for the founder in Needs you, and the approved item is sent by
/// [`SendMail`], once.
#[derive(Clone)]
pub struct Reply {
    pub send: SendMail,
    /// The agent whose mailbox received the mail: the interlocutor's.
    pub mailbox: String,
    pub drafted_by: String,
    pub role: String,
    /// The mail's sender, the only one it answers.
    pub to: String,
    /// The mail's Message-ID, to keep the reply in its thread.
    pub in_reply_to: String,
    /// "Re: " and the mail's subject, when the model gives none.
    pub subject: String,
}

impl Reply {
    /// The full call for [`SendMail`]: Airth's fields, whatever the model
    /// put there, and the model's subject and text. No key: the call is
    /// keyed when it is proposed.
    pub fn bind(&self, input: &Value) -> Value {
        let subject = input["subject"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&self.subject);
        json!({
            "agent": self.mailbox,
            "drafted_by": self.drafted_by,
            "role": self.role,
            "to": self.to,
            "subject": subject,
            "text": input["text"].as_str().unwrap_or_default(),
            "in_reply_to": self.in_reply_to,
        })
    }
}

impl Tool for Reply {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME.into(),
            description: format!(
                "Draft the reply to the mail, to {}. Airth sends it from the business's address, in the mail's thread, signed as {}, once the founder approves it; it never leaves before.",
                self.to, self.role
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "subject": {"type": "string", "description": "Leave it out to answer under the mail's own subject"},
                    "text": {"type": "string", "description": "The reply, without signature: Airth adds it"}
                },
                "required": ["text"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Network
    }

    fn action(&self, _: &Value) -> Result<Option<String>> {
        Ok(Some(ACTION.into()))
    }

    fn preview(&self, input: &Value, ws: &Workspace) -> Result<String> {
        self.send.preview(&self.bind(input), ws)
    }

    /// Never reached through a mail task's authority, which holds every
    /// call for the founder; refused all the same, so nothing else can make
    /// it send.
    fn execute(&self, _: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        anyhow::bail!(
            "not sent: a mail task's reply leaves only once the founder approves it in Needs you"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mail_to_someone_new_says_so_before_it_is_approved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("business.db");
        let mailbox = Mailbox {
            agent: "support".into(),
            address: "support@n.example".into(),
            imap_host: "127.0.0.1".into(),
            imap_port: 0,
            smtp_port: 0,
        };
        let tool = SendMail {
            mailboxes: vec![(mailbox.clone(), "pw".into())],
            business: "Numirama".into(),
            store: path.clone(),
        };
        let ws = Workspace::new(dir.path()).unwrap();
        let input = json!({"agent": "support", "to": "ann@buyer.example", "subject": "Votre lot", "text": "Il part demain.", "key": "k"});
        assert_eq!(tool.action(&input).unwrap().as_deref(), Some("mail.send"));
        let preview = tool.preview(&input, &ws).unwrap();
        assert!(
            preview.contains("New correspondent, never written to before: ann@buyer.example"),
            "{preview}"
        );
        // Written to once: no longer new.
        record(
            &path,
            &mailbox,
            "support",
            "k",
            "m@n.example",
            &outgoing(&input),
            "250 ok",
        )
        .unwrap();
        assert!(
            !tool
                .preview(&input, &ws)
                .unwrap()
                .contains("New correspondent")
        );
        // An agent without a mailbox sends nothing.
        let other = json!({"agent": "writer", "to": "a@b.example", "subject": "s", "text": "t", "key": "k"});
        assert!(tool.preview(&other, &ws).is_err());
    }
}
