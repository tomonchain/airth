//! Airth.app as a window on an Airth that runs elsewhere (decision 112).
//! With a server set, the app opens that server's page and starts nothing
//! on the Mac: no second Airth runs the same routines or answers the same
//! Telegram bot. Without one, it works as before, on this Mac's Airth.

use crate::home::Home;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const FILE: &str = "remote.json";

/// The server the app opens, and its key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Remote {
    pub host: String,
    token: String,
}

impl Remote {
    /// From a pairing link (`https://<name>.ts.net/#token=<key>`), as
    /// `airth phone pair` and the Phone settings give it. Only a tailnet
    /// name is taken: Airth is reached through Tailscale, never the open
    /// internet (decision 37).
    pub fn parse(link: &str) -> Result<Remote> {
        let link = link.trim();
        let rest = link
            .strip_prefix("https://")
            .context("a pairing link starts with https://")?;
        let (host, fragment) = rest
            .split_once("/#")
            .context("a pairing link ends with #token= and the key")?;
        let token = fragment
            .strip_prefix("token=")
            .context("a pairing link ends with #token= and the key")?;
        if token.len() < 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("the key in this link is not one Airth makes");
        }
        Ok(Remote {
            host: crate::phone::check(host.trim_end_matches('/'))?,
            token: token.to_string(),
        })
    }

    /// The page, with its key for the page to keep.
    #[cfg(target_os = "macos")]
    pub fn link(&self) -> String {
        crate::phone::pairing_link(&self.host, &self.token)
    }

    pub fn origin(&self) -> String {
        format!("https://{}", self.host)
    }

    /// What the server says of itself: unread notifications, the newest
    /// approval among them, and the name the founder gave it.
    pub fn seen(&self) -> Result<Seen> {
        let info: serde_json::Value = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?
            .get(format!("{}/api/info", self.origin()))
            .header("X-Airth-Token", &self.token)
            .send()
            .with_context(|| format!("{} does not answer: is Tailscale on?", self.host))?
            .error_for_status()
            .with_context(|| format!("{} refused the key", self.host))?
            .json()?;
        Ok(Seen::of(&info))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub unread: usize,
    pub waiting: Option<u64>,
    pub name: Option<String>,
}

impl Seen {
    fn of(info: &serde_json::Value) -> Seen {
        Seen {
            unread: info["unread"].as_u64().unwrap_or(0) as usize,
            waiting: info["waiting"].as_u64(),
            name: info["name"].as_str().map(String::from),
        }
    }
}

/// The server the app opens, if one is set.
pub fn load(home: &Home) -> Option<Remote> {
    std::fs::read_to_string(home.dir().join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// Set the server the app opens, or with `None` go back to this Mac's Airth.
/// The file holds the key: only the founder reads it.
pub fn save(home: &Home, remote: Option<&Remote>) -> Result<()> {
    let path = home.dir().join(FILE);
    match remote {
        Some(r) => crate::realm::write_private(&path, &serde_json::to_string_pretty(r)?),
        None => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_pairing_link_gives_the_server_and_its_key() {
        let r = Remote::parse(&format!(" https://VPS.tail1f6e69.ts.net/#token={KEY} ")).unwrap();
        assert_eq!(r.host, "vps.tail1f6e69.ts.net");
        assert_eq!(r.origin(), "https://vps.tail1f6e69.ts.net");
        #[cfg(target_os = "macos")]
        assert_eq!(
            r.link(),
            format!("https://vps.tail1f6e69.ts.net/#token={KEY}")
        );
    }

    #[test]
    fn only_a_tailnet_link_with_a_real_key_is_taken() {
        for bad in [
            format!("http://vps.tail1f6e69.ts.net/#token={KEY}"),
            format!("https://example.com/#token={KEY}"),
            format!("https://vps.tail1f6e69.ts.net.evil.com/#token={KEY}"),
            "https://vps.tail1f6e69.ts.net/".to_string(),
            "https://vps.tail1f6e69.ts.net/#token=abc".to_string(),
            "https://vps.tail1f6e69.ts.net/#token=zz23456789abcdef0123456789abcdef".to_string(),
        ] {
            assert!(Remote::parse(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_server_is_kept_private_and_can_be_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        assert_eq!(load(&home), None);
        let r = Remote::parse(&format!("https://vps.tail1f6e69.ts.net/#token={KEY}")).unwrap();
        save(&home, Some(&r)).unwrap();
        assert_eq!(load(&home), Some(r));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(home.dir().join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "only the founder reads the key");
        }
        save(&home, None).unwrap();
        assert_eq!(load(&home), None);
        save(&home, None).unwrap();
    }

    #[test]
    fn the_server_says_what_waits() {
        let info = serde_json::json!({"unread": 3, "waiting": 12, "name": "VPS"});
        assert_eq!(
            Seen::of(&info),
            Seen {
                unread: 3,
                waiting: Some(12),
                name: Some("VPS".into())
            }
        );
        let none = Seen::of(&serde_json::json!({"name": null}));
        assert_eq!((none.unread, none.waiting, none.name), (0, None, None));
    }
}
