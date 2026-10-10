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
use crate::cancel::CancelToken;
use crate::home::NewRealm;
use crate::hub::{Hub, RunHooks, away_reason};
use crate::integration::Integration;
use crate::isolation::Isolation;
use crate::notify::{Inbox, Kind};
use crate::objective::{Objective, Trigger};
use crate::owner::Owner;
use crate::permissions::{Answer, Approver, Capability, PermissionRequest, Rule};
use crate::realm::{PROVIDER_KEYS, Realm};
use crate::sandbox;
use crate::scheduler;
use crate::skill::SkillStatus;
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
    /// The realm to open first.
    pub realm: Option<String>,
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

/// Live runs by "<realm>/<run id>".
type Registry = Arc<Mutex<HashMap<String, Arc<Live>>>>;

fn key(realm: &str, id: &str) -> String {
    format!("{realm}/{id}")
}

struct UiObserver(Arc<Live>);

impl Observer for UiObserver {
    fn changed(&mut self, state: &State) {
        *self.0.snapshot.lock().unwrap() = state.clone();
        self.0.bump();
    }
}

/// Parks the agent thread until the human answers in the UI or stops the run.
/// For a run an objective started, the owner may be away: it tells them, and
/// waits only as long as the objective allows.
struct UiApprover {
    live: Arc<Live>,
    next_id: u64,
    away: Option<Away>,
}

/// An objective's run, which nobody may be watching.
struct Away {
    objective: String,
    realm: String,
    run: String,
    wait_minutes: u32,
    inbox: Inbox,
}

impl Approver for UiApprover {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        let deadline = match &self.away {
            Some(away) if away.wait_minutes == 0 => return Ok(Answer::No(away_reason(0))),
            Some(away) => {
                let _ = away.inbox.push(
                    Kind::Approval,
                    &away.realm,
                    Some(&away.run),
                    &format!("{} is waiting for your approval", away.objective),
                    &format!(
                        "{} ({}). It waits {} minutes, then goes on without it.",
                        request.tool,
                        request.capability.as_str(),
                        away.wait_minutes
                    ),
                );
                Some(
                    std::time::Instant::now()
                        + Duration::from_secs(u64::from(away.wait_minutes) * 60),
                )
            }
            None => None,
        };
        let (tx, rx) = mpsc::channel();
        self.next_id += 1;
        *self.live.pending.lock().unwrap() = Some(Pending {
            id: self.next_id,
            request: request.clone(),
            reply: tx,
        });
        self.live.bump();
        let answer = loop {
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                let waited = self.away.as_ref().map_or(0, |a| a.wait_minutes);
                break Answer::No(away_reason(waited));
            }
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

/// What a background thread does with a run.
enum Job {
    /// Drive the agent (a new run, a follow-up, a resume).
    Drive,
    /// Drive a run an objective started, which the owner may not be watching.
    Unattended {
        objective: String,
        wait_minutes: u32,
    },
    /// Run the realm's checks on the run's change.
    Check,
}

/// Starts runs on background threads and keeps them in the registry. Shared
/// by request handlers, the scheduler and webhooks.
#[derive(Clone)]
struct Runner {
    hub: Arc<Hub>,
    registry: Registry,
    max_iterations: u32,
}

struct App {
    hub: Arc<Hub>,
    registry: Registry,
    runner: Runner,
    token: String,
    port: u16,
    options: UiOptions,
}

/// Where a running app can be reached, for `airth ui` to reuse it.
#[derive(serde::Serialize, serde::Deserialize)]
struct ServerInfo {
    pid: i32,
    port: u16,
    token: String,
}

fn server_file(hub: &Hub) -> std::path::PathBuf {
    hub.home().dir().join("server.json")
}

/// The link to the app already serving this home, if one is.
pub fn running_url(hub: &Hub) -> Option<String> {
    let text = std::fs::read_to_string(server_file(hub)).ok()?;
    let info: ServerInfo = serde_json::from_str(&text).ok()?;
    if !crate::hub::process_alive(info.pid) {
        return None;
    }
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    Some(format!(
        "http://127.0.0.1:{}/#token={}",
        info.port, info.token
    ))
}

pub fn open_url(url: &str) {
    open_in_browser(url);
}

/// Look for due objectives every few seconds and start their runs. Only one
/// app serves a home at a time, so only one scheduler runs.
fn run_scheduler(runner: Runner) {
    loop {
        match scheduler::due(&runner.hub, chrono::Utc::now()) {
            Ok(due) => {
                for d in due {
                    match runner.hub.start_objective(
                        &d.realm,
                        &d.objective,
                        &d.trigger,
                        None,
                        chrono::Utc::now(),
                    ) {
                        Ok((state, o)) => {
                            eprintln!(
                                "objective {}/{} started run {} ({})",
                                d.realm.name(),
                                o.id,
                                state.id,
                                d.trigger
                            );
                            runner.spawn(
                                d.realm,
                                state,
                                Job::Unattended {
                                    objective: o.id,
                                    wait_minutes: o.approval_wait_minutes,
                                },
                            );
                        }
                        Err(e) => eprintln!(
                            "objective {}/{} could not start: {e:#}",
                            d.realm.name(),
                            d.objective
                        ),
                    }
                }
            }
            Err(e) => eprintln!("scheduler: {e:#}"),
        }
        std::thread::sleep(scheduler::TICK);
    }
}

type HttpResponse = Response<std::io::Cursor<Vec<u8>>>;

pub fn serve(hub: Hub, options: UiOptions) -> Result<()> {
    let server = Server::http(("127.0.0.1", options.port))
        .or_else(|_| Server::http(("127.0.0.1", 0)))
        .map_err(|e| anyhow!("cannot start the UI server: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .context("server has no IP address")?
        .port();
    let hub = Arc::new(hub);
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
    let runner = Runner {
        hub: hub.clone(),
        registry: registry.clone(),
        max_iterations: options.max_iterations,
    };
    let app = App {
        hub,
        registry,
        runner,
        token: random_token()?,
        port,
        options,
    };
    let info_path = server_file(&app.hub);
    crate::realm::write_private(
        &info_path,
        &serde_json::to_string(&ServerInfo {
            pid: std::process::id() as i32,
            port,
            token: app.token.clone(),
        })?,
    )?;

    let url = format!("http://127.0.0.1:{port}/#token={}", app.token);
    eprintln!("\x1b[1mairth\x1b[0m → {url}");
    eprintln!(
        "\x1b[2mThis link grants control of the agent; don't share it. Objectives run while this is running. Ctrl-C to quit.\x1b[0m"
    );
    if app.options.open_browser {
        open_in_browser(&url);
    }

    let registry = app.registry.clone();
    let path = info_path.clone();
    ctrlc::set_handler(move || shut_down(&registry, &path))
        .context("cannot install Ctrl-C handler")?;
    #[cfg(unix)]
    {
        // launchd and `kill` stop services with SIGTERM: shut down the same way.
        let _ =
            unsafe { libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t) };
    }

    let runner = app.runner.clone();
    std::thread::spawn(move || run_scheduler(runner));

    for request in server.incoming_requests() {
        app.handle(request);
    }
    Ok(())
}

#[cfg(unix)]
extern "C" fn on_sigterm(_: libc::c_int) {
    // Only async-signal-safe work here: ctrlc's handler thread does the rest.
    unsafe {
        libc::kill(libc::getpid(), libc::SIGINT);
    }
}

/// Stop every live run, give them a moment to save, then exit. A second Ctrl-C exits at once.
fn shut_down(registry: &Registry, info_path: &std::path::Path) {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let _ = std::fs::remove_file(info_path);
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
            _ => {}
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
        let ok = || json_response(200, json!({"ok": true}));
        Ok(match (&method, segments.as_slice()) {
            (Method::Get, ["info"]) => json_response(200, self.info()),
            (Method::Get, ["owner"]) => json_response(200, json!(self.hub.home().owner()?)),
            (Method::Put, ["owner"]) => {
                let owner: Owner =
                    serde_json::from_value(read_json(request)?).context("invalid profile")?;
                self.hub.home().save_owner(&owner)?;
                json_response(200, json!(self.hub.home().owner()?))
            }
            (Method::Get, ["realms"]) => json_response(200, self.realms()?),
            (Method::Post, ["realms"]) => {
                let body = read_json(request)?;
                let realm = self.create_realm(&body)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r]) => json_response(200, self.realm_detail(&self.realm(r)?)?),
            (Method::Patch, ["realms", r]) => {
                let body = read_json(request)?;
                let realm = self.update_realm(r, &body)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Post, ["realms", r, "memory"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                realm
                    .memory()
                    .add(body["text"].as_str().unwrap_or_default(), "owner")?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "memory", id]) => {
                let realm = self.realm(r)?;
                if !realm.memory().remove(id)? {
                    return Err(anyhow!("no such memory"));
                }
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Post, ["realms", r, "secrets"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                realm.set_secret(
                    body["key"].as_str().unwrap_or_default().trim(),
                    body["value"].as_str().unwrap_or_default(),
                )?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "secrets", k]) => {
                let realm = self.realm(r)?;
                if !realm.remove_secret(k)? {
                    return Err(anyhow!("no such credential"));
                }
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r, "runs"]) => json_response(200, self.runs(&self.realm(r)?)?),
            (Method::Post, ["realms", r, "runs"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                let state = self
                    .hub
                    .create_run(&realm, body["objective"].as_str().unwrap_or_default())?;
                let id = state.id.clone();
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": id}))
            }
            (Method::Get, ["realms", r, "runs", id]) => {
                json_response(200, self.run(&self.realm(r)?, id, query)?)
            }
            (Method::Get, ["realms", r, "runs", id, "diff"]) => {
                let diff = self.hub.diff(&self.realm(r)?, id)?;
                json_response(200, json!({"diff": truncate_diff(diff)}))
            }
            (Method::Post, ["realms", r, "runs", id, "messages"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                self.ensure_idle(r, id)?;
                let state =
                    self.hub
                        .follow_up(&realm, id, body["message"].as_str().unwrap_or_default())?;
                self.spawn(realm, state, Job::Drive);
                ok()
            }
            (Method::Get, ["inbox"]) => {
                let mut list = self.hub.home().inbox().list()?;
                list.reverse();
                json_response(200, json!(list))
            }
            (Method::Post, ["inbox", "read"]) => {
                let body = read_json(request)?;
                let ids: Option<Vec<u64>> = body["ids"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_u64).collect());
                self.hub.home().inbox().mark_read(ids.as_deref())?;
                ok()
            }
            (Method::Get, ["summary"]) => {
                let hours: i64 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("hours="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(24);
                let since = chrono::Utc::now() - chrono::Duration::hours(hours.clamp(1, 24 * 90));
                json_response(200, json!(scheduler::summary(&self.hub, since)?))
            }
            (Method::Get, ["realms", r, "skills"]) => {
                json_response(200, self.skills(&self.realm(r)?)?)
            }
            (Method::Post, ["realms", r, "skills"]) => {
                // The owner asks for a new skill: develop it right away.
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                let name = body["name"].as_str().unwrap_or_default().trim();
                if realm.skills().get(name).is_ok() {
                    return Err(anyhow!("a skill named {name} already exists"));
                }
                let description = body["description"].as_str().unwrap_or_default().trim();
                if description.is_empty() {
                    return Err(anyhow!("describe what the skill should do"));
                }
                let state = self.hub.develop_skill(&realm, name, Some(description))?;
                let run = state.id.clone();
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": run}))
            }
            (Method::Post, ["realms", r, "skills", name, "develop"]) => {
                let body = read_json(request).unwrap_or(json!({}));
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                let record = realm.skills().get(name)?;
                let feedback = body["feedback"]
                    .as_str()
                    .map(str::trim)
                    .filter(|f| !f.is_empty());
                // Rework continues the development run, which knows the skill.
                let state = match (&record.dev_run, feedback) {
                    (Some(run), Some(feedback)) => {
                        self.ensure_idle(r, run)?;
                        let state = self.hub.follow_up(&realm, run, feedback)?;
                        realm
                            .skills()
                            .update(name, |s| s.status = SkillStatus::Developing)?;
                        state
                    }
                    _ => self.hub.develop_skill(&realm, name, None)?,
                };
                let run = state.id.clone();
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": run}))
            }
            (Method::Get, ["realms", r, "skills", name, "files"]) => {
                let realm = self.realm(r)?;
                realm.skills().get(name)?;
                let dir = realm.skills().draft_dir(name);
                let files: Vec<Value> = crate::skill::list_files(&dir)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|f| {
                        let content = std::fs::read_to_string(dir.join(&f)).ok();
                        json!({"path": f.display().to_string(), "content": content})
                    })
                    .collect();
                json_response(200, json!(files))
            }
            (Method::Post, ["realms", r, "skills", name, action]) => {
                let realm = self.realm(r)?;
                let store = realm.skills();
                match *action {
                    "evaluate" => {
                        self.hub.evaluate_skill(&realm, name)?;
                    }
                    "install" => {
                        store.install(name)?;
                    }
                    "uninstall" => {
                        store.uninstall(name)?;
                    }
                    "reject" => {
                        store.update(name, |s| s.status = SkillStatus::Rejected)?;
                    }
                    _ => return Err(anyhow!("unknown action")),
                }
                json_response(200, self.skills(&realm)?)
            }
            (Method::Delete, ["realms", r, "skills", name]) => {
                let realm = self.realm(r)?;
                if !realm.skills().remove(name)? {
                    return Err(anyhow!("no such skill"));
                }
                json_response(200, self.skills(&realm)?)
            }
            (Method::Get, ["realms", r, "objectives"]) => {
                json_response(200, self.objectives(&self.realm(r)?)?)
            }
            (Method::Post, ["realms", r, "objectives"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.save_objective(&realm, &body)?;
                json_response(200, self.objectives(&realm)?)
            }
            (Method::Patch, ["realms", r, "objectives", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                if let Some(enabled) = body["enabled"].as_bool() {
                    let found = realm.objectives().update(id, |o| {
                        o.enabled = enabled;
                        // Re-arm: a schedule missed while disabled does not fire.
                        o.next_at = None;
                        o.fingerprint = None;
                    })?;
                    if !found {
                        return Err(anyhow!("no such objective"));
                    }
                }
                json_response(200, self.objectives(&realm)?)
            }
            (Method::Delete, ["realms", r, "objectives", id]) => {
                let realm = self.realm(r)?;
                if !realm.objectives().remove(id)? {
                    return Err(anyhow!("no such objective"));
                }
                json_response(200, self.objectives(&realm)?)
            }
            (Method::Post, ["realms", r, "objectives", id, "run"]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                let (state, _) = self.hub.start_objective(
                    &realm,
                    id,
                    "started by the owner",
                    None,
                    chrono::Utc::now(),
                )?;
                let run = state.id.clone();
                // The owner is here: they answer approvals as for any run.
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": run}))
            }
            (Method::Post, ["realms", r, "runs", id, "resume"]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                self.ensure_idle(r, id)?;
                let state = self.hub.resume(&realm, id)?;
                self.spawn(realm, state, Job::Drive);
                ok()
            }
            (Method::Post, ["realms", r, "integrations"]) => {
                let body = read_json(request)?;
                let mut realm = self.realm(r)?;
                let integration: Integration =
                    serde_json::from_value(body).context("invalid integration")?;
                realm.set_integration(integration)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "integrations", name]) => {
                let mut realm = self.realm(r)?;
                if !realm.remove_integration(name)? {
                    return Err(anyhow!("no such integration"));
                }
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Post, ["realms", r, "runs", id, "approval"]) => {
                let body = read_json(request)?;
                self.answer(r, id, &body)?;
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "stop"]) => {
                let live = self.live(r, id).context("this run is not in progress")?;
                live.cancel.cancel();
                live.bump();
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "checks"]) => {
                let realm = self.realm(r)?;
                self.ensure_idle(r, id)?;
                let state = self.hub.load_run(&realm, id)?;
                self.spawn(realm, state, Job::Check);
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "adopt"]) => {
                self.ensure_idle(r, id)?;
                self.hub.adopt(&self.realm(r)?, id)?;
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "discard"]) => {
                self.ensure_idle(r, id)?;
                self.hub.discard(&self.realm(r)?, id)?;
                ok()
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
            "version": env!("CARGO_PKG_VERSION"),
            "home": self.hub.home().dir().display().to_string(),
            "sandbox": sandbox::available(),
            "realm": self.options.realm,
            "default_realm": self.hub.home().default_realm(),
            "max_iterations": self.options.max_iterations,
            "unread": self.hub.home().inbox().unread(),
            "tools": tools,
        })
    }

    fn realms(&self) -> Result<Value> {
        let registry = self.registry.lock().unwrap();
        let default = self.hub.home().default_realm();
        let realms: Vec<Value> = self
            .hub
            .home()
            .realms()?
            .into_iter()
            .map(|r| {
                let prefix = key(r.name(), "");
                let lives: Vec<&Arc<Live>> = registry
                    .iter()
                    .filter(|(k, _)| k.starts_with(&prefix))
                    .map(|(_, v)| v)
                    .collect();
                json!({
                    "name": r.name(),
                    "description": r.config.description,
                    "archived": r.config.archived,
                    "default": default.as_deref() == Some(r.name()),
                    "live": lives.len(),
                    "needs_approval": lives.iter().filter(|l| l.pending.lock().unwrap().is_some()).count(),
                })
            })
            .collect();
        Ok(json!(realms))
    }

    fn realm_detail(&self, realm: &Realm) -> Result<Value> {
        let c = &realm.config;
        let model = self.hub.model_for(realm);
        let secrets: Vec<Value> = realm
            .secrets()?
            .into_iter()
            .map(|(k, _)| json!({"key": k, "provider": PROVIDER_KEYS.contains(&k.as_str())}))
            .collect();
        Ok(json!({
            "name": c.name,
            "description": c.description,
            "root": realm.root().map(|p| p.display().to_string()).unwrap_or_default(),
            "own_root": c.root.is_some(),
            "isolation": c.isolation.as_str(),
            "policy": c.policy,
            "checks": c.checks,
            "share_owner_profile": c.share_owner_profile,
            "archived": c.archived,
            "default": self.hub.home().default_realm().as_deref() == Some(realm.name()),
            "model": model.as_ref().ok().map(|m| m.name()),
            "model_error": model.err().map(|e| format!("{e:#}")),
            "memory": realm.memory().list()?,
            "secrets": secrets,
            "integrations": c.integrations.iter().map(|i| {
                let missing: Vec<String> = i
                    .secret_names()
                    .into_iter()
                    .filter(|k| realm.secret(k).is_none())
                    .collect();
                json!({
                    "name": i.name,
                    "description": i.description,
                    "base_url": i.base_url,
                    "headers": i.headers,
                    "missing_secrets": missing,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    fn create_realm(&self, body: &Value) -> Result<Realm> {
        let root = body["root"]
            .as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(expand_home);
        let isolation = match body["isolation"].as_str() {
            Some("none") => Some(Isolation::None),
            Some("worktree") => Some(Isolation::Worktree),
            _ => None,
        };
        let realm = self.hub.home().create_realm(NewRealm {
            name: body["name"].as_str().unwrap_or_default().trim().to_string(),
            description: body["description"].as_str().unwrap_or_default().to_string(),
            root,
            isolation,
        })?;
        if let Some(root) = &realm.config.root {
            self.hub.import_runs(&realm, root)?;
        }
        Ok(realm)
    }

    fn update_realm(&self, name: &str, body: &Value) -> Result<Realm> {
        let mut realm = self.realm(name)?;
        let c = &mut realm.config;
        if let Some(d) = body["description"].as_str() {
            c.description = d.trim().to_string();
        }
        if let Some(policy) = body["policy"].as_object() {
            for (cap, rule) in policy {
                c.policy.set(
                    Capability::parse(cap)?,
                    Rule::parse(rule.as_str().unwrap_or_default())?,
                );
            }
        }
        if let Some(checks) = body["checks"].as_array() {
            c.checks = checks
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
        }
        if let Some(share) = body["share_owner_profile"].as_bool() {
            c.share_owner_profile = share;
        }
        if let Some(archived) = body["archived"].as_bool() {
            c.archived = archived;
        }
        realm.save()?;
        if body["default"].as_bool() == Some(true) {
            self.hub.home().set_default_realm(name)?;
        }
        Ok(realm)
    }

    fn runs(&self, realm: &Realm) -> Result<Value> {
        let registry = self.registry.lock().unwrap();
        let runs: Vec<Value> = self
            .hub
            .runs(realm)?
            .into_iter()
            .rev()
            .map(|r| {
                let live = registry.get(&key(realm.name(), &r.id));
                let needs_approval = live.is_some_and(|l| l.pending.lock().unwrap().is_some());
                json!({
                    "id": r.id,
                    "objective": r.objective,
                    "status": r.display_status(),
                    "live": r.live || live.is_some(),
                    "iterations": r.iterations,
                    "updated_at": r.updated_at,
                    "needs_approval": needs_approval,
                    "change": r.change.map(|c| c.as_str()),
                    "checks_passed": r.checks_passed,
                    "plan_done": r.plan_done,
                    "plan_total": r.plan_total,
                })
            })
            .collect();
        Ok(json!(runs))
    }

    fn run(&self, realm: &Realm, id: &str, query: &str) -> Result<Value> {
        let since: Option<u64> = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("since="))
            .and_then(|v| v.parse().ok());
        let checks_required = !realm.config.checks.is_empty();
        if let Some(live) = self.live(realm.name(), id) {
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
                    "grantable": p.request.grantable(),
                    "grant_label": p.request.grant_label(),
                })
            });
            return Ok(json!({
                "version": version,
                "live": true,
                "stopping": live.cancel.is_cancelled(),
                "state": state,
                "pending": pending,
                "checks_required": checks_required,
            }));
        }
        let state = self.hub.load_run(realm, id)?;
        let live_elsewhere = self.hub.is_live(realm, id);
        Ok(json!({
            "version": 0,
            "live": live_elsewhere,
            "elsewhere": live_elsewhere,
            "interrupted": state.status == Status::Running && !live_elsewhere,
            "state": state,
            "pending": null,
            "checks_required": checks_required,
        }))
    }

    fn answer(&self, realm: &str, id: &str, body: &Value) -> Result<()> {
        let live = self
            .live(realm, id)
            .context("this run is not in progress")?;
        let mut pending = live.pending.lock().unwrap();
        let p = pending
            .take_if(|p| Some(p.id) == body["approval_id"].as_u64())
            .context("this approval is no longer pending")?;
        let answer = if body["approve"].as_bool() == Some(true) {
            if body["for_run"].as_bool() == Some(true) && p.request.grantable() {
                Answer::YesForRun
            } else {
                Answer::Yes
            }
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

    fn spawn(&self, realm: Realm, state: State, job: Job) {
        self.runner.spawn(realm, state, job);
    }

    fn skills(&self, realm: &Realm) -> Result<Value> {
        let list: Vec<Value> = realm
            .skills()
            .list()?
            .into_iter()
            .map(|r| {
                let developing = r.dev_run.as_deref().is_some_and(|run| {
                    self.live(realm.name(), run).is_some() || self.hub.is_live(realm, run)
                });
                let mut v = json!(r);
                v["developing_now"] = json!(developing);
                v["passed"] = json!(r.evaluation.as_ref().is_some_and(|e| e.passed()));
                v["summary"] = json!(r.evaluation.as_ref().map(|e| e.describe()));
                v
            })
            .collect();
        Ok(json!(list))
    }

    fn objectives(&self, realm: &Realm) -> Result<Value> {
        let list: Vec<Value> = realm
            .objectives()
            .list()?
            .into_iter()
            .map(|o| {
                let mut v = json!(o);
                if let Trigger::Webhook { token } = &o.trigger {
                    v["webhook_url"] = json!(format!(
                        "http://127.0.0.1:{}/hooks/{}/{}/{token}",
                        self.port,
                        realm.name(),
                        o.id
                    ));
                }
                v["trigger_label"] = json!(o.trigger.describe());
                v["running"] =
                    json!(o.last_run.as_deref().is_some_and(
                        |r| self.live(realm.name(), r).is_some() || self.hub.is_live(realm, r)
                    ));
                v
            })
            .collect();
        Ok(json!(list))
    }

    /// Create or replace an objective from the app's form. Its history (last
    /// run, schedule state, webhook token) is kept when it is replaced.
    fn save_objective(&self, realm: &Realm, body: &Value) -> Result<()> {
        let id = body["id"].as_str().unwrap_or_default().trim().to_string();
        let existing = realm.objectives().get(&id).ok();
        let trigger = match body["trigger"]["kind"].as_str().unwrap_or("manual") {
            "manual" => Trigger::Manual,
            "schedule" => Trigger::Schedule {
                every: body["trigger"]["every"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .into(),
            },
            "watch" => Trigger::Watch {
                path: body["trigger"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .into(),
            },
            "webhook" => Trigger::Webhook {
                token: match existing.as_ref().map(|o| &o.trigger) {
                    Some(Trigger::Webhook { token }) => token.clone(),
                    _ => random_token()?,
                },
            },
            other => return Err(anyhow!("unknown trigger '{other}'")),
        };
        let approvals = serde_json::from_value(body["approvals"].clone()).unwrap_or_default();
        let budget = serde_json::from_value(body["budget"].clone()).unwrap_or_default();
        let base = existing.filter(|o| o.trigger == trigger);
        realm.objectives().save(Objective {
            id,
            objective: body["objective"].as_str().unwrap_or_default().trim().into(),
            trigger,
            approvals,
            budget,
            approval_wait_minutes: body["approval_wait_minutes"]
                .as_u64()
                .unwrap_or(10)
                .min(24 * 60) as u32,
            enabled: body["enabled"].as_bool().unwrap_or(true),
            created_at: base
                .as_ref()
                .map_or_else(chrono::Utc::now, |o| o.created_at),
            next_at: None,
            fingerprint: None,
            last_run: base.as_ref().and_then(|o| o.last_run.clone()),
            last_at: base.as_ref().and_then(|o| o.last_at),
            last_status: base.and_then(|o| o.last_status),
        })
    }

    /// `POST /hooks/<realm>/<objective>/<token>`: start the objective's run
    /// with the request body as its (untrusted) data. Anything that does not
    /// match an enabled webhook objective gets the same 404.
    fn webhook(&self, request: &mut Request, hook: &str) -> HttpResponse {
        let not_found = || json_response(404, json!({"error": "not found"}));
        let parts: Vec<String> = hook.split('/').map(percent_decode).collect();
        let [realm, id, token] = parts.as_slice() else {
            return not_found();
        };
        let Ok(realm) = self.realm(realm) else {
            return not_found();
        };
        let Ok(objective) = realm.objectives().get(id) else {
            return not_found();
        };
        let valid = matches!(&objective.trigger, Trigger::Webhook { token: t } if constant_time_eq(t, token));
        if !valid || !objective.enabled || realm.config.archived {
            return not_found();
        }
        let mut body = Vec::new();
        let _ = request.as_reader().take(64_000).read_to_end(&mut body);
        let payload = String::from_utf8_lossy(&body).into_owned();
        match self
            .hub
            .start_objective(&realm, id, "webhook", Some(&payload), chrono::Utc::now())
        {
            Ok((state, o)) => {
                let run = state.id.clone();
                self.spawn(
                    realm,
                    state,
                    Job::Unattended {
                        objective: o.id,
                        wait_minutes: o.approval_wait_minutes,
                    },
                );
                json_response(202, json!({"run": run}))
            }
            Err(e) => json_response(409, json!({"error": format!("{e:#}")})),
        }
    }
}

impl Runner {
    fn spawn(&self, realm: Realm, state: State, job: Job) {
        let k = key(realm.name(), &state.id);
        let live = Arc::new(Live {
            snapshot: Mutex::new(state.clone()),
            pending: Mutex::new(None),
            cancel: CancelToken::default(),
            version: AtomicU64::new(1),
        });
        self.registry
            .lock()
            .unwrap()
            .insert(k.clone(), live.clone());
        let hub = self.hub.clone();
        let registry = self.registry.clone();
        let max_iterations = self.max_iterations;

        std::thread::spawn(move || {
            let mut state = state;
            let mut observer = UiObserver(live.clone());
            let away = match &job {
                Job::Unattended {
                    objective,
                    wait_minutes,
                } => Some(Away {
                    objective: objective.clone(),
                    realm: realm.name().to_string(),
                    run: state.id.clone(),
                    wait_minutes: *wait_minutes,
                    inbox: hub.home().inbox(),
                }),
                _ => None,
            };
            match job {
                Job::Drive | Job::Unattended { .. } => {
                    let hooks = RunHooks {
                        approver: Box::new(UiApprover {
                            live: live.clone(),
                            next_id: 0,
                            away,
                        }),
                        observer: &mut observer,
                        cancel: live.cancel.clone(),
                    };
                    if let Err(e) =
                        hub.drive(&realm, &mut state, AgentConfig::new(max_iterations), hooks)
                        && state.status == Status::Running
                    {
                        state.status = Status::Failed;
                        state.report = Some(format!("{e:#}"));
                        let _ = state.save();
                    }
                }
                Job::Check => match hub.check(&realm, &state.id, &mut observer, &live.cancel) {
                    Ok(checked) => state = checked,
                    Err(e) => eprintln!("checks of run {} failed to start: {e:#}", state.id),
                },
            }
            *live.snapshot.lock().unwrap() = state;
            live.bump();
            registry.lock().unwrap().remove(&k);
        });
    }
}

/// Keep huge diffs (lockfiles, generated code) from freezing the page.
fn truncate_diff(diff: String) -> String {
    const MAX: usize = 400_000;
    if diff.len() <= MAX {
        return diff;
    }
    let mut cut = MAX;
    while !diff.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n… diff truncated ({} bytes total); see `airth review`",
        &diff[..cut],
        diff.len()
    )
}

fn expand_home(path: &str) -> std::path::PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => std::path::PathBuf::from(home).join(rest),
        _ => std::path::PathBuf::from(path),
    }
}

/// Decode %XX escapes in a path segment.
fn percent_decode(s: &str) -> String {
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

fn json_response(status: u16, value: Value) -> HttpResponse {
    Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(content_type("application/json"))
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
}

fn static_response(body: &str, kind: &str) -> HttpResponse {
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

    #[test]
    fn decodes_path_segments() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%é"), "%é");
    }
}
