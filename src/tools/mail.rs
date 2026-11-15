//! `send_mail` (decision 70): a routine's `mail.send` act, done through the
//! runtime like any declared action, so the agent's levels, the journal and
//! the founder's approval apply. Only routines get it: a mission's model
//! never holds a mailbox.

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
        record(
            &self.store,
            mailbox,
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
    key: &str,
    message_id: &str,
    out: &Outgoing,
    server: &str,
) -> Result<()> {
    Store::open(store)?.record_sent(&Sent {
        mailbox: &mailbox.address,
        agent: &mailbox.agent,
        key,
        message_id,
        to: &out.to,
        subject: &out.subject,
        thread: out.in_reply_to.as_deref(),
        server,
    })
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
