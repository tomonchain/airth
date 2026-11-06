//! A routine's act that launches a mission (decision 49): the routine's
//! agent works the brief as a task, unwatched, and the routine goes on with
//! its report once it completes.

use super::*;
use crate::business::store::Store;
use crate::business::store::autonomy::{Begin, NewAction, NewItem};
use crate::pipeline::run::Item;
use crate::routine::run::LAUNCH_MISSION;
use crate::state::RoutineWait;
use serde_json::{Value, json};

/// What an act hands to a mission.
pub(super) struct Launch<'a> {
    pub agent: &'a str,
    pub routine: &'a str,
    pub run: &'a str,
    pub block: &'a str,
    pub key: &'a str,
    pub brief: &'a str,
    pub item: &'a Item,
}

impl Hub {
    /// Launch the mission at the agent's level for `airth.launch_mission`:
    /// below 3, and not approved, the founder is asked first. Once per key.
    pub(super) fn launch_for_routine(
        &self,
        realm: &Realm,
        launch: &Launch,
        approved: bool,
        declared: &[crate::actions::Declared],
    ) -> Result<String> {
        if launch.brief.trim().is_empty() {
            bail!("{LAUNCH_MISSION} needs a brief");
        }
        let (ceiling, start) = declared
            .iter()
            .find(|d| d.id == LAUNCH_MISSION)
            .map_or((1, 1), |d| (d.ceiling, d.start));
        let mut store = Store::of(realm)?;
        let level = if approved {
            3
        } else {
            store
                .level(launch.agent, LAUNCH_MISSION, ceiling, start)
                .unwrap_or(1)
        };
        if level < 3 {
            // Level 0 cannot be done by the founder's hand: it is asked as 1.
            let (kind, at) = match level {
                2 => (
                    "delayed",
                    Some(
                        Utc::now()
                            + chrono::Duration::minutes(super::autonomy::CANCEL_MINUTES.into()),
                    ),
                ),
                _ => ("approval", None),
            };
            let args = json!({
                "brief": launch.brief,
                "block": launch.block,
                "key": launch.key,
                "item": launch.item,
            });
            let id = store.propose(&NewItem {
                kind,
                title: &format!(
                    "{}: start a mission: {}",
                    launch.agent,
                    first_line(launch.brief, 80)
                ),
                agent: Some(launch.agent),
                routine: Some(launch.routine),
                run: Some(launch.run),
                action: Some(LAUNCH_MISSION),
                args: &args.to_string(),
                execute_at: at,
                // The founder reads the brief, not the routine's whole item.
                source: &json!({"tool": LAUNCH_MISSION, "show": "brief"}).to_string(),
                model_call: None,
            })?;
            bail!("not done: proposed to the founder (item {id})");
        }
        let begin = store.begin_action(&NewAction {
            key: launch.key,
            action: LAUNCH_MISSION,
            connector: "airth",
            agent: Some(launch.agent),
            routine: Some(launch.routine),
            run: Some(launch.run),
            item: None,
            level,
            approved,
            args: &json!({"brief": launch.brief}).to_string(),
            daily_limit: u32::MAX,
        })?;
        let Begin::Started(journal) = begin else {
            bail!(
                "not launched again: this mission was already started under {}",
                launch.key
            );
        };
        drop(store);
        let started = self.start_mission(realm, launch);
        let mut store = Store::of(realm)?;
        match started {
            Ok(id) => {
                store.finish_action(journal, "confirmed", None)?;
                Ok(format!(
                    "mission {id} started; the routine goes on with its report"
                ))
            }
            Err(e) => {
                store.finish_action(journal, "failed", Some(&format!("{e:#}")))?;
                Err(e)
            }
        }
    }

    /// The task, saved with where its routine waits, and left for the app
    /// to drive.
    fn start_mission(&self, realm: &Realm, launch: &Launch) -> Result<String> {
        let mut state = self.start_task(realm, launch.agent, launch.brief)?;
        state.routine = Some(RoutineWait {
            routine: launch.routine.to_string(),
            run: launch.run.to_string(),
            block: launch.block.to_string(),
            item: launch.item.clone(),
            resumed: false,
        });
        state.save()?;
        self.queue_hand_off(realm, &state.id)?;
        Ok(state.id)
    }

    /// The founder approved a mission a routine proposed (or its delay
    /// passed): launched now, once for that item.
    pub(super) fn launch_approved(
        &self,
        realm: &Realm,
        item: &crate::business::store::queue::Item,
    ) -> Result<String> {
        let args = &item.args;
        let text = |k: &str| args[k].as_str().unwrap_or_default().to_string();
        let Value::Object(fields) = args["item"].clone() else {
            bail!("item {} does not hold the routine's item", item.id);
        };
        let (agent, routine, run) = (
            item.agent.as_deref().context("the item names no agent")?,
            item.routine
                .as_deref()
                .context("the item names no routine")?,
            item.run.as_deref().context("the item names no run")?,
        );
        let (brief, block, key) = (text("brief"), text("block"), text("key"));
        let launch = Launch {
            agent,
            routine,
            run,
            block: &block,
            key: &key,
            brief: &brief,
            item: &fields,
        };
        let declared = crate::actions::all(&realm.config.integrations, &[]);
        self.launch_for_routine(realm, &launch, true, &declared)
    }

    /// A mission a routine launched completed: the routine goes on after
    /// the act, with the mission's report as the item's `result`. Once.
    pub(super) fn mission_ended(&self, realm: &Realm, state: &mut State) -> Result<()> {
        let Some(wait) = state.routine.as_mut() else {
            return Ok(());
        };
        if wait.resumed || state.status != Status::Completed {
            return Ok(());
        }
        wait.resumed = true;
        let mut item = wait.item.clone();
        let (routine, run, block) = (wait.routine.clone(), wait.run.clone(), wait.block.clone());
        state.save()?;
        item.insert(
            "result".into(),
            json!({"report": state.report.clone().unwrap_or_default(), "mission": state.id}),
        );
        let start = crate::routine::run::Start::Mission { block, item };
        self.run_routine(realm, &routine, &run, start, None)?;
        Ok(())
    }
}
