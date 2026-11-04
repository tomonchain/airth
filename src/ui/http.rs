//! Small HTTP helpers: reading requests, building responses, tokens.

use super::*;

/// Text made safe to put in an HTML page.
pub(super) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) fn expand_home(path: &str) -> std::path::PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => std::path::PathBuf::from(home).join(rest),
        _ => std::path::PathBuf::from(path),
    }
}

/// Decode %XX escapes in a path segment.
pub(super) fn percent_decode(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(super) fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

pub(super) fn read_json(request: &mut Request) -> Result<Value> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut body)?;
    serde_json::from_str(&body).context("request body must be JSON")
}

/// How long a PDF preview link works.
pub(super) const PREVIEW_FOR: Duration = Duration::from_secs(30 * 60);

/// One query parameter, decoded ("" when missing).
pub(super) fn param(query: &str, name: &str) -> String {
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// A request's raw body, refused past `max` bytes.
pub(super) fn read_body(request: &mut Request, max: usize, what: &str) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(max as u64 + 1)
        .read_to_end(&mut body)?;
    if body.len() > max {
        anyhow::bail!("{what} is too large (at most {} MB)", max >> 20);
    }
    Ok(body)
}

/// A request's raw body, refused past `images::MAX_IMAGE`.
pub(super) fn read_bytes(request: &mut Request) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(images::MAX_IMAGE as u64 + 1)
        .read_to_end(&mut body)?;
    if body.len() > images::MAX_IMAGE {
        anyhow::bail!(
            "the picture is too large (at most {} KB)",
            images::MAX_IMAGE / 1024
        );
    }
    Ok(body)
}

pub(super) fn image_response(dir: &std::path::Path, stem: &str) -> HttpResponse {
    let Some((bytes, mime)) = images::load(dir, stem) else {
        return json_response(404, json!({"error": "no picture"}));
    };
    Response::from_data(bytes)
        .with_header(content_type(mime))
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
}

pub(super) fn json_response(status: u16, value: Value) -> HttpResponse {
    Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(content_type("application/json"))
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
}

pub(super) fn static_response(body: &str, kind: &str) -> HttpResponse {
    static_bytes(body.as_bytes(), kind)
}

pub(super) fn static_bytes(body: &[u8], kind: &str) -> HttpResponse {
    Response::from_data(body)
        .with_header(content_type(kind))
        .with_header(Header::from_bytes("X-Frame-Options", "DENY").unwrap())
        .with_header(
            Header::from_bytes(
                "Content-Security-Policy",
                // Inline styles: the editor (CodeMirror) writes its own. Scripts stay 'self' only.
                "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            )
            .unwrap(),
        )
        .with_header(Header::from_bytes("Referrer-Policy", "no-referrer").unwrap())
}

pub(super) fn content_type(kind: &str) -> Header {
    Header::from_bytes("Content-Type", kind).unwrap()
}

pub(crate) fn random_token() -> Result<String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .context("cannot read /dev/urandom")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

pub(super) fn open_in_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
