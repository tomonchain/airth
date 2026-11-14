//! A small IMAP client (decision 70): only what reading a mailbox needs,
//! `LOGIN`, `SELECT`, `UID SEARCH` and `UID FETCH BODY.PEEK[]`, so the server
//! keeps its unread state and nothing on it is moved or deleted. Blocking,
//! over any stream: TLS in use, plain TCP in the tests only.

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Read, Write};

/// The longest line read from a server: a line past it is refused, so a
/// server cannot make Airth hold an endless line.
const MAX_LINE: usize = 64 * 1024;
/// The largest message fetched; a larger one is refused, not cut.
pub const MAX_MESSAGE: usize = 25 * 1024 * 1024;

pub struct Imap<S: Read + Write> {
    stream: BufReader<S>,
    tag: u32,
}

/// One response line, with the literals (`{n}` and the n bytes after it)
/// it carried, in order.
#[derive(Debug, Default)]
struct Line {
    text: String,
    literals: Vec<Vec<u8>>,
}

/// A string as IMAP quotes it. A line break could end the command early, so
/// it is refused, never escaped.
fn quoted(s: &str) -> Result<String> {
    if s.contains(['\r', '\n', '\0']) {
        bail!("a mailbox name or password cannot hold a line break");
    }
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

impl<S: Read + Write> Imap<S> {
    /// Over a fresh connection: the server's greeting must be `OK`.
    pub fn open(stream: S) -> Result<Self> {
        let mut imap = Self {
            stream: BufReader::new(stream),
            tag: 0,
        };
        let hello = imap.line()?;
        if !hello.text.starts_with("* OK") {
            bail!("the mail server did not greet: {}", first(&hello.text));
        }
        Ok(imap)
    }

    pub fn login(&mut self, user: &str, password: &str) -> Result<()> {
        let cmd = format!("LOGIN {} {}", quoted(user)?, quoted(password)?);
        // The command holds the password: an error never repeats it.
        self.command(&cmd).map(|_| ()).map_err(|e| {
            anyhow::anyhow!(
                "the mail server refused the login for {user}: {}",
                e.to_string().replace(password, "[password]")
            )
        })
    }

    /// Open the inbox, read only; its UIDVALIDITY, which names a mailbox's
    /// numbering: when it changes, every UID before it means nothing.
    pub fn select_inbox(&mut self) -> Result<u32> {
        let lines = self.command("EXAMINE INBOX")?;
        lines
            .iter()
            .find_map(|l| {
                l.text
                    .split_once("[UIDVALIDITY ")
                    .and_then(|(_, rest)| rest.split(']').next())
                    .and_then(|n| n.trim().parse().ok())
            })
            .context("the mail server gave no UIDVALIDITY")
    }

    /// The UIDs from `from` on, oldest first. `n:*` names the last message
    /// even when it is older than n: those are left out.
    pub fn uids_from(&mut self, from: u32) -> Result<Vec<u32>> {
        let lines = self.command(&format!("UID SEARCH UID {from}:*"))?;
        let mut uids: Vec<u32> = lines
            .iter()
            .filter_map(|l| l.text.strip_prefix("* SEARCH"))
            .flat_map(|rest| rest.split_whitespace().filter_map(|n| n.parse().ok()))
            .filter(|uid| *uid >= from)
            .collect();
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// The whole message `uid`, as the sender sent it, without marking it
    /// read. None when the server has no such message any more.
    pub fn fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>> {
        let lines = self.command(&format!("UID FETCH {uid} (UID BODY.PEEK[])"))?;
        Ok(lines
            .into_iter()
            .find(|l| l.text.starts_with("* ") && l.text.contains("FETCH"))
            .and_then(|l| l.literals.into_iter().next()))
    }

    pub fn logout(&mut self) {
        let _ = self.command("LOGOUT");
    }

    /// Send one command; the untagged lines before its tagged answer, if
    /// that answer is OK.
    fn command(&mut self, cmd: &str) -> Result<Vec<Line>> {
        self.tag += 1;
        let tag = format!("a{}", self.tag);
        let out = self.stream.get_mut();
        out.write_all(format!("{tag} {cmd}\r\n").as_bytes())?;
        out.flush()?;
        let mut lines = Vec::new();
        loop {
            let line = self.line()?;
            if let Some(rest) = line.text.strip_prefix(&format!("{tag} ")) {
                if rest.starts_with("OK") {
                    return Ok(lines);
                }
                bail!("{}", first(rest));
            }
            lines.push(line);
        }
    }

    /// One response line, its literals read in full.
    fn line(&mut self) -> Result<Line> {
        let mut out = Line::default();
        loop {
            let mut raw = Vec::new();
            let n = (&mut self.stream)
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut raw)?;
            if n == 0 {
                bail!("the mail server closed the connection");
            }
            if raw.len() > MAX_LINE {
                bail!("the mail server sent a line too long to read");
            }
            let text = String::from_utf8_lossy(&raw);
            let text = text.trim_end_matches(['\r', '\n']);
            out.text.push_str(text);
            // A literal: `{n}` at the end of the line, then n bytes.
            let Some(size) = text
                .strip_suffix('}')
                .and_then(|t| t.rsplit_once('{'))
                .and_then(|(_, n)| n.parse::<usize>().ok())
            else {
                return Ok(out);
            };
            if size > MAX_MESSAGE {
                bail!("the mail server sent a message over 25 MB: it is not read");
            }
            let mut literal = vec![0; size];
            self.stream.read_exact(&mut literal)?;
            out.literals.push(literal);
        }
    }
}

/// A server's line, short enough for an error.
fn first(text: &str) -> String {
    text.chars().take(200).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    /// A fake IMAP server on 127.0.0.1 holding `messages` (UID, raw mail)
    /// under `validity`; it answers one connection, then stops. What the
    /// client sent comes back once the connection closes.
    pub(crate) fn server(
        validity: u32,
        messages: Vec<(u32, Vec<u8>)>,
    ) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut seen = Vec::new();
            out.write_all(b"* OK fake IMAP ready\r\n").unwrap();
            for line in BufReader::new(stream).lines() {
                let line = line.unwrap();
                seen.push(line.clone());
                let (tag, cmd) = line.split_once(' ').unwrap();
                let mut answer = String::new();
                if cmd.starts_with("LOGIN") {
                    if !cmd.contains("\"right\"") {
                        answer = format!("{tag} NO [AUTHENTICATIONFAILED] {cmd}\r\n");
                    }
                } else if cmd.starts_with("EXAMINE") {
                    answer.push_str(&format!("* OK [UIDVALIDITY {validity}] UIDs valid\r\n"));
                } else if let Some(range) = cmd.strip_prefix("UID SEARCH UID ") {
                    let from: u32 = range.split(':').next().unwrap().parse().unwrap();
                    let last = messages.last().map_or(0, |m| m.0);
                    // As servers do, `n:*` names the last message even below n.
                    let found: Vec<String> = messages
                        .iter()
                        .map(|m| m.0)
                        .filter(|u| *u >= from || *u == last)
                        .map(|u| u.to_string())
                        .collect();
                    answer.push_str(&format!("* SEARCH {}\r\n", found.join(" ")));
                } else if let Some(rest) = cmd.strip_prefix("UID FETCH ") {
                    let uid: u32 = rest.split(' ').next().unwrap().parse().unwrap();
                    if let Some((_, raw)) = messages.iter().find(|m| m.0 == uid) {
                        answer.push_str(&format!(
                            "* 1 FETCH (UID {uid} BODY[] {{{}}}\r\n",
                            raw.len()
                        ));
                        out.write_all(answer.as_bytes()).unwrap();
                        out.write_all(raw).unwrap();
                        answer = ")\r\n".into();
                    }
                } else if cmd.starts_with("LOGOUT") {
                    out.write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())
                        .unwrap();
                    break;
                }
                if !answer.contains(&format!("{tag} ")) {
                    answer.push_str(&format!("{tag} OK done\r\n"));
                }
                out.write_all(answer.as_bytes()).unwrap();
            }
            seen
        });
        (port, handle)
    }

    #[test]
    fn reads_new_messages_whole_without_marking_them() {
        let mail = b"Subject: Hi\r\n\r\nA line {3}\r\nand more.\r\n".to_vec();
        let (port, server) = server(7, vec![(4, b"old".to_vec()), (5, mail.clone())]);
        let mut imap = Imap::open(TcpStream::connect(("127.0.0.1", port)).unwrap()).unwrap();
        imap.login("support@example.com", "right").unwrap();
        assert_eq!(imap.select_inbox().unwrap(), 7);
        assert_eq!(imap.uids_from(5).unwrap(), vec![5]);
        // The last message only, though `5:*` would name it anyway.
        assert!(imap.uids_from(6).unwrap().is_empty());
        assert_eq!(imap.fetch(5).unwrap(), Some(mail));
        assert_eq!(imap.fetch(9).unwrap(), None);
        imap.logout();
        let seen = server.join().unwrap();
        // Read only: EXAMINE, never SELECT; PEEK, never a plain BODY[].
        assert!(seen.iter().any(|l| l.ends_with("EXAMINE INBOX")));
        assert!(
            seen.iter()
                .all(|l| !l.contains(" SELECT ") && !l.contains("STORE"))
        );
        assert!(
            seen.iter()
                .filter(|l| l.contains("FETCH"))
                .all(|l| l.contains("BODY.PEEK[]"))
        );
    }

    #[test]
    fn a_refused_login_never_repeats_the_password() {
        let (port, server) = server(1, vec![]);
        let mut imap = Imap::open(TcpStream::connect(("127.0.0.1", port)).unwrap()).unwrap();
        let err = format!(
            "{:#}",
            imap.login("support@example.com", "s3cret-pass")
                .unwrap_err()
        );
        assert!(err.contains("refused"), "{err}");
        assert!(!err.contains("s3cret-pass"), "{err}");
        imap.logout();
        server.join().unwrap();
        // A line break would end the command early: refused before sending.
        assert!(quoted("a\r\nb").is_err());
        assert_eq!(quoted(r#"a"b\c"#).unwrap(), r#""a\"b\\c""#);
    }
}
