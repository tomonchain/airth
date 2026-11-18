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
                // Inline styles: the interface sets some sizes in the page (and the
                // sign-in callback page has its own). Scripts stay 'self' only.
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

/// Whether the browser takes gzip, as every one does: the app's code is
/// about a third of its size so, which counts on a phone's connection.
pub(super) fn takes_gzip(request: &Request) -> bool {
    header(request, "Accept-Encoding").is_some_and(|v| {
        v.split(',').any(|e| {
            let mut parts = e.split(';');
            let named = parts
                .next()
                .is_some_and(|n| n.trim().eq_ignore_ascii_case("gzip"));
            // "gzip;q=0" is a browser saying no.
            let refused = parts.any(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|q| q.trim().parse::<f32>().ok())
                    .is_some_and(|q| q <= 0.0)
            });
            named && !refused
        })
    })
}

/// What compresses: text. Fonts and images are compressed already.
pub(super) fn compressible(kind: &str) -> bool {
    kind.starts_with("text/")
        || kind.starts_with("application/json")
        || kind.starts_with("application/manifest+json")
        || kind.starts_with("image/svg+xml")
}

/// Below this, gzip saves less than it costs.
pub(super) const GZIP_FROM: usize = 1024;

pub(super) fn gzip(body: &[u8], level: u32) -> Vec<u8> {
    use std::io::Write;
    let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(level));
    // Writing to a Vec cannot fail.
    let _ = out.write_all(body);
    out.finish().unwrap_or_default()
}

/// A built file of the app, gzipped once and kept for the process's life:
/// the files are the binary's own and never change while it runs.
pub(super) fn gzipped_file(body: &'static [u8]) -> Arc<Vec<u8>> {
    static KEPT: Mutex<Option<HashMap<usize, Arc<Vec<u8>>>>> = Mutex::new(None);
    let key = body.as_ptr() as usize;
    // Made under the lock: two first asks at once make it once.
    crate::supervise::Locked::locked(&KEPT)
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_insert_with(|| Arc::new(gzip(body, 9)))
        .clone()
}

/// An API answer gzipped when the browser takes it and it is worth it (a
/// long conversation's run is often hundreds of kilobytes of JSON). Fast
/// level: it is made for each request.
pub(super) fn compressed(request: &Request, response: HttpResponse) -> HttpResponse {
    let kind = response
        .headers()
        .iter()
        .find(|h| h.field.equiv("Content-Type"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    let done = response
        .headers()
        .iter()
        .any(|h| h.field.equiv("Content-Encoding"));
    if done || !compressible(&kind) {
        return response;
    }
    // Whether it goes gzipped depends on the ask: said, whichever way it goes.
    let vary = || Header::from_bytes("Vary", "Accept-Encoding").unwrap();
    if !takes_gzip(request) || response.data_length().is_some_and(|n| n < GZIP_FROM) {
        return response.with_header(vary());
    }
    let code = response.status_code();
    let headers: Vec<Header> = response.headers().to_vec();
    let mut body = Vec::new();
    if std::io::Read::read_to_end(&mut response.into_reader(), &mut body).is_err() {
        return json_response(500, json!({"error": "the answer could not be read"}));
    }
    let mut out = Response::from_data(gzip(&body, 1)).with_status_code(code);
    for h in headers {
        if !h.field.equiv("Content-Length") {
            out = out.with_header(h);
        }
    }
    out.with_header(Header::from_bytes("Content-Encoding", "gzip").unwrap())
        .with_header(Header::from_bytes("Vary", "Accept-Encoding").unwrap())
}
