//! `airth ui`: a local app served on 127.0.0.1, built on the hub.
//!
//! Each run started from the UI is driven on its own thread. The UI watches it
//! through a shared snapshot and answers permission requests through a channel;
//! the permission policy itself is still the hub's default.
//!
//! The server only listens on loopback and every API call must carry a random
//! per-launch token in the `X-Airth-Token` header. Since a custom header cannot
//! be sent cross-origin without a CORS preflight (which we never grant), and
//! the Host header is checked against loopback (DNS rebinding), other websites
//! cannot drive the agent or approve its actions.

use crate::agent::{AgentConfig, Observer};
use crate::cancel::CancelToken;
use crate::hub::{Hub, RunHooks};
use crate::permissions::{Answer, Approver, PermissionRequest};
use crate::state::{State, Status};
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX_HTML: &str = include_str!("index.html");
const APP_JS: &str = include_str!("app.js");
const APP_CSS: &str = include_str!("app.css");
const MAX_BODY: u64 = 1 << 20;

pub struct UiOptions {
    pub port: u16,
    pub open_browser: bool,
    pub max_iterations: u32,
    /// Why the model is unavailable, if it is (the UI can still browse runs).
    pub model_error: Option<String>,
}

/// A run driven by this process.
struct Live {
    snapshot: Mutex<State>,
    pending: Mutex<Option<Pending>>,
    cancel: CancelToken,
    version: AtomicU64,
}

impl Live {
    fn bump(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }
}

struct Pending {
    id: u64,
    request: PermissionRequest,
    reply: mpsc::Sender<Answer>,
}

type Registry = Arc<Mutex<HashMap<String, Arc<Live>>>>;

struct UiObserver(Arc<Live>);

impl Observer for UiObserver {
    fn changed(&mut self, state: &State) {
        *self.0.snapshot.lock().unwrap() = state.clone();
        self.0.bump();
    }
}

/// Parks the agent thread until the human answers in the UI or stops the run.
struct UiApprover {
    live: Arc<Live>,
    next_id: u64,
}

impl Approver for UiApprover {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        let (tx, rx) = mpsc::channel();
        self.next_id += 1;
        *self.live.pending.lock().unwrap() = Some(Pending {
            id: self.next_id,
            request: request.clone(),
            reply: tx,
        });
        self.live.bump();
        let answer = loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(answer) => break answer,
                Err(mpsc::RecvTimeoutError::Timeout) if !self.live.cancel.is_cancelled() => {}
                Err(_) => break Answer::No("the run was stopped".into()),
            }
        };
        *self.live.pending.lock().unwrap() = None;
        self.live.bump();
        Ok(answer)
    }
}

struct App {
    hub: Arc<Hub>,
    registry: Registry,
    token: String,
    port: u16,
    options: UiOptions,
}

pub fn serve(hub: Hub, options: UiOptions) -> Result<()> {
    let server = Server::http(("127.0.0.1", options.port))
        .or_else(|_| Server::http(("127.0.0.1", 0)))
        .map_err(|e| anyhow!("cannot start the UI server: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .context("server has no IP address")?
        .port();
    let app = App {
        hub: Arc::new(hub),
        registry: Arc::new(Mutex::new(HashMap::new())),
        token: random_token()?,
        port,
        options,
    };

    let url = format!("http://127.0.0.1:{port}/#token={}", app.token);
    eprintln!("\x1b[1mairth ui\x1b[0m → {url}");
    eprintln!(
        "\x1b[2mThis link grants control of the agent; don't share it. Ctrl-C to quit.\x1b[0m"
    );
    if app.options.open_browser {
        open_in_browser(&url);
    }

    let registry = app.registry.clone();
    ctrlc::set_handler(move || shut_down(&registry)).context("cannot install Ctrl-C handler")?;

    for request in server.incoming_requests() {
        app.handle(request);
    }
    Ok(())
}

/// Stop every live run, give them a moment to save, then exit. A second Ctrl-C exits at once.
fn shut_down(registry: &Registry) {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    if CALLS.fetch_add(1, Ordering::SeqCst) > 0 {
        std::process::exit(130);
    }
    let lives: Vec<Arc<Live>> = registry.lock().unwrap().values().cloned().collect();
    if !lives.is_empty() {
        eprintln!(
            "\nstopping {} run(s)… (Ctrl-C again to quit now)",
            lives.len()
        );
    }
    for live in &lives {
        live.cancel.cancel();
    }
    let registry = registry.clone();
    std::thread::spawn(move || {
        for _ in 0..50 {
            if registry.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::process::exit(0);
    });
}

impl App {
    fn handle(&self, mut request: Request) {
        let response = match self.route(&mut request) {
            Ok(r) => r,
            Err(e) => json_response(400, json!({"error": format!("{e:#}")})),
        };
        let _ = request.respond(response);
    }

    fn route(&self, request: &mut Request) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
        if !self.host_allowed(request) {
            return Ok(json_response(403, json!({"error": "bad host"})));
        }
        let url = request.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((&url, ""));
        let method = request.method().clone();

        match (&method, path) {
            (Method::Get, "/") | (Method::Get, "/index.html") => {
                return Ok(static_response(INDEX_HTML, "text/html; charset=utf-8"));
            }
            (Method::Get, "/app.js") => {
                return Ok(static_response(APP_JS, "text/javascript; charset=utf-8"));
            }
            (Method::Get, "/app.css") => {
                return Ok(static_response(APP_CSS, "text/css; charset=utf-8"));
            }
            _ => {}
        }

        let Some(api) = path.strip_prefix("/api/") else {
            return Ok(json_response(404, json!({"error": "not found"})));
        };
        if !self.token_valid(request) {
            return Ok(json_response(
                401,
                json!({"error": "missing or invalid token"}),
            ));
        }
        let segments: Vec<&str> = api.split('/').collect();
        Ok(match (&method, segments.as_slice()) {
            (Method::Get, ["info"]) => json_response(200, self.info()),
            (Method::Get, ["runs"]) => json_response(200, self.runs()?),
            (Method::Get, ["runs", id]) => json_response(200, self.run(id, query)?),
            (Method::Post, ["runs"]) => {
                let body = read_json(request)?;
                let objective = body["objective"].as_str().unwrap_or_default();
                self.require_model()?;
                let state = self.hub.create_run(objective)?;
                let id = state.id.clone();
                self.spawn(state);
                json_response(200, json!({"id": id}))
            }
            (Method::Post, ["runs", id, "messages"]) => {
                let body = read_json(request)?;
                self.require_model()?;
                if self.registry.lock().unwrap().contains_key(*id) {
                    return Err(anyhow!("this run is still in progress"));
                }
                let state = self
                    .hub
                    .follow_up(id, body["message"].as_str().unwrap_or_default())?;
                self.spawn(state);
                json_response(200, json!({"id": id}))
            }
            (Method::Post, ["runs", id, "approval"]) => {
                let body = read_json(request)?;
                self.answer(id, &body)?;
                json_response(200, json!({"ok": true}))
            }
            (Method::Post, ["runs", id, "stop"]) => {
                let live = self.live(id).context("this run is not in progress")?;
                live.cancel.cancel();
                live.bump();
                json_response(200, json!({"ok": true}))
            }
            _ => json_response(404, json!({"error": "not found"})),
        })
    }

    fn host_allowed(&self, request: &Request) -> bool {
        let host = header(request, "Host").unwrap_or_default();
        host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
    }

    fn token_valid(&self, request: &Request) -> bool {
        header(request, "X-Airth-Token").is_some_and(|t| constant_time_eq(&t, &self.token))
    }

    fn require_model(&self) -> Result<()> {
        match &self.options.model_error {
            Some(e) => Err(anyhow!("no model available: {e}")),
            None => Ok(()),
        }
    }

    fn live(&self, id: &str) -> Option<Arc<Live>> {
        self.registry.lock().unwrap().get(id).cloned()
    }

    fn info(&self) -> Value {
        let tools: Vec<Value> = self
            .hub
            .skills()
            .into_iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "capability": s.capability.as_str(),
                    "description": s.description,
                })
            })
            .collect();
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "model": self.hub.model_name(),
            "model_error": self.options.model_error,
            "workspace": self.hub.workspace().root().display().to_string(),
            "max_iterations": self.options.max_iterations,
            "tools": tools,
        })
    }

    fn runs(&self) -> Result<Value> {
        let registry = self.registry.lock().unwrap();
        let runs: Vec<Value> = self
            .hub
            .runs()?
            .into_iter()
            .rev()
            .map(|r| {
                let live = registry.get(&r.id);
                let needs_approval = live.is_some_and(|l| l.pending.lock().unwrap().is_some());
                json!({
                    "id": r.id,
                    "objective": r.objective,
                    "status": r.display_status(),
                    "live": r.live || live.is_some(),
                    "iterations": r.iterations,
                    "updated_at": r.updated_at,
                    "needs_approval": needs_approval,
                })
            })
            .collect();
        Ok(json!(runs))
    }

    fn run(&self, id: &str, query: &str) -> Result<Value> {
        let since: Option<u64> = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("since="))
            .and_then(|v| v.parse().ok());
        if let Some(live) = self.live(id) {
            let version = live.version.load(Ordering::SeqCst);
            if since == Some(version) {
                return Ok(json!({"unchanged": true, "version": version}));
            }
            let state = live.snapshot.lock().unwrap().clone();
            let pending = live.pending.lock().unwrap().as_ref().map(|p| {
                json!({
                    "id": p.id,
                    "tool": p.request.tool,
                    "capability": p.request.capability.as_str(),
                    "preview": p.request.preview,
                    "warnings": p.request.warnings,
                })
            });
            return Ok(json!({
                "version": version,
                "live": true,
                "stopping": live.cancel.is_cancelled(),
                "state": state,
                "pending": pending,
            }));
        }
        let state = self.hub.load_run(id)?;
        let live_elsewhere = self.hub.is_live(id);
        Ok(json!({
            "version": 0,
            "live": live_elsewhere,
            "elsewhere": live_elsewhere,
            "interrupted": state.status == Status::Running && !live_elsewhere,
            "state": state,
            "pending": null,
        }))
    }

    fn answer(&self, id: &str, body: &Value) -> Result<()> {
        let live = self.live(id).context("this run is not in progress")?;
        let mut pending = live.pending.lock().unwrap();
        let p = pending
            .take_if(|p| Some(p.id) == body["approval_id"].as_u64())
            .context("this approval is no longer pending")?;
        let answer = if body["approve"].as_bool() == Some(true) {
            Answer::Yes
        } else {
            match body["reason"].as_str().map(str::trim) {
                Some(r) if !r.is_empty() => Answer::No(format!("denied by user: {r}")),
                _ => Answer::No("denied by user".into()),
            }
        };
        p.reply
            .send(answer)
            .map_err(|_| anyhow!("the run is no longer waiting"))?;
        live.bump();
        Ok(())
    }

    fn spawn(&self, state: State) {
        let id = state.id.clone();
        let live = Arc::new(Live {
            snapshot: Mutex::new(state.clone()),
            pending: Mutex::new(None),
            cancel: CancelToken::default(),
            version: AtomicU64::new(1),
        });
        self.registry
            .lock()
            .unwrap()
            .insert(id.clone(), live.clone());
        let hub = self.hub.clone();
        let registry = self.registry.clone();
        let max_iterations = self.options.max_iterations;

        std::thread::spawn(move || {
            let mut state = state;
            let mut observer = UiObserver(live.clone());
            let hooks = RunHooks {
                approver: Box::new(UiApprover {
                    live: live.clone(),
                    next_id: 0,
                }),
                observer: &mut observer,
                cancel: live.cancel.clone(),
            };
            if let Err(e) = hub.drive(&mut state, AgentConfig { max_iterations }, hooks)
                && state.status == Status::Running
            {
                state.status = Status::Failed;
                state.report = Some(format!("{e:#}"));
                let _ = state.save();
            }
            *live.snapshot.lock().unwrap() = state;
            live.bump();
            registry.lock().unwrap().remove(&id);
        });
    }
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn read_json(request: &mut Request) -> Result<Value> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut body)?;
    serde_json::from_str(&body).context("request body must be JSON")
}

fn json_response(status: u16, value: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(content_type("application/json"))
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
}

fn static_response(body: &str, kind: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_header(content_type(kind))
        .with_header(Header::from_bytes("X-Frame-Options", "DENY").unwrap())
        .with_header(
            Header::from_bytes(
                "Content-Security-Policy",
                "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            )
            .unwrap(),
        )
        .with_header(Header::from_bytes("Referrer-Policy", "no-referrer").unwrap())
}

fn content_type(kind: &str) -> Header {
    Header::from_bytes("Content-Type", kind).unwrap()
}

fn random_token() -> Result<String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .context("cannot read /dev/urandom")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn open_in_browser(url: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex() {
        let a = random_token().unwrap();
        let b = random_token().unwrap();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(constant_time_eq(&a, &a.clone()));
        assert!(!constant_time_eq(&a, &b));
        assert!(!constant_time_eq(&a, ""));
    }
}
