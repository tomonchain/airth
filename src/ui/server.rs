//! Serving the app: starting and stopping the server, the file that tells
//! other processes where it runs, and the background threads (updates,
//! schedules, pull requests).

use super::*;
use crate::supervise::Locked;

/// Where the self-update stands, as shown in the app.
#[derive(Default, serde::Serialize)]
pub(super) struct UpdateState {
    /// The newer version, if there is one.
    pub(super) available: Option<crate::update::Available>,
    /// idle, preparing, ready, failed or restarting.
    pub(super) phase: &'static str,
    /// What preparing is doing now (building, testing...).
    pub(super) step: Option<String>,
    pub(super) error: Option<String>,
    /// The built and tested binary, for the version in `available`.
    #[serde(skip)]
    pub(super) staged: Option<std::path::PathBuf>,
    #[serde(skip)]
    pub(super) prepared_commit: Option<String>,
}

/// Every minute, look for a newer version (asking the upstream every 30
/// minutes) and prepare it: build and test it, ready to install on restart.
pub(super) fn run_updater(hub: Arc<Hub>, state: Arc<Mutex<UpdateState>>) {
    let work = hub.home().dir().join("update");
    let mut ticks: u64 = 0;
    loop {
        let fetch = ticks.is_multiple_of(30) || state.locked().phase == "checking";
        ticks += 1;
        match crate::update::check(fetch) {
            Ok(found) => {
                let todo = {
                    let mut st = state.locked();
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
                        let mut st = state.locked();
                        st.phase = "preparing";
                        st.error = None;
                        st.prepared_commit = Some(available.commit.clone());
                    }
                    let step_state = state.clone();
                    let result = crate::update::prepare(&available, &work, &|step| {
                        step_state.locked().step = Some(step.to_string());
                    });
                    let mut st = state.locked();
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
                let mut st = state.locked();
                if st.phase == "checking" {
                    st.phase = "idle";
                    st.error = Some(format!("{e:#}"));
                }
            }
        }
        for _ in 0..60 {
            if state.locked().phase == "checking" {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// The app's access key, kept in the home so links keep working across
/// restarts (and updates).
pub fn persistent_token(hub: &Hub) -> Result<String> {
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

impl App {
    /// Replace the API key: kept for the next start, written where `airth ui`
    /// and the Mac app find the running app, and checked from the next
    /// request on. Every page and phone holding the old one is shut out.
    pub(super) fn new_key(&self) -> Result<String> {
        let token = random_token()?;
        let home = self.hub.home().dir();
        crate::realm::write_private(&home.join("ui-token"), &token)?;
        // The devices allowed with the old key go with it.
        crate::devices::forget_all(home)?;
        let info = server_file(&self.hub);
        if let Ok(text) = std::fs::read_to_string(&info)
            && let Ok(mut running) = serde_json::from_str::<ServerInfo>(&text)
            && running.port == self.port
        {
            running.token = token.clone();
            crate::realm::write_private(&info, &serde_json::to_string(&running)?)?;
        }
        *self.token.write().unwrap_or_else(|e| e.into_inner()) = token.clone();
        Ok(token)
    }
}

/// Stop the runs, then come back as the new binary: launchd restarts the
/// service when it exits; otherwise the process replaces itself.
pub(super) fn restart(
    registry: Registry,
    info_path: std::path::PathBuf,
    exe: std::path::PathBuf,
    port: u16,
) {
    let lives: Vec<Arc<Live>> = registry.locked().values().cloned().collect();
    for live in &lives {
        live.cancel.cancel();
    }
    for _ in 0..100 {
        if registry.locked().is_empty() {
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
pub(super) struct ServerInfo {
    pid: i32,
    port: u16,
    token: String,
}

pub(super) fn server_file(hub: &Hub) -> std::path::PathBuf {
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

/// Look for due objectives and pipelines every few seconds and start them. Only one
/// app serves a home at a time, so only one scheduler runs.
pub(super) fn run_scheduler(runner: Runner) {
    let mut held = HashMap::new();
    loop {
        hold_databases(&runner.hub, &mut held);
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
        // Work a routine handed off to another (decision 41).
        match runner.hub.take_hand_offs() {
            Ok(handed) => {
                for (realm, state) in handed {
                    // A mission a routine launched (decision 49).
                    if state.routine.is_some() {
                        eprintln!("mission {}/{} started", realm.name(), state.id);
                        runner.spawn(realm, state, Job::Alone);
                        continue;
                    }
                    // A task a plan passed on, or the interlocutor told how
                    // its plan went (decision 120).
                    if state.agent.is_some() && state.origin.is_none() {
                        eprintln!("task {}/{} started", realm.name(), state.id);
                        runner.spawn(realm, state, Job::Alone);
                        continue;
                    }
                    let Some(routine) = state.origin.as_ref().map(|o| o.objective.clone()) else {
                        continue;
                    };
                    let wait = realm
                        .objectives()
                        .get(&routine)
                        .map_or(0, |o| o.approval_wait_minutes);
                    eprintln!(
                        "hand-off {}/{routine} started run {}",
                        realm.name(),
                        state.id
                    );
                    runner.spawn(
                        realm,
                        state,
                        Job::Unattended {
                            objective: routine,
                            wait_minutes: wait,
                        },
                    );
                }
            }
            Err(e) => eprintln!("hand-offs: {e:#}"),
        }
        match runner.hub.due_pipelines(chrono::Utc::now()) {
            Ok(due) => {
                for (realm, name) in due {
                    eprintln!("pipeline {}/{name} started", realm.name());
                    let hub = runner.hub.clone();
                    std::thread::spawn(move || hub.run_pipeline_unattended(&realm, &name));
                }
            }
            Err(e) => eprintln!("pipelines: {e:#}"),
        }
        // Routines as graphs (decision 49), delayed actions whose window
        // passed (decision 45), and failed actions to try again (decision 55).
        match runner.hub.due_routines(chrono::Utc::now()) {
            Ok(routines) => {
                for (realm, name, due) in routines {
                    eprintln!("routine {}/{name} started", realm.name());
                    let hub = runner.hub.clone();
                    std::thread::spawn(move || hub.run_scheduled_routine(&realm, &name, due));
                }
            }
            Err(e) => eprintln!("routines: {e:#}"),
        }
        match runner.hub.release_delayed(chrono::Utc::now()) {
            Ok(done) => done.iter().for_each(|d| eprintln!("delayed action {d}")),
            Err(e) => eprintln!("delayed actions: {e:#}"),
        }
        // Reports nobody looked at in a week (decision 59).
        // Tasks the founder allowed to go on, unwatched (decision 64).
        match runner.hub.tasks_to_resume() {
            Ok(ready) => {
                for (realm, state) in ready {
                    runner.spawn(realm, state, Job::Alone);
                }
            }
            Err(e) => eprintln!("tasks: {e:#}"),
        }
        // What finished runs leave behind, once an hour (decision 110).
        if runner.hub.sweep_due() {
            match runner.hub.sweep_finished(chrono::Utc::now()) {
                Ok(0) => {}
                Ok(n) => {
                    eprintln!("cleared the worktrees of {n} finished run(s) with nothing changed")
                }
                Err(e) => eprintln!("clearing finished runs: {e:#}"),
            }
        }
        // A model provider whose money runs low, once an hour (decision 111).
        match crate::spend::warn_low(&runner.hub, chrono::Utc::now()) {
            Ok(said) => said.iter().for_each(|t| eprintln!("spend: {t}")),
            Err(e) => eprintln!("spend: {e:#}"),
        }
        // Connected banks, at most once an hour (decision 54).
        match runner.hub.refresh_finance(chrono::Utc::now()) {
            Ok(done) => done.iter().for_each(|line| eprintln!("{line}")),
            Err(e) => eprintln!("finance: {e:#}"),
        }
        // The businesses' own vitals, at most once an hour (decision 90).
        match runner.hub.refresh_vitals(chrono::Utc::now()) {
            Ok(done) => done.iter().for_each(|line| eprintln!("{line}")),
            Err(e) => eprintln!("vitals: {e:#}"),
        }
        // Legal deadlines: their reminders, then overdue (decision 66).
        match runner.hub.remind_deadlines(chrono::Utc::now()) {
            Ok(done) => done.iter().for_each(|line| eprintln!("{line}")),
            Err(e) => eprintln!("legal deadlines: {e:#}"),
        }
        // Drafts not answered within their routine's window (`expires_after`).
        match runner.hub.expire_approvals(chrono::Utc::now()) {
            Ok(gone) => gone
                .iter()
                .for_each(|(realm, n)| eprintln!("{realm}: {n} draft(s) expired unanswered")),
            Err(e) => eprintln!("drafts: {e:#}"),
        }
        match runner.hub.expire_reports(chrono::Utc::now()) {
            Ok(gone) => gone
                .iter()
                .for_each(|(realm, n)| eprintln!("{realm}: {n} report(s) expired unseen")),
            Err(e) => eprintln!("reports: {e:#}"),
        }
        // Failed actions whose next attempt is due (decision 55).
        match runner.hub.retry_due(chrono::Utc::now()) {
            Ok(done) => done.iter().for_each(|d| eprintln!("retried action {d}")),
            Err(e) => eprintln!("retried actions: {e:#}"),
        }
        std::thread::sleep(scheduler::TICK);
    }
}

/// One connection to each business database, held while the service runs.
/// Without one, every short open (a dozen a tick, the bot, every page)
/// creates the database's journal files and deletes them on close, which
/// wrote about a gigabyte a day. A database not created yet is left so.
fn hold_databases(hub: &Hub, held: &mut HashMap<String, crate::business::store::Store>) {
    let Ok(realms) = hub.home().realms() else {
        return;
    };
    for realm in realms {
        if held.contains_key(realm.name())
            || !realm.dir().join(crate::business::store::FILE).exists()
        {
            continue;
        }
        if let Ok(store) = crate::business::store::Store::of(&realm) {
            held.insert(realm.name().to_string(), store);
        }
    }
}

/// Every two minutes, follow the open pull requests of realms that deliver
/// changes that way: record them merged or closed, keep their branches
/// current with the base, and have a chat resolve a conflict.
pub(super) fn follow_pull_requests(runner: Runner) {
    loop {
        std::thread::sleep(Duration::from_secs(120));
        let Ok(realms) = runner.hub.home().realms() else {
            continue;
        };
        for realm in realms {
            if !realm.delivers_pull_requests() || realm.config.archived {
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
                            runner.spawn(realm.clone(), state, Job::Alone);
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("run {} cannot resolve its conflict: {e:#}", run.id),
                    },
                    // A reviewing app reviewed it (decision 87): the chat answers.
                    Ok(PrEvent::Reviewed(review)) => {
                        match runner.hub.answer_review(&realm, &run.id, &review) {
                            Ok(state) => {
                                eprintln!("run {} answers a review of its pull request", run.id);
                                runner.spawn(realm.clone(), state, Job::Alone);
                            }
                            Err(e) => eprintln!("run {} cannot answer its review: {e:#}", run.id),
                        }
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("pull request of run {}: {e:#}", run.id),
                }
            }
        }
    }
}

/// The line under the link: what the link grants, and whether anything
/// runs on its own while it is open.
fn banner(background: bool) -> String {
    let runs = match background {
        true => "Routines run while this is running.",
        false => "Preview: nothing runs on its own (no routines, no Telegram, no updates).",
    };
    format!("This link grants control of the agent; don't share it. {runs} Ctrl-C to quit.")
}

pub fn serve(hub: Hub, options: UiOptions) -> Result<()> {
    let background = options.background;
    let server = start(hub, options)?;
    eprintln!("\x1b[1mairth\x1b[0m → {}", server.url);
    eprintln!("\x1b[2m{}\x1b[0m", banner(background));
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
    /// Stop the runs in progress (they can be resumed) and forget the server:
    /// the Mac's desktop app does, when it quits.
    #[cfg(target_os = "macos")]
    pub fn stop(&self) {
        let lives: Vec<Arc<Live>> = self.registry.locked().values().cloned().collect();
        for live in &lives {
            live.cancel.cancel();
        }
        for _ in 0..50 {
            if self.registry.locked().is_empty() {
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
        token: std::sync::RwLock::new(persistent_token(&token_hub)?),
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
            token: app.token(),
        })?,
    )?;

    let url = format!("http://127.0.0.1:{port}/#token={}", app.token());
    if app.options.open_browser {
        open_in_browser(&url);
    }

    // A preview serves the pages and nothing else: nothing runs on its own.
    if app.options.background {
        // Each loop is kept alive: a panic starts it again, and the founder
        // hears of it (decision 108).
        use crate::supervise::supervise;
        let inbox = || app.hub.home().inbox();
        let runner = app.runner.clone();
        let tell = inbox();
        std::thread::spawn(move || supervise("scheduler", tell, || run_scheduler(runner.clone())));
        let runner = app.runner.clone();
        let tell = inbox();
        std::thread::spawn(move || {
            supervise("pull-request follower", tell, || {
                follow_pull_requests(runner.clone())
            })
        });
        let hub = app.hub.clone();
        let tell = inbox();
        let runner = app.runner.clone();
        std::thread::spawn(move || {
            supervise("Telegram bot", tell, || {
                // The founder's orders from the phone run unwatched, as
                // from the command bar (decision 109).
                crate::telegram::run((*hub).clone(), &|realm, state| {
                    runner.spawn(realm, state, Job::Alone)
                })
            })
        });
        let (hub, update) = (app.hub.clone(), app.update.clone());
        let tell = inbox();
        std::thread::spawn(move || {
            supervise("updater", tell, || run_updater(hub.clone(), update.clone()))
        });
        let home = app.hub.home().clone();
        let tell = inbox();
        std::thread::spawn(move || supervise("backup", tell, || crate::backup::run(home.clone())));
        // Notifications on the founder's phone (decision 117).
        let home = app.hub.home().clone();
        let tell = inbox();
        std::thread::spawn(move || {
            supervise("phone notifications", tell, || {
                crate::push::run(home.clone())
            })
        });
        // Its own loop: a slow fetch never holds the scheduler (decision 114).
        let home = app.hub.home().clone();
        let tell = inbox();
        std::thread::spawn(move || {
            supervise("business folder sync", tell, || {
                crate::business::sync::run(home.clone())
            })
        });
        // Each role's mailbox, read every few minutes (decision 70).
        let home = app.hub.home().clone();
        let tell = inbox();
        std::thread::spawn(move || {
            supervise("mail reader", tell, || crate::mail::run(home.clone()))
        });
        let dir = app.hub.home().dir().to_path_buf();
        let tell = inbox();
        std::thread::spawn(move || {
            supervise("log keeper", tell, || crate::service::keep_log(&dir))
        });
    }

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
pub(super) fn serve_requests(server: Server, app: App) {
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
pub(super) fn shut_down(registry: &Registry, info_path: &std::path::Path) {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let _ = std::fs::remove_file(info_path);
    if CALLS.fetch_add(1, Ordering::SeqCst) > 0 {
        std::process::exit(130);
    }
    let lives: Vec<Arc<Live>> = registry.locked().values().cloned().collect();
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
            if registry.locked().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use crate::business::store::{FILE, Store};

    /// The service holds each business database open, so the journal files
    /// stay between the short opens; it creates no database.
    #[test]
    fn the_service_holds_each_business_database_open() {
        let dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(dir.path()).unwrap();
        let new = |name: &str| crate::home::NewRealm {
            name: name.into(),
            description: String::new(),
            root: None,
            isolation: None,
        };
        let a = home.create_realm(new("a")).unwrap();
        let b = home.create_realm(new("b")).unwrap();
        drop(Store::of(&a).unwrap());
        let hub = crate::hub::Hub::new(home);
        let mut held = std::collections::HashMap::new();
        super::hold_databases(&hub, &mut held);
        assert_eq!(held.keys().collect::<Vec<_>>(), ["a"]);
        assert!(!b.dir().join(FILE).exists(), "no database created");
        drop(Store::of(&a).unwrap());
        assert!(a.dir().join(format!("{FILE}-wal")).exists());
        drop(held);
        drop(Store::of(&a).unwrap());
        assert!(
            !a.dir().join(format!("{FILE}-wal")).exists(),
            "the watch is real"
        );
    }

    /// A preview runs nothing on its own: its start message must not say
    /// that routines run.
    #[test]
    fn a_preview_does_not_say_routines_run() {
        assert!(super::banner(true).contains("Routines run while this is running."));
        let preview = super::banner(false);
        assert!(!preview.contains("run while"), "{preview}");
        assert!(preview.contains("nothing runs on its own"), "{preview}");
    }
}
