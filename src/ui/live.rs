//! Runs driven from the app: each on its own thread, watched through a
//! shared snapshot, with permission requests answered through a channel.

use super::*;

/// A run driven by this process.
pub(super) struct Live {
    pub(super) snapshot: Mutex<State>,
    pub(super) pending: Mutex<Option<Pending>>,
    pub(super) cancel: CancelToken,
    pub(super) version: AtomicU64,
    pub(super) watch: Watch,
    pub(super) draft: Draft,
    /// What the founder wrote while the agent worked, until it reads it. None
    /// once nothing will read it: a job that is not the agent's, or a run
    /// whose thread is ending.
    pub(super) said: Mutex<Option<Vec<String>>>,
}

/// What the model is writing in the run, while it writes it: gone once its
/// answer is in the snapshot.
#[derive(Default)]
pub(super) struct Draft {
    pub(super) text: Mutex<String>,
    /// Counts each change, for a page waiting for the next one.
    pub(super) seq: AtomicU64,
}

impl Draft {
    fn set(&self, text: &str) {
        let mut t = self.text.lock().unwrap();
        if *t != text {
            text.clone_into(&mut t);
            self.seq.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Longer than this between two looks at a run, and the founder has left
/// it. The run's page looks every 600 ms; a hidden tab, about once a second
/// until the browser slows it down further.
const WATCH_GAP: Duration = Duration::from_secs(10);

/// A run that failed on a passing error is tried again after these waits,
/// then left failed (decision 81).
const RETRY_WAITS: [Duration; 3] = [
    Duration::from_secs(30),
    Duration::from_secs(120),
    Duration::from_secs(480),
];
/// A run that reached its time budget goes on alone this many times more.
const TIME_BUDGETS_MORE: usize = 2;

/// What a run that ended on `status` with `report` does next on its own:
/// try again after a wait, go on at once, or nothing (decision 81).
pub(super) fn next_on_its_own(
    status: Status,
    report: Option<&str>,
    retries: usize,
    budgets: usize,
) -> Option<Duration> {
    let report = report.unwrap_or_default();
    match status {
        Status::Failed if crate::models::transient(report) => RETRY_WAITS.get(retries).copied(),
        Status::Stopped
            if report.contains("reached its time budget") && budgets < TIME_BUDGETS_MORE =>
        {
            Some(Duration::ZERO)
        }
        _ => None,
    }
}

/// Whether the founder watches a run in the app (decision 56): its page
/// looks at it again and again. Once they left, they left, for the run.
#[derive(Default)]
pub(super) struct Watch(Mutex<(Option<std::time::Instant>, bool)>);

impl Watch {
    /// Watched from now: the founder just started or followed up the run.
    pub(super) fn from_now() -> Self {
        Self(Mutex::new((Some(std::time::Instant::now()), false)))
    }

    /// The founder asked for a `Drive` in the app: they watch it from now.
    /// Nobody watches the other jobs.
    pub(super) fn for_job(job: &Job) -> Self {
        match job {
            Job::Drive => Self::from_now(),
            _ => Self::default(),
        }
    }

    /// The run's page looked at it.
    pub(super) fn seen(&self) {
        let mut w = self.0.lock().unwrap();
        w.1 = w.1 || w.0.is_none_or(|at| at.elapsed() > WATCH_GAP);
        w.0 = Some(std::time::Instant::now());
    }

    pub(super) fn watching(&self) -> bool {
        let mut w = self.0.lock().unwrap();
        w.1 = w.1 || w.0.is_none_or(|at| at.elapsed() > WATCH_GAP);
        !w.1
    }
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

pub(super) struct UiObserver(pub(super) Arc<Live>);

impl Observer for UiObserver {
    fn changed(&mut self, state: &State) {
        *self.0.snapshot.lock().unwrap() = state.clone();
        self.0.bump();
        self.0.draft.set("");
    }

    fn writing(&mut self, text: &str) {
        self.0.draft.set(text);
    }

    fn said(&mut self) -> Vec<String> {
        self.0
            .said
            .lock()
            .unwrap()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

impl Live {
    /// Keeps what the founder wrote for the agent's next turn; false when
    /// nothing will read it.
    pub(super) fn tell(&self, text: &str) -> bool {
        match self.said.lock().unwrap().as_mut() {
            Some(said) if !self.cancel.is_cancelled() => {
                said.push(text.to_string());
                true
            }
            _ => false,
        }
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
    /// A run an objective started is never watched; another, while its page
    /// in the app follows it.
    fn watched(&self) -> bool {
        self.away.is_none() && self.live.watch.watching()
    }

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
            // Asked for a commit, the founder left: the runtime puts it in
            // Needs you instead (decision 56).
            if request
                .warnings
                .iter()
                .any(|w| w == crate::runtime::COMMITS_WARNING)
                && !self.watched()
            {
                break Answer::No("the founder left the run before answering".into());
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
    /// Drive the agent (a new run, a follow-up, a resume), for the founder
    /// who just asked for it in the app.
    Drive,
    /// Drive the agent for the founder with nobody watching: Airth started
    /// it on its own.
    Alone,
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
            watch: Watch::for_job(&job),
            draft: Draft::default(),
            said: Mutex::new(
                matches!(job, Job::Drive | Job::Alone | Job::Unattended { .. }).then(Vec::new),
            ),
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
            let mut away = match &job {
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
                Job::Drive | Job::Alone | Job::Unattended { .. } => {
                    let (mut retries, mut budgets) = (0, 0);
                    loop {
                        let hooks = RunHooks {
                            approver: Box::new(UiApprover {
                                live: live.clone(),
                                next_id: 0,
                                // Who writes to a run is there to answer it.
                                away: away.take(),
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
                        // A passing failure is tried again, a time budget renewed,
                        // without the founder's go (decision 81).
                        if !live.cancel.is_cancelled()
                            && let Some(wait) = next_on_its_own(
                                state.status,
                                state.report.as_deref(),
                                retries,
                                budgets,
                            )
                        {
                            if state.status == Status::Failed {
                                retries += 1;
                                state.report = Some(format!(
                                    "Trying again in {} ({retries} of {}): {}",
                                    if wait.as_secs() < 60 {
                                        format!("{} s", wait.as_secs())
                                    } else {
                                        format!("{} min", wait.as_secs() / 60)
                                    },
                                    RETRY_WAITS.len(),
                                    state.report.as_deref().unwrap_or_default()
                                ));
                            } else {
                                budgets += 1;
                            }
                            state.status = Status::Running;
                            let _ = state.save();
                            observer.changed(&state);
                            let until = std::time::Instant::now() + wait;
                            while std::time::Instant::now() < until && !live.cancel.is_cancelled() {
                                std::thread::sleep(Duration::from_millis(250).min(wait));
                            }
                            if live.cancel.is_cancelled() {
                                state.status = Status::Stopped;
                                state.report = Some("Stopped by the user.".into());
                                let _ = state.save();
                                break;
                            }
                            state.report = None;
                            let _ = state.save();
                            continue;
                        }
                        // Written as the run ended: it goes on with it, as a
                        // follow-up would. Past this, what is written starts
                        // the run again the usual way.
                        let mut said = live.said.lock().unwrap();
                        let left = said.take().unwrap_or_default();
                        if left.is_empty()
                            || live.cancel.is_cancelled()
                            || state.status == Status::Failed
                        {
                            break;
                        }
                        *said = Some(Vec::new());
                        drop(said);
                        for text in &left {
                            crate::hub::append_user_text(&mut state.messages, text);
                        }
                        state.status = Status::Running;
                        state.report = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Decision 81: a passing failure is tried again three times, later and
    /// later; a time budget is renewed twice; anything else waits for the
    /// founder.
    #[test]
    fn a_run_goes_on_alone_after_a_passing_failure_or_its_time_budget() {
        let busy = Some("DeepSeek API error 503: overloaded");
        let waits: Vec<_> = (0..4)
            .map(|n| next_on_its_own(Status::Failed, busy, n, 0))
            .collect();
        assert_eq!(
            waits,
            [
                Some(Duration::from_secs(30)),
                Some(Duration::from_secs(120)),
                Some(Duration::from_secs(480)),
                None
            ]
        );
        assert_eq!(
            next_on_its_own(
                Status::Failed,
                Some("API error 401: api key is invalid"),
                0,
                0
            ),
            None
        );
        let budget = Some("Stopped: reached its time budget (30 minutes).");
        assert_eq!(
            next_on_its_own(Status::Stopped, budget, 0, 0),
            Some(Duration::ZERO)
        );
        assert_eq!(
            next_on_its_own(Status::Stopped, budget, 0, 1),
            Some(Duration::ZERO)
        );
        assert_eq!(next_on_its_own(Status::Stopped, budget, 0, 2), None);
        for waits_for_the_founder in [
            "Stopped: reached its cost budget ($2.00 of $2.00).",
            "Stopped by the user.",
            "not done: nobody is watching this task, so the founder is asked in Needs you (item 9)",
        ] {
            assert_eq!(
                next_on_its_own(Status::Stopped, Some(waits_for_the_founder), 0, 0),
                None
            );
        }
        assert_eq!(next_on_its_own(Status::Completed, None, 0, 0), None);
    }

    fn left(ago: u64) -> Watch {
        Watch(Mutex::new((
            Some(Instant::now() - Duration::from_secs(ago)),
            false,
        )))
    }

    #[test]
    fn a_run_is_watched_while_its_page_looks_and_never_again_once_left() {
        let w = Watch::from_now();
        assert!(w.watching());
        w.seen();
        assert!(w.watching());
        // Its page stopped looking: left, even once it looks again.
        let w = left(11);
        assert!(!w.watching());
        w.seen();
        assert!(!w.watching());
        // Left between two looks, and nothing asked in between.
        let w = left(11);
        w.seen();
        assert!(!w.watching());
    }

    #[test]
    fn only_a_run_the_founder_just_asked_for_in_the_app_starts_watched() {
        assert!(Watch::for_job(&Job::Drive).watching());
        assert!(!Watch::for_job(&Job::Alone).watching());
        let objective = Job::Unattended {
            objective: "o".into(),
            wait_minutes: 5,
        };
        assert!(!Watch::for_job(&objective).watching());
        assert!(!Watch::default().watching());
    }

    fn live(watch: Watch) -> Arc<Live> {
        Arc::new(Live {
            snapshot: Mutex::new(State::new("x", None)),
            pending: Mutex::new(None),
            cancel: CancelToken::default(),
            version: AtomicU64::new(1),
            watch,
            draft: Draft::default(),
            said: Mutex::new(None),
        })
    }

    #[test]
    fn asked_for_a_commit_the_app_stops_waiting_once_the_founder_left() {
        let mut approver = UiApprover {
            live: live(left(11)),
            next_id: 0,
            away: None,
        };
        assert!(!approver.watched());
        let request = PermissionRequest {
            tool: "http_request".into(),
            capability: Capability::Network,
            preview: String::new(),
            warnings: vec![crate::runtime::COMMITS_WARNING.into()],
            scope: None,
        };
        // Were it still waiting, the run's stop would end it a second on.
        let cancel = approver.live.cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(1));
            cancel.cancel();
        });
        let Answer::No(why) = approver.ask(&request).unwrap() else {
            panic!("not approved");
        };
        assert!(why.contains("left"), "{why}");
    }

    /// What the founder writes to a run at work waits for the agent's next
    /// turn, once; nothing is kept for a run nothing will read it in, or one
    /// they stopped.
    #[test]
    fn what_the_founder_writes_to_a_run_at_work_waits_for_its_next_turn() {
        let at_work = live(Watch::from_now());
        *at_work.said.lock().unwrap() = Some(Vec::new());
        assert!(at_work.tell("Add a title too."));
        let mut observer = UiObserver(at_work.clone());
        assert_eq!(observer.said(), ["Add a title too."]);
        assert!(observer.said().is_empty());
        at_work.cancel.cancel();
        assert!(!at_work.tell("And a summary."));
        assert!(!live(Watch::from_now()).tell("Checking? Not read."));
    }

    #[test]
    fn a_run_an_objective_started_is_never_watched() {
        let approver = UiApprover {
            live: live(Watch::from_now()),
            next_id: 0,
            away: Some(Away {
                objective: "o".into(),
                realm: "r".into(),
                run: "x".into(),
                wait_minutes: 5,
                inbox: crate::notify::Inbox::new(std::env::temp_dir().join("airth-watch-test")),
            }),
        };
        assert!(!approver.watched());
    }
}
