use super::{Tool, Workspace, run_shell_with_input, str_arg, truncate};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::notify::{Inbox, Kind};
use crate::permissions::Capability;
use crate::skill::{self, Manifest, SkillRecord, SkillStatus, SkillStore};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

const MAX_OUTPUT: usize = 50_000;

/// A skill the owner installed, as a tool.
pub struct SkillTool {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

impl Tool for SkillTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.manifest.name.clone(),
            description: format!(
                "{} (Skill installed by the owner.)",
                self.manifest.description
            ),
            input_schema: self.manifest.input_schema.clone(),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Skill
    }

    fn scope(&self, _: &Value) -> Option<String> {
        Some(self.manifest.name.clone())
    }

    fn preview(&self, input: &Value, ws: &Workspace) -> Result<String> {
        Ok(format!(
            "skill {} (in {})\n{}",
            self.manifest.name,
            ws.root().display(),
            serde_json::to_string_pretty(input)?
        ))
    }

    fn execute(&self, input: &Value, ws: &Workspace, cancel: &CancelToken) -> Result<String> {
        let ws = ws
            .clone()
            .with_env("AIRTH_SKILL_DIR", &self.dir.display().to_string());
        let timeout = Duration::from_secs(self.manifest.timeout_secs);
        let out = run_shell_with_input(
            &self.manifest.run,
            &ws,
            cancel,
            timeout,
            Some(&serde_json::to_string(input)?),
        )?;
        if !out.success() {
            bail!(
                "skill {} failed ({})\n{}",
                self.manifest.name,
                out.code
                    .map_or("killed or timed out".into(), |c| format!("exit code {c}")),
                out.streams()
            );
        }
        let mut result = truncate(out.stdout, MAX_OUTPUT);
        if !out.stderr.trim().is_empty() {
            result.push_str(&format!("\n--- stderr ---\n{}", truncate(out.stderr, 5000)));
        }
        Ok(result)
    }
}

/// Record a capability the run lacks, for the owner to have it developed.
pub struct ProposeSkill {
    pub store: SkillStore,
    pub source: String,
    pub inbox: Inbox,
    pub realm: String,
}

impl Tool for ProposeSkill {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "propose_skill".into(),
            description: "Propose a new skill: a small, tested program that would become a tool for future runs in this realm. Use it when the objective needs a capability you lack (or keep re-improvising with commands) and it would be worth having for good, e.g. parsing a file format, querying a service the same way, a recurring calculation. Proposing builds nothing: the owner decides whether Airth develops it, reviews the result and its tests, and installs it. Carry on with the objective as best you can meanwhile.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Tool name: lowercase letters, digits, '_' (e.g. ical_events)"},
                    "description": {"type": "string", "description": "What the skill does, in one or two sentences"},
                    "reason": {"type": "string", "description": "The limitation you hit, and why a skill beats improvising"},
                    "spec": {"type": "string", "description": "Inputs, outputs, edge cases and how to test it"}
                },
                "required": ["name", "description", "reason", "spec"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        Ok(format!(
            "propose skill {}: {}",
            str_arg(input, "name")?,
            str_arg(input, "description")?
        ))
    }

    fn execute(&self, input: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        let name = str_arg(input, "name")?.trim().to_string();
        skill::validate_name(&name)?;
        let text = |k: &str| -> Result<String> {
            Ok(str_arg(input, k)?.trim().chars().take(4000).collect())
        };
        let (record, new) = self.store.propose(SkillRecord {
            name: name.clone(),
            status: SkillStatus::Proposed,
            description: text("description")?,
            reason: text("reason")?,
            spec: text("spec")?,
            proposed_by: self.source.clone(),
            created_at: chrono::Utc::now(),
            dev_run: None,
            evaluation: None,
            installed: None,
        })?;
        if !new {
            return Ok(format!(
                "A skill named {name} already exists ({}). Pick another name if you need something different.",
                record.status.as_str()
            ));
        }
        let _ = self.inbox.push(
            Kind::Review,
            &self.realm,
            self.source.strip_prefix("run:"),
            &format!("Airth proposes a new skill: {name}"),
            &record.description,
        );
        Ok(format!(
            "Proposed skill {name}. The owner decides whether to develop it; it is not available in this run. Continue without it."
        ))
    }
}

/// Run the evaluation of the skill being developed, as the runtime will.
pub struct TestSkill {
    pub name: String,
    pub scratch: PathBuf,
    /// Folders the tests must not reach.
    pub deny: Vec<PathBuf>,
}

impl Tool for TestSkill {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "test_skill".into(),
            description: "Evaluate the skill in your workspace exactly as the runtime will before the owner can install it: check skill.json and tests.json, then run every test in the sandbox. Returns each problem and failed test.".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, _: &Value, _: &Workspace) -> Result<String> {
        Ok(format!("evaluate skill {}", self.name))
    }

    fn execute(&self, _: &Value, ws: &Workspace, cancel: &CancelToken) -> Result<String> {
        let eval = skill::evaluate(ws.root(), &self.name, &self.scratch, &self.deny, cancel)?;
        Ok(eval.report())
    }
}
