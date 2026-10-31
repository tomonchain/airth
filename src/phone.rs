//! Reaching the app from the owner's phone (decision 37). The app listens on
//! loopback only; Tailscale (`tailscale serve`) brings it over HTTPS to the
//! owner's own devices, under the Mac's name in their tailnet. Airth accepts
//! requests for the host names the owner allowed here, beside loopback, and
//! only names of a tailnet (`*.ts.net`): a typo cannot open the app to a
//! domain someone else controls. The API token is still required; the phone
//! gets it once, from a pairing link shown as a QR code.

use crate::realm::write_atomic;
use anyhow::{Context, Result, bail};
use qrcode::QrCode;
use std::path::{Path, PathBuf};

const FILE: &str = "phone-hosts.json";
const TAILNET: &str = ".ts.net";

fn path(home: &Path) -> PathBuf {
    home.join(FILE)
}

/// The host names the owner allowed, in the order they were added.
pub fn hosts(home: &Path) -> Result<Vec<String>> {
    match std::fs::read_to_string(path(home)) {
        Ok(text) => serde_json::from_str(&text).context("parsing the phone's host names"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).context("reading the phone's host names"),
    }
}

/// A host name as Tailscale gives it: `<machine>.<tailnet>.ts.net`, lower case.
pub fn check(host: &str) -> Result<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let valid_label = |l: &&str| {
        !l.is_empty()
            && l.len() <= 63
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !l.starts_with('-')
            && !l.ends_with('-')
    };
    if !host.ends_with(TAILNET) || labels.len() < 4 || !labels.iter().all(valid_label) {
        bail!(
            "'{host}' is not a Tailscale name: give the Mac's name in your tailnet, like mac.tail1234.ts.net (`tailscale status` shows it)"
        );
    }
    Ok(host)
}

pub fn allow(home: &Path, host: &str) -> Result<String> {
    let host = check(host)?;
    let mut list = hosts(home)?;
    if !list.contains(&host) {
        list.push(host.clone());
        write_atomic(&path(home), &serde_json::to_string_pretty(&list)?)?;
    }
    Ok(host)
}

/// Stop accepting `host`. Whether it was allowed.
pub fn forget(home: &Path, host: &str) -> Result<bool> {
    let host = host.trim().to_ascii_lowercase();
    let mut list = hosts(home)?;
    let before = list.len();
    list.retain(|h| *h != host);
    if list.len() == before {
        return Ok(false);
    }
    write_atomic(&path(home), &serde_json::to_string_pretty(&list)?)?;
    Ok(true)
}

/// Whether a request's Host header names the app: loopback on its port, or
/// an allowed name served by Tailscale over HTTPS (no port, or 443).
pub fn host_allowed(header: &str, port: u16, allowed: &[String]) -> bool {
    if header == format!("127.0.0.1:{port}") || header == format!("localhost:{port}") {
        return true;
    }
    let name = header
        .strip_suffix(":443")
        .unwrap_or(header)
        .to_ascii_lowercase();
    allowed.contains(&name)
}

/// The link that pairs the phone: the app over HTTPS, carrying the token in
/// its fragment, which the browser never sends to a server.
pub fn pairing_link(host: &str, token: &str) -> String {
    format!("https://{host}/#token={token}")
}

/// A QR code of `text`: its width, and its modules row by row (true: dark).
pub fn qr(text: &str) -> Result<(usize, Vec<bool>)> {
    let code = QrCode::new(text.as_bytes()).context("making the QR code")?;
    let cells = code
        .to_colors()
        .into_iter()
        .map(|c| c == qrcode::Color::Dark)
        .collect();
    Ok((code.width(), cells))
}

/// A QR code of `text` drawn for a terminal.
pub fn qr_text(text: &str) -> Result<String> {
    let code = QrCode::new(text.as_bytes()).context("making the QR code")?;
    Ok(code
        .render::<qrcode::render::unicode::Dense1x2>()
        .quiet_zone(true)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tailnet_names_are_allowed() {
        assert_eq!(
            check(" Mac.Tail1234.ts.net. ").unwrap(),
            "mac.tail1234.ts.net"
        );
        for bad in [
            "example.com",
            "ts.net",
            "x.ts.net",
            "evil.com.ts.net.evil.com",
            "a b.tail.ts.net",
            "-a.tail1.ts.net",
            "",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_owner_allows_and_forgets_hosts() {
        let home = tempfile::tempdir().unwrap();
        assert!(hosts(home.path()).unwrap().is_empty());
        allow(home.path(), "mac.tail1234.ts.net").unwrap();
        allow(home.path(), "MAC.tail1234.ts.net").unwrap();
        assert_eq!(hosts(home.path()).unwrap(), vec!["mac.tail1234.ts.net"]);
        assert!(allow(home.path(), "attacker.example").is_err());
        assert!(forget(home.path(), "mac.tail1234.ts.net").unwrap());
        assert!(!forget(home.path(), "mac.tail1234.ts.net").unwrap());
        assert!(hosts(home.path()).unwrap().is_empty());
    }

    #[test]
    fn requests_name_loopback_or_an_allowed_host() {
        let allowed = vec!["mac.tail1234.ts.net".to_string()];
        assert!(host_allowed("127.0.0.1:7457", 7457, &[]));
        assert!(host_allowed("localhost:7457", 7457, &[]));
        assert!(!host_allowed("127.0.0.1:8000", 7457, &[]));
        assert!(host_allowed("mac.tail1234.ts.net", 7457, &allowed));
        assert!(host_allowed("MAC.tail1234.ts.net:443", 7457, &allowed));
        assert!(!host_allowed("mac.tail1234.ts.net", 7457, &[]));
        // DNS rebinding: another name pointing at the Mac is still refused.
        assert!(!host_allowed("evil.example", 7457, &allowed));
        assert!(!host_allowed(
            "mac.tail1234.ts.net.evil.example",
            7457,
            &allowed
        ));
    }

    #[test]
    fn the_pairing_link_carries_the_token_in_its_fragment() {
        let link = pairing_link("mac.tail1234.ts.net", "abc123");
        assert_eq!(link, "https://mac.tail1234.ts.net/#token=abc123");
        let (width, cells) = qr(&link).unwrap();
        assert_eq!(cells.len(), width * width);
        assert!(cells.iter().any(|c| *c) && cells.iter().any(|c| !*c));
        assert!(qr_text(&link).unwrap().contains('█'));
    }
}
