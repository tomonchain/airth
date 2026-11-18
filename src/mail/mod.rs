//! Agent mail (decision 70): one mailbox per role, on the business's
//! domain. This part reads it: new mail is fetched over IMAP, read only,
//! parsed, and kept once in `business.db` as untrusted data. The password
//! is a realm secret, used here only: never given to a command or a model.

pub mod check;
pub mod imap;
pub mod send;

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
/// Submission over TLS from the first byte.
const SMTP_PORT: u16 = 465;
/// Submission upgraded by STARTTLS before the login.
const SUBMISSION_PORT: u16 = 587;
/// Each connection gives up after this long without an answer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// How SMTP is secured (decision 154): TLS from the first byte, or STARTTLS
/// before the login. There is no third: mail never goes in plain text, and
/// IMAP is TLS from the first byte only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Security {
    #[default]
    Ssl,
    StartTls,
}

impl Security {
    /// As an agent's file or the app writes it: `ssl` or `starttls`; empty
    /// is `ssl`.
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "ssl" => Ok(Self::Ssl),
            "starttls" => Ok(Self::StartTls),
            other => bail!("'{other}' is not a mail security: ssl or starttls"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ssl => "ssl",
            Self::StartTls => "starttls",
        }
    }
}

/// A role's mailbox, as the agent's file names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Mailbox {
    pub agent: String,
    /// Its address, which is also its login.
    pub address: String,
    /// The server it is read from (IMAP, TLS only).
    pub imap_host: String,
    pub imap_port: u16,
    /// The server it sends through: the IMAP one unless the file says.
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: Security,
}

impl Mailbox {
    /// The mailbox of the agent `agent`, from its file's `mailbox:` field
    /// (and `mail_server:`, `smtp_server:`, their ports and
    /// `smtp_security:`, for a provider other than OVH). None when it has
    /// none.
    pub fn of(root: &Path, agent: &str) -> Result<Option<Self>> {
        crate::realm::validate_name(agent).context("invalid agent name")?;
        let path = root.join("agents").join(format!("{agent}.md"));
        let text = std::fs::read_to_string(&path).with_context(|| format!("no agent {agent}"))?;
        let front = crate::frontmatter::parse(&text);
        let field = |key: &str| {
            front
                .text(key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(address) = field("mailbox") else {
            return Ok(None);
        };
        let port = |key: &str, default: u16| -> Result<u16> {
            match field(key) {
                None => Ok(default),
                Some(p) => p
                    .parse()
                    .ok()
                    .filter(|p| *p > 0)
                    .with_context(|| format!("{agent}'s {key} '{p}' is not a port")),
            }
        };
        let imap_host = field("mail_server").unwrap_or_else(|| IMAP_HOST.into());
        let smtp_security = Security::parse(&field("smtp_security").unwrap_or_default())
            .with_context(|| format!("{agent}'s smtp_security"))?;
        let smtp_default = match smtp_security {
            Security::Ssl => SMTP_PORT,
            Security::StartTls => SUBMISSION_PORT,
        };
        Self {
            agent: agent.to_string(),
            address,
            smtp_host: field("smtp_server").unwrap_or_else(|| imap_host.clone()),
            imap_host,
            imap_port: port("imap_port", IMAP_PORT)?,
            smtp_port: port("smtp_port", smtp_default)?,
            smtp_security,
        }
        .checked()
        .with_context(|| format!("{agent}'s mailbox"))
        .map(Some)
    }

    /// Itself, if its address is one and its servers are host names.
    pub fn checked(self) -> Result<Self> {
        if !valid_address(&self.address) {
            bail!("'{}' is not an e-mail address", self.address);
        }
        for (what, host) in [
            ("mail server", &self.imap_host),
            ("SMTP server", &self.smtp_host),
        ] {
            if !valid_host(host) {
                bail!("the {what} '{host}' is not a host name");
            }
        }
        if self.imap_port == 0 || self.smtp_port == 0 {
            bail!("a port is a number from 1 to 65535");
        }
        Ok(self)
    }

    /// The name of the realm secret holding its password.
    pub fn secret(&self) -> String {
        format!(
            "{SECRET_PREFIX}{}",
            self.agent.to_uppercase().replace('-', "_")
        )
    }

    /// The domain of its address, which names Airth to its servers.
    pub(crate) fn domain(&self) -> &str {
        self.address
            .split_once('@')
            .map(|(_, d)| d)
            .unwrap_or_default()
    }
}

/// A server's name: letters, digits, dots and dashes.
pub(crate) fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

pub(crate) fn valid_address(a: &str) -> bool {
    let Some((local, domain)) = a.split_once('@') else {
        return false;
    };
    // The domain is a host name of two labels or more, none of them empty.
    let labels: Vec<&str> = domain.split('.').collect();
    !local.is_empty()
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && !a.contains(char::is_whitespace)
        && !local.contains(['<', '>', '"', ',', ';', '@'])
}

/// A TLS connection to `host`, its certificate checked against the
/// system's roots, the handshake done: a server that cannot speak TLS, or
/// shows a certificate that is not its own, fails here, before a word is
/// sent. Each step gives up after `timeout`.
fn connect(
    host: &str,
    port: u16,
    timeout: std::time::Duration,
) -> Result<impl std::io::Read + std::io::Write> {
    use rustls_platform_verifier::ConfigVerifierExt;
    use std::net::ToSocketAddrs;
    let config =
        rustls::ClientConfig::with_platform_verifier().context("setting up TLS for mail")?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .with_context(|| format!("'{host}' is not a server name"))?;
    let mut conn = rustls::ClientConnection::new(std::sync::Arc::new(config), name)?;
    let mut tcp = Err(anyhow::anyhow!("cannot find {host}"));
    for addr in (host, port)
        .to_socket_addrs()
        .with_context(|| format!("cannot find {host}"))?
    {
        tcp = std::net::TcpStream::connect_timeout(&addr, timeout)
            .with_context(|| format!("cannot reach {host}:{port}"));
        if tcp.is_ok() {
            break;
        }
    }
    let mut tcp = tcp?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp)
            .with_context(|| format!("no secure connection with {host}:{port}"))?;
    }
    Ok(rustls::StreamOwned::new(conn, tcp))
}

/// The last reading of each mailbox since Airth started, by business and
/// agent. Settings shows it; nothing is stored.
static READS: std::sync::Mutex<Option<std::collections::HashMap<String, Reading>>> =
    std::sync::Mutex::new(None);

/// A mailbox's last reading: when, and what stopped it, if anything.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub at: String,
    pub error: Option<String>,
}

/// Note how reading `mailbox` in `realm` went.
pub fn note_read(realm: &str, mailbox: &Mailbox, error: Option<String>) {
    let mut reads = READS.lock().unwrap_or_else(|e| e.into_inner());
    reads.get_or_insert_with(Default::default).insert(
        format!("{realm}/{}/{}", mailbox.agent, mailbox.address),
        Reading {
            at: Utc::now().to_rfc3339(),
            error,
        },
    );
}

/// How the last reading of `mailbox` in `realm` went, since Airth started:
/// none for an address the agent was given since.
pub fn last_read(realm: &str, mailbox: &Mailbox) -> Option<Reading> {
    let reads = READS.lock().unwrap_or_else(|e| e.into_inner());
    reads
        .as_ref()?
        .get(&format!("{realm}/{}/{}", mailbox.agent, mailbox.address))
        .cloned()
}

/// `text` without `password` in any form a server could echo it: as typed,
/// as IMAP quotes it, and in base64 as SMTP's AUTH PLAIN and LOGIN send it.
pub(crate) fn redact(text: &str, address: &str, password: &str) -> String {
    if password.is_empty() {
        return text.to_string();
    }
    let forms = [
        crate::github::base64(format!("\0{address}\0{password}").as_bytes()),
        crate::github::base64(password.as_bytes()),
        password.replace('\\', "\\\\").replace('"', "\\\""),
        password.to_string(),
    ];
    forms
        .iter()
        .fold(text.to_string(), |t, f| t.replace(f.as_str(), "[password]"))
}

/// Fetch what is new in `mailbox` and keep it; what was kept, by id.
pub fn fetch(mailbox: &Mailbox, password: &str, store: &mut Store) -> Result<Vec<(i64, Mail)>> {
    let stream = connect(&mailbox.imap_host, mailbox.imap_port, TIMEOUT)?;
    fetch_over(stream, mailbox, password, store)
}

/// What [`fetch`] does, over any stream: the tests use plain TCP on
/// loopback.
fn fetch_over<S: std::io::Read + std::io::Write>(
    stream: S,
    mailbox: &Mailbox,
    password: &str,
    store: &mut Store,
) -> Result<Vec<(i64, Mail)>> {
    let mut imap = imap::Imap::open(stream)?;
    imap.login(&mailbox.address, password)?;
    let validity = imap.select_inbox()?;
    let from = store.last_mail_uid(&mailbox.address, validity)? + 1;
    let mut kept = Vec::new();
    for uid in imap.uids_from(from)? {
        let Some(raw) = imap.fetch(uid)? else {
            continue;
        };
        let mail = read(&raw);
        if let Some(id) = store.keep_mail(&NewMail {
            mailbox: &mailbox.address,
            agent: &mailbox.agent,
            uid_validity: validity,
            uid,
            mail: &mail,
            at: Utc::now(),
        })? {
            kept.push((id, mail));
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

/// Each kept mail that speaks of a change of bank details goes to the
/// founder in Needs you (and so on Telegram), without exception (rule 4).
/// What the mail asks is never done on its word: the founder checks it.
pub fn flag_bank_changes(store: &mut Store, mailbox: &Mailbox, kept: &[(i64, Mail)]) -> Result<()> {
    for (id, mail) in kept.iter().filter(|(_, m)| bank_change(m)) {
        let detail = format!(
            "From {}, \"{}\". It speaks of bank details. Change nothing on a mail's word: check by phone, with a number you already had.",
            if mail.from.is_empty() {
                "an unknown sender"
            } else {
                &mail.from
            },
            mail.subject
        );
        store.propose(&crate::business::store::autonomy::NewItem {
            kind: "incident",
            title: &format!("A mail to {} speaks of bank details", mailbox.address),
            agent: Some(&mailbox.agent),
            routine: None,
            run: None,
            action: None,
            args: &serde_json::json!({"detail": detail, "mail": id, "from": mail.from, "subject": mail.subject})
                .to_string(),
            execute_at: None,
            source: &serde_json::json!({"mail": id, "bank": true}).to_string(),
            model_call: None,
        })?;
    }
    Ok(())
}

/// The business's mailboxes that have their password, for sending; None
/// when it has none.
pub fn sender(realm: &crate::realm::Realm) -> Result<Option<crate::tools::mail::SendMail>> {
    let Ok(root) = realm.root() else {
        return Ok(None);
    };
    let mut mailboxes = Vec::new();
    for (agent, _) in crate::business::bots(&root)? {
        if let Ok(Some(m)) = Mailbox::of(&root, &agent)
            && let Some(password) = realm.secret(&m.secret())
        {
            mailboxes.push((m, password));
        }
    }
    if mailboxes.is_empty() {
        return Ok(None);
    }
    Ok(Some(crate::tools::mail::SendMail {
        mailboxes,
        business: crate::business::title(&root).unwrap_or_else(|| realm.name().to_string()),
        store: realm.dir().join(crate::business::store::FILE),
    }))
}

/// How often each mailbox is read.
const EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// The loop the service runs: every mailbox of every business is read
/// every few minutes; what stops one is told once, until it clears.
pub fn run(home: crate::home::Home) {
    let mut last: std::collections::HashMap<String, std::time::Instant> = Default::default();
    loop {
        if let Ok(realms) = home.realms() {
            for realm in realms.iter().filter(|r| !r.config.archived) {
                let Ok(root) = realm.root() else { continue };
                let agents = crate::business::bots(&root).unwrap_or_default();
                for (agent, _) in agents {
                    let Ok(Some(mailbox)) = Mailbox::of(&root, &agent) else {
                        continue;
                    };
                    let key = format!("mail/{}/{}", realm.name(), agent);
                    if last.get(&key).is_some_and(|at| at.elapsed() < EVERY) {
                        continue;
                    }
                    last.insert(key.clone(), std::time::Instant::now());
                    // A mailbox without its password waits for it, silently:
                    // Settings says what is missing.
                    let Some(password) = realm.secret(&mailbox.secret()) else {
                        continue;
                    };
                    let outcome = Store::of(realm).and_then(|mut store| {
                        let kept = fetch(&mailbox, &password, &mut store)?;
                        flag_bank_changes(&mut store, &mailbox, &kept)
                    });
                    note_read(
                        realm.name(),
                        &mailbox,
                        outcome
                            .as_ref()
                            .err()
                            .map(|e| redact(&format!("{e:#}"), &mailbox.address, &password)),
                    );
                    crate::supervise::report(
                        &home.inbox(),
                        &key,
                        realm.name(),
                        &format!("Reading {}", mailbox.address),
                        outcome,
                    );
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Whether a mail speaks of a change of bank details (rule 4, decision 70):
/// words in English and French, or an IBAN. A check in code, never a model,
/// so no mail can talk its way past it; it errs on the side of the founder.
pub fn bank_change(mail: &Mail) -> bool {
    const WORDS: &[&str] = &[
        "iban",
        "bic",
        "swift",
        "sort code",
        "account number",
        "routing number",
        "bank details",
        "banking details",
        "bank account",
        "new account",
        "payment details",
        "rib",
        "coordonnées bancaires",
        "coordonnees bancaires",
        "relevé d'identité bancaire",
        "releve d'identite bancaire",
        "nouveau compte",
        "changement de banque",
        "nouvelles coordonnées",
        "nouvelles coordonnees",
        "compte bancaire",
        "virement sur",
    ];
    let text = format!("{} {}", mail.subject, mail.text).to_lowercase();
    let word = |w: &str| {
        text.match_indices(w).any(|(i, _)| {
            let before = text[..i].chars().next_back();
            let after = text[i + w.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    };
    WORDS.iter().any(|w| word(w)) || has_iban(&text)
}

/// Whether the text holds an IBAN: two letters and two digits at the start
/// of a word, then letters and digits (single spaces allowed), whose check
/// digits hold (ISO 13616, modulo 97).
fn has_iban(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len()).any(|i| {
        let starts = i == 0 || !chars[i - 1].is_alphanumeric();
        let head = chars.get(i..i + 4).is_some_and(|h| {
            h[0].is_ascii_alphabetic()
                && h[1].is_ascii_alphabetic()
                && h[2].is_ascii_digit()
                && h[3].is_ascii_digit()
        });
        if !starts || !head {
            return false;
        }
        let mut code = Vec::new();
        let mut j = i;
        while j < chars.len() && code.len() < 34 {
            let c = chars[j];
            if c.is_ascii_alphanumeric() {
                code.push(c.to_ascii_uppercase());
            } else if c != ' ' || !chars.get(j + 1).is_some_and(char::is_ascii_alphanumeric) {
                break;
            }
            j += 1;
        }
        (15..=code.len()).any(|n| iban_valid(&code[..n]))
    })
}

fn iban_valid(code: &[char]) -> bool {
    let moved = code[4..].iter().chain(&code[..4]);
    let mut rest: u32 = 0;
    for c in moved {
        let v = match c.to_digit(36) {
            Some(v) => v,
            None => return false,
        };
        rest = if v < 10 {
            (rest * 10 + v) % 97
        } else {
            (rest * 100 + v) % 97
        };
    }
    rest == 1
}

/// A kept mail as a routine's item: who, what, its thread, and whether it
/// speaks of bank details.
pub fn item(k: &crate::business::store::mail::Kept, source: &str) -> crate::pipeline::run::Item {
    let m = &k.mail;
    let mut it = crate::pipeline::run::Item::new();
    let mut set = |key: &str, v: String| {
        it.insert(key.into(), v.into());
    };
    set(crate::routine::run::SOURCE, source.to_string());
    set("from", m.from.clone());
    set("to", m.to.join(", "));
    set("subject", m.subject.clone());
    set("text", m.text.clone());
    set("thread", m.thread.clone());
    set("message_id", m.message_id.clone().unwrap_or_default());
    set(
        "date",
        m.date.clone().unwrap_or_else(|| k.fetched_at.clone()),
    );
    set("attachments", m.attachments.join(", "));
    set(
        "bank_details",
        if bank_change(m) { "yes" } else { "no" }.into(),
    );
    it
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
            smtp_host: "127.0.0.1".into(),
            smtp_port: 0,
            smtp_security: Security::Ssl,
        };
        let run = |messages: Vec<(u32, Vec<u8>)>, store: &mut Store| {
            let (port, server) = imap::tests::server(3, messages);
            let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let kept = fetch_over(tcp, &mailbox, "right", store).unwrap().len();
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
        assert!(
            fetch_over(tcp, &mailbox, "right", &mut store)
                .unwrap()
                .is_empty()
        );
        server.join().unwrap();
    }

    #[test]
    fn a_change_of_bank_details_is_always_seen() {
        let said = |subject: &str, text: &str| {
            bank_change(&Mail {
                subject: subject.into(),
                text: text.into(),
                ..Default::default()
            })
        };
        assert!(said(
            "Nouvelles coordonnées bancaires",
            "Merci de mettre à jour."
        ));
        assert!(said("Invoice", "Please use our new account from now on."));
        assert!(said(
            "Facture",
            "Virement sur FR76 3000 6000 0112 3456 7890 189 svp"
        ));
        assert!(said("Re: lot 12", "Voici mon RIB pour le remboursement."));
        assert!(said("Update", "Our IBAN changed."));
        // Its item goes to the founder, saying where it is from.
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("business.db")).unwrap();
        let mailbox = Mailbox {
            agent: "support".into(),
            address: "support@n.example".into(),
            imap_host: "h".into(),
            imap_port: 1,
            smtp_host: "h".into(),
            smtp_port: 1,
            smtp_security: Security::Ssl,
        };
        let rib = Mail {
            from: "x@y.example".into(),
            subject: "Nouveau RIB".into(),
            text: "Merci.".into(),
            ..Default::default()
        };
        let fine = Mail {
            subject: "Où est mon lot ?".into(),
            ..Default::default()
        };
        flag_bank_changes(&mut store, &mailbox, &[(1, rib), (2, fine)]).unwrap();
        let items = store.queue(&["pending"]).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            (items[0].kind.as_str(), items[0].args["mail"].as_i64()),
            ("incident", Some(1))
        );
        assert!(
            items[0].args["detail"]
                .as_str()
                .unwrap()
                .contains("x@y.example")
        );
        // Words inside others are not words: no flag.
        assert!(!said(
            "Où est mon lot ?",
            "Bonjour, je n'ai pas reçu le colis. Merci, Ribeiro"
        ));
        assert!(!said(
            "Order 123456789",
            "Tracking 1Z999AA10123456784 please"
        ));
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
        // Another provider: its server and ports, as the agent's own fields,
        // which do not set its file aside.
        std::fs::write(
            agents.join("own.md"),
            "---\ntitle: Own\nmailbox: hi@own.example\nmail_server: mail.own.example\nsmtp_port: 587\n---\nx\n",
        )
        .unwrap();
        let own = Mailbox::of(root.path(), "own").unwrap().unwrap();
        assert_eq!(
            (own.imap_host.as_str(), own.imap_port, own.smtp_port),
            ("mail.own.example", 993, 587)
        );
        // A file from before decision 154 reads as it did: SMTP on the IMAP
        // server, TLS from the first byte.
        assert_eq!(
            (own.smtp_host.as_str(), own.smtp_security),
            ("mail.own.example", Security::Ssl)
        );
        assert_eq!(
            (m.smtp_host.as_str(), m.smtp_port, m.smtp_security),
            ("ssl0.ovh.net", 465, Security::Ssl)
        );
        // Its own SMTP server, by STARTTLS: on 587 unless it says.
        std::fs::write(
            agents.join("split.md"),
            "---\ntitle: Split\nmailbox: hi@split.example\nmail_server: imap.split.example\nsmtp_server: smtp.split.example\nsmtp_security: starttls\n---\nx\n",
        )
        .unwrap();
        let split = Mailbox::of(root.path(), "split").unwrap().unwrap();
        assert_eq!(
            (
                split.smtp_host.as_str(),
                split.smtp_port,
                split.smtp_security
            ),
            ("smtp.split.example", 587, Security::StartTls)
        );
        // Plain text is not a security, and a server is a host name.
        for bad in [
            "smtp_security: none",
            "smtp_security: plain",
            "smtp_server: a b",
        ] {
            std::fs::write(
                agents.join("bad.md"),
                format!("---\ntitle: Bad\nmailbox: hi@bad.example\n{bad}\n---\nx\n"),
            )
            .unwrap();
            assert!(Mailbox::of(root.path(), "bad").is_err(), "{bad}");
        }
        std::fs::remove_file(agents.join("bad.md")).unwrap();
        let config = crate::business::config::load(root.path(), &[]).unwrap();
        assert!(
            config.broken.iter().all(|b| !b.contains("own")),
            "{:?}",
            config.broken
        );
    }

    #[test]
    fn an_address_is_one_local_part_at_one_host_name() {
        for ok in [
            "support@n.example",
            "first.last@mail.n-a.example",
            "a+b@x.co",
        ] {
            assert!(valid_address(ok), "{ok}");
        }
        for bad in [
            "a@@example.com",
            "a@.example.com",
            "a@example.",
            "a@example",
            "@example.com",
            ".a@example.com",
            "a..b@example.com",
            "a@-x.example",
            "a b@example.com",
            "a@b.example\r\nBcc: x@y.example",
        ] {
            assert!(!valid_address(bad), "{bad}");
        }
    }
}
