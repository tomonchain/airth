//! Trying a mailbox before it is saved (decision 154): the IMAP login and
//! the SMTP login, each over TLS, nothing read and nothing sent. What
//! failed is said in plain words, the password never in them.

use super::{Mailbox, Security, imap::Imap, redact};
use lettre::transport::smtp::authentication::{Credentials, DEFAULT_MECHANISMS};
use lettre::transport::smtp::client::{SmtpConnection, TlsParameters};
use lettre::transport::smtp::extension::ClientId;
use std::time::Duration;

/// Each connection gives up after this long without an answer.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Whether `mailbox` takes `password` on both its servers: IMAP first,
/// then SMTP. The error names the step: "IMAP: login refused (…)".
pub fn check(mailbox: &Mailbox, password: &str) -> anyhow::Result<()> {
    let plain = |e: String| anyhow::anyhow!("{}", redact(&e, &mailbox.address, password));
    imap(mailbox, password).map_err(|e| plain(format!("IMAP: {e}")))?;
    smtp(mailbox, password).map_err(|e| plain(format!("SMTP: {e}")))
}

fn imap(m: &Mailbox, password: &str) -> Result<(), String> {
    let at = format!("{}:{}", m.imap_host, m.imap_port);
    // The tests' fake server, on loopback: plain IMAP there, in tests only.
    #[cfg(test)]
    if m.imap_host == "127.0.0.1" {
        let tcp = std::net::TcpStream::connect(("127.0.0.1", m.imap_port))
            .map_err(|_| format!("could not connect to {at}"))?;
        return imap_over(tcp, m, password, &at);
    }
    let stream = super::connect(&m.imap_host, m.imap_port, TIMEOUT)
        .map_err(|e| format!("could not connect to {at} ({})", e.root_cause()))?;
    imap_over(stream, m, password, &at)
}

fn imap_over<S: std::io::Read + std::io::Write>(
    stream: S,
    m: &Mailbox,
    password: &str,
    at: &str,
) -> Result<(), String> {
    let mut imap =
        Imap::open(stream).map_err(|e| format!("{at} did not answer as a mail server ({e})"))?;
    imap.login(&m.address, password).map_err(|e| {
        // "the mail server refused the login for <address>: <its words>"
        let said = e.to_string();
        let words = said.split_once(": ").map_or(said.as_str(), |(_, w)| w);
        format!("login refused ({words})")
    })?;
    imap.select_inbox()
        .map_err(|e| format!("logged in, but the inbox did not open ({e})"))?;
    imap.logout();
    Ok(())
}

fn smtp(m: &Mailbox, password: &str) -> Result<(), String> {
    let at = format!("{}:{}", m.smtp_host, m.smtp_port);
    let hello = ClientId::Domain(m.domain().to_string());
    let tls = TlsParameters::new(m.smtp_host.clone())
        .map_err(|_| format!("'{}' is not a server name", m.smtp_host))?;
    // TLS from the first byte, or STARTTLS before the login: never plain.
    let (wrapper, upgrade) = match m.smtp_security {
        Security::Ssl => (Some(&tls), false),
        Security::StartTls => (None, true),
    };
    // The tests' fake server, on loopback: plain SMTP there, in tests only.
    #[cfg(test)]
    let (wrapper, upgrade) = if m.smtp_host == "127.0.0.1" {
        (None, false)
    } else {
        (wrapper, upgrade)
    };
    let mut conn = SmtpConnection::connect(
        (m.smtp_host.as_str(), m.smtp_port),
        Some(TIMEOUT),
        &hello,
        wrapper,
        None,
    )
    .map_err(|e| format!("could not connect to {at} ({e})"))?;
    if upgrade {
        // Refused when the server does not offer it: the login is never
        // sent over a connection that stayed plain.
        conn.starttls(&tls, &hello)
            .map_err(|e| format!("{at} did not start TLS ({e})"))?;
    }
    conn.auth(
        DEFAULT_MECHANISMS,
        &Credentials::new(m.address.clone(), password.to_string()),
    )
    .map_err(|e| format!("login refused ({e})"))?;
    let _ = conn.quit();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    pub(crate) fn mailbox(imap_port: u16, smtp_port: u16) -> Mailbox {
        Mailbox {
            agent: "chief".into(),
            address: "contact@n.example".into(),
            imap_host: "127.0.0.1".into(),
            imap_port,
            smtp_host: "127.0.0.1".into(),
            smtp_port,
            smtp_security: Security::Ssl,
        }
    }

    #[test]
    fn both_logins_are_tried_and_nothing_is_read_or_sent() {
        let (imap_port, imap_seen) = crate::mail::imap::tests::server(1, vec![]);
        let (smtp_port, smtp_seen) = crate::mail::send::tests::server();
        check(&mailbox(imap_port, smtp_port), "right").unwrap();
        let seen = imap_seen.join().unwrap();
        assert!(seen.iter().any(|l| l.contains("LOGIN")), "{seen:?}");
        assert!(
            seen.iter().any(|l| l.ends_with("EXAMINE INBOX")),
            "{seen:?}"
        );
        assert!(seen.iter().all(|l| !l.contains("FETCH")), "{seen:?}");
        let (rcpt, data) = smtp_seen.join().unwrap();
        assert!(rcpt.is_empty() && data.is_empty(), "{rcpt:?} {data}");
    }

    #[test]
    fn a_refused_login_says_which_and_never_the_password() {
        // IMAP refuses: SMTP is never tried, and the server's echo of the
        // command is not repeated with the password in it.
        let (imap_port, imap_seen) = crate::mail::imap::tests::server(1, vec![]);
        let err = check(&mailbox(imap_port, 1), r#"s3cret"pass"#)
            .unwrap_err()
            .to_string();
        imap_seen.join().unwrap();
        assert!(err.starts_with("IMAP: login refused"), "{err}");
        assert!(!err.contains("s3cret"), "{err}");
        // SMTP refuses, and echoes the AUTH line: neither the password nor
        // its base64 comes back.
        let (imap_port, imap_seen) = crate::mail::imap::tests::server(1, vec![]);
        let (smtp_port, smtp_seen) = crate::mail::send::tests::server_refusing();
        let err = check(&mailbox(imap_port, smtp_port), "right")
            .unwrap_err()
            .to_string();
        imap_seen.join().unwrap();
        smtp_seen.join().unwrap();
        assert!(err.starts_with("SMTP: login refused"), "{err}");
        assert!(err.contains("[password]"), "{err}");
        assert!(!err.contains("right"), "{err}");
        assert!(
            !err.contains(&crate::github::base64(b"\0contact@n.example\0right")),
            "{err}"
        );
        // Nothing listening: which server, and where.
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        let err = check(&mailbox(port, 1), "right").unwrap_err().to_string();
        assert_eq!(err, format!("IMAP: could not connect to 127.0.0.1:{port}"));
    }

    #[test]
    fn redaction_takes_every_form_of_the_password() {
        let said = format!(
            "NO \"a\\\"b\" {} {} a\"b",
            crate::github::base64(b"\0x@n.example\0a\"b"),
            crate::github::base64(b"a\"b")
        );
        let out = redact(&said, "x@n.example", "a\"b");
        assert_eq!(out, "NO \"[password]\" [password] [password] [password]");
        assert_eq!(redact("as is", "x@n.example", ""), "as is");
    }

    /// A server that takes whatever comes on loopback and answers with a
    /// plain greeting; what it received, once the client gives up.
    fn plain_listener(greeting: &'static [u8]) -> (u16, std::thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let _ = stream.write_all(greeting);
            let mut got = Vec::new();
            let mut buf = [0; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                // A client that stays plain would log in now: say yes to
                // anything, so it would go on.
                let _ = stream.write_all(b"250 ok\r\n");
            }
            got
        });
        (port, handle)
    }

    /// No path sends a login in plain text: away from the tests' loopback
    /// address, IMAP and SMTP over SSL want TLS from the first byte, and
    /// STARTTLS refuses a server that does not offer it, before the login.
    #[test]
    fn a_login_never_goes_in_plain_text() {
        let away = |imap_port: u16, smtp_port: u16, security: Security| Mailbox {
            imap_host: "localhost".into(),
            smtp_host: "localhost".into(),
            smtp_security: security,
            ..mailbox(imap_port, smtp_port)
        };
        let (port, got) = plain_listener(b"* OK plain IMAP\r\n");
        let err = check(&away(port, 1, Security::Ssl), "right")
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("IMAP: could not connect"), "{err}");
        assert!(!String::from_utf8_lossy(&got.join().unwrap()).contains("LOGIN"));
        // SMTP over SSL to a plain server.
        let (port, got) = plain_listener(b"220 plain ESMTP\r\n");
        let err = smtp(&away(1, port, Security::Ssl), "right").unwrap_err();
        assert!(err.starts_with("could not connect"), "{err}");
        assert!(!String::from_utf8_lossy(&got.join().unwrap()).contains("AUTH"));
        // STARTTLS to a server that does not offer it.
        let (port, got) = plain_listener(b"220 plain ESMTP\r\n");
        let err = smtp(&away(1, port, Security::StartTls), "right").unwrap_err();
        assert!(err.contains("did not start TLS"), "{err}");
        let got = String::from_utf8_lossy(&got.join().unwrap()).to_string();
        assert!(got.starts_with("EHLO"), "{got}");
        assert!(!got.contains("AUTH"), "{got}");
        // And a third kind of security is not one.
        assert!(Security::parse("none").is_err());
        assert!(Security::parse("plain").is_err());
        assert_eq!(Security::parse(" STARTTLS ").unwrap(), Security::StartTls);
        assert_eq!(Security::parse("").unwrap(), Security::Ssl);
    }
}
