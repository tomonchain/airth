//! Connectors' logos, fetched by Airth itself (decision 122): a connector
//! that Airth carries no logo for gets one from Simple Icons (CC0), else
//! from its own site's icon, fetched once by the server and kept in the
//! home. The app reads it from Airth, never from a third party: the
//! founder's devices tell no one what they look at, and the page's rule
//! against remote images (decision 20) stands.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The largest logo kept.
const MAX_BYTES: usize = 256 * 1024;
/// How long "none found" is remembered before Airth looks again.
const RETRY: Duration = Duration::from_secs(7 * 24 * 3600);

/// The kinds of image kept, by extension.
const KINDS: &[(&str, &str)] = &[
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("ico", "image/x-icon"),
];

fn dir(home: &Path) -> PathBuf {
    home.join("logos")
}

/// A connector's id as a file name and a Simple Icons slug: lowercase
/// letters and digits only.
fn slug(id: &str) -> Result<String> {
    let s: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if s.is_empty() || s.len() > 40 {
        bail!("not a connector's name");
    }
    Ok(s)
}

/// The site a connector belongs to, from its server's host: `mcp.canva.com`
/// is `canva.com`. A name only, never an address or a port.
fn site(domain: &str) -> Option<String> {
    let d = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    let ok = d.contains('.')
        && d.len() <= 253
        && d.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && !d.split('.').all(|l| l.chars().all(|c| c.is_ascii_digit()));
    if !ok {
        return None;
    }
    let parts: Vec<&str> = d.split('.').collect();
    let trimmed = match parts.first() {
        Some(&("mcp" | "api" | "www" | "app")) if parts.len() > 2 => parts[1..].join("."),
        _ => d,
    };
    Some(trimmed)
}

/// What Airth kept for `id`: the image and its type, or None while a
/// recent search found nothing.
fn kept(home: &Path, id: &str) -> Option<Option<(Vec<u8>, &'static str)>> {
    for (ext, kind) in KINDS {
        if let Ok(bytes) = std::fs::read(dir(home).join(format!("{id}.{ext}"))) {
            return Some(Some((bytes, kind)));
        }
    }
    let none = dir(home).join(format!("{id}.none"));
    let recent = std::fs::metadata(&none)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < RETRY);
    recent.then_some(None)
}

/// An answer that is an image Airth keeps: its extension and type.
fn image(kind: &str, bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    let kind = kind.split(';').next().unwrap_or_default().trim();
    match kind {
        "image/svg+xml" => {
            let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
            // Shown as an image only, never as a page; nothing that runs.
            (text.contains("<svg") && !text.contains("<script") && !text.contains("javascript:"))
                .then_some(KINDS[0])
        }
        "image/png" => bytes.starts_with(b"\x89PNG").then_some(KINDS[1]),
        "image/x-icon" | "image/vnd.microsoft.icon" => {
            bytes.starts_with(&[0, 0, 1, 0]).then_some(KINDS[2])
        }
        _ => None,
    }
}

/// GET `url` from a public address only, following redirects to public
/// addresses only: the image and its type.
fn fetch(url: &str) -> Result<(Vec<u8>, String)> {
    let url = url::Url::parse(url)?;
    if url.scheme() != "https" {
        bail!("https only");
    }
    let (name, addr) = crate::tools::public_address(&url)?;
    let mut client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let next = attempt.url().clone();
            if attempt.previous().len() > 3 || next.scheme() != "https" {
                attempt.stop()
            } else if crate::tools::public_address(&next).is_err() {
                attempt.error("a redirect to a local or private address")
            } else {
                attempt.follow()
            }
        }));
    // The address checked is the one connected to.
    if let Some(name) = name {
        client = client.resolve(&name, addr);
    }
    let response = client.build()?.get(url).send()?;
    if !response.status().is_success() {
        bail!("{}", response.status());
    }
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response.bytes()?;
    if bytes.len() > MAX_BYTES {
        bail!("too large");
    }
    Ok((bytes.to_vec(), kind))
}

/// The logo of connector `id`, whose server is on `domain`: kept, else
/// fetched now from Simple Icons, then from the site's own icons, and kept.
pub fn logo(
    home: &Path,
    id: &str,
    domain: Option<&str>,
) -> Result<Option<(Vec<u8>, &'static str)>> {
    let id = slug(id)?;
    if let Some(found) = kept(home, &id) {
        return Ok(found);
    }
    let mut tries = vec![format!("https://cdn.simpleicons.org/{id}")];
    if let Some(site) = domain.and_then(site) {
        for path in ["apple-touch-icon.png", "favicon.svg", "favicon.ico"] {
            tries.push(format!("https://{site}/{path}"));
        }
    }
    std::fs::create_dir_all(dir(home)).context("cannot keep logos")?;
    for url in tries {
        let Ok((bytes, kind)) = fetch(&url) else {
            continue;
        };
        if let Some((ext, kind)) = image(&kind, &bytes) {
            std::fs::write(dir(home).join(format!("{id}.{ext}")), &bytes)?;
            return Ok(Some((bytes, kind)));
        }
    }
    std::fs::write(dir(home).join(format!("{id}.none")), "")?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connector_names_a_site_never_an_address() {
        assert_eq!(site("mcp.canva.com").as_deref(), Some("canva.com"));
        assert_eq!(site("api.notion.com").as_deref(), Some("notion.com"));
        assert_eq!(site("linear.app").as_deref(), Some("linear.app"));
        assert_eq!(site("127.0.0.1"), None);
        assert_eq!(site("localhost"), None);
        assert_eq!(site("evil.com:8080"), None);
        assert_eq!(site("a/b.com"), None);
        assert_eq!(slug("google-drive").unwrap(), "googledrive");
        assert!(slug("../../etc").is_ok_and(|s| s == "etc"));
        assert!(slug("--").is_err());
        // Nothing local or private is ever fetched, nor anything but https.
        assert!(fetch("https://127.0.0.1/favicon.ico").is_err());
        assert!(fetch("https://10.0.0.1/favicon.ico").is_err());
        assert!(fetch("http://example.com/favicon.ico").is_err());
    }

    #[test]
    fn only_an_image_that_runs_nothing_is_kept() {
        assert_eq!(
            image(
                "image/svg+xml",
                b"<svg viewBox='0 0 24 24'><path d='M0 0'/></svg>"
            ),
            Some(KINDS[0])
        );
        assert_eq!(
            image("image/svg+xml", b"<svg><script>alert(1)</script></svg>"),
            None
        );
        assert_eq!(image("image/png", b"\x89PNG\r\n\x1a\nrest"), Some(KINDS[1]));
        assert_eq!(image("image/png", b"<html>"), None);
        assert_eq!(image("text/html", b"<svg></svg>"), None);
        assert_eq!(image("image/png", &vec![0u8; MAX_BYTES + 1]), None);
    }

    #[test]
    fn a_logo_is_kept_and_none_found_is_remembered() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir(home.path())).unwrap();
        std::fs::write(dir(home.path()).join("canva.svg"), "<svg/>").unwrap();
        let (bytes, kind) = logo(home.path(), "canva", None).unwrap().unwrap();
        assert_eq!(
            (bytes.as_slice(), kind),
            (b"<svg/>".as_slice(), "image/svg+xml")
        );
        // A local or private site is never asked: nothing is found, and that is kept.
        assert!(
            logo(home.path(), "intranet", Some("localhost"))
                .unwrap()
                .is_none()
        );
        assert!(dir(home.path()).join("intranet.none").exists());
        assert_eq!(kept(home.path(), "intranet"), Some(None));
    }
}
