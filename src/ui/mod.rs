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
//! cannot drive the agent or approve its actions. The phone reaches the app
//! through Tailscale, under a tailnet name the owner allowed (see `phone`).

use crate::agent::{AgentConfig, Observer};
use crate::business::store::Store;
use crate::business::{drafts, time};
use crate::cancel::CancelToken;
use crate::connectors;
use crate::home::NewRealm;
use crate::hub::{Adoption, Hub, PrEvent, RunHooks, away_reason};
use crate::images;
use crate::integration::Integration;
use crate::isolation::{Delivery, Isolation};
use crate::notify::{Inbox, Kind};
use crate::objective::Trigger;
use crate::owner::Owner;
use crate::permissions::{Answer, Approver, Capability, PermissionRequest, Rule};
use crate::realm::{PROVIDER_KEYS, Realm};
use crate::sandbox;
use crate::scheduler;
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

mod calendar;
mod company;
mod extensions;
mod general;
mod graphs;
mod hq;
mod http;
mod library;
mod live;
mod orders;
mod queue;
mod realms;
mod runs;
mod server;

use http::*;
use live::*;
use server::*;

pub(crate) use http::random_token;
#[cfg(target_os = "macos")]
pub use server::start;
pub use server::{open_url, persistent_token, running_url, serve};

/// The interface in web/ (decision 73), embedded by build.rs from web/dist.
mod web {
    include!(concat!(env!("OUT_DIR"), "/web.rs"));
}
/// The sign-in callback's page stands alone: a few lines of style, light or
/// dark as the system is.
const CALLBACK_STYLE: &str = "body{margin:0;min-height:100vh;display:grid;place-items:center;\
font:15px/1.5 -apple-system,BlinkMacSystemFont,system-ui,sans-serif;background:Canvas;color:CanvasText}\
main{max-width:28rem;padding:2rem;text-align:center}h1{font-size:1.25rem;margin:0 0 .5rem}p{margin:0;opacity:.7}";
const MAX_BODY: u64 = 1 << 20;

pub struct UiOptions {
    pub port: u16,
    pub open_browser: bool,
    pub max_iterations: u32,
    /// The realm to open first.
    pub realm: Option<String>,
    /// The scheduler, Telegram, pull requests and update checks run; off
    /// for a preview (`airth ui --preview`).
    pub background: bool,
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
            // One interface, the one in web/ (decisions 72 and 73).
            (Method::Get, p) if web_file(p).is_some() => {
                let (body, kind) = web_file(p).unwrap();
                let response = static_bytes(body, kind);
                // Built files carry a hash in their name: a new build is a new name.
                return Ok(if p.starts_with("/assets/") {
                    response.with_header(
                        Header::from_bytes("Cache-Control", "public, max-age=31536000, immutable")
                            .unwrap(),
                    )
                } else {
                    response.with_header(Header::from_bytes("Cache-Control", "no-cache").unwrap())
                });
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
                             <meta name=\"color-scheme\" content=\"light dark\"><style>{}</style></head>\
                             <body><main><h1>{}</h1><p>{}</p></main></body></html>",
                            CALLBACK_STYLE,
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
                    | ["realms", _, "graphs", _, "dry-run"]
                    | ["realms", _, "orders", "route"]
                    | ["realms", _, "connect"]
                    | ["realms", _, "connectors", ..]
            );
        let _writes = (!concurrent).then(|| self.write_lock());
        let areas = [
            Self::general_routes,
            Self::realms_routes,
            Self::runs_routes,
            Self::extensions_routes,
            Self::queue_routes,
            Self::company_routes,
            Self::graph_routes,
            Self::calendar_routes,
            Self::hq_routes,
            Self::order_routes,
            Self::library_routes,
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
        // Beside loopback, the names the owner allowed for the phone (decision 37).
        let phone = crate::phone::hosts(self.hub.home().dir()).unwrap_or_default();
        crate::phone::host_allowed(&host, self.port, &phone)
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
            "tools": tools,
        })
    }
}

/// A file of the interface built in web/dist, by its path in the address,
/// with its type. Only the built files answer: a path is looked up, never
/// joined to a folder.
fn web_file(path: &str) -> Option<(&'static [u8], &'static str)> {
    let rel = match path {
        "/" | "/index.html" => "index.html",
        p => p.strip_prefix('/')?,
    };
    let (_, body) = web::FILES.iter().find(|(p, _)| *p == rel)?;
    let kind = match rel.rsplit('.').next()? {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "png" => "image/png",
        _ => return None,
    };
    Some((body, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn test_app(home: crate::home::Home, port: u16) -> App {
        let hub = Arc::new(Hub::new(home));
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        App {
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
                background: false,
            },
            update: Arc::new(Mutex::new(UpdateState::default())),
            writes: Mutex::new(()),
            previews: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn an_answer_goes_to_the_owner_time_ledger() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(home_dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "r".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let app = test_app(home, 0);
        let (tx, rx) = mpsc::channel();
        let live = Arc::new(Live {
            snapshot: Mutex::new(State::new("x", None)),
            pending: Mutex::new(Some(Pending {
                id: 1,
                request: PermissionRequest {
                    tool: "write_file".into(),
                    capability: Capability::Write,
                    preview: String::new(),
                    warnings: Vec::new(),
                    scope: None,
                },
                reply: tx,
                asked: std::time::Instant::now() - Duration::from_secs(5),
            })),
            cancel: CancelToken::default(),
            version: AtomicU64::new(1),
            watch: Watch::default(),
            draft: Draft::default(),
            said: Mutex::new(None),
        });
        app.registry.lock().unwrap().insert(key("r", "run1"), live);
        app.answer("r", "run1", &json!({"approval_id": 1, "approve": true}))
            .unwrap();
        assert!(matches!(rx.recv().unwrap(), Answer::Yes));
        let since = chrono::Utc::now() - chrono::Duration::hours(1);
        let entries = Store::of(&realm).unwrap().time_since(since).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].activity, time::ANSWER);
        assert_eq!(entries[0].seconds, 5);
        assert_eq!(entries[0].note, "write_file in run run1");
    }

    /// The page waiting on a run hears the model's words as it writes them,
    /// and its answer once it is in the run; it waits only while nothing
    /// changed, and is told when the run is not driven here.
    #[test]
    fn a_page_hears_the_words_as_the_model_writes_them() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(home_dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "r".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let app = test_app(home, 0);
        assert_eq!(app.next_change(&realm, "run1", "")["here"], false);
        let live = Arc::new(Live {
            snapshot: Mutex::new(State::new("x", None)),
            pending: Mutex::new(None),
            cancel: CancelToken::default(),
            version: AtomicU64::new(1),
            watch: Watch::default(),
            draft: Draft::default(),
            said: Mutex::new(None),
        });
        app.registry
            .lock()
            .unwrap()
            .insert(key("r", "run1"), live.clone());
        let mut observer = UiObserver(live.clone());
        observer.writing("Hel");
        observer.writing("Hello.");
        let now = app.next_change(&realm, "run1", "version=1&draft=1");
        assert_eq!(now["draft"], "Hello.");
        assert_eq!(now["draft_seq"], 2);
        // Nothing new: it waits, and hears the next words at once.
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            observer.writing("Hello. Done");
            observer
        });
        let started = std::time::Instant::now();
        let next = app.next_change(&realm, "run1", "version=1&draft=2");
        assert_eq!(next["draft"], "Hello. Done");
        assert!(started.elapsed() < Duration::from_secs(2));
        // The answer is in the run: the words being written are gone.
        writer.join().unwrap().changed(&State::new("x", None));
        let done = app.next_change(&realm, "run1", "version=1&draft=3");
        assert_eq!(
            (done["version"].as_u64(), done["draft"].as_str()),
            (Some(2), Some(""))
        );
    }

    #[test]
    fn the_interface_built_in_web_is_served_and_nothing_else() {
        let (page, kind) = web_file("/").unwrap();
        assert!(String::from_utf8_lossy(page).contains("id=\"root\""));
        assert_eq!(kind, "text/html; charset=utf-8");
        let script = web::FILES
            .iter()
            .find(|(p, _)| p.starts_with("assets/") && p.ends_with(".js"))
            .unwrap();
        let (_, kind) = web_file(&format!("/{}", script.0)).unwrap();
        assert_eq!(kind, "text/javascript; charset=utf-8");
        for path in [
            "/assets/nothing.js",
            "/../Cargo.toml",
            "/assets/../index.html",
            "/api/realms",
            "",
            // The classic and /next interfaces left once their features moved.
            "/classic",
            "/next",
            "/app.css",
            "/theme.js",
        ] {
            assert!(web_file(path).is_none(), "{path}");
        }
    }

    /// Agents lists each bot with the routines that name it; a bot a
    /// routine still names cannot be let go.
    #[test]
    fn agents_and_their_routines() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(home_dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "r".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let root = realm.root().unwrap();
        crate::business::set_job(&root, "writer", "Write one article a week.").unwrap();
        crate::business::set_job(&root, "analyst", "Watch the markets.").unwrap();
        realm
            .objectives()
            .save(crate::objective::Objective {
                id: "weekly-article".into(),
                objective: "Write this week's article".into(),
                trigger: crate::objective::Trigger::Manual,
                approvals: vec![],
                budget: Default::default(),
                approval_wait_minutes: 0,
                enabled: true,
                created_at: chrono::Utc::now(),
                repo: None,
                bot: Some("writer".into()),
                tools: Some(vec![]),
                delegates: Vec::new(),
                next_at: None,
                fingerprint: None,
                last_run: None,
                last_at: None,
                last_status: None,
            })
            .unwrap();
        let app = test_app(home, 0);
        let d = app.agents(&realm).unwrap();
        let agents = d["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[1]["name"], "writer");
        assert_eq!(agents[1]["routines"][0]["id"], "weekly-article");
        assert_eq!(agents[1]["routines"][0]["tools"], json!([]));
        assert_eq!(agents[0]["routines"], json!([]));
        let err = realms::let_go(&realm, "writer").unwrap_err();
        assert!(format!("{err:#}").contains("weekly-article"), "{err:#}");
        realms::let_go(&realm, "analyst").unwrap();
        assert_eq!(crate::business::bots(&root).unwrap().len(), 1);
    }

    /// A routine made in the app names a hired bot and the business's own
    /// extensions only; Planning shows its coming week.
    #[test]
    fn routines_from_the_app_and_their_planning() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(home_dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "r".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let routine = |bot: &str, tools: Value| {
            json!({"id": "weekly-article", "objective": "Write this week's article", "bot": bot,
                "every": "daily 08:00", "tools": tools, "allow": ["network"], "max_minutes": 30})
        };
        let err = realms::save_routine(&realm, &routine("writer", json!(null))).unwrap_err();
        assert!(format!("{err:#}").contains("agents/writer.md"), "{err:#}");
        crate::business::set_job(&realm.root().unwrap(), "writer", "Write.").unwrap();
        let err = realms::save_routine(&realm, &routine("writer", json!(["bank"]))).unwrap_err();
        assert!(format!("{err:#}").contains("'bank'"), "{err:#}");
        realms::save_routine(&realm, &routine("writer", json!([]))).unwrap();
        let o = realm.objectives().get("weekly-article").unwrap();
        assert_eq!(o.bot.as_deref(), Some("writer"));
        assert_eq!(o.tools, Some(vec![]));
        assert_eq!(o.budget.max_minutes, Some(30));
        assert_eq!(o.approvals[0].capability, Capability::Network);

        let app = test_app(home, 0);
        let listed = app.routines(&realm).unwrap();
        assert_eq!(listed["routines"][0]["allow"], json!(["network"]));
        assert_eq!(listed["agents"], json!(["writer"]));
        let plan = app.planning(&realm, chrono::Utc::now()).unwrap();
        let coming = plan["coming"].as_array().unwrap();
        assert_eq!(coming.len(), 7, "{plan}");
        assert_eq!(coming[0]["bot"], "writer");
        assert!(coming[0]["at"].as_str() < coming[1]["at"].as_str());
        realm
            .objectives()
            .update("weekly-article", |o| o.enabled = false)
            .unwrap();
        let plan = app.planning(&realm, chrono::Utc::now()).unwrap();
        assert_eq!(
            plan["coming"],
            json!([]),
            "a paused routine has no coming times"
        );
    }

    /// Steering shows what waits for the owner in this business only.
    #[test]
    fn steering_lists_this_realms_approvals_and_strategy() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(home_dir.path()).unwrap();
        let new = |name: &str| NewRealm {
            name: name.into(),
            description: String::new(),
            root: None,
            isolation: None,
        };
        let realm = home.create_realm(new("r")).unwrap();
        home.create_realm(new("rr")).unwrap();
        crate::business::set_strategy(&realm.root().unwrap(), "Four articles a month.").unwrap();
        crate::business::set_job(&realm.root().unwrap(), "writer", "Write.").unwrap();
        let app = test_app(home, 0);
        let waiting = |preview: &str| {
            let (tx, _rx) = mpsc::channel();
            Arc::new(Live {
                snapshot: Mutex::new(State::new("Publish the article", None)),
                pending: Mutex::new(Some(Pending {
                    id: 7,
                    request: PermissionRequest {
                        tool: "execute_command".into(),
                        capability: Capability::Execute,
                        preview: preview.into(),
                        warnings: Vec::new(),
                        scope: None,
                    },
                    reply: tx,
                    asked: std::time::Instant::now(),
                })),
                cancel: CancelToken::default(),
                version: AtomicU64::new(1),
                watch: Watch::default(),
                draft: Draft::default(),
                said: Mutex::new(None),
            })
        };
        app.registry
            .lock()
            .unwrap()
            .insert(key("r", "run1"), waiting("$ git push"));
        app.registry
            .lock()
            .unwrap()
            .insert(key("rr", "run2"), waiting("$ rm -rf other"));
        let d = app.steering(&realm).unwrap();
        let approvals = d["approvals"].as_array().unwrap();
        assert_eq!(approvals.len(), 1, "{d}");
        assert_eq!(approvals[0]["run"], "run1");
        assert_eq!(approvals[0]["approval_id"], 7);
        assert_eq!(approvals[0]["preview"], "$ git push");
        assert_eq!(d["strategy"], "Four articles a month.");
        assert_eq!(d["attention"], json!([]));
        assert_eq!(d["delivered"], json!([]));
        assert_eq!(d["team"][0]["name"], "writer");
        assert_eq!(d["team"][0]["running"], false);
        assert_eq!(d["team"][0]["routines"], 0);
    }

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
        let app = test_app(home, port);
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
