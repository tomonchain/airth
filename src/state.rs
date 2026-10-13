//! Persistent execution state of a run, saved as JSON in its realm's `runs/`.

use crate::agent::Budget;
use crate::isolation::Change;
use crate::models::{ContentBlock, Message, Role};
use crate::permissions::Grant;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Completed,
    Failed,
    Stopped,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Completed => "completed",
            Status::Failed => "failed",
            Status::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Ok,
    Error,
    Denied,
}

/// One tool invocation, as recorded for observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub arguments: Value,
    pub result: String,
    pub status: EventStatus,
    /// The model's id for the call that produced this event.
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    Skipped,
}

impl StepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StepStatus::Pending => "pending",
            StepStatus::InProgress => "in_progress",
            StepStatus::Done => "done",
            StepStatus::Blocked => "blocked",
            StepStatus::Skipped => "skipped",
        }
    }

    fn mark(self) -> &'static str {
        match self {
            StepStatus::Pending => "[ ]",
            StepStatus::InProgress => "[~]",
            StepStatus::Done => "[x]",
            StepStatus::Blocked => "[!]",
            StepStatus::Skipped => "[-]",
        }
    }

    /// Nothing left to do for this step.
    pub fn is_closed(self) -> bool {
        matches!(self, StepStatus::Done | StepStatus::Skipped)
    }
}

/// One step of a run's plan, kept by the agent with the `update_plan` tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub title: String,
    pub status: StepStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The plan as text, one step per line: `[x] title — note`.
pub fn render_plan(plan: &[PlanStep]) -> String {
    plan.iter()
        .enumerate()
        .map(|(i, s)| {
            let note = s
                .note
                .as_deref()
                .map(|n| format!(" — {n}"))
                .unwrap_or_default();
            format!("{} {}. {}{note}", s.status.mark(), i + 1, s.title)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What started a run that the owner did not start by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// The id of the realm's objective.
    pub objective: String,
    /// What fired it, e.g. "schedule: daily 08:00".
    pub trigger: String,
}

/// One model call's token usage, as the provider reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub at: DateTime<Utc>,
    pub model: String,
    /// All input tokens, cached ones included.
    pub input: u64,
    pub output: u64,
    /// Of `input`: read from the prompt cache.
    #[serde(default)]
    pub cache_read: u64,
    /// Of `input`: written to the prompt cache.
    #[serde(default)]
    pub cache_write: u64,
}

/// A long conversation replaced, for the model, by the model's own summary
/// of it. The messages stay in the run, for the owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compaction {
    /// The model sees the summary, then `messages[upto..]`.
    pub upto: usize,
    /// The message that stands for `messages[..upto]`.
    pub summary: String,
    /// How many times the run was compacted.
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub id: String,
    pub task: String,
    pub objective: String,
    pub status: Status,
    pub iterations: u32,
    pub messages: Vec<Message>,
    pub events: Vec<Event>,
    pub report: Option<String>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    /// Every model call's usage, in order (runs from before this was
    /// recorded only have the totals above).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage: Vec<Usage>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    /// The realm the run belongs to.
    #[serde(default)]
    pub realm: String,
    /// The run's isolated change, when its realm uses worktrees.
    #[serde(default)]
    pub change: Option<Change>,
    /// The agent's plan for the objective, as it last updated it.
    #[serde(default)]
    pub plan: Vec<PlanStep>,
    /// Standing approvals the owner gave during this run, or in advance for
    /// an objective's runs.
    #[serde(default)]
    pub grants: Vec<Grant>,
    /// Limits on every drive of this run, on top of the client's.
    #[serde(default, skip_serializing_if = "Budget::is_empty")]
    pub budget: Budget,
    /// The objective that started this run, when it started on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    /// For a run developing a skill: the skill's name. Its workspace is the
    /// skill's draft folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    /// Which model connection the run uses, when the realm has a fast one
    /// (see models/router.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<crate::models::router::Tier>,
    /// The model the owner picked for this run (`provider/model`), used
    /// instead of the realm's routing; None: the realm decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_choice: Option<String>,
    /// The choice ("auto" or `provider/model`) of the last drive, to notice
    /// when the run moves to another model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_by: Option<String>,
    /// The run works toward the realm's North Star (brainstorming it, or
    /// started to advance it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub north_star: bool,
    /// What the run was told about its realm when it started. Every drive
    /// sends it unchanged, so the provider's cache of the conversation stays
    /// valid; later changes are sent as a message (`context_update`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// The latest context sent as a message, if it changed since the start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_update: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<Compaction>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl State {
    pub fn new(objective: &str, runs_dir: Option<&Path>) -> Self {
        let id = Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        let path = runs_dir.map(|d| d.join(format!("{id}.json")));
        Self {
            id,
            task: objective.to_string(),
            objective: objective.to_string(),
            status: Status::Running,
            iterations: 0,
            messages: Vec::new(),
            events: Vec::new(),
            report: None,
            input_tokens: 0,
            output_tokens: 0,
            usage: Vec::new(),
            updated_at: Utc::now(),
            realm: String::new(),
            change: None,
            plan: Vec::new(),
            grants: Vec::new(),
            budget: Budget::default(),
            origin: None,
            skill: None,
            tier: None,
            context: None,
            context_update: None,
            compaction: None,
            model_choice: None,
            answered_by: None,
            north_star: false,
            path,
        }
    }

    /// Load a saved run; later saves go back to the same file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading run {}", path.display()))?;
        let mut state: State = serde_json::from_str(&text)
            .with_context(|| format!("parsing run {}", path.display()))?;
        state.path = Some(path.to_path_buf());
        Ok(state)
    }

    /// After an interruption the last assistant turn may hold tool calls with no
    /// results. Answer each one, from the recorded event if the tool did run, so
    /// the conversation is valid to continue.
    /// The conversation as the model sees it: after a compaction, the
    /// summary and what came after it.
    pub fn model_messages(&self) -> std::borrow::Cow<'_, [Message]> {
        match &self.compaction {
            Some(c) => {
                let mut out = vec![Message::user_text(&c.summary)];
                out.extend_from_slice(&self.messages[c.upto.min(self.messages.len())..]);
                std::borrow::Cow::Owned(out)
            }
            None => std::borrow::Cow::Borrowed(&self.messages),
        }
    }

    /// How many messages the model sees.
    pub fn model_len(&self) -> usize {
        match &self.compaction {
            Some(c) => 1 + self.messages.len().saturating_sub(c.upto),
            None => self.messages.len(),
        }
    }

    pub fn close_dangling_tool_calls(&mut self) {
        let Some(last) = self.messages.last() else {
            return;
        };
        if last.role != Role::Assistant {
            return;
        }
        let results: Vec<ContentBlock> = last
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, .. } => Some(id),
                _ => None,
            })
            .map(|id| {
                let event = self
                    .events
                    .iter()
                    .find(|e| e.tool_use_id.as_deref() == Some(id.as_str()));
                match event {
                    Some(e) => ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: e.result.clone(),
                        is_error: e.status != EventStatus::Ok,
                    },
                    None => ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: "Not run: the run was interrupted.".into(),
                        is_error: true,
                    },
                }
            })
            .collect();
        if !results.is_empty() {
            self.messages.push(Message {
                role: Role::User,
                content: results,
            });
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn save(&mut self) -> Result<()> {
        self.updated_at = Utc::now();
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            // Write then rename, so a reader (or a crash) never sees a half-written file.
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
            std::fs::rename(&tmp, path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new("obj", Some(dir.path()));
        state.messages.push(Message::user_text("obj"));
        state.status = Status::Completed;
        state.save().unwrap();

        let loaded = State::load(state.path().unwrap()).unwrap();
        assert_eq!(loaded.id, state.id);
        assert_eq!(loaded.status, Status::Completed);
        assert_eq!(loaded.messages, state.messages);
        assert_eq!(loaded.path(), state.path());
    }

    #[test]
    fn closes_dangling_tool_calls_from_recorded_events() {
        let mut state = State::new("obj", None);
        state.messages.push(Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::ToolUse {
                    id: "a".into(),
                    name: "read_file".into(),
                    input: Value::Null,
                },
                ContentBlock::ToolUse {
                    id: "b".into(),
                    name: "read_file".into(),
                    input: Value::Null,
                },
            ],
        });
        state.events.push(Event {
            timestamp: Utc::now(),
            action: "read_file".into(),
            arguments: Value::Null,
            result: "contents".into(),
            status: EventStatus::Ok,
            tool_use_id: Some("a".into()),
        });
        state.close_dangling_tool_calls();
        let last = state.messages.last().unwrap();
        assert_eq!(last.role, Role::User);
        assert!(
            matches!(&last.content[0], ContentBlock::ToolResult { content, is_error: false, .. } if content == "contents")
        );
        assert!(matches!(
            &last.content[1],
            ContentBlock::ToolResult { is_error: true, .. }
        ));

        // Idempotent: nothing dangling any more.
        state.close_dangling_tool_calls();
        assert_eq!(state.messages.len(), 2);
    }
}
