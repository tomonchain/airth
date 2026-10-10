//! HTTP requests: to the realm's integrations (with their credentials), or to
//! public web addresses (without any).
//!
//! The model never handles credentials: it names an integration and the
//! runtime adds that integration's headers. Requests that name no integration
//! may only reach public addresses, so a run cannot probe the owner's local
//! network or Airth's own app; an owner who wants a local service makes it an
//! integration. Redirects are not followed: a redirect is reported, and
//! following it is a new request that goes through the same checks.

use super::{Tool, Workspace, str_arg, truncate};
use crate::cancel::CancelToken;
use crate::integration::Integration;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Context, Result, bail};
use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_BODY_IN: usize = 100_000;
const MAX_RESPONSE: usize = 5_000_000;
const MAX_SHOWN: usize = 100_000;
const METHODS: &[&str] = &["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];
/// Headers the model may not set: transport framing, and where the request goes.
const RESERVED_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "connection",
    "transfer-encoding",
    "upgrade",
    "proxy-authorization",
];

pub struct HttpRequest {
    pub integrations: Vec<Integration>,
}

/// A request, checked and ready to send.
struct Prepared {
    method: reqwest::Method,
    url: Url,
    integration: Option<Integration>,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl HttpRequest {
    fn integration(&self, name: &str) -> Result<&Integration> {
        self.integrations
            .iter()
            .find(|i| i.name == name)
            .with_context(|| {
                let names: Vec<&str> = self.integrations.iter().map(|i| i.name.as_str()).collect();
                if names.is_empty() {
                    format!("no integration '{name}': this realm has none")
                } else {
                    format!(
                        "no integration '{name}' (this realm has: {})",
                        names.join(", ")
                    )
                }
            })
    }

    fn prepare(&self, input: &Value) -> Result<Prepared> {
        let method = input["method"]
            .as_str()
            .unwrap_or("GET")
            .to_ascii_uppercase();
        if !METHODS.contains(&method.as_str()) {
            bail!("unsupported method '{method}' ({})", METHODS.join(", "));
        }
        let target = str_arg(input, "url")?;
        let integration = match input["integration"].as_str().filter(|s| !s.is_empty()) {
            Some(name) => Some(self.integration(name)?.clone()),
            None => None,
        };
        let url = match &integration {
            Some(i) => i.url_for(target)?,
            None => {
                let url = Url::parse(target).with_context(|| {
                    format!(
                        "invalid URL '{target}' (without an integration, give a full https:// URL)"
                    )
                })?;
                if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
                    bail!("only http(s) URLs are supported");
                }
                if !url.username().is_empty() || url.password().is_some() {
                    bail!("URLs with credentials are not allowed; use an integration");
                }
                url
            }
        };
        let reserved: Vec<String> = integration
            .iter()
            .flat_map(|i| i.headers.keys().map(|k| k.to_ascii_lowercase()))
            .collect();
        let mut headers = Vec::new();
        if let Some(map) = input["headers"].as_object() {
            for (name, value) in map {
                let lower = name.to_ascii_lowercase();
                if RESERVED_HEADERS.contains(&lower.as_str()) || reserved.contains(&lower) {
                    bail!("the header '{name}' cannot be set here");
                }
                let value = value.as_str().context("header values must be strings")?;
                headers.push((name.clone(), value.to_string()));
            }
        }
        let body = input["body"].as_str().map(String::from);
        if body.as_ref().is_some_and(|b| b.len() > MAX_BODY_IN) {
            bail!("the request body is limited to {MAX_BODY_IN} bytes");
        }
        Ok(Prepared {
            method: method.parse()?,
            url,
            integration,
            headers,
            body,
        })
    }
}

impl Tool for HttpRequest {
    fn spec(&self) -> ToolSpec {
        let integrations = if self.integrations.is_empty() {
            "This realm has no integrations, so only public URLs can be reached, without credentials.".to_string()
        } else {
            let list: Vec<String> = self
                .integrations
                .iter()
                .map(|i| {
                    let desc = if i.description.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", i.description)
                    };
                    format!("- {} ({}){desc}", i.name, i.base_url)
                })
                .collect();
            format!("Integrations of this realm:\n{}", list.join("\n"))
        };
        ToolSpec {
            name: "http_request".into(),
            description: format!(
                "Send an HTTP request and get the status, main headers and body (text, truncated to {MAX_SHOWN} bytes). To call one of the realm's integrations, set `integration` and give `url` as a path under its base URL: the runtime adds its credentials, which you never see. Without an integration, `url` must be a full public URL and no credentials are sent. Redirects are not followed. The owner may need to approve each request.\n\n{integrations}"
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "integration": {"type": "string", "description": "Name of the integration to call (optional)"},
                    "method": {"type": "string", "enum": METHODS, "description": "Default GET"},
                    "url": {"type": "string", "description": "A path under the integration's base URL, or a full URL"},
                    "headers": {"type": "object", "additionalProperties": {"type": "string"}},
                    "body": {"type": "string", "description": "Request body, e.g. JSON text (set content-type in headers)"}
                },
                "required": ["url"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Network
    }

    fn scope(&self, input: &Value) -> Option<String> {
        if let Some(name) = input["integration"].as_str().filter(|s| !s.is_empty()) {
            return Some(format!("integration {name}"));
        }
        Url::parse(input["url"].as_str()?)
            .ok()?
            .host_str()
            .map(String::from)
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let p = self.prepare(input)?;
        let mut out = format!("{} {}", p.method, p.url);
        if let Some(i) = &p.integration {
            out.push_str(&format!("\nvia integration {}", i.name));
            let secrets = i.secret_names();
            if !secrets.is_empty() {
                out.push_str(&format!(" (sends {})", secrets.join(", ")));
            }
        }
        for (name, value) in &p.headers {
            out.push_str(&format!("\n{name}: {value}"));
        }
        if let Some(body) = &p.body {
            out.push_str("\n\n");
            out.push_str(&truncate(body.clone(), 4000));
        }
        Ok(out)
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let p = self.prepare(input)?;
        let mut builder = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")));
        if p.integration.is_none() {
            // Resolve once, check, and connect to that address, so the name
            // cannot resolve to something else between the check and the request.
            let (host, addr) = public_address(&p.url)?;
            if let Some(host) = host {
                builder = builder.resolve(&host, addr);
            }
        }
        let client = builder.build()?;

        let mut headers = HeaderMap::new();
        let secret = |key: &str| {
            ws.env()
                .iter()
                .find(|(k, v)| k == key && !v.is_empty())
                .map(|(_, v)| v.clone())
        };
        let integration_headers = match &p.integration {
            Some(i) => i.resolve_headers(&secret)?,
            None => Vec::new(),
        };
        for (name, value) in p.headers.iter().chain(&integration_headers) {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())
                    .with_context(|| format!("invalid header name '{name}'"))?,
                HeaderValue::from_str(value)
                    .with_context(|| format!("invalid value for header '{name}'"))?,
            );
        }
        let mut request = client.request(p.method, p.url.clone()).headers(headers);
        if let Some(body) = p.body {
            request = request.body(body);
        }
        let response = request
            .send()
            .map_err(|e| anyhow::anyhow!("request to {} failed: {}", p.url, e.without_url()))?;

        let status = response.status();
        let mut out = format!("HTTP {}", status);
        for name in ["content-type", "location", "retry-after", "link"] {
            if let Some(v) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
                out.push_str(&format!("\n{name}: {v}"));
            }
        }
        if response
            .content_length()
            .is_some_and(|n| n as usize > MAX_RESPONSE)
        {
            out.push_str("\n\n[body not read: larger than 5 MB]");
            return Ok(out);
        }
        let is_text = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_none_or(|t| {
                t.starts_with("text/")
                    || t.contains("json")
                    || t.contains("xml")
                    || t.contains("javascript")
                    || t.contains("x-www-form-urlencoded")
            });
        let bytes = response.bytes().context("reading the response body")?;
        if bytes.is_empty() {
            return Ok(out);
        }
        out.push_str("\n\n");
        if is_text {
            out.push_str(&truncate(
                String::from_utf8_lossy(&bytes).into_owned(),
                MAX_SHOWN,
            ));
        } else {
            out.push_str(&format!("[binary body, {} bytes]", bytes.len()));
        }
        Ok(out)
    }
}

/// The address to connect to for `url`, if every address it resolves to is
/// public. Returns the host name to pin (None for an IP literal).
fn public_address(url: &Url) -> Result<(Option<String>, SocketAddr)> {
    let port = url.port_or_known_default().context("URL without a port")?;
    let (name, addrs): (Option<String>, Vec<SocketAddr>) = match url.host() {
        Some(url::Host::Ipv4(ip)) => (None, vec![SocketAddr::new(IpAddr::V4(ip), port)]),
        Some(url::Host::Ipv6(ip)) => (None, vec![SocketAddr::new(IpAddr::V6(ip), port)]),
        Some(url::Host::Domain(d)) => (
            Some(d.to_string()),
            (d, port)
                .to_socket_addrs()
                .with_context(|| format!("cannot resolve {d}"))?
                .collect(),
        ),
        None => bail!("URL without a host"),
    };
    let Some(first) = addrs.first().copied() else {
        bail!("{} resolves to no address", url.host_str().unwrap_or("?"));
    };
    if let Some(private) = addrs.iter().find(|a| !is_public(a.ip())) {
        bail!(
            "{} is a local or private address ({}); only an integration the owner set up can reach it",
            url.host_str().unwrap_or("?"),
            private.ip()
        );
    }
    Ok((name, first))
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let [o0, o1, ..] = a.octets();
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_broadcast()
                || a.is_documentation()
                || a.is_unspecified()
                || o0 == 0
                || o0 >= 224
                || (o0 == 100 && (o1 & 0xc0) == 64)
                || (o0 == 198 && (o1 & 0xfe) == 18))
        }
        IpAddr::V6(a) => {
            if let Some(v4) = a.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s0 = a.segments()[0];
            !(a.is_loopback()
                || a.is_unspecified()
                || a.is_multicast()
                || (s0 & 0xfe00) == 0xfc00
                || (s0 & 0xffc0) == 0xfe80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn local_server() -> (tiny_http::Server, String) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        (server, url)
    }

    fn tool(base: &str) -> HttpRequest {
        HttpRequest {
            integrations: vec![Integration {
                name: "svc".into(),
                description: "A test service".into(),
                base_url: format!("{base}/api"),
                headers: BTreeMap::from([(
                    "Authorization".into(),
                    "Bearer [secret:SVC_TOKEN]".into(),
                )]),
            }],
        }
    }

    #[test]
    fn integration_requests_carry_credentials() {
        let (server, base) = local_server();
        let handle = std::thread::spawn(move || {
            let req = server.recv().unwrap();
            let auth = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"))
                .map(|h| h.value.to_string());
            let url = req.url().to_string();
            req.respond(
                tiny_http::Response::from_string(r#"{"ok":true}"#).with_header(
                    "Content-Type: application/json"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                ),
            )
            .unwrap();
            (auth, url)
        });
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path())
            .unwrap()
            .with_secrets(vec![("SVC_TOKEN".into(), "tok-123".into())], vec![]);
        let input = json!({"integration": "svc", "url": "items?x=1"});
        let t = tool(&base);
        assert_eq!(t.scope(&input).as_deref(), Some("integration svc"));
        let preview = t.preview(&input, &ws).unwrap();
        assert!(preview.contains("sends SVC_TOKEN"), "{preview}");
        assert!(!preview.contains("tok-123"));
        let out = t.execute(&input, &ws, &CancelToken::default()).unwrap();
        assert!(out.starts_with("HTTP 200"), "{out}");
        assert!(out.contains(r#"{"ok":true}"#), "{out}");
        let (auth, url) = handle.join().unwrap();
        assert_eq!(auth.as_deref(), Some("Bearer tok-123"));
        assert_eq!(url, "/api/items?x=1");
    }

    #[test]
    fn plain_requests_cannot_reach_local_addresses() {
        let (_server, base) = local_server();
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let t = tool(&base);
        for url in [
            format!("{base}/api/items"),
            "http://localhost:9/".to_string(),
            "http://[::1]:9/".to_string(),
            "http://169.254.169.254/latest/meta-data".to_string(),
        ] {
            let err = t
                .execute(&json!({"url": url}), &ws, &CancelToken::default())
                .unwrap_err();
            assert!(format!("{err:#}").contains("private"), "{url}: {err:#}");
        }
    }

    #[test]
    fn rejects_unsafe_requests() {
        let t = tool("https://api.example.com");
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        for input in [
            json!({"integration": "svc", "url": "https://api.example.com/other"}),
            json!({"integration": "nope", "url": "x"}),
            json!({"integration": "svc", "url": "x", "headers": {"authorization": "mine"}}),
            json!({"url": "https://x.com", "headers": {"Host": "y.com"}}),
            json!({"url": "file:///etc/passwd"}),
            json!({"url": "https://u:p@x.com/"}),
            json!({"url": "https://x.com", "method": "TRACE"}),
        ] {
            assert!(t.preview(&input, &ws).is_err(), "{input}");
        }
    }

    #[test]
    fn classifies_addresses() {
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
    }
}
