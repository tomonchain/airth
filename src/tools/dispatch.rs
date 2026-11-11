//! The founder's interlocutor passes an order on to the team (decision
//! 120). The tool only proposes a plan, one task per agent; the founder
//! approves it in Needs you, and only then does each agent get its task,
//! within its own scope. Nothing of the interlocutor's rights passes on.

use super::{Tool, Workspace};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

/// The most tasks one plan holds.
pub const MAX_STEPS: usize = 6;

/// One agent's part of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub agent: String,
    pub task: String,
}

/// Put the plan to the founder; what it says back to the agent.
pub type Propose = Arc<dyn Fn(&[Step], &str) -> Result<String> + Send + Sync>;

pub struct Dispatch {
    /// The agents a plan may name, with a line on each one's job.
    pub team: Vec<(String, String)>,
    pub propose: Propose,
}

pub struct ProposePlan(pub Arc<Dispatch>);

fn steps(input: &Value, team: &[(String, String)]) -> Result<(Vec<Step>, String)> {
    let why = input["why"].as_str().unwrap_or_default().trim().to_string();
    let mut out = Vec::new();
    for s in input["steps"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let agent = s["agent"].as_str().unwrap_or_default().trim();
        let task = s["task"].as_str().unwrap_or_default().trim();
        if !team.iter().any(|(name, _)| name == agent) {
            bail!("'{agent}' is not one of the agents you may pass work to");
        }
        if task.is_empty() {
            bail!("say what {agent} must do");
        }
        out.push(Step {
            agent: agent.to_string(),
            task: task.to_string(),
        });
    }
    if out.is_empty() {
        bail!("a plan gives at least one task");
    }
    if out.len() > MAX_STEPS {
        bail!("a plan gives at most {MAX_STEPS} tasks: split the order");
    }
    Ok((out, why))
}

/// The plan as the founder reads it, one task a line.
pub fn render(steps: &[Step], why: &str) -> String {
    let lines: Vec<String> = steps
        .iter()
        .map(|s| format!("- {}: {}", s.agent, s.task))
        .collect();
    match why {
        "" => lines.join("\n"),
        why => format!("{why}\n\n{}", lines.join("\n")),
    }
}

impl Tool for ProposePlan {
    fn spec(&self) -> ToolSpec {
        let team: Vec<String> = self
            .0
            .team
            .iter()
            .map(|(name, job)| format!("- {name}: {job}"))
            .collect();
        ToolSpec {
            name: "propose_plan".into(),
            description: format!(
                "Pass the founder's order on to the agents whose job it is: one task per agent, each with everything it must know (it sees nothing of this conversation). The plan goes to the founder in Needs you; once they approve it, each agent works on its task within its own tools and limits, and you are told when they end. After proposing, end your turn, saying in two lines what you planned. Do what is yours yourself; pass on only what another agent's job covers. The agents:\n{}",
                team.join("\n")
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "why": {"type": "string", "description": "The order, in one line, as the founder will read it above the plan"},
                    "steps": {
                        "type": "array",
                        "maxItems": MAX_STEPS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "agent": {"type": "string", "enum": self.0.team.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()},
                                "task": {"type": "string", "description": "What this agent must do and deliver, complete"}
                            },
                            "required": ["agent", "task"]
                        }
                    }
                },
                "required": ["why", "steps"]
            }),
        }
    }

    // It only puts a plan to the founder; nothing starts before they approve.
    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let (steps, why) = steps(input, &self.0.team)?;
        Ok(format!("propose to the founder:\n{}", render(&steps, &why)))
    }

    fn execute(&self, input: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        let (steps, why) = steps(input, &self.0.team)?;
        (self.0.propose)(&steps, &why)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> Vec<(String, String)> {
        vec![
            ("writer".into(), "the blog".into()),
            ("developer".into(), "the site".into()),
        ]
    }

    #[test]
    fn a_plan_names_only_the_team_and_says_what_each_does() {
        let ok = json!({"why": "Year-end", "steps": [{"agent": "writer", "task": "A recap article"}, {"agent": "developer", "task": "A banner"}]});
        let (s, why) = steps(&ok, &team()).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            render(&s, &why),
            "Year-end\n\n- writer: A recap article\n- developer: A banner"
        );
        let stranger = json!({"steps": [{"agent": "chief-of-staff", "task": "x"}]});
        assert!(steps(&stranger, &team()).is_err());
        assert!(
            steps(
                &json!({"steps": [{"agent": "writer", "task": " "}]}),
                &team()
            )
            .is_err()
        );
        assert!(steps(&json!({"steps": []}), &team()).is_err());
        let many: Vec<Value> = (0..7)
            .map(|_| json!({"agent": "writer", "task": "x"}))
            .collect();
        assert!(steps(&json!({"steps": many}), &team()).is_err());
    }
}
