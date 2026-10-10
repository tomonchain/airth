//! The scheduler decides which objectives are due; the background runtime
//! (the app's server, see ui) asks it every few seconds and starts their runs.
//! It also builds the summaries the owner reads after being away.

use crate::hub::Hub;
use crate::objective::{self, Schedule, Trigger};
use crate::realm::Realm;
use crate::state::Status;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// How often the background runtime looks for due objectives.
pub const TICK: std::time::Duration = std::time::Duration::from_secs(10);

/// An objective whose trigger fired.
pub struct Due {
    pub realm: Realm,
    pub objective: String,
    /// What fired, for the run and the owner: "schedule: daily 08:00".
    pub trigger: String,
}

/// The objectives due at `now`, across all realms. A schedule seen for the
/// first time is armed for its next time instead of firing; a watch records
/// what its files look like. A schedule missed while Airth was not running
/// fires once, not once per missed time.
pub fn due(hub: &Hub, now: DateTime<Utc>) -> Result<Vec<Due>> {
    let mut out = Vec::new();
    for realm in hub.home().realms()? {
        if realm.config.archived {
            continue;
        }
        let store = realm.objectives();
        for o in store.list()? {
            if !o.enabled {
                continue;
            }
            if o.last_run
                .as_deref()
                .is_some_and(|r| hub.is_live(&realm, r))
            {
                continue;
            }
            let fire = match &o.trigger {
                Trigger::Schedule { every } => {
                    let Ok(schedule) = Schedule::parse(every) else {
                        continue;
                    };
                    match o.next_at {
                        Some(at) => at <= now,
                        None => {
                            let next = schedule.next_after(now);
                            store.update(&o.id, |o| o.next_at = Some(next))?;
                            false
                        }
                    }
                }
                Trigger::Watch { path } => {
                    let Ok(root) = realm.root() else { continue };
                    let Ok(path) = objective::watched_path(&root, path) else {
                        continue;
                    };
                    let now_fp = objective::fingerprint(&path);
                    match &o.fingerprint {
                        Some(fp) => *fp != now_fp,
                        None => {
                            store.update(&o.id, |o| o.fingerprint = Some(now_fp))?;
                            false
                        }
                    }
                }
                Trigger::Manual | Trigger::Webhook { .. } => false,
            };
            if fire {
                out.push(Due {
                    trigger: o.trigger.describe(),
                    realm: realm.clone(),
                    objective: o.id,
                });
            }
        }
    }
    Ok(out)
}

/// What happened in one realm since some time.
#[derive(Debug, Serialize)]
pub struct RealmSummary {
    pub realm: String,
    pub runs: usize,
    pub completed: usize,
    pub failed: usize,
    pub stopped: usize,
    /// Runs whose change waits for the owner's review.
    pub to_review: Vec<RunLine>,
    /// Runs that did not finish and can be resumed.
    pub unfinished: Vec<RunLine>,
    /// Finished runs, newest first, with the first line of their report.
    pub recent: Vec<RunLine>,
    pub objectives: Vec<ObjectiveLine>,
}

#[derive(Debug, Serialize)]
pub struct RunLine {
    pub id: String,
    pub objective: String,
    pub status: String,
    pub report: String,
    /// The objective that started it, if one did.
    pub origin: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ObjectiveLine {
    pub id: String,
    pub trigger: String,
    pub enabled: bool,
    pub next_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
}

/// Per realm: what ran since `since`, what waits for the owner, what is
/// scheduled. Realms with nothing to say are left out.
pub fn summary(hub: &Hub, since: DateTime<Utc>) -> Result<Vec<RealmSummary>> {
    let mut out = Vec::new();
    for realm in hub.home().realms()? {
        if realm.config.archived {
            continue;
        }
        let mut s = RealmSummary {
            realm: realm.name().to_string(),
            runs: 0,
            completed: 0,
            failed: 0,
            stopped: 0,
            to_review: Vec::new(),
            unfinished: Vec::new(),
            recent: Vec::new(),
            objectives: Vec::new(),
        };
        for summary in hub.runs(&realm)?.into_iter().rev() {
            let state = hub.load_run(&realm, &summary.id)?;
            let line = || RunLine {
                id: state.id.clone(),
                objective: first_line(&state.objective),
                status: summary.display_status().to_string(),
                report: first_line(state.report.as_deref().unwrap_or_default()),
                origin: state.origin.as_ref().map(|o| o.objective.clone()),
            };
            if let Some(c) = &state.change
                && c.status == crate::isolation::ChangeStatus::Open
                && c.has_changes()
                && !summary.live
            {
                s.to_review.push(line());
            }
            if !summary.live && state.status != Status::Completed {
                let resumable = state
                    .change
                    .as_ref()
                    .is_none_or(|c| c.status == crate::isolation::ChangeStatus::Open);
                if resumable && state.updated_at >= since {
                    s.unfinished.push(line());
                }
            }
            if state.updated_at < since {
                continue;
            }
            s.runs += 1;
            match state.status {
                Status::Completed => s.completed += 1,
                Status::Failed => s.failed += 1,
                Status::Stopped => s.stopped += 1,
                Status::Running => {}
            }
            if state.status == Status::Completed && s.recent.len() < 10 {
                s.recent.push(line());
            }
        }
        for o in realm.objectives().list()? {
            s.objectives.push(ObjectiveLine {
                trigger: o.trigger.describe(),
                id: o.id,
                enabled: o.enabled,
                next_at: o.next_at,
                last_status: o.last_status,
            });
        }
        if s.runs > 0
            || !s.to_review.is_empty()
            || !s.unfinished.is_empty()
            || !s.objectives.is_empty()
        {
            out.push(s);
        }
    }
    Ok(out)
}

fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.chars().count() > 160 {
        format!("{}…", line.chars().take(159).collect::<String>())
    } else {
        line.to_string()
    }
}
