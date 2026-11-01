//! The authority that applies an agent's autonomy levels to the declared
//! actions of a routine's run (decision 45). It reads the levels from
//! `business.db` before each action, so a change, a demotion included,
//! applies to the runs already going.

use super::*;
use crate::actions::Declared;
use crate::business::store::Store;
use crate::business::store::autonomy::{Begin, NewAction, NewItem};
use crate::runtime::{Authority, Decision};
use serde_json::Value;

/// Actions a connector takes alone in 24 hours, and the minutes the founder
/// has to cancel a delayed one, unless declared otherwise.
const DAILY_LIMIT: u32 = 20;
const CANCEL_MINUTES: u32 = 30;

pub struct BusinessAuthority {
    realm: Realm,
    agent: String,
    routine: String,
    run: String,
    declared: Vec<Declared>,
    /// A routine's graph keys its acts itself (routine, source id, block);
    /// a mission's calls are keyed by its run and call ids.
    graph: bool,
}

impl BusinessAuthority {
    pub fn for_run(realm: &Realm, state: &State) -> Result<Option<Self>> {
        let Some(origin) = &state.origin else {
            // A run the founder started asks the founder, as before.
            return Ok(None);
        };
        let agent = realm
            .objectives()
            .get(&origin.objective)
            .ok()
            .and_then(|o| o.bot)
            .unwrap_or_else(|| format!("routine:{}", origin.objective));
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        Ok(Some(Self {
            realm: realm.clone(),
            agent,
            routine: origin.objective.clone(),
            run: state.id.clone(),
            declared: crate::actions::all(&realm.config.integrations, &servers),
            graph: false,
        }))
    }

    /// For the acts of a routine's graph, run by `agent`.
    pub fn for_graph(realm: &Realm, routine: &str, agent: &str, run: &str) -> Result<Self> {
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        Ok(Self {
            realm: realm.clone(),
            agent: agent.to_string(),
            routine: routine.to_string(),
            run: run.to_string(),
            declared: crate::actions::all(&realm.config.integrations, &servers),
            graph: true,
        })
    }

    /// The daily limit and cancellation window declared for `action`.
    fn limits(&self, action: &str) -> (u32, u32) {
        let declared = action.split_once('.').and_then(|(connector, name)| {
            let i = self
                .realm
                .config
                .integrations
                .iter()
                .find(|i| i.name == connector)?;
            i.actions.iter().find(|a| a.name == name)
        });
        (
            declared.and_then(|a| a.daily_limit).unwrap_or(DAILY_LIMIT),
            declared
                .and_then(|a| a.cancel_minutes)
                .unwrap_or(CANCEL_MINUTES),
        )
    }
}

impl Authority for BusinessAuthority {
    fn decide(
        &mut self,
        tool: &str,
        action: &str,
        arguments: &Value,
        call: &str,
    ) -> Result<Decision> {
        // Not declared at all: it commits.
        let (ceiling, start) = self
            .declared
            .iter()
            .find(|d| d.id == action)
            .map_or((1, 1), |d| (d.ceiling, d.start));
        let (daily_limit, window) = self.limits(action);
        let mut store = Store::of(&self.realm)?;
        let level = store
            .level(&self.agent, action, ceiling, start)
            .unwrap_or(1);
        let args = arguments.to_string();
        // The tool to call again with these arguments once the founder
        // approves.
        let source = serde_json::json!({"tool": tool}).to_string();
        let mut queue = |kind: &str, execute_at: Option<DateTime<Utc>>| {
            store.propose(&NewItem {
                kind,
                title: &format!("{}: {action}", self.agent),
                agent: Some(&self.agent),
                routine: Some(&self.routine),
                run: Some(&self.run),
                action: Some(action),
                args: &args,
                execute_at,
                source: &source,
                model_call: None,
            })
        };
        Ok(Decision::Held(match level {
            0 => format!(
                "not done: prepared for the founder, who does it (item {}). Say so in your report.",
                queue("prepared", None)?
            ),
            1 => format!(
                "not done: proposed to the founder for approval (item {}). Say so in your report.",
                queue("approval", None)?
            ),
            // Done by Airth once the window passes, unless the founder
            // cancels it (Hub::release_delayed).
            2 => {
                let at = Utc::now() + chrono::Duration::minutes(window as i64);
                format!(
                    "queued: done in {window} minutes unless the founder cancels it (item {}).",
                    queue("delayed", Some(at))?
                )
            }
            _ => {
                // Limit: in a mission, a run that is resumed and makes the
                // same action again under a new call id gets a new key, so
                // this does not stop it. Graphs key acts by the item's
                // source id instead (decision 49).
                let key = if self.graph {
                    call.to_string()
                } else {
                    format!("{}/{call}", self.run)
                };
                let connector = action.split('.').next().unwrap_or(action);
                let begin = store.begin_action(&NewAction {
                    key: &key,
                    action,
                    connector,
                    agent: Some(&self.agent),
                    routine: Some(&self.routine),
                    run: Some(&self.run),
                    level,
                    approved: false,
                    args: &args,
                    daily_limit,
                })?;
                match begin {
                    Begin::Started(id) => return Ok(Decision::Act(id)),
                    Begin::Duplicate(status) => {
                        format!("not done again: this call was already {status}.")
                    }
                    Begin::LimitReached => format!(
                        "not done: {connector} reached its {daily_limit} actions a day; proposed to the founder instead (item {}).",
                        store.propose(&NewItem {
                            kind: "approval",
                            title: &format!("{}: {action} (daily limit reached)", self.agent),
                            agent: Some(&self.agent),
                            routine: Some(&self.routine),
                            run: Some(&self.run),
                            action: Some(action),
                            args: &args,
                            execute_at: None,
                            source: &source,
                            model_call: None,
                        })?
                    ),
                }
            }
        }))
    }

    fn finish(&mut self, journal: i64, error: Option<&str>) {
        let status = if error.is_some() {
            "failed"
        } else {
            "confirmed"
        };
        if let Err(e) =
            Store::of(&self.realm).and_then(|mut s| s.finish_action(journal, status, error))
        {
            // The journal no longer says what happened: the founder must know.
            let detail =
                format!("Action {journal} ended, but its outcome could not be recorded: {e:#}");
            let incident = super::incidents::Incident {
                title: "An action's outcome was not recorded",
                detail: &detail,
                agent: Some(&self.agent),
                routine: Some(&self.routine),
                action: None,
            };
            if let Err(e) = super::incidents::record(&self.realm, &incident) {
                eprintln!("{detail}; and the incident could not be recorded: {e:#}");
            }
        }
    }
}
