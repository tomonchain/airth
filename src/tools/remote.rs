//! Remote MCP servers (Streamable HTTP), connected with OAuth like the
//! connectors of Claude or ChatGPT.
//!
//! Connecting follows the MCP authorization spec: the server answers 401 and
//! points to its protected-resource metadata, which names the authorization
//! server; Airth reads that server's metadata, registers itself as a client
//! (dynamic client registration), and opens the sign-in page with PKCE. The
//! browser comes back to `http://127.0.0.1:<port>/oauth/callback`, Airth trades
//! the code for tokens and lists the server's tools. Tokens live in a private
//! file of the realm (`skills/mcp-auth/<server>.json`, mode 600), out of every
//! run's reach; they are refreshed when they expire and never shown to the model.

use super::mcp::McpStore;
use crate::cancel::CancelToken;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::Url;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PROTOCOL_VERSION: &str = "2025-06-18";

/// What Airth keeps to call a server on the owner's behalf.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Auth {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

fn auth_path(skills_dir: &Path, name: &str) -> PathBuf {
    skills_dir.join("mcp-auth").join(format!("{name}.json"))
}

pub fn load_auth(skills_dir: &Path, name: &str) -> Option<Auth> {
    let text = std::fs::read_to_string(auth_path(skills_dir, name)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_auth(skills_dir: &Path, name: &str, auth: &Auth) -> Result<()> {
    let path = auth_path(skills_dir, name);
    std::fs::create_dir_all(path.parent().unwrap_or(skills_dir))?;
    crate::realm::write_private(&path, &serde_json::to_string_pretty(auth)?)
}

pub fn forget_auth(skills_dir: &Path, name: &str) {
    let _ = std::fs::remove_file(auth_path(skills_dir, name));
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}

fn base64url(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// PKCE (RFC 7636, S256): the challenge for a verifier.
pub fn challenge(verifier: &str) -> String {
    base64url(aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, verifier.as_bytes()).as_ref())
}

/// `{origin}/.well-known/{kind}{path}`, as RFC 8414 / RFC 9728 place metadata.
fn well_known(url: &Url, kind: &str, with_path: bool) -> Result<Url> {
    let path = url.path().trim_end_matches('/');
    let mut u = url.clone();
    u.set_query(None);
    u.set_fragment(None);
    u.set_path(&format!(
        "/.well-known/{kind}{}",
        if with_path { path } else { "" }
    ));
    Ok(u)
}

fn get_json(http: &Client, url: &Url) -> Option<Value> {
    let resp = http
        .get(url.clone())
        .header("Accept", "application/json")
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json().ok()
}

/// Where to sign in, from the server's first answer and its metadata. None
/// when the server answers without asking for a token.
fn discover(http: &Client, mcp_url: &Url) -> Result<Option<Auth>> {
    let resp = http
        .post(mcp_url.clone())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", PROTOCOL_VERSION)
        .body(initialize_body(0).to_string())
        .send()
        .with_context(|| format!("cannot reach {mcp_url}"))?;
    if resp.status().is_success() {
        return Ok(None);
    }
    if resp.status().as_u16() != 401 {
        bail!(
            "{mcp_url} answered {} instead of asking to sign in",
            resp.status()
        );
    }
    let header = resp
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let mut prm_urls = Vec::new();
    if let Some(at) = header.find("resource_metadata=\"") {
        let rest = &header[at + 19..];
        if let Some(end) = rest.find('"')
            && let Ok(u) = Url::parse(&rest[..end])
        {
            prm_urls.push(u);
        }
    }
    prm_urls.push(well_known(mcp_url, "oauth-protected-resource", true)?);
    prm_urls.push(well_known(mcp_url, "oauth-protected-resource", false)?);
    let prm = prm_urls.iter().find_map(|u| get_json(http, u));
    // Without protected-resource metadata (older servers), the MCP server's
    // own origin is the authorization server.
    let issuer = prm
        .as_ref()
        .and_then(|p| p["authorization_servers"][0].as_str())
        .and_then(|s| Url::parse(s).ok())
        .unwrap_or_else(|| {
            let mut u = mcp_url.clone();
            u.set_path("/");
            u.set_query(None);
            u
        });
    let candidates = [
        well_known(&issuer, "oauth-authorization-server", true)?,
        well_known(&issuer, "openid-configuration", true)?,
        Url::parse(&format!(
            "{}/.well-known/openid-configuration",
            issuer.as_str().trim_end_matches('/')
        ))?,
        well_known(&issuer, "oauth-authorization-server", false)?,
    ];
    let meta = candidates
        .iter()
        .find_map(|u| get_json(http, u))
        .unwrap_or(Value::Null);
    let endpoint = |key: &str, default: &str| -> String {
        meta[key].as_str().map(String::from).unwrap_or_else(|| {
            let mut u = issuer.clone();
            u.set_path(default);
            u.to_string()
        })
    };
    let scope = prm
        .as_ref()
        .and_then(|p| p["scopes_supported"].as_array())
        .or_else(|| meta["scopes_supported"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty() && prm.is_some());
    Ok(Some(Auth {
        authorization_endpoint: endpoint("authorization_endpoint", "/authorize"),
        token_endpoint: endpoint("token_endpoint", "/token"),
        registration_endpoint: meta["registration_endpoint"]
            .as_str()
            .map(String::from)
            .or_else(|| meta.is_null().then(|| endpoint("", "/register"))),
        scope,
        ..Default::default()
    }))
}

/// Dynamic client registration (RFC 7591), as a public client.
fn register(http: &Client, auth: &mut Auth, redirect_uri: &str) -> Result<()> {
    let endpoint = auth.registration_endpoint.clone().ok_or_else(|| {
        anyhow!("this service does not let apps register themselves; it needs an app created by hand in its developer settings")
    })?;
    let resp = http
        .post(&endpoint)
        .json(&json!({
            "client_name": "Airth",
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        }))
        .send()
        .context("registering Airth with the service")?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if !status.is_success() {
        bail!(
            "the service refused to register Airth ({status}): {}",
            body["error_description"]
                .as_str()
                .or(body["error"].as_str())
                .unwrap_or("")
        );
    }
    auth.client_id = body["client_id"]
        .as_str()
        .context("no client_id in the registration")?
        .to_string();
    auth.client_secret = body["client_secret"].as_str().map(String::from);
    auth.redirect_uri = redirect_uri.to_string();
    Ok(())
}

/// A sign-in waiting for the browser to come back.
struct Pending {
    skills_dir: PathBuf,
    name: String,
    url: String,
    verifier: String,
    auth: Auth,
    started: DateTime<Utc>,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static P: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What starting a connection led to.
pub enum Started {
    /// Open this page to sign in.
    SignIn(String),
    /// The server needs no sign-in.
    Open,
}

/// Begin connecting `name` (already in the store): discover, register if
/// needed, and return the sign-in page with a fresh PKCE challenge.
pub fn start(skills_dir: &Path, name: &str, redirect_uri: &str) -> Result<Started> {
    let server = McpStore::new(skills_dir).get(name)?;
    let url = server.url.clone().context("not a remote server")?;
    let mcp_url = Url::parse(&url).context("invalid server URL")?;
    let http = client()?;
    let Some(mut auth) = discover(&http, &mcp_url)? else {
        return Ok(Started::Open);
    };
    // Keep an earlier registration when it still fits.
    match load_auth(skills_dir, name) {
        Some(old)
            if old.redirect_uri == redirect_uri
                && old.token_endpoint == auth.token_endpoint
                && !old.client_id.is_empty() =>
        {
            auth.client_id = old.client_id;
            auth.client_secret = old.client_secret;
            auth.redirect_uri = old.redirect_uri;
        }
        _ => register(&http, &mut auth, redirect_uri)?,
    }
    let verifier = format!(
        "{}{}",
        crate::ui::random_token()?,
        crate::ui::random_token()?
    );
    let state = crate::ui::random_token()?;
    let mut authorize =
        Url::parse(&auth.authorization_endpoint).context("invalid authorization endpoint")?;
    {
        let mut q = authorize.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &auth.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &url);
        if let Some(scope) = &auth.scope {
            q.append_pair("scope", scope);
        }
    }
    let mut map = pending().lock().unwrap();
    map.retain(|_, p| Utc::now() - p.started < chrono::Duration::minutes(15));
    map.insert(
        state,
        Pending {
            skills_dir: skills_dir.to_path_buf(),
            name: name.to_string(),
            url,
            verifier,
            auth,
            started: Utc::now(),
        },
    );
    Ok(Started::SignIn(authorize.to_string()))
}

fn token_request(http: &Client, auth: &Auth, form: &[(&str, &str)]) -> Result<Auth> {
    let mut form: Vec<(&str, &str)> = form.to_vec();
    form.push(("client_id", &auth.client_id));
    if let Some(secret) = &auth.client_secret {
        form.push(("client_secret", secret));
    }
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form.iter())
        .finish();
    let resp = http
        .post(&auth.token_endpoint)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .context("asking the service for tokens")?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if !status.is_success() {
        bail!(
            "the service refused ({status}): {}",
            body["error_description"]
                .as_str()
                .or(body["error"].as_str())
                .unwrap_or("")
        );
    }
    let mut next = auth.clone();
    next.access_token = Some(
        body["access_token"]
            .as_str()
            .context("no access token in the answer")?
            .to_string(),
    );
    if let Some(r) = body["refresh_token"].as_str() {
        next.refresh_token = Some(r.to_string());
    }
    next.expires_at = body["expires_in"]
        .as_i64()
        .map(|s| Utc::now() + chrono::Duration::seconds(s));
    Ok(next)
}

/// The browser came back with `code` for `state`: get the tokens, then the
/// server's tools. Returns the server's name.
pub fn finish(state: &str, code: &str) -> Result<String> {
    let p = pending().lock().unwrap().remove(state).ok_or_else(|| {
        anyhow!("this sign-in expired or was already used; start again from Airth")
    })?;
    let http = client()?;
    let auth = token_request(
        &http,
        &p.auth,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &p.auth.redirect_uri),
            ("code_verifier", &p.verifier),
            ("resource", &p.url),
        ],
    )?;
    save_auth(&p.skills_dir, &p.name, &auth)?;
    refresh_tools(&p.skills_dir, &p.name)?;
    Ok(p.name)
}

/// A valid access token for the server, refreshed when it expired. None for
/// servers that need no sign-in.
pub fn access_token(skills_dir: &Path, name: &str, force_refresh: bool) -> Result<Option<String>> {
    let Some(auth) = load_auth(skills_dir, name) else {
        return Ok(None);
    };
    let expired = auth
        .expires_at
        .is_some_and(|t| t - chrono::Duration::seconds(60) < Utc::now());
    if (expired || force_refresh)
        && let Some(refresh) = auth.refresh_token.clone()
    {
        let next = token_request(
            &client()?,
            &auth,
            &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
        )
        .context("the connection expired and could not be renewed: connect again")?;
        save_auth(skills_dir, name, &next)?;
        return Ok(next.access_token);
    }
    if expired {
        bail!("the connection expired: connect again");
    }
    Ok(auth.access_token)
}

fn initialize_body(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "initialize", "params": {
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": {"name": "airth", "version": env!("CARGO_PKG_VERSION")},
    }})
}

/// Signals a 401, so the caller can renew the token once.
#[derive(Debug)]
struct Unauthorized;

impl std::fmt::Display for Unauthorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the service no longer accepts Airth's token: connect again")
    }
}

impl std::error::Error for Unauthorized {}

/// One Streamable HTTP conversation with a server.
struct Session {
    http: Client,
    url: String,
    token: Option<String>,
    session_id: Option<String>,
    next_id: u64,
    /// The server's name and version, from the handshake.
    info: String,
}

impl Session {
    fn post(&mut self, message: &Value, cancel: &CancelToken) -> Result<Response> {
        if cancel.is_cancelled() {
            bail!("stopped");
        }
        let mut req = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .timeout(Duration::from_secs(120))
            .body(message.to_string());
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        if let Some(s) = &self.session_id {
            req = req.header("Mcp-Session-Id", s);
        }
        let resp = req.send().context("cannot reach the server")?;
        if resp.status().as_u16() == 401 {
            return Err(Unauthorized.into());
        }
        if let Some(s) = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            self.session_id = Some(s.to_string());
        }
        Ok(resp)
    }

    fn request(&mut self, method: &str, params: Value, cancel: &CancelToken) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let resp = self.post(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            cancel,
        )?;
        let status = resp.status();
        let sse = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|c| c.starts_with("text/event-stream"));
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            bail!("{method}: the server answered {status}");
        }
        let messages = if sse {
            sse_messages(&text)
        } else {
            json_messages(&text)
        };
        let reply = messages
            .into_iter()
            .find(|m| m.get("id").and_then(Value::as_u64) == Some(id) && m.get("method").is_none())
            .ok_or_else(|| anyhow!("{method}: no answer from the server"))?;
        if let Some(error) = reply.get("error") {
            bail!(
                "{method} failed: {}",
                error["message"].as_str().unwrap_or("unknown error")
            );
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn initialize(&mut self, cancel: &CancelToken) -> Result<String> {
        let result = self.request("initialize", initialize_body(0)["params"].clone(), cancel)?;
        self.post(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            cancel,
        )?;
        let info = &result["serverInfo"];
        Ok(format!(
            "{} {}",
            info["name"].as_str().unwrap_or("server"),
            info["version"].as_str().unwrap_or("")
        )
        .trim()
        .to_string())
    }
}

fn json_messages(text: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(items)) => items,
        Ok(v) => vec![v],
        Err(_) => Vec::new(),
    }
}

/// The JSON-RPC messages in a server-sent events body (`data:` lines, events
/// separated by blank lines).
pub fn sse_messages(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for event in text.replace("\r\n", "\n").split("\n\n") {
        let data: Vec<&str> = event
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|l| l.strip_prefix(' ').unwrap_or(l))
            .collect();
        if let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) {
            out.extend(json_messages(&v.to_string()));
        }
    }
    out
}

/// Run `f` in a fresh session; on a 401, renew the token once and retry.
fn with_session<T>(
    skills_dir: &Path,
    name: &str,
    url: &str,
    cancel: &CancelToken,
    f: impl Fn(&mut Session) -> Result<T>,
) -> Result<T> {
    for attempt in 0..2 {
        let mut session = Session {
            http: client()?,
            url: url.to_string(),
            token: access_token(skills_dir, name, attempt == 1)?,
            session_id: None,
            next_id: 1,
            info: String::new(),
        };
        match session.initialize(cancel).and_then(|info| {
            session.info = info;
            f(&mut session)
        }) {
            Err(e) if attempt == 0 && e.is::<Unauthorized>() && session.token.is_some() => continue,
            other => return other,
        }
    }
    Err(Unauthorized.into())
}

/// List a connected server's tools and save them.
pub fn refresh_tools(skills_dir: &Path, name: &str) -> Result<()> {
    let store = McpStore::new(skills_dir);
    let url = store.get(name)?.url.context("not a remote server")?;
    let cancel = CancelToken::default();
    let result = with_session(skills_dir, name, &url, &cancel, |s| {
        let info = s.info.clone();
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let page = s.request(
                "tools/list",
                cursor.as_ref().map_or(json!({}), |c| json!({"cursor": c})),
                &cancel,
            )?;
            tools.extend(super::mcp::parse_tools(&page));
            cursor = page["nextCursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok((info, tools))
    });
    store.update(name, |s| {
        s.connecting_since = None;
        match result {
            Ok((info, tools)) => {
                s.server_info = (!info.is_empty()).then_some(info);
                s.tools = tools;
                s.connected_at = Some(Utc::now());
                s.error = None;
            }
            Err(ref e) => s.error = Some(format!("{e:#}")),
        }
    })?;
    Ok(())
}

/// Call one tool of a remote server.
pub fn call(
    skills_dir: &Path,
    name: &str,
    url: &str,
    tool: &str,
    arguments: &Value,
    cancel: &CancelToken,
) -> Result<String> {
    let result = with_session(skills_dir, name, url, cancel, |s| {
        s.request(
            "tools/call",
            json!({"name": tool, "arguments": arguments}),
            cancel,
        )
    })?;
    super::mcp::result_text(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc_7636() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
    }

    #[test]
    fn reads_sse_bodies() {
        let body = "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{\"x\":1}}\r\n\r\n: ping\n\n";
        let msgs = sse_messages(body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["result"]["x"], 1);
    }

    /// A fake service: protected-resource and authorization-server metadata,
    /// client registration, tokens (with refresh), and an MCP endpoint that
    /// answers in JSON or SSE and rejects bad tokens.
    fn fake_service() -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let b = base.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let auth = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string())
                    .unwrap_or_default();
                let json = |v: Value| {
                    tiny_http::Response::from_string(v.to_string()).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    )
                };
                let url = req.url().to_string();
                let resp = match url.as_str() {
                    "/.well-known/oauth-protected-resource/mcp" => {
                        json(json!({"resource": format!("{b}/mcp"), "authorization_servers": [b.clone()]}))
                    }
                    "/.well-known/oauth-authorization-server" => json(json!({
                        "issuer": b, "authorization_endpoint": format!("{b}/authorize"),
                        "token_endpoint": format!("{b}/token"), "registration_endpoint": format!("{b}/register")})),
                    "/register" => json(json!({"client_id": "c1"})),
                    "/token" if body.contains("grant_type=authorization_code") && body.contains("code=good") && body.contains("code_verifier=") => {
                        json(json!({"access_token": "old", "refresh_token": "r1", "expires_in": 3600}))
                    }
                    "/token" if body.contains("grant_type=refresh_token") => json(json!({"access_token": "new", "expires_in": 3600})),
                    "/mcp" if auth == "Bearer new" || auth == "Bearer old" && !body.contains("tools/call") => {
                        let msg: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                        let id = msg["id"].clone();
                        match msg["method"].as_str() {
                            Some("initialize") => json(json!({"jsonrpc": "2.0", "id": id, "result": {"serverInfo": {"name": "fake"}}}))
                                .with_header(tiny_http::Header::from_bytes("Mcp-Session-Id", "s1").unwrap()),
                            Some("tools/list") => json(json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [{"name": "search", "description": "Find"}]}})),
                            Some("tools/call") => tiny_http::Response::from_string(format!(
                                "event: message\ndata: {}\n\n",
                                json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "found it"}]}})
                            ))
                            .with_header(tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap()),
                            _ => tiny_http::Response::from_string("").with_status_code(202),
                        }
                    }
                    "/mcp" => tiny_http::Response::from_string("")
                        .with_status_code(401)
                        .with_header(tiny_http::Header::from_bytes(
                            "WWW-Authenticate",
                            format!("Bearer resource_metadata=\"{b}/.well-known/oauth-protected-resource/mcp\""),
                        ).unwrap()),
                    _ => tiny_http::Response::from_string("").with_status_code(404),
                };
                let _ = req.respond(resp);
            }
        });
        base
    }

    #[test]
    fn signs_in_lists_tools_and_renews_the_token() {
        let base = fake_service();
        let dir = tempfile::tempdir().unwrap();
        let store = McpStore::new(dir.path());
        store
            .save_remote("svc", &format!("{base}/mcp"), "")
            .unwrap();
        let Started::SignIn(url) =
            start(dir.path(), "svc", "http://127.0.0.1:9/oauth/callback").unwrap()
        else {
            panic!("expected a sign-in");
        };
        let url = Url::parse(&url).unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.path(), "/authorize");
        assert_eq!(q["client_id"], "c1");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["resource"], format!("{base}/mcp"));
        assert!(finish("not-the-state", "good").is_err());
        assert_eq!(finish(&q["state"], "good").unwrap(), "svc");
        assert!(finish(&q["state"], "good").is_err(), "a state is used once");
        let server = store.get("svc").unwrap();
        assert_eq!(server.tools[0].name, "search", "{:?}", server.error);
        // The fake refuses the first token for calls: Airth renews it once and retries.
        let out = call(
            dir.path(),
            "svc",
            &format!("{base}/mcp"),
            "search",
            &json!({}),
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(out, "found it");
        assert_eq!(
            load_auth(dir.path(), "svc")
                .unwrap()
                .access_token
                .as_deref(),
            Some("new")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(auth_path(dir.path(), "svc"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "tokens are private");
        }
    }
}
