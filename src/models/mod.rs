//! Provider-neutral model abstraction.
//!
//! The agent only talks to `dyn Model`. Each provider translates these types
//! to and from its own wire format.

pub mod anthropic;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    /// Model reasoning summary, shown to the developer.
    Reasoning {
        summary: String,
        raw: Value,
    },
    /// A provider-specific block the agent does not interpret but must send
    /// back unchanged (e.g. fallback markers).
    Opaque {
        raw: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct ModelRequest<'a> {
    pub system: &'a str,
    pub messages: &'a [Message],
    pub tools: &'a [ToolSpec],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other,
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub trait Model: Send + Sync {
    fn name(&self) -> String;
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse>;
}

/// Build the configured provider. Only Anthropic exists in v0.1.
pub fn from_env() -> Result<Box<dyn Model>> {
    let provider = std::env::var("AIRTH_PROVIDER").unwrap_or_else(|_| "anthropic".into());
    match provider.as_str() {
        "anthropic" => Ok(Box::new(anthropic::Anthropic::from_env()?)),
        other => anyhow::bail!("unknown provider '{other}' (supported: anthropic)"),
    }
}
