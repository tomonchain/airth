//! The runtime owns the tools and the permission gate. `invoke` is the only
//! path from a model's tool call to a tool's `execute`.

use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::{PermissionGate, PermissionRequest, Verdict};
use crate::state::{Event, EventStatus};
use crate::tools::{Tool, Workspace};
use chrono::Utc;
use serde_json::Value;

pub struct Runtime {
    tools: Vec<Box<dyn Tool>>,
    gate: PermissionGate,
    workspace: Workspace,
    cancel: CancelToken,
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
            workspace,
            cancel,
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
        let (result, status) = self.run(name, arguments);
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

    fn run(&mut self, name: &str, arguments: &Value) -> (String, EventStatus) {
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
        };
        match self.gate.check(&request) {
            Ok(Verdict::Denied(reason)) => {
                return (format!("permission denied: {reason}"), EventStatus::Denied);
            }
            Err(e) => {
                return (
                    format!("permission check failed: {e:#}"),
                    EventStatus::Denied,
                );
            }
            Ok(_) => {}
        }
        // The run may have been stopped while approval was pending.
        if self.cancel.is_cancelled() {
            return ("not run: the run was stopped".into(), EventStatus::Denied);
        }
        match tool.execute(arguments, &self.workspace, &self.cancel) {
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
