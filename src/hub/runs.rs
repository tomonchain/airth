//! Saved runs: listing, loading, importing and deleting them, and the lock
//! that marks a run as driven by a live process.

use super::*;

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
        realm.set_run_title(&state.id, "")?;
        realm.set_run_archived(&state.id, false)?;
        Ok(())
    }

    /// All saved runs of a realm, oldest first. Unreadable files are reported and skipped.
    pub fn runs(&self, realm: &Realm) -> Result<Vec<RunSummary>> {
        Ok(self
            .load_all(realm)?
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
                routine: s.origin.map(|o| o.objective),
                id: s.id,
                objective: s.objective,
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
        let dir = realm.runs_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut runs = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                match State::load(&path) {
                    Ok(state) => runs.push(state),
                    Err(e) => eprintln!("warning: skipping {}: {e:#}", path.display()),
                }
            }
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

    fn is_held(dir: &Path, id: &str) -> bool {
        match std::fs::read_to_string(Self::path(dir, id)) {
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
