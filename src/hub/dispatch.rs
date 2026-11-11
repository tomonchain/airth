//! The founder's one interlocutor (decision 120): an order given to it is
//! passed on to the agents whose job it is, as a plan the founder approves;
//! when the plan's tasks end, the interlocutor is told, and tells the
//! founder where things stand.

use super::*;
use crate::business::store::Store;
use crate::tools::dispatch::{Dispatch, Step, render};
use serde_json::json;

/// The action of a plan's item in the queue.
pub const DISPATCH: &str = "airth.dispatch";

impl Hub {
    /// The tool a task of the business's interlocutor gets: the other
    /// agents, and the founder's approval to ask for.
    pub(super) fn dispatch_for(&self, realm: &Realm, state: &State) -> Result<Option<Dispatch>> {
        let (Some(agent), None) = (&state.agent, &state.routine) else {
            return Ok(None);
        };
        let Ok(root) = realm.root() else {
            return Ok(None);
        };
        if crate::business::dispatcher(&root).as_deref() != Some(agent.as_str()) {
            return Ok(None);
        }
        let team: Vec<(String, String)> = crate::business::bots(&root)?
            .into_iter()
            .filter(|(name, _)| name != agent)
            .map(|(name, job)| {
                let line = job
                    .text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty() && !l.starts_with('#'))
                    .unwrap_or_default()
                    .chars()
                    .take(160)
                    .collect();
                (name, line)
            })
            .collect();
        if team.is_empty() {
            return Ok(None);
        }
        let (realm, agent, run) = (realm.clone(), agent.clone(), state.id.clone());
        let propose = std::sync::Arc::new(move |steps: &[Step], why: &str| -> Result<String> {
            let text = render(steps, why);
            let plan: Vec<serde_json::Value> = steps
                .iter()
                .map(|s| json!({"agent": s.agent, "task": s.task}))
                .collect();
            let title = match why {
                "" => format!("A plan for {} agents", steps.len()),
                why => why.chars().take(80).collect(),
            };
            let id = Store::of(&realm)?.propose(&crate::business::store::autonomy::NewItem {
                kind: "approval",
                title: &title,
                agent: Some(&agent),
                routine: None,
                run: Some(&run),
                action: Some(DISPATCH),
                args: &json!({"steps": plan, "text": text}).to_string(),
                execute_at: None,
                source: &json!({"show": "text"}).to_string(),
                model_call: None,
            })?;
            Ok(format!(
                "The plan waits for the founder in Needs you (item {id}). Once they approve it, each agent gets its task, and you are told when they end. End your turn now, saying in two lines what you planned."
            ))
        });
        Ok(Some(Dispatch { team, propose }))
    }

    /// The founder approved plan `item`: each agent gets its task, started
    /// as an order of the founder's, within its own scope. What it did.
    pub(super) fn start_plan(
        &self,
        realm: &Realm,
        item: &crate::business::store::queue::Item,
    ) -> Result<String> {
        let from = item.agent.as_deref().unwrap_or("the interlocutor");
        let mark = format!("{}/{}", item.run.as_deref().unwrap_or_default(), item.id);
        let mut started = Vec::new();
        for step in item.args["steps"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let (Some(agent), Some(task)) = (step["agent"].as_str(), step["task"].as_str()) else {
                continue;
            };
            let order = format!(
                "{task}\n\n(Passed on by {from}, from an order of the founder's; the founder approved this plan.)"
            );
            let mut state = self.start_task(realm, agent, &order)?;
            state.dispatched_by = Some(mark.clone());
            state.save()?;
            self.queue_hand_off(realm, &state.id)?;
            started.push(agent.to_string());
            // A run's id is its start time, to the millisecond.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        anyhow::ensure!(!started.is_empty(), "the plan gives no task");
        Ok(format!("tasks started for {}", started.join(", ")))
    }

    /// A task of a plan ended: once all of the plan's have, its interlocutor
    /// is told how each went, once, and tells the founder.
    pub(super) fn dispatched_ended(&self, realm: &Realm, state: &State) -> Result<()> {
        let Some(mark) = state.dispatched_by.as_deref() else {
            return Ok(());
        };
        if state.status == Status::Running {
            return Ok(());
        }
        let plan: Vec<std::sync::Arc<State>> = self
            .load_shared(realm)?
            .into_iter()
            .filter(|s| s.dispatched_by.as_deref() == Some(mark))
            .collect();
        let busy = |s: &State| {
            s.id != state.id && (s.status == Status::Running || self.is_live(realm, &s.id))
        };
        if plan.iter().any(|s| busy(s)) {
            return Ok(());
        }
        // Told once, though two tasks may end together.
        let told = realm.dir().join("plans-told");
        std::fs::create_dir_all(&told)?;
        let file = told.join(mark.replace('/', "-"));
        if std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
            .is_err()
        {
            return Ok(());
        }
        let lines: Vec<String> = plan
            .iter()
            .map(|s| {
                let s = if s.id == state.id { state } else { s.as_ref() };
                let report: String = s
                    .report
                    .as_deref()
                    .unwrap_or("(no report)")
                    .chars()
                    .take(800)
                    .collect();
                format!(
                    "- {} ({}): {}",
                    s.agent.as_deref().unwrap_or("an agent"),
                    s.status.as_str(),
                    report.trim()
                )
            })
            .collect();
        let message = format!(
            "The tasks of your plan ended:\n{}\n\nTell the founder, in a few lines, where things stand: what is done, what waits for them in Needs you, what failed and what you suggest. Do not start anything new without their order.",
            lines.join("\n")
        );
        let interlocutor = mark.split('/').next().unwrap_or_default();
        self.follow_up(realm, interlocutor, &message)?;
        self.queue_hand_off(realm, interlocutor)?;
        Ok(())
    }
}
