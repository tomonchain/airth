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
                "Propose one thing for the whole team to keep: every agent and routine of this business is told it from then on. Only what the owner decided, tested or approved, in their sense: never an idea from a brainstorm, an option still open, or your own suggestion the owner has not approved. When it changes an earlier memory, give that memory's id (as [m3] in what you remember) as replaces, rather than adding a second one. One fact per call, at most {MAX_ENTRY_CHARS} characters. Never secrets or credentials. The owner approves each one, and can edit or delete it."
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "The decision, written to make sense on its own later"},
                    "replaces": {"type": "string", "description": "The id of the memory it replaces, if it changes one (e.g. m3)"}
                },
                "required": ["text"]
            }),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Memory
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let text = str_arg(input, "text")?;
        let replaced = input["replaces"]
            .as_str()
            .and_then(|id| self.memory.list().ok()?.into_iter().find(|e| e.id == id));
        Ok(match replaced {
            Some(old) => format!("{text}\n\nReplaces: {}", old.text),
            None => text.to_string(),
        })
    }

    fn execute(&self, input: &Value, ws: &Workspace, _: &CancelToken) -> Result<String> {
        let text = str_arg(input, "text")?;
        if ws.redact(text) != text {
            anyhow::bail!("not saved: the text contains a secret");
        }
        let entry = match input["replaces"].as_str().filter(|id| !id.is_empty()) {
            Some(id) => self.memory.replace(id, text, &self.source)?,
            None => self.memory.add(text, &self.source)?,
        };
        Ok(format!("remembered ({})", entry.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent's tool: the founder is shown what it replaces, and a yes
    /// replaces it.
    #[test]
    fn the_agent_proposes_a_decision_that_replaces_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join("memory.json"));
        store.add("Posts go out at 08:00", "owner").unwrap();
        let tool = Remember {
            memory: store.clone(),
            source: "run:1".into(),
        };
        let ws = Workspace::new(dir.path()).unwrap();
        let input = serde_json::json!({"text": "Posts go out at 09:00", "replaces": "m1"});
        assert_eq!(
            tool.preview(&input, &ws).unwrap(),
            "Posts go out at 09:00\n\nReplaces: Posts go out at 08:00"
        );
        tool.execute(&input, &ws, &crate::cancel::CancelToken::default())
            .unwrap();
        let texts: Vec<_> = store.list().unwrap().into_iter().map(|e| e.text).collect();
        assert_eq!(texts, ["Posts go out at 09:00"]);
    }
}
