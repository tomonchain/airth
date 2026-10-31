//! Asking another routine of the business for work (decision 41). The tools
//! only describe the request; the hub checks it is allowed and starts the
//! other routine's run, with that routine's own limits.

use super::{Tool, Workspace};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

/// A routine this run may ask, as the agent sees it.
#[derive(Debug, Clone)]
pub struct Target {
    pub routine: String,
    pub agent: Option<String>,
    pub task: String,
}

/// Start `routine` with `brief`; with `wait`, until it ends, returning its
/// report; without, returning at once.
pub type Delegate = Arc<dyn Fn(&str, &str, bool, &CancelToken) -> Result<String> + Send + Sync>;

pub struct Delegation {
    pub targets: Vec<Target>,
    pub run: Delegate,
}

fn listing(targets: &[Target]) -> String {
    targets
        .iter()
        .map(|t| match &t.agent {
            Some(agent) => format!("- {} ({agent}): {}", t.routine, t.task),
            None => format!("- {}: {}", t.routine, t.task),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn request(input: &Value, targets: &[Target]) -> Result<(String, String)> {
    let routine = input["routine"].as_str().unwrap_or_default().trim();
    let brief = input["brief"].as_str().unwrap_or_default().trim();
    if !targets.iter().any(|t| t.routine == routine) {
        bail!(
            "you may not ask '{routine}'; you may ask:\n{}",
            listing(targets)
        );
    }
    if brief.is_empty() {
        bail!("say what you need in the brief");
    }
    Ok((routine.to_string(), brief.to_string()))
}

fn schema(targets: &[Target]) -> Value {
    json!({
        "type": "object",
        "properties": {
            "routine": {"type": "string", "enum": targets.iter().map(|t| t.routine.clone()).collect::<Vec<_>>()},
            "brief": {"type": "string", "description": "What you need, with everything the other agent must know: it sees nothing of your run"}
        },
        "required": ["routine", "brief"]
    })
}

pub struct HandOff(pub Arc<Delegation>);
pub struct AskAgent(pub Arc<Delegation>);

impl Tool for HandOff {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "hand_off".into(),
            description: format!(
                "Hand work over to another routine of the business, and carry on without waiting: it starts at once with your brief, within its own tools and limits, and the owner sees it in Tasks. You may hand off to:\n{}",
                listing(&self.0.targets)
            ),
            input_schema: schema(&self.0.targets),
        }
    }

    // The owner allowed these routines on this one; each runs within its own
    // limits, so the request itself reaches nothing.
    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let (routine, brief) = request(input, &self.0.targets)?;
        Ok(format!("hand off to {routine}: {brief}"))
    }

    fn execute(&self, input: &Value, _: &Workspace, cancel: &CancelToken) -> Result<String> {
        let (routine, brief) = request(input, &self.0.targets)?;
        (self.0.run)(&routine, &brief, false, cancel)
    }
}

impl Tool for AskAgent {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "ask_agent".into(),
            description: format!(
                "Ask another routine of the business for something and wait for its report (it may take minutes). It works within its own tools and limits; what needs the owner is left out and listed in its report. You may ask:\n{}",
                listing(&self.0.targets)
            ),
            input_schema: schema(&self.0.targets),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let (routine, brief) = request(input, &self.0.targets)?;
        Ok(format!("ask {routine}: {brief}"))
    }

    fn execute(&self, input: &Value, _: &Workspace, cancel: &CancelToken) -> Result<String> {
        let (routine, brief) = request(input, &self.0.targets)?;
        (self.0.run)(&routine, &brief, true, cancel)
    }
}
