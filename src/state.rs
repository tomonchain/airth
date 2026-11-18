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

/// Where a routine waits for the mission it launched: its run, the act
/// that launched it and the item, to go on with the mission's report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineWait {
    pub routine: String,
    pub run: String,
    pub block: String,
    pub item: serde_json::Map<String, serde_json::Value>,
    /// Set once the routine went on: a mission resumed later does not
    /// start it again.
    #[serde(default)]
    pub resumed: bool,
}

/// What started a run that the owner did not start by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// The id of the realm's objective.
    pub objective: String,
    /// What fired it, e.g. "schedule: daily 08:00".
    pub trigger: String,
    /// The version of the bot's job this run followed, when a bot did it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_version: Option<String>,
    /// For a run another routine asked for: the routines that led to it,
    /// the one that asked last (decision 41).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    /// The bot it ran for, as the objective named it then: its spend is
    /// that bot's month (decision 139), whatever the objective says later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
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
    /// Calls recorded before purposes count as work.
    #[serde(default)]
    pub purpose: Purpose,
}

/// Why a model call was made.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// A step of the run itself.
    #[default]
    Work,
    /// Summarizing a long conversation.
    Compaction,
    /// Refreshing the prompt cache while tools wait.
    Warm,
    /// Choosing between the fast and the main model.
    Triage,
    /// Titling a delivered change.
    Title,
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
    /// A task's agent (decision 64): the founder's order, worked as this
    /// agent of the business, within its scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// A task the founder allowed to go on: the message it resumes with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    /// A task the founder's interlocutor passed on (decision 120): its run
    /// and the plan's item, `run/item`, to tell it when the plan's tasks end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_by: Option<String>,
    /// The founder's project this task is about (decision 147), checked
    /// against the business's own projects when the order came in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<i64>,
    /// A task started by a mail (decision 152): the mail's id in
    /// `business.db`. Such a task holds only `use_skill` and the reply to
    /// that mail, whatever its agent's scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mail: Option<i64>,
    /// How many times the run was sent back to correct its change to the
    /// business config (decision 93).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub config_fixes: u32,
    /// A mission a routine launched (decision 49): where the routine waits
    /// for its report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<RoutineWait>,
    /// For a bot's run: the realm's extensions it may use (see `RealmTools`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
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
    /// When each of `messages` was added, in the same order (decision 148),
    /// for the founder's chat to place them in time; null for those older
    /// than this record. Never part of what the model is sent: messages go
    /// to the providers on their own, and this field is not among them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_at: Option<Vec<Option<DateTime<Utc>>>>,
    /// A message already saved that text was added to since (the founder's
    /// words after the agent's last tool results): it takes the time of the
    /// next save, so those words show when they were said.
    #[serde(skip)]
    grown: Option<usize>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl State {
    pub fn new(objective: &str, runs_dir: Option<&Path>) -> Self {
        // An id is the time to the millisecond; two runs started within
        // one would share a file, the second erasing the first. Within
        // this process, ids are handed out one at a time and always after
        // the last one; against a file another process wrote, the next
        // free millisecond is taken.
        static LAST: std::sync::Mutex<Option<DateTime<Utc>>> = std::sync::Mutex::new(None);
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let mut at = Utc::now();
        if let Some(prev) = *last
            && at.timestamp_millis() <= prev.timestamp_millis()
        {
            at = prev + chrono::Duration::milliseconds(1);
        }
        let free = |at: DateTime<Utc>| {
            let id = at.format("%Y%m%dT%H%M%S%.3fZ").to_string();
            let path = runs_dir.map(|d| d.join(format!("{id}.json")));
            (id, path)
        };
        let (mut id, mut path) = free(at);
        while path.as_ref().is_some_and(|p| p.exists()) {
            at += chrono::Duration::milliseconds(1);
            (id, path) = free(at);
        }
        *last = Some(at);
        drop(last);
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
            agent: None,
            resume: None,
            dispatched_by: None,
            project: None,
            mail: None,
            config_fixes: 0,
            routine: None,
            tools: None,
            tier: None,
            context: None,
            context_update: None,
            compaction: None,
            message_at: Some(Vec::new()),
            model_choice: None,
            answered_by: None,
            grown: None,
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
        // A run saved before decision 148: its messages' times are unknown.
        if state.message_at.is_none() {
            state.message_at = Some(vec![None; state.messages.len()]);
        }
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

    /// Give the messages added since the last save the time of this one,
    /// so `message_at` stays as long as `messages`. Messages are only ever
    /// appended, so the new ones are at the end.
    fn stamp_messages(&mut self, now: DateTime<Utc>) {
        let len = self.messages.len();
        let at = self.message_at.get_or_insert_with(|| vec![None; len]);
        let saved = at.len();
        at.truncate(len);
        at.resize(len, Some(now));
        if let Some(i) = self.grown.filter(|&i| i < saved.min(len)) {
            at[i] = Some(now);
        }
    }

    /// Add the founder's (or Airth's) words: to the last message when it is
    /// already the user's (two in a row are one turn for the model), else
    /// as a new one.
    pub fn append_user_text(&mut self, text: &str) {
        match self.messages.last_mut() {
            Some(last) if last.role == Role::User => {
                last.content.push(ContentBlock::Text {
                    text: text.to_string(),
                });
                self.grown = Some(self.messages.len() - 1);
            }
            _ => self.messages.push(Message::user_text(text)),
        }
    }

    pub fn save(&mut self) -> Result<()> {
        self.updated_at = Utc::now();
        // The times are kept only once the file holds them: a failed write
        // leaves them as they were, for the next save to give.
        let (at, grown) = (self.message_at.clone(), self.grown);
        self.stamp_messages(self.updated_at);
        if let Err(e) = self.write() {
            (self.message_at, self.grown) = (at, grown);
            return Err(e);
        }
        self.grown = None;
        Ok(())
    }

    fn write(&self) -> Result<()> {
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

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_started_at_once_on_threads_get_their_own_ids() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let ids: Vec<String> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|i| s.spawn(move || State::new(&format!("run {i}"), Some(dir)).id))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 8, "{ids:?}");
    }

    #[test]
    fn runs_started_in_one_millisecond_keep_their_own_files() {
        let dir = tempfile::tempdir().unwrap();
        let ids: Vec<String> = (0..5)
            .map(|i| {
                let mut s = State::new(&format!("run {i}"), Some(dir.path()));
                s.save().unwrap();
                s.id
            })
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "{ids:?}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 5);
    }

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

    /// An answer from the agent.
    fn said(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    #[test]
    fn each_message_keeps_the_time_it_was_saved_at() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new("obj", Some(dir.path()));
        state.messages.push(Message::user_text("obj"));
        state.save().unwrap();
        let first = state.message_at.clone().unwrap();
        assert_eq!(first.len(), 1);
        assert!(first[0].is_some());

        std::thread::sleep(std::time::Duration::from_millis(5));
        state.messages.push(said("done"));
        state.save().unwrap();
        let at = State::load(state.path().unwrap())
            .unwrap()
            .message_at
            .unwrap();
        assert_eq!(at.len(), 2);
        // The first message keeps its time; the second has a later one.
        assert_eq!(at[0], first[0]);
        assert!(at[1] > at[0], "{at:?}");
    }

    #[test]
    fn words_added_to_a_saved_message_take_the_time_they_were_added() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new("obj", Some(dir.path()));
        state.messages.push(Message::user_text("obj"));
        state.messages.push(said("Shall I send it?"));
        state.messages.push(Message::user_text("tool results"));
        state.save().unwrap();
        let before = state.message_at.clone().unwrap();

        // Picked up again later: the founder's answer joins the last turn.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut state = State::load(state.path().unwrap()).unwrap();
        state.append_user_text("Yes, send it.");
        assert_eq!(state.messages.len(), 3, "one turn, not two");
        state.save().unwrap();
        let at = State::load(state.path().unwrap())
            .unwrap()
            .message_at
            .unwrap();
        assert_eq!(at[..2], before[..2], "the others keep theirs");
        assert!(at[2] > before[2], "{at:?}");

        // Saved again with nothing added: the time stays.
        state.save().unwrap();
        assert_eq!(state.message_at.as_ref().unwrap()[2], at[2]);
    }

    #[test]
    fn a_failed_save_keeps_no_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new("obj", Some(dir.path()));
        state.messages.push(Message::user_text("obj"));
        state.save().unwrap();
        let before = state.message_at.clone();
        // The runs folder is now a file: the write fails.
        std::fs::remove_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path(), "").unwrap();
        state.messages.push(said("done"));
        assert!(state.save().is_err());
        assert_eq!(
            state.message_at, before,
            "not stamped for a save that failed"
        );
    }

    #[test]
    fn a_run_saved_before_message_times_loads_with_them_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new("obj", Some(dir.path()));
        state.messages.push(Message::user_text("obj"));
        state.messages.push(said("done"));
        state.save().unwrap();
        // As written before decision 148: no `message_at`.
        let path = state.path().unwrap().to_path_buf();
        let mut json: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("message_at");
        std::fs::write(&path, json.to_string()).unwrap();

        let mut old = State::load(&path).unwrap();
        assert_eq!(old.message_at, Some(vec![None, None]));
        // A message added later gets its time; the old ones stay unknown.
        old.messages.push(Message::user_text("more"));
        old.save().unwrap();
        let at = State::load(&path).unwrap().message_at.unwrap();
        assert_eq!(at[..2], [None, None]);
        assert!(at[2].is_some());
    }

    #[test]
    fn message_times_never_reach_what_the_model_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let mut stamped = State::new("obj", Some(dir.path()));
        stamped.messages.push(Message::user_text("obj"));
        stamped.messages.push(said("step"));
        stamped.save().unwrap();
        stamped.messages.push(Message::user_text("more"));
        stamped.compaction = Some(Compaction {
            summary: "so far".into(),
            upto: 1,
            count: 1,
        });
        stamped.save().unwrap();
        assert!(
            stamped
                .message_at
                .as_ref()
                .unwrap()
                .iter()
                .all(Option::is_some)
        );

        let mut plain = stamped.clone();
        plain.message_at = None;
        assert_eq!(stamped.model_messages(), plain.model_messages());
        // Nothing of the times is in the messages themselves.
        let sent = serde_json::to_string(&*stamped.model_messages()).unwrap();
        let year = stamped.updated_at.format("%Y-%m-%d").to_string();
        assert!(!sent.contains(&year), "{sent}");
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
