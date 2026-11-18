//! Sending a role's mail (decision 70). What the agent wrote is the body;
//! the code, never the model, adds the rest: the signature, a blind copy to
//! `archive@<domain>`, and the headers that keep a reply in its thread.

use super::Mailbox;
use anyhow::{Context, Result, bail};
use lettre::Transport;
use lettre::message::{Mailbox as Address, Message, header::ContentType};
use lettre::transport::smtp::{SmtpTransport, authentication::Credentials, extension::ClientId};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// What an agent sends: to whom, the subject, its text, and the message it
/// answers, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
    /// The Message-ID it replies to, without angle brackets.
    pub in_reply_to: Option<String>,
    /// The role it is signed with ("Support"), when an agent drafted it
    /// for another's mailbox (decision 152).
    pub role: Option<String>,
}

/// The mail as it leaves: its Message-ID and the message itself.
pub struct Composed {
    pub message_id: String,
    pub message: Message,
}

/// The line every sent mail ends with (rule 5), after the role that
/// drafted it, when it names one ("Support, Northwind"), on one line
/// whatever spaces or line breaks the role was written with.
pub fn signature(business: &str, role: Option<&str>) -> String {
    let role = role
        .map(|r| r.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|r| !r.is_empty())
        .map(|r| format!("{r}, {business}\n"))
        .unwrap_or_default();
    format!("--\n{role}Prepared by an AI agent of {business} and sent with the founder's approval.")
}

/// The mail `mailbox` sends for `out`, keyed by `key`: its Message-ID is
/// derived from the key, so a mail sent twice by mistake would show as one.
pub fn compose(mailbox: &Mailbox, business: &str, out: &Outgoing, key: &str) -> Result<Composed> {
    if out.to.is_empty() {
        bail!("a mail needs someone to send it to");
    }
    if out.subject.trim().is_empty() || out.text.trim().is_empty() {
        bail!("a mail needs a subject and a text");
    }
    let domain = mailbox
        .address
        .split_once('@')
        .map(|(_, d)| d)
        .context("the mailbox has no domain")?;
    let from: Address = mailbox.address.parse().context("the mailbox's address")?;
    let message_id = format!(
        "{}@{domain}",
        &crate::update::sha256_hex(format!("{}\n{key}", mailbox.address).as_bytes())[..32]
    );
    let mut builder = Message::builder()
        .from(from)
        .bcc(
            format!("archive@{domain}")
                .parse()
                .context("the archive address")?,
        )
        .subject(out.subject.trim())
        .message_id(Some(format!("<{message_id}>")));
    for to in &out.to {
        let to = to.trim();
        if !super::valid_address(to) {
            bail!("'{to}' is not an e-mail address");
        }
        builder = builder.to(to.parse().with_context(|| format!("'{to}'"))?);
    }
    if let Some(parent) = out
        .in_reply_to
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let parent = parent.trim_start_matches('<').trim_end_matches('>');
        builder = builder
            .in_reply_to(format!("<{parent}>"))
            .references(format!("<{parent}>"));
    }
    let body = format!(
        "{}\n\n{}\n",
        out.text.trim_end(),
        signature(business, out.role.as_deref())
    );
    let message = builder.header(ContentType::TEXT_PLAIN).body(body)?;
    Ok(Composed {
        message_id,
        message,
    })
}

/// Send over TLS with the mailbox's login; the server's answer.
pub fn send(mailbox: &Mailbox, password: &str, composed: &Composed) -> Result<String> {
    let domain = mailbox
        .address
        .split_once('@')
        .map(|(_, d)| d)
        .unwrap_or_default();
    // The tests' fake server, on loopback: plain SMTP there, in tests only.
    #[cfg(test)]
    if mailbox.imap_host == "127.0.0.1" {
        let mailer = SmtpTransport::builder_dangerous("127.0.0.1")
            .port(mailbox.smtp_port)
            .credentials(Credentials::new(
                mailbox.address.clone(),
                password.to_string(),
            ))
            .build();
        return deliver(&mailer, composed, password);
    }
    let mailer = SmtpTransport::relay(&mailbox.imap_host)?
        .port(mailbox.smtp_port)
        .hello_name(ClientId::Domain(domain.to_string()))
        .credentials(Credentials::new(
            mailbox.address.clone(),
            password.to_string(),
        ))
        .timeout(Some(TIMEOUT))
        .build();
    deliver(&mailer, composed, password)
}

fn deliver(mailer: &SmtpTransport, composed: &Composed, password: &str) -> Result<String> {
    let answer = mailer
        .send(&composed.message)
        // An error never repeats the password.
        .map_err(|e| anyhow::anyhow!("{}", e.to_string().replace(password, "[password]")))?;
    Ok(format!(
        "{} {}",
        answer.code(),
        answer.message().collect::<Vec<_>>().join(" ")
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A fake SMTP server on 127.0.0.1 that takes one mail: the envelope's
    /// recipients and the data, once the connection closes.
    pub(crate) fn server() -> (u16, std::thread::JoinHandle<(Vec<String>, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut lines = BufReader::new(stream).lines();
            let (mut rcpt, mut data) = (Vec::new(), String::new());
            out.write_all(b"220 fake ESMTP\r\n").unwrap();
            while let Some(Ok(line)) = lines.next() {
                let upper = line.to_uppercase();
                let answer = if upper.starts_with("EHLO") {
                    "250-fake\r\n250 AUTH PLAIN LOGIN\r\n".to_string()
                } else if upper.starts_with("AUTH") {
                    "235 ok\r\n".into()
                } else if upper.starts_with("RCPT TO:") {
                    rcpt.push(line[8..].trim().trim_matches(['<', '>']).to_string());
                    "250 ok\r\n".into()
                } else if upper.starts_with("DATA") {
                    out.write_all(b"354 go\r\n").unwrap();
                    for l in lines.by_ref() {
                        let l = l.unwrap();
                        if l == "." {
                            break;
                        }
                        data.push_str(&l);
                        data.push('\n');
                    }
                    "250 2.0.0 queued as 42\r\n".into()
                } else if upper.starts_with("QUIT") {
                    out.write_all(b"221 bye\r\n").unwrap();
                    break;
                } else {
                    "250 ok\r\n".into()
                };
                out.write_all(answer.as_bytes()).unwrap();
            }
            (rcpt, data)
        });
        (port, handle)
    }

    /// Plain SMTP to the fake server: loopback, in tests only.
    pub(crate) fn send_plain(port: u16, composed: &Composed) -> Result<String> {
        let mailer = SmtpTransport::builder_dangerous("127.0.0.1")
            .port(port)
            .credentials(Credentials::new("support@n.example".into(), "pw".into()))
            .build();
        deliver(&mailer, composed, "pw")
    }

    fn mailbox() -> Mailbox {
        Mailbox {
            agent: "support".into(),
            address: "support@n.example".into(),
            imap_host: "127.0.0.1".into(),
            imap_port: 0,
            smtp_port: 0,
        }
    }

    #[test]
    fn a_reply_is_signed_copied_to_the_archive_and_kept_in_its_thread() {
        let out = Outgoing {
            to: vec!["ann@buyer.example".into()],
            subject: "Re: Où est mon lot ?".into(),
            text: "Bonjour Ann, votre lot part demain.".into(),
            in_reply_to: Some("<2@buyer.example>".into()),
            role: Some("Support".into()),
        };
        let composed =
            compose(&mailbox(), "Numirama", &out, "support-replies/mail:7/send").unwrap();
        let (port, server) = server();
        let answer = send_plain(port, &composed).unwrap();
        assert!(answer.starts_with("250"), "{answer}");
        let (rcpt, data) = server.join().unwrap();
        // The archive gets a blind copy: in the envelope, not in the headers.
        assert_eq!(rcpt, ["ann@buyer.example", "archive@n.example"]);
        assert!(!data.to_lowercase().contains("bcc:"), "{data}");
        assert!(data.contains("In-Reply-To: <2@buyer.example>"), "{data}");
        assert!(data.contains("References: <2@buyer.example>"), "{data}");
        assert!(
            data.contains(&format!("Message-ID: <{}>", composed.message_id)),
            "{data}"
        );
        assert!(
            data.contains("Support, Numirama")
                && data.contains("Prepared by an AI agent of Numirama"),
            "{data}"
        );
        // The same key, the same Message-ID: a second send would show as one.
        let again = compose(&mailbox(), "Numirama", &out, "support-replies/mail:7/send").unwrap();
        assert_eq!(again.message_id, composed.message_id);
    }

    /// A role's spaces and line breaks become single spaces: the role
    /// stays one line of the signature.
    #[test]
    fn a_role_signs_on_one_line() {
        assert_eq!(
            signature("Numirama", Some("  Customer\n support \r\n\tlead ")),
            "--\nCustomer support lead, Numirama\nPrepared by an AI agent of Numirama and sent with the founder's approval."
        );
        assert_eq!(
            signature("N", Some(" \n ")),
            "--\nPrepared by an AI agent of N and sent with the founder's approval."
        );
    }

    #[test]
    fn a_mail_without_a_recipient_or_text_is_refused() {
        let out = |to: &[&str], text: &str| Outgoing {
            to: to.iter().map(|s| s.to_string()).collect(),
            subject: "Hello".into(),
            text: text.into(),
            in_reply_to: None,
            role: None,
        };
        assert!(compose(&mailbox(), "N", &out(&[], "Hi"), "k").is_err());
        assert!(compose(&mailbox(), "N", &out(&["ann@b.example"], " "), "k").is_err());
        assert!(compose(&mailbox(), "N", &out(&["not an address"], "Hi"), "k").is_err());
        // A header cannot be smuggled through an address.
        assert!(
            compose(
                &mailbox(),
                "N",
                &out(&["a@b.example\r\nBcc: x@y.example"], "Hi"),
                "k"
            )
            .is_err()
        );
    }
}
