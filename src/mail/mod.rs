//! Agent mail (decision 70): one mailbox per role, on the business's
//! domain. This part reads it: new mail is fetched over IMAP, read only,
//! parsed, and kept once in `business.db` as untrusted data. The password
//! is a realm secret, used here only: never given to a command or a model.

pub mod imap;

use crate::business::store::Store;
use crate::business::store::mail::NewMail;
use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::path::Path;

/// The realm secret holding a mailbox's password: this prefix, then the
/// agent's name in capitals (`MAIL_PASSWORD_SUPPORT`).
pub const SECRET_PREFIX: &str = "MAIL_PASSWORD_";

/// OVH MX Plan (decision 70); another provider is config, not code.
const IMAP_HOST: &str = "ssl0.ovh.net";
const IMAP_PORT: u16 = 993;
/// Each connection gives up after this long without an answer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// A role's mailbox, as the agent's file names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Mailbox {
    pub agent: String,
    /// Its address, which is also its login.
    pub address: String,
    pub imap_host: String,
    pub imap_port: u16,
}

impl Mailbox {
    /// The mailbox of the agent `agent`, from its file's `mailbox:` field
    /// (and `mail_server:`, for a provider other than OVH). None when it has
    /// none.
    pub fn of(root: &Path, agent: &str) -> Result<Option<Self>> {
        crate::realm::validate_name(agent).context("invalid agent name")?;
        let path = root.join("agents").join(format!("{agent}.md"));
        let text = std::fs::read_to_string(&path).with_context(|| format!("no agent {agent}"))?;
        let front = crate::frontmatter::parse(&text);
        let Some(address) = front
            .text("mailbox")
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
        else {
            return Ok(None);
        };
        if !valid_address(&address) {
            bail!("{agent}'s mailbox '{address}' is not an e-mail address");
        }
        let imap_host = front
            .text("mail_server")
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| IMAP_HOST.into());
        if !imap_host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        {
            bail!("{agent}'s mail server '{imap_host}' is not a host name");
        }
        Ok(Some(Self {
            agent: agent.to_string(),
            address,
            imap_host,
            imap_port: IMAP_PORT,
        }))
    }

    /// The name of the realm secret holding its password.
    pub fn secret(&self) -> String {
        format!(
            "{SECRET_PREFIX}{}",
            self.agent.to_uppercase().replace('-', "_")
        )
    }
}

fn valid_address(a: &str) -> bool {
    let Some((local, domain)) = a.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !a.contains(char::is_whitespace)
        && !a.contains(['<', '>', '"', ',', ';'])
}

/// A TLS connection to `host`, its certificate checked against the
/// system's roots.
fn connect(host: &str, port: u16) -> Result<impl std::io::Read + std::io::Write> {
    use rustls_platform_verifier::ConfigVerifierExt;
    let config =
        rustls::ClientConfig::with_platform_verifier().context("setting up TLS for mail")?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .with_context(|| format!("'{host}' is not a server name"))?;
    let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), name)?;
    let tcp = std::net::TcpStream::connect((host, port))
        .with_context(|| format!("cannot reach {host}:{port}"))?;
    tcp.set_read_timeout(Some(TIMEOUT))?;
    tcp.set_write_timeout(Some(TIMEOUT))?;
    Ok(rustls::StreamOwned::new(conn, tcp))
}

/// Fetch what is new in `mailbox` and keep it; how many were kept.
pub fn fetch(mailbox: &Mailbox, password: &str, store: &mut Store) -> Result<usize> {
    let stream = connect(&mailbox.imap_host, mailbox.imap_port)?;
    fetch_over(stream, mailbox, password, store)
}

/// What [`fetch`] does, over any stream: the tests use plain TCP on
/// loopback.
fn fetch_over<S: std::io::Read + std::io::Write>(
    stream: S,
    mailbox: &Mailbox,
    password: &str,
    store: &mut Store,
) -> Result<usize> {
    let mut imap = imap::Imap::open(stream)?;
    imap.login(&mailbox.address, password)?;
    let validity = imap.select_inbox()?;
    let from = store.last_mail_uid(&mailbox.address, validity)? + 1;
    let mut kept = 0;
    for uid in imap.uids_from(from)? {
        let Some(raw) = imap.fetch(uid)? else {
            continue;
        };
        let mail = read(&raw);
        if store.keep_mail(&NewMail {
            mailbox: &mailbox.address,
            agent: &mailbox.agent,
            uid_validity: validity,
            uid,
            mail: &mail,
            at: Utc::now(),
        })? {
            kept += 1;
        }
    }
    imap.logout();
    Ok(kept)
}

/// A message as Airth keeps it: who, what, in which thread, and its text.
/// HTML is reduced to text; attachments are named, never opened.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mail {
    pub message_id: Option<String>,
    /// The conversation it belongs to: the first message it refers to, or
    /// its own id.
    pub thread: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub date: Option<String>,
    pub text: String,
    pub attachments: Vec<String>,
}

/// The longest text kept of one message.
const MAX_TEXT: usize = 100_000;

pub fn read(raw: &[u8]) -> Mail {
    let Some(m) = mail_parser::MessageParser::default().parse(raw) else {
        return Mail {
            text: String::from_utf8_lossy(raw)
                .chars()
                .take(MAX_TEXT)
                .collect(),
            thread: String::new(),
            ..Default::default()
        };
    };
    let addresses = |a: Option<mail_parser::AddressList>| -> Vec<String> {
        a.map(|a| {
            a.mailboxes()
                .filter_map(|x| x.address().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
    };
    let message_id = m.message_id().map(str::to_string);
    let refers = m
        .references()
        .and_then(|l| l.get(0).map(str::to_string))
        .or_else(|| m.in_reply_to().and_then(|l| l.get(0).map(str::to_string)));
    let text = m
        .body_text(0)
        .map(|t| t.to_string())
        .unwrap_or_default()
        .chars()
        .take(MAX_TEXT)
        .collect();
    Mail {
        thread: refers.or(message_id.clone()).unwrap_or_default(),
        message_id,
        from: addresses(m.from()).into_iter().next().unwrap_or_default(),
        to: addresses(m.to()),
        subject: m.subject().unwrap_or_default().to_string(),
        date: m.date().map(|d| d.to_rfc3339()),
        text,
        attachments: m
            .attachments()
            .map(|a| a.attachment_name().unwrap_or("unnamed").to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    const MAIL: &[u8] = b"From: Ann <ann@buyer.example>\r\nTo: support@numirama.example\r\nSubject: =?UTF-8?Q?O=C3=B9_est_mon_lot_=3F?=\r\nMessage-ID: <2@buyer.example>\r\nIn-Reply-To: <1@numirama.example>\r\nReferences: <1@numirama.example>\r\nDate: Sat, 14 Nov 2026 10:00:00 +0100\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Bonjour, o&ugrave; est mon lot&nbsp;?</p>\r\n--b\r\nContent-Type: application/pdf; name=facture.pdf\r\nContent-Disposition: attachment; filename=facture.pdf\r\n\r\nJVBERi0=\r\n--b--\r\n";

    #[test]
    fn a_message_is_kept_as_text_in_its_thread_attachments_named() {
        let m = read(MAIL);
        assert_eq!(m.from, "ann@buyer.example");
        assert_eq!(m.to, ["support@numirama.example"]);
        assert_eq!(m.subject, "Où est mon lot ?");
        assert_eq!(m.message_id.as_deref(), Some("2@buyer.example"));
        assert_eq!(m.thread, "1@numirama.example");
        assert!(m.text.contains("où est mon lot"), "{}", m.text);
        assert!(!m.text.contains("<p>"));
        assert_eq!(m.attachments, ["facture.pdf"]);
        // Not mail at all: kept as text, never refused.
        assert_eq!(read(b"just text").text, "just text");
    }

    #[test]
    fn new_mail_is_fetched_once_by_uid() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("business.db")).unwrap();
        let mailbox = Mailbox {
            agent: "support".into(),
            address: "support@numirama.example".into(),
            imap_host: "127.0.0.1".into(),
            imap_port: 0,
        };
        let run = |messages: Vec<(u32, Vec<u8>)>, store: &mut Store| {
            let (port, server) = imap::tests::server(3, messages);
            let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let kept = fetch_over(tcp, &mailbox, "right", store).unwrap();
            (kept, server.join().unwrap())
        };
        let (kept, _) = run(vec![(10, MAIL.to_vec())], &mut store);
        assert_eq!(kept, 1);
        // Again, and with one more: only the new one is fetched.
        let (kept, seen) = run(
            vec![(10, MAIL.to_vec()), (11, b"Subject: b\r\n\r\nB".to_vec())],
            &mut store,
        );
        assert_eq!(kept, 1);
        assert!(
            seen.iter().any(|l| l.contains("UID SEARCH UID 11:*")),
            "{seen:?}"
        );
        assert!(
            !seen.iter().any(|l| l.contains("UID FETCH 10 ")),
            "{seen:?}"
        );
        let mails = store.mails("support@numirama.example", 10).unwrap();
        assert_eq!(mails.len(), 2);
        assert_eq!(mails[0].mail.subject, "Où est mon lot ?");
        // A new UIDVALIDITY renumbers the box: what it holds is read again,
        // and a message kept already is not kept twice.
        let (port, server) = imap::tests::server(4, vec![(1, MAIL.to_vec())]);
        let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
        assert_eq!(fetch_over(tcp, &mailbox, "right", &mut store).unwrap(), 0);
        server.join().unwrap();
    }

    #[test]
    fn a_mailbox_is_named_in_the_agents_file() {
        let root = tempfile::tempdir().unwrap();
        let agents = root.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("support.md"),
            "---\ntitle: Support\nmailbox: support@numirama.example\n---\nAnswer.\n",
        )
        .unwrap();
        std::fs::write(
            agents.join("writer.md"),
            "---\ntitle: Writer\nmailbox:\n---\nWrite.\n",
        )
        .unwrap();
        std::fs::write(
            agents.join("odd.md"),
            "---\ntitle: Odd\nmailbox: not an address\n---\nx\n",
        )
        .unwrap();
        let m = Mailbox::of(root.path(), "support").unwrap().unwrap();
        assert_eq!((m.imap_host.as_str(), m.imap_port), ("ssl0.ovh.net", 993));
        assert_eq!(m.secret(), "MAIL_PASSWORD_SUPPORT");
        assert!(Mailbox::of(root.path(), "writer").unwrap().is_none());
        assert!(Mailbox::of(root.path(), "odd").is_err());
        assert!(Mailbox::of(root.path(), "../x").is_err());
    }
}
