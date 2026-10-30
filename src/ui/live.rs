//! Runs driven from the app: each on its own thread, watched through a
//! shared snapshot, with permission requests answered through a channel.

use super::*;

/// A run driven by this process.
pub(super) struct Live {
    pub(super) snapshot: Mutex<State>,
    pub(super) pending: Mutex<Option<Pending>>,
    pub(super) cancel: CancelToken,
    pub(super) version: AtomicU64,
}

impl Live {
    pub(super) fn bump(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) struct Pending {
    pub(super) id: u64,
    pub(super) request: PermissionRequest,
    pub(super) reply: mpsc::Sender<Answer>,
    /// When the owner was asked: their answer's time goes to the ledger.
    pub(super) asked: std::time::Instant,
}

/// Live runs by "<realm>/<run id>".
pub(super) type Registry = Arc<Mutex<HashMap<String, Arc<Live>>>>;

pub(super) fn key(realm: &str, id: &str) -> String {
    format!("{realm}/{id}")
}

pub(super) struct UiObserver(Arc<Live>);

impl Observer for UiObserver {
    fn changed(&mut self, state: &State) {
        *self.0.snapshot.lock().unwrap() = state.clone();
        self.0.bump();
    }
}

/// Parks the agent thread until the human answers in the UI or stops the run.
/// For a run an objective started, the owner may be away: it tells them, and
/// waits only as long as the objective allows.
pub(super) struct UiApprover {
    pub(super) live: Arc<Live>,
    pub(super) next_id: u64,
    pub(super) away: Option<Away>,
}

/// An objective's run, which nobody may be watching.
pub(super) struct Away {
    pub(super) objective: String,
    pub(super) realm: String,
    pub(super) run: String,
    pub(super) wait_minutes: u32,
    pub(super) inbox: Inbox,
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
            asked: std::time::Instant::now(),
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
pub(super) enum Job {
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
pub(super) struct Runner {
    pub(super) hub: Arc<Hub>,
    pub(super) registry: Registry,
    pub(super) max_iterations: u32,
}

impl Runner {
    pub(super) fn is_running(&self, realm: &str, id: &str) -> bool {
        self.registry.lock().unwrap().contains_key(&key(realm, id))
    }

    pub(super) fn spawn(&self, realm: Realm, state: State, job: Job) {
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
