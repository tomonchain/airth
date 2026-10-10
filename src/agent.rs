//! The agent loop: observe → reason → act (through the runtime) → observe result → repeat.
//!
//! Reasoning, planning and evaluation happen in the model; this loop feeds it
//! observations, routes its tool calls through the runtime, and decides when
//! the run is over. Clients watch a run through an `Observer` and stop it
//! through a `CancelToken`.

use crate::cancel::CancelToken;
use crate::models::{ContentBlock, Message, Model, ModelRequest, ModelResponse, Role, StopReason};
use crate::runtime::Runtime;
use crate::state::{Event, EventStatus, State, Status};
use anyhow::Result;
use serde_json::Value;

pub struct AgentConfig {
    pub max_iterations: u32,
}

/// Receives progress from a running agent. All methods default to no-ops.
pub trait Observer: Send {
    fn response(&mut self, _iteration: u32, _response: &ModelResponse) {}
    fn tool_started(&mut self, _name: &str, _input: &Value) {}
    fn tool_finished(&mut self, _event: &Event) {}
    /// The state changed and was saved.
    fn changed(&mut self, _state: &State) {}
}

#[cfg(test)]
pub struct NoObserver;
#[cfg(test)]
impl Observer for NoObserver {}

pub const STOPPED_TOOL_RESULT: &str = "Not run: the run was stopped by the user.";

pub struct Agent<'a> {
    model: &'a dyn Model,
    runtime: Runtime,
    config: AgentConfig,
    observer: &'a mut dyn Observer,
    cancel: CancelToken,
}

impl<'a> Agent<'a> {
    pub fn new(
        model: &'a dyn Model,
        runtime: Runtime,
        config: AgentConfig,
        observer: &'a mut dyn Observer,
        cancel: CancelToken,
    ) -> Self {
        Self {
            model,
            runtime,
            config,
            observer,
            cancel,
        }
    }

    fn checkpoint(&mut self, state: &mut State) -> Result<()> {
        state.save()?;
        self.observer.changed(state);
        Ok(())
    }

    pub fn run(&mut self, state: &mut State) -> Result<()> {
        let system = system_prompt(&self.runtime.workspace().root().display().to_string());
        let tools = self.runtime.specs();
        if state.messages.is_empty() {
            state.messages.push(Message::user_text(&state.objective));
        }
        self.checkpoint(state)?;

        // The limit applies per call, so a continued run gets a fresh budget.
        let start = state.iterations;
        while state.status == Status::Running {
            if self.cancel.is_cancelled() {
                state.status = Status::Stopped;
                state.report = Some("Stopped by the user.".into());
                break;
            }
            if state.iterations - start >= self.config.max_iterations {
                state.status = Status::Stopped;
                state.report = Some(format!(
                    "Stopped after reaching the iteration limit ({}).",
                    self.config.max_iterations
                ));
                break;
            }
            state.iterations += 1;

            let request = ModelRequest {
                system: &system,
                messages: &state.messages,
                tools: &tools,
            };
            let response = match self.model.complete(&request) {
                Ok(r) => r,
                Err(e) => {
                    state.status = Status::Failed;
                    state.report = Some(format!("model error: {e:#}"));
                    self.checkpoint(state)?;
                    return Err(e);
                }
            };
            self.observer.response(state.iterations, &response);
            state.input_tokens += response.input_tokens;
            state.output_tokens += response.output_tokens;

            let tool_calls: Vec<(String, String, Value)> = response
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();
            let final_text = last_text(&response.content);
            state.messages.push(Message {
                role: Role::Assistant,
                content: response.content,
            });
            self.checkpoint(state)?;

            match response.stop_reason {
                StopReason::Refusal => {
                    state.status = Status::Failed;
                    state.report = Some("The model declined to continue this task.".into());
                }
                StopReason::MaxTokens if !tool_calls.is_empty() => {
                    // A tool call cut off by the output limit may have truncated input: never run it.
                    let results = tool_calls
                        .into_iter()
                        .map(|(id, _, _)| ContentBlock::ToolResult {
                            tool_use_id: id,
                            content: "Not executed: your response hit the output token limit, so this call may be truncated. Retry with smaller steps (e.g. smaller files).".into(),
                            is_error: true,
                        })
                        .collect();
                    state.messages.push(Message {
                        role: Role::User,
                        content: results,
                    });
                }
                StopReason::MaxTokens => {
                    state.messages.push(Message::user_text(
                        "Your response hit the output token limit. Continue where you left off.",
                    ));
                }
                _ if !tool_calls.is_empty() => {
                    let mut results = Vec::new();
                    for (id, name, input) in tool_calls {
                        if self.cancel.is_cancelled() {
                            results.push(ContentBlock::ToolResult {
                                tool_use_id: id,
                                content: STOPPED_TOOL_RESULT.into(),
                                is_error: true,
                            });
                            continue;
                        }
                        self.observer.tool_started(&name, &input);
                        let event = self.runtime.invoke(&id, &name, &input);
                        self.observer.tool_finished(&event);
                        results.push(ContentBlock::ToolResult {
                            tool_use_id: id,
                            content: event.result.clone(),
                            is_error: event.status != EventStatus::Ok,
                        });
                        state.events.push(event);
                        self.checkpoint(state)?;
                    }
                    state.messages.push(Message {
                        role: Role::User,
                        content: results,
                    });
                }
                _ => {
                    state.status = Status::Completed;
                    state.report = Some(final_text);
                }
            }
            self.checkpoint(state)?;
        }
        self.checkpoint(state)?;
        Ok(())
    }
}

fn last_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compact argument display; file contents are shown in the approval diff instead.
pub fn summarize_args(name: &str, input: &Value) -> String {
    match name {
        "write_file" => format!(
            "{} ({} bytes)",
            input["path"].as_str().unwrap_or("?"),
            input["content"].as_str().map_or(0, str::len)
        ),
        "execute_command" => format!("$ {}", input["command"].as_str().unwrap_or("?")),
        _ => input.to_string(),
    }
}

fn system_prompt(workspace: &str) -> String {
    format!(
        "You are Airth, an autonomous agent runtime written in Rust, operating on the repository at {workspace}. \
That repository is usually your own source code: you can inspect it, change it, build it and test it.

You work in a loop: observe the environment with your tools, reason about the objective, plan, act, check the result, and continue until the objective is met or you cannot make progress. One response is rarely the whole task.

Tools:
- read_file, list_directory: free to use; read before you change anything.
- write_file: writes a whole file; the human approves it after seeing a diff.
- execute_command: runs a shell command in the workspace (use it for `cargo check`, `cargo test`, `cargo build`, `git status`, `git diff`); the human approves each command.

How to work:
- Change files only with write_file, never through shell commands (sed, python, heredocs, redirection), so the human reviews every change as a diff.
- Prefer small, reversible, well-scoped changes. One coherent improvement per run is better than many half-finished ones.
- Before your first write, state the change you propose and why, in a few sentences, so the human can judge the approval.
- After changing code, run `cargo check` and `cargo test`, read failures, and fix them. Do not report success unless they pass.
- If a tool call is denied, take the reason into account; do not retry the same action unchanged.
- Never weaken, disable or route around the permission layer (src/permissions.rs, src/runtime.rs).
- When you are done, reply without calling tools: summarize what you changed, how you verified it, and anything left open."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cancel::CancelToken;
    use crate::models::{ModelResponse, ToolSpec};
    use crate::permissions::{Answer, Approver, PermissionGate, PermissionRequest, Policy};
    use crate::tools::{Workspace, default_tools};
    use serde_json::json;
    use std::sync::Mutex as RefCell;

    /// Replays scripted responses and records what it was sent.
    struct Scripted {
        responses: RefCell<Vec<ModelResponse>>,
        seen: RefCell<Vec<usize>>,
    }

    impl Model for Scripted {
        fn name(&self) -> String {
            "scripted".into()
        }
        fn complete(&self, req: &ModelRequest) -> Result<ModelResponse> {
            assert!(req.tools.iter().any(|t: &ToolSpec| t.name == "write_file"));
            self.seen.lock().unwrap().push(req.messages.len());
            Ok(self.responses.lock().unwrap().remove(0))
        }
    }

    struct Approve;
    impl Approver for Approve {
        fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
            Ok(Answer::Yes)
        }
    }

    fn resp(content: Vec<ContentBlock>, stop_reason: StopReason) -> ModelResponse {
        ModelResponse {
            content,
            stop_reason,
            model: "scripted".into(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[test]
    fn runs_tools_across_iterations_until_done() {
        let dir = tempfile::tempdir().unwrap();
        let model = Scripted {
            responses: RefCell::new(vec![
                resp(
                    vec![ContentBlock::ToolUse {
                        id: "1".into(),
                        name: "write_file".into(),
                        input: json!({"path": "notes.md", "content": "hi"}),
                    }],
                    StopReason::ToolUse,
                ),
                resp(
                    vec![ContentBlock::ToolUse {
                        id: "2".into(),
                        name: "read_file".into(),
                        input: json!({"path": "notes.md"}),
                    }],
                    StopReason::ToolUse,
                ),
                resp(
                    vec![ContentBlock::Text {
                        text: "Done.".into(),
                    }],
                    StopReason::EndTurn,
                ),
            ]),
            seen: RefCell::new(vec![]),
        };
        let runtime = Runtime::new(
            default_tools(),
            PermissionGate::new(Policy::default(), Box::new(Approve)),
            Workspace::new(dir.path()).unwrap(),
            CancelToken::default(),
        );
        let mut observer = NoObserver;
        let mut agent = Agent::new(
            &model,
            runtime,
            AgentConfig { max_iterations: 10 },
            &mut observer,
            CancelToken::default(),
        );
        let mut state = State::new("write notes", None);
        agent.run(&mut state).unwrap();

        assert_eq!(state.status, Status::Completed);
        assert_eq!(state.report.as_deref(), Some("Done."));
        assert_eq!(state.iterations, 3);
        assert_eq!(*model.seen.lock().unwrap(), vec![1, 3, 5]);
        assert_eq!(state.events.len(), 2);
        assert_eq!(state.events[1].result, "hi");
    }

    #[test]
    fn stops_at_iteration_limit() {
        let dir = tempfile::tempdir().unwrap();
        let looping = || {
            resp(
                vec![ContentBlock::ToolUse {
                    id: "x".into(),
                    name: "list_directory".into(),
                    input: json!({}),
                }],
                StopReason::ToolUse,
            )
        };
        let model = Scripted {
            responses: RefCell::new(vec![looping(), looping(), looping()]),
            seen: RefCell::new(vec![]),
        };
        let runtime = Runtime::new(
            default_tools(),
            PermissionGate::new(Policy::default(), Box::new(Approve)),
            Workspace::new(dir.path()).unwrap(),
            CancelToken::default(),
        );
        let mut observer = NoObserver;
        let mut agent = Agent::new(
            &model,
            runtime,
            AgentConfig { max_iterations: 2 },
            &mut observer,
            CancelToken::default(),
        );
        let mut state = State::new("loop", None);
        agent.run(&mut state).unwrap();
        assert_eq!(state.status, Status::Stopped);
        assert_eq!(state.iterations, 2);
    }

    #[test]
    fn cancel_stops_before_next_model_call() {
        let dir = tempfile::tempdir().unwrap();
        let model = Scripted {
            responses: RefCell::new(vec![]),
            seen: RefCell::new(vec![]),
        };
        let cancel = CancelToken::default();
        cancel.cancel();
        let runtime = Runtime::new(
            default_tools(),
            PermissionGate::new(Policy::default(), Box::new(Approve)),
            Workspace::new(dir.path()).unwrap(),
            cancel.clone(),
        );
        let mut observer = NoObserver;
        let mut agent = Agent::new(
            &model,
            runtime,
            AgentConfig { max_iterations: 5 },
            &mut observer,
            cancel,
        );
        let mut state = State::new("x", None);
        agent.run(&mut state).unwrap();
        assert_eq!(state.status, Status::Stopped);
        assert_eq!(state.iterations, 0);
    }
}
