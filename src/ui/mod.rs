//! `airth ui`: a local app served on 127.0.0.1, built on the hub.
//!
//! Each run started from the UI is driven on its own thread. The UI watches it
//! through a shared snapshot and answers permission requests through a channel;
//! the permission policy itself is the realm's, applied by the hub.
//!
//! The server only listens on loopback and every API call must carry a random
//! per-launch token in the `X-Airth-Token` header. Since a custom header cannot
//! be sent cross-origin without a CORS preflight (which we never grant), and
//! the Host header is checked against loopback (DNS rebinding), other websites
//! cannot drive the agent or approve its actions.

use crate::agent::{AgentConfig, Observer};
use crate::bench::Bench;
use crate::bench::metrics::Summary;
use crate::bench::storage::{BenchRun, RunOptions};
use crate::cancel::CancelToken;
use crate::connectors;
use crate::home::NewRealm;
use crate::hub::{Adoption, Hub, PrEvent, RunHooks, away_reason};
use crate::images;
use crate::integration::Integration;
use crate::isolation::{Delivery, Isolation};
use crate::notify::{Inbox, Kind};
use crate::objective::{Objective, Trigger};
use crate::owner::Owner;
use crate::permissions::{Answer, Approver, Capability, PermissionRequest, Rule};
use crate::realm::{PROVIDER_KEYS, Realm};
use crate::sandbox;
use crate::scheduler;
use crate::skill::SkillStatus;
use crate::state::{State, Status};
use crate::tools::catalog;
use crate::tools::mcp::McpStore;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

mod bench;
mod extensions;
mod general;
mod http;
mod live;
mod realms;
mod runs;
mod server;

use http::*;
use live::*;
use server::*;

pub(crate) use http::random_token;
pub use server::{open_url, running_url, serve, start};

const INDEX_HTML: &str = include_str!("index.html");
// js/ and css/, joined in name order by build.rs.
const APP_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/app.js"));
const APP_CSS: &str = include_str!(concat!(env!("OUT_DIR"), "/app.css"));
const THEME_JS: &str = include_str!("theme.js");
/// The Documents editor (CodeMirror), built from editor/ by editor/build.sh.
const EDITOR_JS: &str = include_str!("vendor/editor.js");
const FAVICON: &str = include_str!("favicon.svg");
const MAX_BODY: u64 = 1 << 20;

pub struct UiOptions {
    pub port: u16,
    pub open_browser: bool,
    pub max_iterations: u32,
    /// The realm to open first.
    pub realm: Option<String>,
}

struct App {
    hub: Arc<Hub>,
    registry: Registry,
    runner: Runner,
    token: String,
    port: u16,
    options: UiOptions,
    update: Arc<Mutex<UpdateState>>,
    /// Requests run on their own threads; changes to stored settings still
    /// happen one at a time (see `route`).
    writes: Mutex<()>,
    /// Links to PDF previews (see `files/preview`): a frame cannot send the
    /// API token, so each preview gets its own short-lived, unguessable link.
    previews: Mutex<HashMap<String, (std::path::PathBuf, std::time::Instant)>>,
    /// Benchmark runs this process executes, to stop them.
    benches: Arc<Mutex<HashMap<String, CancelToken>>>,
}

impl App {
    fn write_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }
}

type HttpResponse = Response<std::io::Cursor<Vec<u8>>>;

impl App {
    fn handle(&self, mut request: Request) {
        let response = match self.route(&mut request) {
            Ok(r) => r,
            Err(e) => json_response(400, json!({"error": format!("{e:#}")})),
        };
        let _ = request.respond(response);
    }

    fn route(&self, request: &mut Request) -> Result<HttpResponse> {
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
            (Method::Get, "/vendor/editor.js") => {
                return Ok(static_response(EDITOR_JS, "text/javascript; charset=utf-8"));
            }
            (Method::Get, "/theme.js") => {
                return Ok(static_response(THEME_JS, "text/javascript; charset=utf-8"));
            }
            (Method::Get, "/favicon.svg") => {
                return Ok(static_response(FAVICON, "image/svg+xml"));
            }
            // The browser coming back from a connector's sign-in page. It cannot
            // carry the API token; the one-time `state` Airth issued is the proof.
            (Method::Get, "/oauth/callback") => {
                let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect();
                let page = |title: &str, text: &str| {
                    static_response(
                        &format!(
                            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Airth</title>\
                             <script src=\"/theme.js\"></script><link rel=\"stylesheet\" href=\"/app.css\"></head>\
                             <body><div class=\"center-note\"><span class=\"logo\"></span><h1>{}</h1><p>{}</p></div></body></html>",
                            html_escape(title),
                            html_escape(text)
                        ),
                        "text/html; charset=utf-8",
                    )
                };
                let result = match (params.get("state"), params.get("code"), params.get("error")) {
                    (_, _, Some(error)) => Err(anyhow!(
                        "{}",
                        params.get("error_description").unwrap_or(error)
                    )),
                    (Some(state), Some(code), None) => crate::tools::remote::finish(state, code),
                    _ => Err(anyhow!("the sign-in page did not say whether it worked")),
                };
                return Ok(match result {
                    Ok(name) => page(
                        &format!("{name} is connected"),
                        "Airth can now use it in this realm. You can close this tab.",
                    ),
                    Err(e) => page("The connection did not work", &format!("{e:#}")),
                });
            }
            _ => {}
        }

        // A PDF preview, by the link `files/preview` gave (see `previews`).
        if method == Method::Get
            && let Some(ticket) = path.strip_prefix("/preview/")
        {
            let found = {
                let mut previews = self.previews.lock().unwrap();
                previews.retain(|_, (_, at)| at.elapsed() < PREVIEW_FOR);
                previews.get(ticket).map(|(p, _)| p.clone())
            };
            let Some(file) = found else {
                return Ok(json_response(
                    404,
                    json!({"error": "this preview has expired"}),
                ));
            };
            return Ok(Response::from_data(std::fs::read(&file)?)
                .with_header(content_type("application/pdf"))
                .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
                .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
                .with_header(Header::from_bytes("X-Frame-Options", "SAMEORIGIN").unwrap())
                .with_header(Header::from_bytes("Referrer-Policy", "no-referrer").unwrap()));
        }
        if method == Method::Post
            && let Some(hook) = path.strip_prefix("/hooks/")
        {
            return Ok(self.webhook(request, hook));
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
        let segments: Vec<String> = api.split('/').map(percent_decode).collect();
        let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
        // Changes to stored settings happen one at a time. Exceptions: reads;
        // approvals and stops, which only touch live runs; and the routes that
        // wait on a remote service, which take the lock only around their writes.
        let concurrent = method == Method::Get
            || matches!(
                segments.as_slice(),
                [.., "approval"]
                    | [.., "stop"]
                    | ["realms", _, "connect"]
                    | ["realms", _, "connectors", ..]
            );
        let _writes = (!concurrent).then(|| self.write_lock());
        let areas = [
            Self::general_routes,
            Self::realms_routes,
            Self::runs_routes,
            Self::extensions_routes,
            Self::bench_routes,
        ];
        for routes in areas {
            if let Some(response) = routes(self, request, &method, &segments, query)? {
                return Ok(response);
            }
        }
        Ok(json_response(404, json!({"error": "not found"})))
    }

    fn host_allowed(&self, request: &Request) -> bool {
        let host = header(request, "Host").unwrap_or_default();
        host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
    }

    fn token_valid(&self, request: &Request) -> bool {
        header(request, "X-Airth-Token").is_some_and(|t| constant_time_eq(&t, &self.token))
    }

    fn realm(&self, name: &str) -> Result<Realm> {
        self.hub.home().realm(name)
    }

    fn live(&self, realm: &str, id: &str) -> Option<Arc<Live>> {
        self.registry.lock().unwrap().get(&key(realm, id)).cloned()
    }

    fn ensure_idle(&self, realm: &str, id: &str) -> Result<()> {
        if self.live(realm, id).is_some() {
            return Err(anyhow!("this run is still in progress"));
        }
        Ok(())
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
            "version": crate::update::version(),
            "update": &*self.update.lock().unwrap(),
            "home": self.hub.home().dir().display().to_string(),
            "sandbox": sandbox::available(),
            "realm": self.options.realm,
            "default_realm": self.hub.home().default_realm(),
            "max_iterations": self.options.max_iterations,
            "unread": self.hub.home().inbox().unread(),
            "benchmarks": self.hub.home().benchmarks_enabled(),
            "tools": tools,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_service_does_not_freeze_the_app() {
        // A remote MCP server that takes 2 seconds to answer.
        let slow = Server::http("127.0.0.1:0").unwrap();
        let slow_url = format!("http://{}/mcp", slow.server_addr().to_ip().unwrap());
        std::thread::spawn(move || {
            for req in slow.incoming_requests() {
                std::thread::sleep(Duration::from_secs(2));
                let _ = req.respond(Response::from_string("").with_status_code(500));
            }
        });
        let home_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(home_dir.path()).unwrap();
        home.create_realm(NewRealm {
            name: "r".into(),
            description: String::new(),
            root: Some(root.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
        let server = Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let hub = Arc::new(Hub::new(home));
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        let app = App {
            runner: Runner {
                hub: hub.clone(),
                registry: registry.clone(),
                max_iterations: 1,
            },
            hub,
            registry,
            token: "t0k3n".into(),
            port,
            options: UiOptions {
                port,
                open_browser: false,
                max_iterations: 1,
                realm: None,
            },
            update: Arc::new(Mutex::new(UpdateState::default())),
            writes: Mutex::new(()),
            previews: Mutex::new(HashMap::new()),
            benches: Arc::new(Mutex::new(HashMap::new())),
        };
        std::thread::spawn(move || serve_requests(server, app));
        let base = format!("http://127.0.0.1:{port}");
        let b = base.clone();
        let pending = std::thread::spawn(move || {
            reqwest::blocking::Client::new()
                .post(format!("{b}/api/realms/r/connect"))
                .header("X-Airth-Token", "t0k3n")
                .body(json!({"name": "slow", "url": slow_url}).to_string())
                .send()
                .map(|r| r.status().as_u16())
        });
        std::thread::sleep(Duration::from_millis(300));
        let started = std::time::Instant::now();
        let info = reqwest::blocking::Client::new()
            .get(format!("{base}/api/realms/r"))
            .header("X-Airth-Token", "t0k3n")
            .send()
            .unwrap();
        assert!(info.status().is_success(), "{}", info.status());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the app waited for the slow service ({:?})",
            started.elapsed()
        );
        // The slow connection itself still finishes (with the service's error).
        assert_eq!(pending.join().unwrap().unwrap(), 400);
    }

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

    #[test]
    fn decodes_path_segments() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%é"), "%é");
    }
}
