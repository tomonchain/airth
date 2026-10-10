//! Minimal persistent execution state, saved as JSON under `.airth/runs/`.

use crate::models::{ContentBlock, Message, Role};
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
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
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
            updated_at: Utc::now(),
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
