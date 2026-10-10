use super::{Tool, Workspace, str_arg};
use crate::cancel::CancelToken;
use crate::memory::{MAX_ENTRY_CHARS, MemoryStore};
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::Result;
use serde_json::{Value, json};

/// Save a fact to the realm's long-term memory.
pub struct Remember {
    pub memory: MemoryStore,
    /// Provenance recorded with each entry, e.g. "run:<id>".
    pub source: String,
}

impl Tool for Remember {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "remember".into(),
            description: format!(
                "Save one durable fact to this realm's long-term memory, so future runs in this realm know it: the owner's preferences, decisions, conventions, recurring context. One fact per call, at most {MAX_ENTRY_CHARS} characters. Never store secrets or credentials. Memory never leaves this realm. The owner may need to approve, and can edit or delete memories."
            ),
            input_schema: json!({
                "type": "object",
                "properties": {"text": {"type": "string", "description": "The fact, written to make sense on its own later"}},
                "required": ["text"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Memory
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        Ok(format!("remember: {}", str_arg(input, "text")?))
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let text = str_arg(input, "text")?;
        if ws.redact(text) != text {
            anyhow::bail!("not saved: the text contains a secret");
        }
        let entry = self.memory.add(text, &self.source)?;
        Ok(format!("remembered ({})", entry.id))
    }
}
