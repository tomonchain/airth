//! Minimal persistent execution state, saved as JSON under `.airth/runs/`.

use crate::models::Message;
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

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(self)?)?;
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
}
