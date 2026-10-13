use super::{Tool, Workspace, str_arg};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::northstar::{NorthStarStore, SCOPE, Status};
use crate::permissions::Capability;
use anyhow::{Result, bail};
use serde_json::{Value, json};

/// Record the milestones toward the realm's North Star, and progress on them.
pub struct NorthStarTool {
    pub store: NorthStarStore,
    /// The run using it, credited with the progress it records.
    pub run: Option<String>,
}

fn milestones(input: &Value) -> Result<Vec<(String, String)>> {
    let Some(list) = input["milestones"].as_array() else {
        bail!("set_milestones needs 'milestones': [{{title, note}}]");
    };
    Ok(list
        .iter()
        .map(|m| match m {
            Value::String(title) => (title.clone(), String::new()),
            _ => (
                m["title"].as_str().unwrap_or_default().to_string(),
                m["note"].as_str().unwrap_or_default().to_string(),
            ),
        })
        .collect())
}

fn status(input: &Value) -> Result<Option<Status>> {
    input["status"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(Status::parse)
        .transpose()
}

impl Tool for NorthStarTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "north_star".into(),
            description: "The milestones toward this realm's North Star (the goal the owner set, shown in your briefing). action=set_milestones: record the agreed milestones, in order (replaces the list; milestones with an existing title keep their id and progress); only after brainstorming them with the owner and getting their agreement. action=update: record progress on one milestone (`milestone` id, optional `status` todo|doing|done|dropped, one-line `note`); mark done only once its outcome is verified. The owner may need to approve; only the owner changes the goal itself.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["set_milestones", "update"]},
                    "milestones": {
                        "type": "array",
                        "description": "For set_milestones: concrete, verifiable outcomes, in order",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string"},
                                "note": {"type": "string", "description": "How you will know it is reached"}
                            },
                            "required": ["title"]
                        }
                    },
                    "milestone": {"type": "string", "description": "For update: the milestone id, e.g. m2"},
                    "status": {"type": "string", "enum": ["todo", "doing", "done", "dropped"]},
                    "note": {"type": "string", "description": "For update: what moved, in one line"}
                },
                "required": ["action"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Memory
    }

    fn scope(&self, _: &Value) -> Option<String> {
        Some(SCOPE.into())
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let goal = self
            .store
            .get()?
            .map(|s| s.goal)
            .unwrap_or_else(|| "(no North Star set)".into());
        Ok(match str_arg(input, "action")? {
            "set_milestones" => {
                let mut out = format!("North Star: {goal}\nMilestones:\n");
                for (i, (title, note)) in milestones(input)?.iter().enumerate() {
                    out.push_str(&format!("{}. {title}", i + 1));
                    if !note.trim().is_empty() {
                        out.push_str(&format!(" — {}", note.trim()));
                    }
                    out.push('\n');
                }
                out
            }
            "update" => {
                let id = str_arg(input, "milestone")?;
                let title = self
                    .store
                    .get()?
                    .and_then(|s| s.milestones.into_iter().find(|m| m.id == id))
                    .map(|m| m.title)
                    .unwrap_or_else(|| "?".into());
                let mut out = format!("North Star milestone {id}: {title}");
                if let Some(status) = status(input)? {
                    out.push_str(&format!("\nstatus → {}", status.as_str()));
                }
                if let Some(note) = input["note"].as_str().filter(|n| !n.trim().is_empty()) {
                    out.push_str(&format!("\nnote: {}", note.trim()));
                }
                out
            }
            other => bail!("unknown action '{other}' (set_milestones, update)"),
        })
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let text = input.to_string();
        if ws.redact(&text) != text {
            bail!("not saved: the text contains a secret");
        }
        let run = self.run.as_deref();
        let star = match str_arg(input, "action")? {
            "set_milestones" => self.store.set_milestones(&milestones(input)?, run)?,
            "update" => self.store.update(
                str_arg(input, "milestone")?,
                status(input)?,
                input["note"].as_str(),
                run,
            )?,
            other => bail!("unknown action '{other}' (set_milestones, update)"),
        };
        let (done, total) = star.progress();
        Ok(format!(
            "Recorded. North Star: {done} of {total} milestones done.\n{}",
            star.render_milestones()
        ))
    }
}
