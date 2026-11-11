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
    /// The agents of earlier steps this one waits for: it starts once they
    /// have finished, with what they delivered.
    pub after: Vec<String>,
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
        out.push(Step {
            agent: s["agent"].as_str().unwrap_or_default().trim().to_string(),
            task: s["task"].as_str().unwrap_or_default().trim().to_string(),
            after: s["after"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
        });
    }
    let names: Vec<&str> = team.iter().map(|(n, _)| n.as_str()).collect();
    check(&out, &names)?;
    Ok((out, why))
}

/// A plan the founder can run: agents of the team, a task each, at most
/// [`MAX_STEPS`], and each step waiting only for agents of earlier steps.
pub fn check(steps: &[Step], team: &[&str]) -> Result<()> {
    if steps.is_empty() {
        bail!("a plan gives at least one task");
    }
    if steps.len() > MAX_STEPS {
        bail!("a plan gives at most {MAX_STEPS} tasks: split the order");
    }
    for (i, s) in steps.iter().enumerate() {
        if !team.contains(&s.agent.as_str()) {
            bail!(
                "'{}' is not one of the agents work can be passed to",
                s.agent
            );
        }
        if s.task.is_empty() {
            bail!("say what {} must do", s.agent);
        }
        for a in &s.after {
            if !steps[..i].iter().any(|e| &e.agent == a) {
                bail!(
                    "{} can only wait for an agent of an earlier step, not '{a}'",
                    s.agent
                );
            }
        }
    }
    Ok(())
}

/// The plan back from its text, as the founder may have edited it: a step
/// a line, `- agent: task` or `- agent (after a, b): task`; a line that is
/// not a step goes on the task above it; what comes before the first step
/// is the order.
pub fn parse(text: &str) -> (Vec<Step>, String) {
    let mut steps: Vec<Step> = Vec::new();
    let mut why = Vec::new();
    for line in text.lines() {
        let head = line.trim_start().strip_prefix("- ").and_then(|rest| {
            let (who, task) = rest.split_once(':')?;
            let (agent, after) = match who.split_once('(') {
                Some((agent, waits)) => {
                    let waits = waits.trim().trim_end_matches(')').trim();
                    let names = waits.strip_prefix("after").unwrap_or(waits);
                    (
                        agent.trim(),
                        names
                            .split(',')
                            .map(|a| a.trim().to_string())
                            .filter(|a| !a.is_empty())
                            .collect(),
                    )
                }
                None => (who.trim(), Vec::new()),
            };
            let simple = !agent.is_empty()
                && agent
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            simple.then(|| Step {
                agent: agent.to_string(),
                task: task.trim().to_string(),
                after,
            })
        });
        match (head, steps.last_mut()) {
            (Some(step), _) => steps.push(step),
            (None, Some(last)) if !line.trim().is_empty() => {
                last.task = format!("{}\n{}", last.task, line.trim());
            }
            (None, None) if !line.trim().is_empty() => why.push(line.trim().to_string()),
            _ => {}
        }
    }
    (steps, why.join(" "))
}

/// The plan as the founder reads it, one task a line.
pub fn render(steps: &[Step], why: &str) -> String {
    let lines: Vec<String> = steps
        .iter()
        .map(|s| match s.after.as_slice() {
            [] => format!("- {}: {}", s.agent, s.task),
            after => format!("- {} (after {}): {}", s.agent, after.join(", "), s.task),
        })
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
                "Pass the founder's order on to the agents whose job it is: one task per agent, each with everything it must know (it sees nothing of this conversation). A step that needs another's result (a thread linking to an article) names it in `after`: it starts once that one has finished, with what it delivered. The plan goes to the founder in Needs you, who may edit it; once they approve it, each agent works on its task within its own tools and limits, and you are told when all have ended. After proposing, end your turn, saying in two lines what you planned. Do what is yours yourself; pass on only what another agent's job covers. The agents:\n{}",
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
                                "task": {"type": "string", "description": "What this agent must do and deliver, complete"},
                                "after": {"type": "array", "items": {"type": "string"}, "description": "Agents of earlier steps whose result this one needs; empty when it can start at once"}
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
        let ok = json!({"why": "Year-end", "steps": [{"agent": "writer", "task": "A recap article"}, {"agent": "developer", "task": "A banner", "after": ["writer"]}]});
        let (s, why) = steps(&ok, &team()).unwrap();
        assert_eq!(s.len(), 2);
        let text = render(&s, &why);
        assert_eq!(
            text,
            "Year-end\n\n- writer: A recap article\n- developer (after writer): A banner"
        );
        // The text reads back as the same plan.
        assert_eq!(parse(&text), (s, why));
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
        // A step waits only for an earlier one: never itself, a later one or a stranger.
        let late = json!({"steps": [{"agent": "writer", "task": "x", "after": ["developer"]}, {"agent": "developer", "task": "y"}]});
        assert!(steps(&late, &team()).is_err());
    }

    #[test]
    fn the_founders_edit_of_a_plan_is_read_back() {
        let edited = "Year-end\n\n- writer: A recap article\n  of 800 words\n\n- developer (after writer, ): A banner";
        let (s, why) = parse(edited);
        assert_eq!(why, "Year-end");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].task, "A recap article\nof 800 words");
        assert_eq!(s[1].after, ["writer"]);
        // A line removed is a step removed.
        let (s, _) = parse("Year-end\n\n- developer: A banner");
        assert_eq!(s.len(), 1);
        check(&s, &["writer", "developer"]).unwrap();
    }
}
