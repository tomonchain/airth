//! The founder's one interlocutor (decision 120): an order given to it is
//! passed on to the agents whose job it is, as a plan the founder approves;
//! when the plan's tasks end, the interlocutor is told, and tells the
//! founder where things stand.

use super::*;
use crate::business::store::Store;
use crate::supervise::Locked;
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
                .map(|s| json!({"agent": s.agent, "task": s.task, "after": s.after}))
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
                // The founder may edit the plan before approving it.
                source: &json!({"show": "text", "editable": "text"}).to_string(),
                model_call: None,
            })?;
            Ok(format!(
                "The plan waits for the founder in Needs you (item {id}); they may edit it. Once they approve it, each agent gets its task when the ones it waits for have finished, and you are told when all have ended. End your turn now, saying in two lines what you planned."
            ))
        });
        Ok(Some(Dispatch { team, propose }))
    }

    /// The founder approved plan `item`, as its text now reads (they may
    /// have edited it): the plan is kept, and each step that waits for
    /// nobody starts, as an order of the founder's, within its agent's
    /// scope. What it did.
    pub(super) fn start_plan(
        &self,
        realm: &Realm,
        item: &crate::business::store::queue::Item,
    ) -> Result<String> {
        let from = item.agent.clone().unwrap_or_default();
        let root = realm.root()?;
        let team: Vec<String> = crate::business::bots(&root)?
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| *name != from)
            .collect();
        let (steps, _) =
            crate::tools::dispatch::parse(item.args["text"].as_str().unwrap_or_default());
        crate::tools::dispatch::check(
            &steps,
            &team.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
        let mark = format!("{}/{}", item.run.as_deref().unwrap_or_default(), item.id);
        let plan = Plan {
            from,
            mark: mark.clone(),
            steps: steps
                .into_iter()
                .map(|s| PlanStep {
                    agent: s.agent,
                    task: s.task,
                    after: s.after,
                    run: None,
                    skipped: None,
                })
                .collect(),
            told: false,
        };
        let _guard = PLANS.locked();
        let mut plan = plan;
        let started = self.advance(realm, &mut plan)?;
        save_plan(realm, &plan)?;
        let waiting = plan.steps.iter().filter(|s| s.run.is_none()).count();
        Ok(match waiting {
            0 => format!("tasks started for {}", started.join(", ")),
            n => format!(
                "tasks started for {}; {n} wait for the ones before them",
                started.join(", ")
            ),
        })
    }

    /// Start every step of `plan` whose waits are over, with what the
    /// steps it waited for delivered; set aside a step whose wait failed.
    /// The agents started.
    fn advance(&self, realm: &Realm, plan: &mut Plan) -> Result<Vec<String>> {
        let mut started = Vec::new();
        for i in 0..plan.steps.len() {
            if plan.steps[i].run.is_some() || plan.steps[i].skipped.is_some() {
                continue;
            }
            // The earlier steps it waits for, by agent.
            let waits: Vec<&PlanStep> = plan.steps[i]
                .after
                .iter()
                .filter_map(|a| plan.steps[..i].iter().rev().find(|e| &e.agent == a))
                .collect();
            let mut done = Vec::new();
            let mut blocked = None;
            let mut pending = false;
            for w in &waits {
                match (&w.run, &w.skipped) {
                    (_, Some(_)) => blocked = Some(format!("{} did not run", w.agent)),
                    (None, None) => pending = true,
                    (Some(run), None) => match self.ended(realm, run) {
                        None => pending = true,
                        Some(state) if state.status == Status::Completed => done.push(format!(
                            "- {}: {}",
                            w.agent,
                            state.report.as_deref().unwrap_or("(no report)").trim()
                        )),
                        Some(state) => {
                            blocked = Some(format!("{} ended {}", w.agent, state.status.as_str()))
                        }
                    },
                }
            }
            if let Some(why) = blocked {
                plan.steps[i].skipped = Some(why);
                continue;
            }
            if pending {
                continue;
            }
            let step = &plan.steps[i];
            let earlier = match done.is_empty() {
                true => String::new(),
                false => format!(
                    "\n\nWhat the agents before you delivered, to build on:\n{}",
                    done.join("\n")
                ),
            };
            let order = format!(
                "{}{earlier}\n\n(Passed on by {}, from an order of the founder's; the founder approved this plan.)",
                step.task, plan.from
            );
            let mut state = self.start_task(realm, &step.agent, &order)?;
            state.dispatched_by = Some(plan.mark.clone());
            state.save()?;
            self.queue_hand_off(realm, &state.id)?;
            let agent = step.agent.clone();
            plan.steps[i].run = Some(state.id.clone());
            started.push(agent);
            // A run's id is its start time, to the millisecond.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(started)
    }

    /// The run `id` once it has ended; None while it runs.
    fn ended(&self, realm: &Realm, id: &str) -> Option<State> {
        let state = self.load_run(realm, id).ok()?;
        (state.status != Status::Running && !self.is_live(realm, id)).then_some(state)
    }

    /// A task of a plan ended: the steps waiting for it start; once every
    /// step has ended or been set aside, the interlocutor is told how each
    /// went, once, and tells the founder.
    pub(super) fn dispatched_ended(&self, realm: &Realm, state: &State) -> Result<()> {
        let Some(mark) = state.dispatched_by.as_deref() else {
            return Ok(());
        };
        if state.status == Status::Running {
            return Ok(());
        }
        let _guard = PLANS.locked();
        let Some(mut plan) = load_plan(realm, mark) else {
            return Ok(());
        };
        self.advance(realm, &mut plan)?;
        let over = plan.steps.iter().all(|s| {
            s.skipped.is_some()
                || s.run
                    .as_deref()
                    .is_some_and(|r| r == state.id || self.ended(realm, r).is_some())
        });
        if !over || plan.told {
            save_plan(realm, &plan)?;
            return Ok(());
        }
        plan.told = true;
        save_plan(realm, &plan)?;
        let lines: Vec<String> = plan
            .steps
            .iter()
            .map(|s| match (&s.run, &s.skipped) {
                (_, Some(why)) => format!("- {} (not started): {why}", s.agent),
                (Some(run), None) => {
                    let ran = if *run == state.id {
                        Some(state.clone())
                    } else {
                        self.load_run(realm, run).ok()
                    };
                    let (status, report) = ran.map_or(("unknown", String::new()), |r| {
                        (r.status.as_str(), r.report.unwrap_or_default())
                    });
                    let report: String = report.chars().take(800).collect();
                    format!("- {} ({status}): {}", s.agent, report.trim())
                }
                (None, None) => format!("- {} (not started)", s.agent),
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

/// One read-modify-write of a plan at a time.
static PLANS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A plan the founder approved, kept in the realm's directory until its
/// last step ends.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Plan {
    from: String,
    mark: String,
    steps: Vec<PlanStep>,
    told: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PlanStep {
    agent: String,
    task: String,
    after: Vec<String>,
    /// Its task, once started.
    run: Option<String>,
    /// Why it never started: a step it waited for failed.
    skipped: Option<String>,
}

fn plan_path(realm: &Realm, mark: &str) -> std::path::PathBuf {
    realm
        .dir()
        .join("plans")
        .join(format!("{}.json", mark.replace('/', "-")))
}

fn load_plan(realm: &Realm, mark: &str) -> Option<Plan> {
    serde_json::from_str(&std::fs::read_to_string(plan_path(realm, mark)).ok()?).ok()
}

fn save_plan(realm: &Realm, plan: &Plan) -> Result<()> {
    let path = plan_path(realm, &plan.mark);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::realm::write_private(&path, &serde_json::to_string_pretty(plan)?)
}
