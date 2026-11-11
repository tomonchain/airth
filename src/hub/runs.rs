//! Saved runs: listing, loading, importing and deleting them, and the lock
//! that marks a run as driven by a live process.

use super::*;
use crate::supervise::Locked;
use std::collections::HashMap;
use std::path::PathBuf;

impl Hub {
    /// Delete a chat for good: its file, its name and archive mark, and its
    /// worktree if the change is still open. An adopted change stays.
    pub fn delete_run(&self, realm: &Realm, run: &str) -> Result<()> {
        let mut state = self.load_run(realm, run)?;
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        if let Some(change) = state.change.as_mut()
            && change.status == ChangeStatus::Open
        {
            self.close_pull_request(realm, change);
            isolation::discard(change)?;
        }
        let path = state.path().context("this run is not saved")?.to_path_buf();
        std::fs::remove_file(path)?;
        let _ = std::fs::remove_dir_all(self.run_tmp(realm, &state.id));
        realm.set_run_title(&state.id, "")?;
        realm.set_run_archived(&state.id, false)?;
        Ok(())
    }

    /// All saved runs of a realm, oldest first. Unreadable files are reported and skipped.
    pub fn runs(&self, realm: &Realm) -> Result<Vec<RunSummary>> {
        Ok(self
            .load_shared(realm)?
            .into_iter()
            .map(|s| RunSummary {
                live: self.is_live(realm, &s.id),
                change: s.change.as_ref().map(|c| c.status),
                checks_passed: s
                    .change
                    .as_ref()
                    .filter(|c| c.has_changes() && c.checks.as_ref().is_some_and(|k| k.finished))
                    .map(|c| c.checks_passed()),
                touches: s
                    .change
                    .as_ref()
                    .filter(|c| c.status == ChangeStatus::Open)
                    .map(|c| {
                        c.files
                            .iter()
                            .filter_map(|f| f.split('\t').next_back())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                conflicts: s
                    .change
                    .as_ref()
                    .is_some_and(|c| c.status == ChangeStatus::Open && !c.conflicts.is_empty()),
                pull_request: s
                    .change
                    .as_ref()
                    .filter(|c| c.status == ChangeStatus::Open)
                    .and_then(|c| c.pull_request.clone()),
                plan_done: s.plan.iter().filter(|p| p.status.is_closed()).count(),
                plan_total: s.plan.len(),
                asked_by: s.origin.as_ref().and_then(|o| o.chain.last().cloned()),
                agent: s.agent.clone(),
                dispatched_by: s
                    .dispatched_by
                    .as_deref()
                    .and_then(|m| m.split('/').next())
                    .map(str::to_string),
                report: s
                    .report
                    .as_deref()
                    .and_then(|r| r.lines().find(|l| !l.trim().is_empty()))
                    .map(|l| l.chars().take(160).collect()),
                routine: s.origin.as_ref().map(|o| o.objective.clone()),
                id: s.id.clone(),
                objective: s.objective.clone(),
                status: s.status,
                iterations: s.iterations,
                updated_at: s.updated_at,
            })
            .collect())
    }

    /// Every saved run of the realm, in full (for reports such as token usage).
    pub fn all_runs(&self, realm: &Realm) -> Result<Vec<State>> {
        self.load_all(realm)
    }

    pub(super) fn load_all(&self, realm: &Realm) -> Result<Vec<State>> {
        Ok(self
            .load_shared(realm)?
            .into_iter()
            .map(|s| (*s).clone())
            .collect())
    }

    /// Every saved run of the realm, oldest first, each parsed only when its
    /// file changed since it was last read (decision 110): the pages and the
    /// scheduler list the runs every few seconds, and each file holds a
    /// whole transcript.
    pub(super) fn load_shared(&self, realm: &Realm) -> Result<Vec<Arc<State>>> {
        let dir = realm.runs_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut runs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                match load_cached(&path) {
                    Ok(state) => runs.push(state),
                    Err(e) => eprintln!("warning: skipping {}: {e:#}", path.display()),
                }
                seen.insert(path);
            }
        }
        // Runs deleted since are forgotten.
        if let Some(read) = RUNS.locked().as_mut() {
            read.retain(|path, _| path.parent() != Some(dir.as_path()) || seen.contains(path));
        }
        runs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(runs)
    }

    pub fn load_run(&self, realm: &Realm, run: &str) -> Result<State> {
        if run == "last" {
            return self.load_all(realm)?.pop().context("no runs yet");
        }
        validate_id(run)?;
        let path = realm.runs_dir().join(format!("{run}.json"));
        if !path.exists() {
            bail!(
                "no run '{run}' in realm '{}' (see `airth runs`)",
                realm.name()
            );
        }
        State::load(&path)
    }

    /// Clear what finished runs leave behind (decision 110), once an hour
    /// from the scheduler: a run's temporary folder a day after its last
    /// work (made again if it goes on), and the worktree and branch of a run
    /// that ended with nothing changed, a week after (it can be followed up
    /// until then). A change to review is never touched. How many worktrees
    /// went.
    pub fn sweep_finished(&self, now: DateTime<Utc>) -> Result<usize> {
        let mut cleared = 0;
        for realm in self.home.realms()? {
            let Ok(runs) = self.runs(&realm) else {
                continue;
            };
            for run in runs.iter().filter(|r| !r.live) {
                let idle = now - run.updated_at;
                if idle > chrono::Duration::days(1) {
                    let _ = std::fs::remove_dir_all(self.run_tmp(&realm, &run.id));
                }
                if run.status != Status::Completed
                    || run.change != Some(ChangeStatus::Open)
                    || idle <= chrono::Duration::days(7)
                {
                    continue;
                }
                let Ok(mut state) = self.load_run(&realm, &run.id) else {
                    continue;
                };
                let Some(change) = state.change.as_mut() else {
                    continue;
                };
                if change.has_changes() {
                    continue;
                }
                isolation::discard(change)?;
                state.save()?;
                cleared += 1;
            }
        }
        Ok(cleared)
    }

    /// Whether an hour passed since the last sweep (and so one is due now).
    pub fn sweep_due(&self) -> bool {
        static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
        let mut last = LAST.locked();
        let due = last.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(3600));
        if due {
            *last = Some(std::time::Instant::now());
        }
        due
    }

    /// Whether a live process (this one or another) is driving the run.
    pub fn is_live(&self, realm: &Realm, id: &str) -> bool {
        validate_id(id).is_ok() && RunLock::is_held(&realm.runs_dir(), id)
    }

    /// Copy runs saved by Airth v0.1 in `<dir>/.airth/runs` into a realm.
    pub fn import_runs(&self, realm: &Realm, dir: &Path) -> Result<usize> {
        let legacy = dir.join(".airth/runs");
        let Ok(entries) = std::fs::read_dir(&legacy) else {
            return Ok(0);
        };
        std::fs::create_dir_all(realm.runs_dir())?;
        let mut count = 0;
        for entry in entries {
            let path = entry?.path();
            let Some(name) = path.file_name() else {
                continue;
            };
            let target = realm.runs_dir().join(name);
            if path.extension().is_some_and(|e| e == "json") && !target.exists() {
                let mut state = State::load(&path)?;
                state.realm = realm.name().to_string();
                if state.status == Status::Running {
                    state.status = Status::Stopped;
                }
                let text = serde_json::to_string_pretty(&state)?;
                std::fs::write(&target, text)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// A saved run, as listed to clients.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub id: String,
    pub objective: String,
    pub status: Status,
    /// Some process is driving this run right now.
    pub live: bool,
    pub iterations: u32,
    pub updated_at: DateTime<Utc>,
    /// For isolated runs: whether the change is open, adopted or discarded.
    pub change: Option<ChangeStatus>,
    /// For isolated runs with changes: whether the latest checks passed.
    pub checks_passed: Option<bool>,
    /// Steps of the run's plan that are closed (done or skipped), and all steps.
    pub plan_done: usize,
    pub plan_total: usize,
    /// For open changes: the files they touch, to spot chats that overlap.
    pub touches: Vec<String>,
    /// For open changes: merging their base conflicts.
    pub conflicts: bool,
    /// For open changes delivered as pull requests: the pull request.
    pub pull_request: Option<PullRequest>,
    /// The routine (objective) that started it; None when the owner did.
    pub routine: Option<String>,
    /// The routine that asked for it, when another one did (decision 41).
    pub asked_by: Option<String>,
    /// A task's agent (decision 64).
    pub agent: Option<String>,
    /// For a task a plan passed on: the interlocutor's run (decision 120).
    pub dispatched_by: Option<String>,
    /// The first line of its report, if it has one.
    pub report: Option<String>,
}

impl RunSummary {
    /// Status for display: a run marked running that nobody drives was interrupted.
    pub fn display_status(&self) -> &'static str {
        if self.status == Status::Running && !self.live {
            "interrupted"
        } else {
            self.status.as_str()
        }
    }
}

/// A run's file as last read: what tells a change (time, size, file), and
/// what it held.
type Read = (std::time::SystemTime, u64, u64, Arc<State>);

static RUNS: std::sync::Mutex<Option<HashMap<PathBuf, Read>>> = std::sync::Mutex::new(None);

/// A run's state, parsed again only if its file changed: a save writes a new
/// file and renames it over the old one, which gives it a new inode and time.
fn load_cached(path: &Path) -> Result<Arc<State>> {
    let meta = std::fs::metadata(path)?;
    let modified = meta.modified()?;
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&meta);
    #[cfg(not(unix))]
    let inode = 0;
    if let Some((at, len, ino, state)) = RUNS.locked().get_or_insert_with(HashMap::new).get(path)
        && (*at, *len, *ino) == (modified, meta.len(), inode)
    {
        return Ok(state.clone());
    }
    let state = Arc::new(State::load(path)?);
    RUNS.locked().get_or_insert_with(HashMap::new).insert(
        path.to_path_buf(),
        (modified, meta.len(), inode, state.clone()),
    );
    Ok(state)
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        bail!("invalid run id '{id}'");
    }
    Ok(())
}

/// Marks a run as being driven by this process: `<id>.lock` holds our pid and is
/// removed when the run ends. A lock whose process is gone is stale.
pub(super) struct RunLock(PathBuf);

impl RunLock {
    fn path(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!("{id}.lock"))
    }

    pub(super) fn acquire(dir: &Path, id: &str) -> Result<Self> {
        if Self::is_held(dir, id) {
            bail!("run {id} is already in progress");
        }
        std::fs::create_dir_all(dir)?;
        let path = Self::path(dir, id);
        std::fs::write(&path, std::process::id().to_string())?;
        Ok(Self(path))
    }

    pub(super) fn is_held(dir: &Path, id: &str) -> bool {
        let path = Self::path(dir, id);
        // Written before the machine last started: its process is gone,
        // whatever now has its pid (decision 108).
        let before_boot = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .is_ok_and(|at| booted_at().is_some_and(|boot| at < boot));
        if before_boot {
            return false;
        }
        match std::fs::read_to_string(path) {
            Ok(pid) => pid.trim().parse().is_ok_and(process_alive),
            Err(_) => false,
        }
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// When this machine last started, if the system says.
#[cfg(target_os = "linux")]
fn booted_at() -> Option<std::time::SystemTime> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let secs: u64 = stat
        .lines()
        .find_map(|l| l.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

#[cfg(target_os = "macos")]
fn booted_at() -> Option<std::time::SystemTime> {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut size = std::mem::size_of::<libc::timeval>();
    let mut mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
    // SAFETY: sysctl writes at most `size` bytes into `tv`, a timeval, as
    // KERN_BOOTTIME returns one.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut tv as *mut libc::timeval).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && tv.tv_sec > 0)
        .then(|| std::time::UNIX_EPOCH + std::time::Duration::from_secs(tv.tv_sec as u64))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn booted_at() -> Option<std::time::SystemTime> {
    None
}

#[cfg(unix)]
pub(crate) fn process_alive(pid: i32) -> bool {
    // SAFETY: kill(2) with signal 0 only checks for existence and permission.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub(crate) fn process_alive(_pid: i32) -> bool {
    true
}
