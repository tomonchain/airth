//! Routines asking each other for work (decision 41). A run may ask only the
//! routines its own routine lists, and what it starts is the other routine's
//! run, with that routine's limits: no right passes from one to the other.

use super::*;

/// How many routines a chain of requests may hold.
pub const MAX_DEPTH: usize = 3;

/// Hand-offs waiting for the app to start them, one empty file per run.
const QUEUE: &str = "hand-offs";

/// Nobody watches a run another routine asked for as it goes: its saved
/// state shows in Tasks.
struct Unwatched;

impl Observer for Unwatched {}

/// Nobody is there to approve inside a run another routine asked for.
struct Unattended;

impl Approver for Unattended {
    fn ask(
        &mut self,
        _: &crate::permissions::PermissionRequest,
    ) -> Result<crate::permissions::Answer> {
        Ok(crate::permissions::Answer::No(
            "this task was asked for by another routine, and nobody is there to approve: do without it, and list it in your report for the owner".into(),
        ))
    }
}

impl Hub {
    /// Start `routine`'s run for a run of the routine `from`, with `brief`
    /// as its data. Saved, not driven.
    pub fn delegate(
        &self,
        realm: &Realm,
        from: &Origin,
        routine: &str,
        brief: &str,
    ) -> Result<State> {
        let asking = realm.objectives().get(&from.objective)?;
        if !asking.delegates.iter().any(|d| d == routine) {
            bail!(
                "{} may not ask {routine}: the owner did not allow it",
                from.objective
            );
        }
        let mut chain = from.chain.clone();
        chain.push(from.objective.clone());
        if chain.iter().any(|c| c == routine) {
            bail!(
                "{routine} already asked for this work ({}): asking it again would go round in circles",
                chain.join(" → ")
            );
        }
        if chain.len() >= MAX_DEPTH {
            bail!(
                "this chain of requests is {MAX_DEPTH} routines deep ({}): do it yourself, or report what is missing",
                chain.join(" → ")
            );
        }
        let (mut state, _) = self.start_objective(
            realm,
            routine,
            &format!("asked by {}", from.objective),
            Some(brief),
            Utc::now(),
        )?;
        if let Some(origin) = state.origin.as_mut() {
            origin.chain = chain;
        }
        state.save()?;
        Ok(state)
    }

    /// Leave a started run for the app to drive, as it drives scheduled ones.
    pub fn queue_hand_off(&self, realm: &Realm, run: &str) -> Result<()> {
        let dir = realm.dir().join(QUEUE);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(run), "")?;
        Ok(())
    }

    /// The hand-offs waiting, across realms, each taken once.
    pub fn take_hand_offs(&self) -> Result<Vec<(Realm, State)>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            let Ok(entries) = std::fs::read_dir(realm.dir().join(QUEUE)) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let id = entry.file_name().to_string_lossy().to_string();
                std::fs::remove_file(&path)?;
                match self.load_run(&realm, &id) {
                    Ok(state) => out.push((realm.clone(), state)),
                    Err(e) => eprintln!("hand-off {id} in {}: {e:#}", realm.name()),
                }
            }
        }
        Ok(out)
    }

    /// Drive a run another routine asked for until it ends, nobody there to
    /// approve, and give back its report.
    pub fn answer_request(
        &self,
        realm: &Realm,
        state: &mut State,
        cancel: &CancelToken,
    ) -> Result<String> {
        let mut observer = Unwatched;
        self.drive(
            realm,
            state,
            AgentConfig::new(50),
            RunHooks {
                approver: Box::new(Unattended),
                observer: &mut observer,
                cancel: cancel.clone(),
            },
        )?;
        Ok(format!(
            "{} answered ({}):\n{}",
            state
                .origin
                .as_ref()
                .map_or("the routine", |o| o.objective.as_str()),
            state.status.as_str(),
            state.report.as_deref().unwrap_or("(no report)")
        ))
    }

    /// The routines a run may ask for work, and how, for its tools.
    pub(super) fn delegation_for(
        &self,
        realm: &Realm,
        state: &State,
    ) -> Result<Option<tools::delegate::Delegation>> {
        let Some(origin) = &state.origin else {
            return Ok(None);
        };
        let Ok(routine) = realm.objectives().get(&origin.objective) else {
            return Ok(None);
        };
        if routine.delegates.is_empty() {
            return Ok(None);
        }
        let all = realm.objectives().list()?;
        let targets: Vec<tools::delegate::Target> = routine
            .delegates
            .iter()
            .filter_map(|d| all.iter().find(|o| o.id == *d))
            .map(|o| tools::delegate::Target {
                routine: o.id.clone(),
                agent: o.bot.clone(),
                task: o.objective.lines().next().unwrap_or_default().to_string(),
            })
            .collect();
        if targets.is_empty() {
            return Ok(None);
        }
        let (hub, realm, origin) = (self.clone(), realm.clone(), origin.clone());
        let run: tools::delegate::Delegate = Arc::new(move |routine, brief, wait, cancel| {
            let mut state = hub.delegate(&realm, &origin, routine, brief)?;
            if wait {
                hub.answer_request(&realm, &mut state, cancel)
            } else {
                hub.queue_hand_off(&realm, &state.id)?;
                Ok(format!(
                    "handed off to {routine}: it starts now as task {}",
                    state.id
                ))
            }
        });
        Ok(Some(tools::delegate::Delegation { targets, run }))
    }
}
