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

const INDEX_HTML: &str = include_str!("index.html");
const APP_JS: &str = include_str!("app.js");
const APP_CSS: &str = include_str!("app.css");
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
    /// Run the checks, and adopt the change if they pass.
    CheckThenAdopt,
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

/// Where the self-update stands, as shown in the app.
#[derive(Default, serde::Serialize)]
struct UpdateState {
    /// The newer version, if there is one.
    available: Option<crate::update::Available>,
    /// idle, preparing, ready, failed or restarting.
    phase: &'static str,
    /// What preparing is doing now (building, testing...).
    step: Option<String>,
    error: Option<String>,
    /// The built and tested binary, for the version in `available`.
    #[serde(skip)]
    staged: Option<std::path::PathBuf>,
    #[serde(skip)]
    prepared_commit: Option<String>,
}

/// Every minute, look for a newer version (asking the upstream every 30
/// minutes) and prepare it: build and test it, ready to install on restart.
fn run_updater(hub: Arc<Hub>, state: Arc<Mutex<UpdateState>>) {
    let work = hub.home().dir().join("update");
    let mut ticks: u64 = 0;
    loop {
        let fetch = ticks.is_multiple_of(30) || state.lock().unwrap().phase == "checking";
        ticks += 1;
        match crate::update::check(fetch) {
            Ok(found) => {
                let todo = {
                    let mut st = state.lock().unwrap();
                    if st.phase == "checking" {
                        st.phase = "idle";
                    }
                    let todo = match &found {
                        Some(a)
                            if st.prepared_commit.as_deref() != Some(a.commit.as_str())
                                && st.phase != "restarting" =>
                        {
                            Some(a.clone())
                        }
                        _ => None,
                    };
                    if found.is_none() && st.phase != "restarting" {
                        *st = UpdateState {
                            phase: "idle",
                            ..Default::default()
                        };
                    }
                    st.available = found;
                    todo
                };
                if let Some(available) = todo {
                    {
                        let mut st = state.lock().unwrap();
                        st.phase = "preparing";
                        st.error = None;
                        st.prepared_commit = Some(available.commit.clone());
                    }
                    let step_state = state.clone();
                    let result = crate::update::prepare(&available, &work, &|step| {
                        step_state.lock().unwrap().step = Some(step.to_string());
                    });
                    let mut st = state.lock().unwrap();
                    st.step = None;
                    match result {
                        Ok(staged) => {
                            eprintln!("update {} is ready: restart to install it", available.short);
                            st.phase = "ready";
                            st.staged = Some(staged);
                        }
                        Err(e) => {
                            eprintln!("update {} failed: {e:#}", available.short);
                            st.phase = "failed";
                            st.error = Some(format!("{e:#}"));
                        }
                    }
                }
            }
            Err(e) => {
                let mut st = state.lock().unwrap();
                if st.phase == "checking" {
                    st.phase = "idle";
                    st.error = Some(format!("{e:#}"));
                }
            }
        }
        for _ in 0..60 {
            if state.lock().unwrap().phase == "checking" {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// The app's access key, kept in the home so links keep working across
/// restarts (and updates).
fn persistent_token(hub: &Hub) -> Result<String> {
    let path = hub.home().dir().join("ui-token");
    if let Ok(t) = std::fs::read_to_string(&path)
        && t.trim().len() >= 32
        && t.trim().bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(t.trim().to_string());
    }
    let token = random_token()?;
    crate::realm::write_private(&path, &token)?;
    Ok(token)
}

/// Stop the runs, then come back as the new binary: launchd restarts the
/// service when it exits; otherwise the process replaces itself.
fn restart(registry: Registry, info_path: std::path::PathBuf, exe: std::path::PathBuf, port: u16) {
    let lives: Vec<Arc<Live>> = registry.lock().unwrap().values().cloned().collect();
    for live in &lives {
        live.cancel.cancel();
    }
    for _ in 0..100 {
        if registry.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(&info_path);
    if std::env::var("AIRTH_SERVICE").as_deref() == Ok("1") {
        std::process::exit(0);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut args: Vec<String> = std::env::args().skip(1).collect();
        // Started as Airth.app (no arguments): come back as the app.
        if args.is_empty() && exe.to_string_lossy().contains(".app/Contents/MacOS/") {
            let err = std::process::Command::new(&exe).exec();
            eprintln!("restart failed: {err}");
            std::process::exit(1);
        }
        // Same port (so the page reconnects), and no new browser tab.
        if let Some(i) = args.iter().position(|a| a == "--port") {
            args.drain(i..(i + 2).min(args.len()));
        }
        args.retain(|a| !a.starts_with("--port="));
        args.push("--port".into());
        args.push(port.to_string());
        if args.iter().any(|a| a == "ui") && !args.iter().any(|a| a == "--no-open") {
            args.push("--no-open".into());
        }
        let err = std::process::Command::new(&exe).args(&args).exec();
        eprintln!("restart failed: {err}");
    }
    std::process::exit(1);
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

/// Open `url` and wait for the opener, so a command that exits right after
/// (as `airth ui` does when Airth is already running) cannot cut it off.
pub fn open_url(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "/usr/bin/open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
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

/// Every two minutes, follow the open pull requests of realms that deliver
/// changes that way: record them merged or closed, keep their branches
/// current with the base, and have a chat resolve a conflict.
fn follow_pull_requests(runner: Runner) {
    loop {
        std::thread::sleep(Duration::from_secs(120));
        let Ok(realms) = runner.hub.home().realms() else {
            continue;
        };
        for realm in realms {
            if realm.config.delivery != Delivery::PullRequest || realm.config.archived {
                continue;
            }
            let Ok(runs) = runner.hub.runs(&realm) else {
                continue;
            };
            for run in runs.iter().filter(|r| r.pull_request.is_some() && !r.live) {
                if runner.is_running(realm.name(), &run.id) {
                    continue;
                }
                match runner.hub.follow_pull_request(&realm, &run.id) {
                    Ok(PrEvent::Conflict) => match runner.hub.resolve(&realm, &run.id) {
                        Ok(Some(state)) => {
                            eprintln!("run {} resolves a conflict with its base", run.id);
                            runner.spawn(realm.clone(), state, Job::Drive);
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("run {} cannot resolve its conflict: {e:#}", run.id),
                    },
                    Ok(_) => {}
                    Err(e) => eprintln!("pull request of run {}: {e:#}", run.id),
                }
            }
        }
    }
}

type HttpResponse = Response<std::io::Cursor<Vec<u8>>>;

pub fn serve(hub: Hub, options: UiOptions) -> Result<()> {
    let server = start(hub, options)?;
    eprintln!("\x1b[1mairth\x1b[0m → {}", server.url);
    eprintln!(
        "\x1b[2mThis link grants control of the agent; don't share it. Objectives run while this is running. Ctrl-C to quit.\x1b[0m"
    );
    let registry = server.registry.clone();
    let path = server.info_path.clone();
    ctrlc::set_handler(move || shut_down(&registry, &path))
        .context("cannot install Ctrl-C handler")?;
    #[cfg(unix)]
    {
        // launchd and `kill` stop services with SIGTERM: shut down the same way.
        let _ =
            unsafe { libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t) };
    }
    let _ = server.thread.join();
    Ok(())
}

/// A running app server: requests are served on their own thread.
pub struct Running {
    pub url: String,
    registry: Registry,
    info_path: std::path::PathBuf,
    thread: std::thread::JoinHandle<()>,
}

impl Running {
    /// Stop the runs in progress (they can be resumed) and forget the server.
    pub fn stop(&self) {
        let lives: Vec<Arc<Live>> = self.registry.lock().unwrap().values().cloned().collect();
        for live in &lives {
            live.cancel.cancel();
        }
        for _ in 0..50 {
            if self.registry.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = std::fs::remove_file(&self.info_path);
    }
}

/// Start the app server, the scheduler and the updater, without blocking.
pub fn start(hub: Hub, options: UiOptions) -> Result<Running> {
    let server = Server::http(("127.0.0.1", options.port))
        .or_else(|_| Server::http(("127.0.0.1", 0)))
        .map_err(|e| anyhow!("cannot start the UI server: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .context("server has no IP address")?
        .port();
    let hub = Arc::new(hub);
    let token_hub = hub.clone();
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
        token: persistent_token(&token_hub)?,
        port,
        options,
        update: Arc::new(Mutex::new(UpdateState {
            phase: "idle",
            ..Default::default()
        })),
        writes: Mutex::new(()),
        previews: Mutex::new(HashMap::new()),
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
    if app.options.open_browser {
        open_in_browser(&url);
    }

    let runner = app.runner.clone();
    std::thread::spawn(move || run_scheduler(runner));
    let runner = app.runner.clone();
    std::thread::spawn(move || follow_pull_requests(runner));
    let (hub, update) = (app.hub.clone(), app.update.clone());
    std::thread::spawn(move || run_updater(hub, update));

    let registry = app.registry.clone();
    let thread = std::thread::spawn(move || serve_requests(server, app));
    Ok(Running {
        url,
        registry,
        info_path,
        thread,
    })
}

/// Serve the app: one thread per request, so a slow provider (sign-in, model
/// lists, registry search) never freezes the app, nor an approval.
fn serve_requests(server: Server, app: App) {
    let app = Arc::new(app);
    for request in server.incoming_requests() {
        let app = Arc::clone(&app);
        std::thread::spawn(move || app.handle(request));
    }
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
        let ok = || json_response(200, json!({"ok": true}));
        Ok(match (&method, segments.as_slice()) {
            (Method::Get, ["info"]) => json_response(200, self.info()),
            (Method::Get, ["setup"]) => json_response(200, self.setup()?),
            (Method::Post, ["setup", "key"]) => {
                let body = read_json(request)?;
                self.hub
                    .set_shared_anthropic_key(body["key"].as_str().unwrap_or_default())?;
                json_response(200, self.setup()?)
            }
            (Method::Get, ["owner"]) => json_response(200, self.owner()?),
            (Method::Put, ["owner"]) => {
                let owner: Owner =
                    serde_json::from_value(read_json(request)?).context("invalid profile")?;
                self.hub.home().save_owner(&owner)?;
                json_response(200, self.owner()?)
            }
            // Pictures of the owner and of each realm, for the app.
            (Method::Get, ["owner", "avatar"]) => image_response(self.hub.home().dir(), "avatar")?,
            (Method::Put, ["owner", "avatar"]) => {
                images::save(self.hub.home().dir(), "avatar", &read_bytes(request)?)?;
                json_response(200, self.owner()?)
            }
            (Method::Delete, ["owner", "avatar"]) => {
                images::remove(self.hub.home().dir(), "avatar")?;
                json_response(200, self.owner()?)
            }
            (Method::Get, ["realms", r, "image"]) => image_response(self.realm(r)?.dir(), "image")?,
            (Method::Put, ["realms", r, "image"]) => {
                let realm = self.realm(r)?;
                images::save(realm.dir(), "image", &read_bytes(request)?)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "image"]) => {
                let realm = self.realm(r)?;
                images::remove(realm.dir(), "image")?;
                json_response(200, self.realm_detail(&realm)?)
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
            (Method::Get, ["realms", r, "files"]) => {
                json_response(200, self.files(&self.realm(r)?, &param(query, "path"))?)
            }
            (Method::Get, ["realms", r, "files", "raw"]) => {
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, &param(query, "path"))?;
                let size = std::fs::metadata(&path)?.len();
                if !path.is_file() || size > crate::documents::MAX_FILE as u64 {
                    return Ok(json_response(413, json!({"error": "too large to preview"})));
                }
                Response::from_data(std::fs::read(&path)?)
                    .with_header(content_type(crate::documents::mime(&path)))
                    .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
                    .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
                    .with_header(Header::from_bytes("Content-Security-Policy", "sandbox").unwrap())
            }
            (Method::Get, ["realms", r, "files", "text"]) => {
                let root = self.realm(r)?.root()?;
                json_response(
                    200,
                    json!(crate::documents::read_text(&root, &param(query, "path"))?),
                )
            }
            // Cheap enough to ask every few seconds: has the open file changed?
            (Method::Get, ["realms", r, "files", "version"]) => {
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, &param(query, "path"))?;
                json_response(200, json!({ "version": crate::documents::version(&path)? }))
            }
            (Method::Put, ["realms", r, "files", "text"]) => {
                // JSON escapes can make the text up to six times longer.
                let bytes = read_body(
                    request,
                    6 * crate::documents::MAX_TEXT as usize + 4096,
                    "the text",
                )?;
                let body: Value =
                    serde_json::from_slice(&bytes).context("request body must be JSON")?;
                let root = self.realm(r)?.root()?;
                let str_of = |k: &str| body[k].as_str().unwrap_or_default();
                match crate::documents::save_text(
                    &root,
                    str_of("path"),
                    str_of("text"),
                    str_of("version"),
                ) {
                    Ok(version) => json_response(200, json!({ "version": version })),
                    Err(crate::documents::SaveError::Changed(version)) => json_response(
                        409,
                        json!({ "error": "the file changed since you opened it", "changed": true, "version": version }),
                    ),
                    Err(crate::documents::SaveError::Other(e)) => return Err(e),
                }
            }
            (Method::Post, ["realms", r, "files", "new"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                let path = crate::documents::create(
                    &root,
                    body["dir"].as_str().unwrap_or(""),
                    body["name"].as_str().unwrap_or(""),
                    body["folder"].as_bool() == Some(true),
                )?;
                json_response(200, json!({ "path": path }))
            }
            (Method::Get, ["realms", r, "files", "all"]) => {
                const LIMIT: usize = 5000;
                let root = self.realm(r)?.root()?;
                let files = crate::documents::all_files(&root, LIMIT);
                json_response(
                    200,
                    json!({ "truncated": files.len() >= LIMIT, "files": files }),
                )
            }
            (Method::Post, ["realms", r, "files", "preview"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, body["path"].as_str().unwrap_or(""))?;
                if crate::documents::mime(&path) != "application/pdf" || !path.is_file() {
                    return Err(anyhow!("only PDF files open this way"));
                }
                let ticket = random_token()?;
                self.previews
                    .lock()
                    .unwrap()
                    .insert(ticket.clone(), (path, std::time::Instant::now()));
                json_response(200, json!({ "url": format!("/preview/{ticket}") }))
            }
            (Method::Put, ["realms", r, "files", "raw"]) => {
                let root = self.realm(r)?.root()?;
                let bytes = read_body(request, crate::documents::MAX_FILE, "the file")?;
                let path = crate::documents::add(
                    &root,
                    &param(query, "dir"),
                    &param(query, "name"),
                    &bytes,
                )?;
                json_response(200, json!({ "path": path }))
            }
            (Method::Post, ["realms", r, "files", "open"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, body["path"].as_str().unwrap_or(""))?;
                // With the Mac's default app, or shown in the Finder.
                let mut open = std::process::Command::new("/usr/bin/open");
                if body["reveal"].as_bool() == Some(true) {
                    open.arg("-R");
                }
                let status = open.arg(&path).status().context("cannot run open")?;
                if !status.success() {
                    return Err(anyhow!("macOS could not open it"));
                }
                ok()
            }
            (Method::Post, ["realms", r, "files", "trash"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                crate::documents::trash(&root, body["path"].as_str().unwrap_or(""))?;
                ok()
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
            (Method::Put, ["realms", r, "northstar"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                realm.north_star().set_goal(
                    body["goal"].as_str().unwrap_or_default(),
                    body["why"].as_str().unwrap_or_default(),
                )?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "northstar"]) => {
                let realm = self.realm(r)?;
                realm.north_star().clear()?;
                self.hub.schedule_north_star(&realm, None)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Patch, ["realms", r, "northstar", "milestones", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let status = body["status"]
                    .as_str()
                    .map(crate::northstar::Status::parse)
                    .transpose()?;
                realm.north_star().update(id, status, None, None)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Post, ["realms", r, "northstar", "schedule"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.hub
                    .schedule_north_star(&realm, body["every"].as_str())?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Post, ["realms", r, "northstar", action @ ("brainstorm" | "push")]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                let state = if *action == "brainstorm" {
                    self.hub.brainstorm_north_star(&realm)?
                } else {
                    self.hub.push_north_star(&realm)?
                };
                let id = state.id.clone();
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": id}))
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
                let choice = self.model_choice(&realm, &body)?;
                let mut state = self
                    .hub
                    .create_run(&realm, body["objective"].as_str().unwrap_or_default())?;
                if choice.is_some() {
                    state.model_choice = choice;
                    state.save()?;
                }
                let id = state.id.clone();
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": id}))
            }
            (Method::Post, ["realms", r, "models"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
                self.hub.set_model_tier(
                    &realm,
                    &text("tier"),
                    &text("provider"),
                    &text("model"),
                    &text("effort"),
                    body["all_realms"].as_bool().unwrap_or(false),
                )?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r, "models"]) => {
                let realm = self.realm(r)?;
                let force = query.split('&').any(|kv| kv == "refresh=1");
                json_response(200, self.model_options(&realm, force)?)
            }
            (Method::Get, ["realms", r, "runs", id]) => {
                json_response(200, self.run(&self.realm(r)?, id, query)?)
            }
            (Method::Patch, ["realms", r, "runs", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.hub.load_run(&realm, id)?;
                realm.set_run_title(id, body["title"].as_str().unwrap_or_default())?;
                json_response(200, json!({"title": realm.run_titles().get(*id)}))
            }
            (Method::Post, ["realms", r, "runs", id, "archive"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.ensure_idle(r, id)?;
                self.hub.load_run(&realm, id)?;
                let archived = body["archived"].as_bool().unwrap_or(true);
                realm.set_run_archived(id, archived)?;
                json_response(200, json!({"archived": archived}))
            }
            (Method::Delete, ["realms", r, "runs", id]) => {
                self.ensure_idle(r, id)?;
                self.hub.delete_run(&self.realm(r)?, id)?;
                ok()
            }
            (Method::Get, ["realms", r, "runs", id, "diff"]) => {
                let diff = self.hub.diff(&self.realm(r)?, id)?;
                json_response(200, json!({"diff": truncate_diff(diff)}))
            }
            (Method::Post, ["realms", r, "runs", id, "messages"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let choice = self.model_choice(&realm, &body)?;
                self.ensure_idle(r, id)?;
                let mut state =
                    self.hub
                        .follow_up(&realm, id, body["message"].as_str().unwrap_or_default())?;
                // The picker always sends its value: "auto" goes back to the realm's routing.
                if body.get("model").is_some() {
                    state.model_choice = choice;
                    state.save()?;
                }
                self.spawn(realm, state, Job::Drive);
                ok()
            }
            (Method::Post, ["update", "check"]) => {
                let mut st = self.update.lock().unwrap();
                if st.phase == "idle" || st.phase == "failed" {
                    st.phase = "checking";
                    st.prepared_commit = None;
                }
                ok()
            }
            (Method::Post, ["update", "restart"]) => {
                let staged = {
                    let mut st = self.update.lock().unwrap();
                    if st.phase != "ready" {
                        return Err(anyhow!("no update is ready to install"));
                    }
                    let staged = st.staged.clone().context("no update is ready")?;
                    st.phase = "restarting";
                    staged
                };
                let exe = match crate::update::install(&staged) {
                    Ok(exe) => exe,
                    Err(e) => {
                        let mut st = self.update.lock().unwrap();
                        st.phase = "failed";
                        st.error = Some(format!("installing failed: {e:#}"));
                        return Err(e);
                    }
                };
                eprintln!("installed the update; restarting");
                let (registry, path, port) =
                    (self.registry.clone(), server_file(&self.hub), self.port);
                std::thread::spawn(move || restart(registry, path, exe, port));
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
            (Method::Get, ["usage"]) => {
                let days: u32 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("days="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(30);
                let report = crate::usage::report(&self.hub, days, chrono::Local::now())?;
                json_response(200, json!(report))
            }
            (Method::Get, ["catalog"]) => {
                let force = query.split('&').any(|kv| kv == "refresh=1");
                let (index, refreshing) =
                    catalog::index(&self.hub.home().dir().join("catalog.json"), force);
                let skills: Vec<Value> = index
                    .skills
                    .iter()
                    .map(|s| {
                        let (take, skip) = s.installable();
                        json!({
                            "name": s.name,
                            "title": s.title,
                            "description": s.description,
                            "files": take.len(),
                            "skipped": skip.len(),
                        })
                    })
                    .collect();
                json_response(
                    200,
                    json!({
                        "source": format!("https://github.com/{}", catalog::REPO),
                        "fetched_at": index.fetched_at,
                        "error": index.error,
                        "refreshing": refreshing,
                        "skills": skills,
                    }),
                )
            }
            (Method::Get, ["realms", r, "catalog"]) => {
                json_response(200, self.catalog_installed(&self.realm(r)?))
            }
            (Method::Post, ["realms", r, "catalog", name, "install"]) => {
                let realm = self.realm(r)?;
                let (index, _) = catalog::index(&self.hub.home().dir().join("catalog.json"), false);
                let skill = index
                    .skills
                    .into_iter()
                    .find(|s| s.name == *name)
                    .context("this skill is not in the catalog (refresh it?)")?;
                catalog::install_in_background(skill, realm.skills().dir().to_path_buf());
                json_response(200, self.catalog_installed(&realm))
            }
            (Method::Delete, ["realms", r, "catalog", name]) => {
                let realm = self.realm(r)?;
                if !catalog::uninstall(realm.skills().dir(), name)? {
                    return Err(anyhow!("this skill is not installed"));
                }
                json_response(200, self.catalog_installed(&realm))
            }
            (Method::Get, ["realms", r, "connectors"]) => {
                json_response(200, self.connectors(&self.realm(r)?, None))
            }
            (Method::Post, ["realms", r, "connectors", id]) => {
                let body = read_json(request)?;
                let c = connectors::find(id)?;
                let realm = {
                    let _w = self.write_lock();
                    let mut realm = self.realm(r)?;
                    connectors::connect(
                        &mut realm,
                        &c,
                        body["token"].as_str().unwrap_or_default(),
                    )?;
                    realm
                };
                // Prove it works right away (no lock held while waiting on the
                // service); a refused token stays saved, to fix or remove.
                let check = connectors::check(&realm, &c);
                json_response(200, self.connectors(&realm, Some((id, check))))
            }
            (Method::Post, ["realms", r, "connectors", id, "check"]) => {
                let realm = self.realm(r)?;
                let check = connectors::check(&realm, &connectors::find(id)?);
                json_response(200, self.connectors(&realm, Some((id, check))))
            }
            (Method::Delete, ["realms", r, "connectors", id]) => {
                let _w = self.write_lock();
                let mut realm = self.realm(r)?;
                if !connectors::disconnect(&mut realm, &connectors::find(id)?)? {
                    return Err(anyhow!("this connector is not connected"));
                }
                json_response(200, self.connectors(&realm, None))
            }
            (Method::Post, ["realms", r, "connect"]) => {
                // A remote MCP connector: save it, then open its sign-in page.
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let dir = realm.skills().dir().to_path_buf();
                let name = body["name"].as_str().unwrap_or_default().trim().to_string();
                {
                    let _w = self.write_lock();
                    McpStore::new(&dir).save_remote(
                        &name,
                        body["url"].as_str().unwrap_or_default(),
                        body["description"].as_str().unwrap_or_default(),
                    )?;
                }
                let redirect = format!("http://127.0.0.1:{}/oauth/callback", self.port);
                match crate::tools::remote::start(&dir, &name, &redirect) {
                    Ok(crate::tools::remote::Started::SignIn(url)) => {
                        let open = url.clone();
                        std::thread::spawn(move || open_url(&open));
                        json_response(200, json!({"sign_in": url}))
                    }
                    Ok(crate::tools::remote::Started::Open) => {
                        self.mcp_connect(&realm, &name)?;
                        json_response(200, json!({"open": true}))
                    }
                    Err(e) => {
                        let message = format!("{e:#}");
                        let _w = self.write_lock();
                        McpStore::new(&dir).update(&name, |s| s.error = Some(message.clone()))?;
                        return Err(e);
                    }
                }
            }
            (Method::Get, ["registry"]) => {
                let search = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("search="))
                    .map(percent_decode)
                    .unwrap_or_default();
                json_response(200, json!(registry_search(&search)?))
            }
            (Method::Get, ["realms", r, "mcp"]) => json_response(200, self.mcp(&self.realm(r)?)?),
            (Method::Post, ["realms", r, "mcp"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let name = body["name"].as_str().unwrap_or_default().trim();
                McpStore::new(realm.skills().dir()).save(
                    name,
                    body["command"].as_str().unwrap_or_default(),
                    body["description"].as_str().unwrap_or_default(),
                )?;
                self.mcp_connect(&realm, name)?;
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Post, ["realms", r, "mcp", name, "connect"]) => {
                let realm = self.realm(r)?;
                self.mcp_connect(&realm, name)?;
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Patch, ["realms", r, "mcp", name]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                if let Some(enabled) = body["enabled"].as_bool() {
                    McpStore::new(realm.skills().dir()).update(name, |s| s.enabled = enabled)?;
                }
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Delete, ["realms", r, "mcp", name]) => {
                let realm = self.realm(r)?;
                if !McpStore::new(realm.skills().dir()).remove(name)? {
                    return Err(anyhow!("no such MCP server"));
                }
                crate::tools::remote::forget_auth(realm.skills().dir(), name);
                json_response(200, self.mcp(&realm)?)
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
                let realm = self.realm(r)?;
                match self.hub.prepare_adoption(&realm, id)? {
                    Adoption::Ready => {
                        self.hub.adopt(&realm, id)?;
                        json_response(200, json!({"adopted": true}))
                    }
                    Adoption::NeedsChecks => {
                        let state = self.hub.load_run(&realm, id)?;
                        self.spawn(realm, state, Job::CheckThenAdopt);
                        json_response(200, json!({"checking": true}))
                    }
                    Adoption::Conflict(files) => json_response(200, json!({"conflicts": files})),
                }
            }
            (Method::Post, ["realms", r, "runs", id, "resolve"]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                self.ensure_idle(r, id)?;
                match self.hub.resolve(&realm, id)? {
                    Some(state) => {
                        self.spawn(realm, state, Job::Drive);
                        json_response(200, json!({"resolving": true}))
                    }
                    None => {
                        let state = self.hub.load_run(&realm, id)?;
                        if !realm.config.checks.is_empty() {
                            self.spawn(realm, state, Job::Check);
                        }
                        json_response(200, json!({"resolving": false}))
                    }
                }
            }
            (Method::Get, ["realms", r, "runs", id, "pull-request"]) => {
                let status = self.hub.pull_request_status(&self.realm(r)?, id)?;
                json_response(200, json!({"status": status}))
            }
            (Method::Post, ["realms", r, "runs", id, "merge"]) => {
                self.ensure_idle(r, id)?;
                self.hub.merge_pull_request(&self.realm(r)?, id)?;
                json_response(200, json!({"merged": true}))
            }
            (Method::Post, ["realms", r, "runs", id, "pull-request"]) => {
                self.ensure_idle(r, id)?;
                let realm = self.realm(r)?;
                let mut state = self.hub.load_run(&realm, id)?;
                self.hub.deliver(&realm, &mut state, true)?;
                let pr = state.change.and_then(|c| c.pull_request);
                json_response(200, json!({"pull_request": pr}))
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

    /// What is set up already, for the welcome guide.
    fn setup(&self) -> Result<Value> {
        let realms: Vec<_> = self
            .hub
            .home()
            .realms()?
            .into_iter()
            .filter(|r| !r.config.archived)
            .collect();
        let model_ready =
            self.hub.shared_model_ready() || realms.iter().any(|r| self.hub.model_for(r).is_ok());
        Ok(json!({ "realms": realms.len(), "model_ready": model_ready }))
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
                    "image": images::version(r.dir(), "image"),
                    "live": lives.len(),
                    "needs_approval": lives.iter().filter(|l| l.pending.lock().unwrap().is_some()).count(),
                })
            })
            .collect();
        Ok(json!(realms))
    }

    /// The owner profile, and the version of their picture (None: none).
    fn owner(&self) -> Result<Value> {
        let mut owner = json!(self.hub.home().owner()?);
        owner["avatar"] = json!(images::version(self.hub.home().dir(), "avatar"));
        Ok(owner)
    }

    /// A folder of the realm's documents; at the top, also what runs made.
    fn files(&self, realm: &Realm, rel: &str) -> Result<Value> {
        let root = realm.root()?;
        let dir = crate::documents::resolve(&root, rel)?;
        if !dir.is_dir() {
            return Err(anyhow!("{rel} is not a folder"));
        }
        let made = if dir == root {
            let titles = realm.run_titles();
            let runs = self.hub.all_runs(realm)?;
            crate::documents::made_by_runs(&root, &runs, 8)
                .into_iter()
                .map(|m| {
                    let run = runs.iter().find(|r| r.id == m.run);
                    json!({
                        "path": m.path,
                        "run": m.run,
                        "run_title": titles.get(&m.run).cloned().or_else(|| run.map(|r| r.objective.lines().next().unwrap_or("").to_string())),
                        "at": m.at,
                        "edited": m.edited,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(json!({
            "root": root.display().to_string(),
            "own_root": realm.config.root.is_some(),
            "path": crate::documents::relative(&root, &dir),
            "entries": crate::documents::list(&dir)?,
            "made": made,
        }))
    }

    fn realm_detail(&self, realm: &Realm) -> Result<Value> {
        let c = &realm.config;
        let model = self.hub.connections_for(realm);
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
            "delivery": c.delivery.as_str(),
            "github": (c.isolation == Isolation::Worktree)
                .then(|| realm.root().ok().and_then(|r| crate::github::Remote::of(&r).ok()))
                .flatten()
                .map(|r| r.slug()),
            "github_token": realm.secret(crate::github::TOKEN).is_some(),
            "policy": c.policy,
            "checks": c.checks,
            "share_owner_profile": c.share_owner_profile,
            "archived": c.archived,
            "default": self.hub.home().default_realm().as_deref() == Some(realm.name()),
            "image": images::version(realm.dir(), "image"),
            "model": model.as_ref().ok().map(|m| m.describe()),
            "model_error": model.err().map(|e| format!("{e:#}")),
            "models": self.hub.model_settings(realm)?,
            "memory": realm.memory().list()?,
            "north_star": self.north_star(realm)?,
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

    /// The realm's North Star for the app: the goal and milestones, progress,
    /// the milestone to work on, and the schedule that advances it.
    fn north_star(&self, realm: &Realm) -> Result<Value> {
        let Some(star) = realm.north_star().get()? else {
            return Ok(Value::Null);
        };
        let (done, total) = star.progress();
        let objective = realm.objectives().get(crate::northstar::OBJECTIVE_ID).ok();
        let mut v = json!(star);
        v["done"] = json!(done);
        v["total"] = json!(total);
        v["next"] = json!(star.next().map(|m| &m.id));
        v["schedule"] = match objective {
            Some(o) => json!({
                "every": match &o.trigger {
                    Trigger::Schedule { every } => every.clone(),
                    _ => String::new(),
                },
                "enabled": o.enabled,
                "next_at": o.next_at,
                "last_run": o.last_run,
                "last_status": o.last_status,
                "running": o.last_run.as_deref().is_some_and(
                    |r| self.live(realm.name(), r).is_some() || self.hub.is_live(realm, r)
                ),
            }),
            None => Value::Null,
        };
        Ok(v)
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
        if let Some(delivery) = body["delivery"].as_str() {
            let delivery = Delivery::parse(delivery)?;
            if delivery == Delivery::PullRequest {
                if c.isolation != Isolation::Worktree {
                    bail!("pull requests need a realm whose chats work in their own worktree");
                }
                crate::github::Remote::of(&realm.root()?)?;
            }
            realm.config.delivery = delivery;
        }
        realm.save()?;
        if body["default"].as_bool() == Some(true) {
            self.hub.home().set_default_realm(name)?;
        }
        Ok(realm)
    }

    fn runs(&self, realm: &Realm) -> Result<Value> {
        let registry = self.registry.lock().unwrap();
        let titles = realm.run_titles();
        let archived = realm.archived_runs();
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
                    "title": titles.get(&r.id),
                    "archived": archived.contains(&r.id),
                    "objective": r.objective,
                    "status": r.display_status(),
                    "live": r.live || live.is_some(),
                    "iterations": r.iterations,
                    "updated_at": r.updated_at,
                    "needs_approval": needs_approval,
                    "change": r.change.map(|c| c.as_str()),
                    "checks_passed": r.checks_passed,
                    "touches": r.touches,
                    "conflicts": r.conflicts,
                    "pull_request": r.pull_request,
                    "plan_done": r.plan_done,
                    "plan_total": r.plan_total,
                    "focus": r.focus,
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
                "title": realm.run_titles().get(id),
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
            "title": realm.run_titles().get(id),
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

    /// The model picked in the app for a request (`provider/model`), checked
    /// now so a missing key is reported before the run starts. "auto" or
    /// nothing: the realm's own model and routing.
    fn model_choice(&self, realm: &Realm, body: &Value) -> Result<Option<String>> {
        match body["model"]
            .as_str()
            .map(str::trim)
            .filter(|m| !m.is_empty() && *m != "auto")
        {
            Some(choice) => {
                self.hub.chosen_model(realm, choice)?;
                Ok(Some(choice.to_string()))
            }
            None => {
                self.hub.model_for(realm).context("no model available")?;
                Ok(None)
            }
        }
    }

    /// Providers and models for the picker, cached ten minutes per realm (the
    /// lists come from each provider's API).
    fn model_options(&self, realm: &Realm, force: bool) -> Result<Value> {
        static CACHE: std::sync::OnceLock<Mutex<HashMap<String, (std::time::Instant, Value)>>> =
            std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if !force
            && let Some((at, v)) = cache.lock().unwrap().get(realm.name())
            && at.elapsed() < Duration::from_secs(600)
        {
            return Ok(v.clone());
        }
        let value = json!({
            "providers": self.hub.model_options(realm)?,
            "default": self.hub.model_for(realm).ok().map(|m| m.name()),
        });
        cache.lock().unwrap().insert(
            realm.name().to_string(),
            (std::time::Instant::now(), value.clone()),
        );
        Ok(value)
    }

    /// The connectors catalog with each one's state in the realm, and the
    /// result of a connection check when one just ran.
    fn connectors(&self, realm: &Realm, check: Option<(&str, Result<Option<String>>)>) -> Value {
        let list: Vec<Value> = connectors::catalog()
            .iter()
            .map(|c| connectors::status(realm, c))
            .collect();
        let check = check.map(|(id, r)| match r {
            Ok(message) => json!({"id": id, "ok": true, "message": message}),
            Err(e) => json!({"id": id, "ok": false, "message": format!("{e:#}")}),
        });
        json!({
            "connectors": list,
            "check": check,
            "apps_rule": realm.config.policy.rule(Capability::Apps).as_str(),
        })
    }

    /// Catalog skills installed in the realm, those installing, and failures.
    fn catalog_installed(&self, realm: &Realm) -> Value {
        let store = realm.skills();
        let dir = store.dir();
        let (installing, errors) = catalog::install_status(dir);
        json!({
            "installed": catalog::installed(dir),
            "installing": installing,
            "errors": errors.into_iter().collect::<HashMap<_, _>>(),
        })
    }

    /// The realm's MCP servers, with the names the agent sees for their tools.
    fn mcp(&self, realm: &Realm) -> Result<Value> {
        let servers: Vec<Value> = McpStore::new(realm.skills().dir())
            .list()?
            .into_iter()
            .map(|s| {
                let tools: Vec<Value> = s
                    .tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "agent_name": crate::tools::mcp::tool_name(&s.name, &t.name),
                            "description": t.description,
                        })
                    })
                    .collect();
                json!({
                    "name": s.name,
                    "command": s.command,
                    "url": s.url,
                    "signed_in": crate::tools::remote::load_auth(realm.skills().dir(), &s.name)
                        .is_some_and(|a| a.access_token.is_some()),
                    "description": s.description,
                    "enabled": s.enabled,
                    "connecting": s.connecting(),
                    "server_info": s.server_info,
                    "connected_at": s.connected_at,
                    "error": s.error,
                    "tools": tools,
                })
            })
            .collect();
        Ok(json!(servers))
    }

    /// Start a server in the realm's sandbox, with its credentials, exactly as
    /// a run would, and list its tools in the background.
    fn mcp_connect(&self, realm: &Realm, name: &str) -> Result<()> {
        let ws = self
            .hub
            .workspace_for(realm, &State::new("connect an MCP server", None))?;
        crate::tools::mcp::connect_in_background(realm.skills().dir(), name, ws)
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
    fn is_running(&self, realm: &str, id: &str) -> bool {
        self.registry.lock().unwrap().contains_key(&key(realm, id))
    }

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
                Job::CheckThenAdopt => {
                    match hub.check(&realm, &state.id, &mut observer, &live.cancel) {
                        Ok(checked) => state = checked,
                        Err(e) => eprintln!("checks of run {} failed to start: {e:#}", state.id),
                    }
                    let passed = state.change.as_ref().is_some_and(|c| c.checks_passed());
                    if passed && !live.cancel.is_cancelled() {
                        // The registry entry stays until the end of this
                        // thread: adopt as if idle, under the run's lock.
                        match hub
                            .prepare_adoption(&realm, &state.id)
                            .and_then(|a| match a {
                                Adoption::Ready => hub.adopt(&realm, &state.id),
                                _ => bail!("its base branch moved again while the checks ran"),
                            }) {
                            Ok(adopted) => state = adopted,
                            Err(e) => eprintln!("could not adopt run {}: {e:#}", state.id),
                        }
                    }
                }
            }
            *live.snapshot.lock().unwrap() = state;
            live.bump();
            registry.lock().unwrap().remove(&k);
        });
    }
}

/// Text made safe to put in an HTML page.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Search the official MCP Registry for servers that can be connected as
/// connectors: a Streamable HTTP endpoint that needs no API key header (the
/// ones that need sign-in use OAuth).
fn registry_search(search: &str) -> Result<Vec<Value>> {
    let mut url = reqwest::Url::parse("https://registry.modelcontextprotocol.io/v0/servers")?;
    url.query_pairs_mut()
        .append_pair("limit", "60")
        .append_pair("version", "latest");
    if !search.trim().is_empty() {
        url.query_pairs_mut().append_pair("search", search.trim());
    }
    let body: Value = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get(url)
        .send()
        .context("cannot reach the MCP Registry")?
        .json()
        .context("unexpected answer from the MCP Registry")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in body["servers"].as_array().into_iter().flatten() {
        let s = &entry["server"];
        let Some(name) = s["name"].as_str() else {
            continue;
        };
        let remote = s["remotes"].as_array().into_iter().flatten().find(|r| {
            r["type"] == "streamable-http"
                && !r["headers"]
                    .as_array()
                    .is_some_and(|h| h.iter().any(|h| h["isRequired"] == true))
                && r["url"].as_str().is_some_and(|u| !u.contains('{'))
        });
        let Some(remote) = remote else { continue };
        if !seen.insert(name.to_string()) {
            continue;
        }
        out.push(json!({
            "name": name,
            "title": s["title"].as_str().unwrap_or_else(|| name.rsplit('/').next().unwrap_or(name)),
            "description": s["description"],
            "url": remote["url"],
            "website": s["websiteUrl"].as_str().or(s["repository"]["url"].as_str()),
        }));
    }
    Ok(out)
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

/// How long a PDF preview link works.
const PREVIEW_FOR: Duration = Duration::from_secs(30 * 60);

/// One query parameter, decoded ("" when missing).
fn param(query: &str, name: &str) -> String {
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// A request's raw body, refused past `max` bytes.
fn read_body(request: &mut Request, max: usize, what: &str) -> Result<Vec<u8>> {
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
fn read_bytes(request: &mut Request) -> Result<Vec<u8>> {
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

fn image_response(dir: &std::path::Path, stem: &str) -> Result<HttpResponse> {
    let Some((bytes, mime)) = images::load(dir, stem) else {
        return Ok(json_response(404, json!({"error": "no picture"})));
    };
    Ok(Response::from_data(bytes)
        .with_header(content_type(mime))
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap()))
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
                // Inline styles: the editor (CodeMirror) writes its own. Scripts stay 'self' only.
                "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
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
