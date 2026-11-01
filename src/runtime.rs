//! The runtime owns the tools and the permission gate. `invoke` is the only
//! path from a model's tool call to a tool's `execute`.

use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::{PermissionGate, PermissionRequest, Verdict};
use crate::state::{Event, EventStatus, State};
use crate::tools::{PlanHandle, Tool, Workspace};
use chrono::Utc;
use serde_json::Value;

/// What an agent may do alone with a declared action (decision 45),
/// decided before each one, from the levels as they are now.
pub trait Authority: Send {
    /// `tool` is the tool called, `call` the call's id, unique in the run.
    fn decide(
        &mut self,
        tool: &str,
        action: &str,
        arguments: &Value,
        call: &str,
    ) -> anyhow::Result<Decision>;
    /// What became of an action decided `Act`.
    fn finish(&mut self, journal: i64, error: Option<&str>);
}

pub enum Decision {
    /// Act now; the action is journaled under this id.
    Act(i64),
    /// Not done now: queued, prepared or refused, and why.
    Held(String),
}

pub struct Runtime {
    tools: Vec<Box<dyn Tool>>,
    gate: PermissionGate,
    authority: Option<Box<dyn Authority>>,
    workspace: Workspace,
    cancel: CancelToken,
    plan: Option<PlanHandle>,
}

impl Runtime {
    pub fn new(
        tools: Vec<Box<dyn Tool>>,
        gate: PermissionGate,
        workspace: Workspace,
        cancel: CancelToken,
    ) -> Self {
        Self {
            tools,
            gate,
            authority: None,
            workspace,
            cancel,
            plan: None,
        }
    }

    /// Declared actions go through `authority` before the gate.
    pub fn with_authority(mut self, authority: Box<dyn Authority>) -> Self {
        self.authority = Some(authority);
        self
    }

    /// The handle the run's `update_plan` tool writes to.
    pub fn with_plan(mut self, plan: PlanHandle) -> Self {
        self.plan = Some(plan);
        self
    }

    /// Copy what the tools and the gate keep during the run (its plan and the
    /// owner's grants) into the run's state, to be saved with it.
    pub fn sync(&self, state: &mut State) {
        state.grants = self.gate.grants().to_vec();
        if let Some(plan) = &self.plan {
            state.plan = plan.lock().unwrap().clone();
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|t| t.spec()).collect()
    }

    /// Check permission, execute, and record the outcome as an event.
    pub fn invoke(&mut self, tool_use_id: &str, name: &str, arguments: &Value) -> Event {
        let (result, status) = self.run(name, arguments, tool_use_id);
        // Whatever a tool returned, the model never sees the realm's secrets.
        let result = self.workspace.redact(&result);
        Event {
            timestamp: Utc::now(),
            action: name.to_string(),
            arguments: arguments.clone(),
            result,
            status,
            tool_use_id: Some(tool_use_id.to_string()),
        }
    }

    fn run(&mut self, name: &str, arguments: &Value, call: &str) -> (String, EventStatus) {
        let Some(tool) = self.tools.iter().find(|t| t.spec().name == name) else {
            return (format!("unknown tool '{name}'"), EventStatus::Error);
        };
        let preview = match tool.preview(arguments, &self.workspace) {
            Ok(p) => p,
            Err(e) => return (format!("invalid arguments: {e:#}"), EventStatus::Error),
        };
        let request = PermissionRequest {
            tool: name.to_string(),
            capability: tool.capability(),
            preview,
            warnings: tool.warnings(arguments),
            scope: tool.scope(arguments),
        };
        let action = match tool.action(arguments) {
            Ok(action) => action,
            Err(e) => return (format!("permission denied: {e:#}"), EventStatus::Denied),
        };
        let mut journal = None;
        if let (Some(action), Some(authority)) = (&action, self.authority.as_mut()) {
            match authority.decide(name, action, arguments, call) {
                Ok(Decision::Act(id)) => journal = Some(id),
                Ok(Decision::Held(why)) => return (why, EventStatus::Denied),
                // Unsure what the agent may do: it does not act alone.
                Err(e) => return (format!("not done: {e:#}"), EventStatus::Denied),
            }
        }
        let refused = match self.gate.check(&request) {
            Ok(Verdict::Denied(reason)) => Some(format!("permission denied: {reason}")),
            Err(e) => Some(format!("permission check failed: {e:#}")),
            // The run may have been stopped while approval was pending.
            Ok(_) if self.cancel.is_cancelled() => Some("not run: the run was stopped".into()),
            Ok(_) => None,
        };
        if let Some(why) = refused {
            if let (Some(id), Some(authority)) = (journal, self.authority.as_mut()) {
                authority.finish(id, Some(&why));
            }
            return (why, EventStatus::Denied);
        }
        let outcome = tool.execute(arguments, &self.workspace, &self.cancel);
        if let (Some(id), Some(authority)) = (journal, self.authority.as_mut()) {
            let error = outcome.as_ref().err().map(|e| format!("{e:#}"));
            authority.finish(id, error.as_deref());
        }
        match outcome {
            Ok(out) => (out, EventStatus::Ok),
            Err(e) => (format!("{e:#}"), EventStatus::Error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{Answer, Approver, Policy};
    use crate::tools::default_tools;
    use serde_json::json;

    struct Deny;
    impl Approver for Deny {
        fn ask(&mut self, _: &PermissionRequest) -> anyhow::Result<Answer> {
            Ok(Answer::No("nope".into()))
        }
    }

    #[test]
    fn denied_write_does_not_touch_disk() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let mut rt = Runtime::new(
            default_tools(),
            PermissionGate::new(Policy::default(), Box::new(Deny)),
            ws,
            CancelToken::default(),
        );

        let ev = rt.invoke(
            "id",
            "write_file",
            &json!({"path": "x.txt", "content": "hi"}),
        );
        assert_eq!(ev.status, EventStatus::Denied);
        assert!(!dir.path().join("x.txt").exists());

        let ev = rt.invoke("id", "execute_command", &json!({"command": "touch y"}));
        assert_eq!(ev.status, EventStatus::Denied);
        assert!(!dir.path().join("y").exists());

        let ev = rt.invoke("id", "list_directory", &json!({}));
        assert_eq!(ev.status, EventStatus::Ok);

        assert_eq!(
            rt.invoke("id", "nope", &json!({})).status,
            EventStatus::Error
        );
    }
}
