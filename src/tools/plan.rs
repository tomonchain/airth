use super::{Tool, Workspace};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use crate::state::{PlanStep, render_plan};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const MAX_STEPS: usize = 50;
const MAX_TITLE: usize = 300;
const MAX_NOTE: usize = 1000;

/// The plan of the run in progress, shared with the agent loop, which saves
/// it with the run.
pub type PlanHandle = Arc<Mutex<Vec<PlanStep>>>;

/// Replace the run's plan. Bookkeeping only: it changes nothing outside the
/// run, so it needs no approval.
pub struct UpdatePlan {
    pub plan: PlanHandle,
}

impl UpdatePlan {
    fn parse(input: &Value) -> Result<Vec<PlanStep>> {
        let steps: Vec<PlanStep> = serde_json::from_value(input["steps"].clone())
            .context("'steps' must be a list of {title, status, note?}")?;
        if steps.len() > MAX_STEPS {
            bail!("a plan has at most {MAX_STEPS} steps");
        }
        for s in &steps {
            if s.title.trim().is_empty() || s.title.chars().count() > MAX_TITLE {
                bail!("step titles must be 1 to {MAX_TITLE} characters");
            }
            if s.note
                .as_ref()
                .is_some_and(|n| n.chars().count() > MAX_NOTE)
            {
                bail!("step notes are at most {MAX_NOTE} characters");
            }
        }
        Ok(steps)
    }
}

impl Tool for UpdatePlan {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "update_plan".into(),
            description: "Set the plan for this run: the full, ordered list of steps with their status. Send the whole list every time (it replaces the previous one). Use it for any objective with more than a couple of steps: make the plan before acting, mark a step in_progress when you start it and done when it is verified, and add, split or skip steps as you learn. The owner follows your progress here, and if the run is interrupted you resume from this plan. Needs no approval.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string", "description": "What the step achieves, in a few words"},
                                "status": {"type": "string", "enum": ["pending", "in_progress", "done", "blocked", "skipped"]},
                                "note": {"type": "string", "description": "Optional: outcome, evidence, or why it is blocked or skipped"}
                            },
                            "required": ["title", "status"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        Ok(render_plan(&Self::parse(input)?))
    }

    fn execute(&self, input: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        let steps = Self::parse(input)?;
        let text = render_plan(&steps);
        *self.plan.lock().unwrap() = steps;
        Ok(format!("Plan updated:\n{text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_plan() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let tool = UpdatePlan {
            plan: PlanHandle::default(),
        };
        let out = tool
            .execute(
                &json!({"steps": [
                    {"title": "Fetch issues", "status": "done", "note": "3 open"},
                    {"title": "Write summary", "status": "in_progress"}
                ]}),
                &ws,
                &CancelToken::default(),
            )
            .unwrap();
        assert!(out.contains("[x] 1. Fetch issues — 3 open"), "{out}");
        assert!(out.contains("[~] 2. Write summary"), "{out}");
        assert_eq!(tool.plan.lock().unwrap().len(), 2);

        let bad = json!({"steps": [{"title": "", "status": "done"}]});
        assert!(tool.execute(&bad, &ws, &CancelToken::default()).is_err());
        let bad = json!({"steps": [{"title": "x", "status": "maybe"}]});
        assert!(tool.execute(&bad, &ws, &CancelToken::default()).is_err());
        assert_eq!(tool.plan.lock().unwrap().len(), 2);
    }
}
