//! Provider-neutral model abstraction.
//!
//! The agent only talks to `dyn Model`. Each provider translates these types
//! to and from its own wire format.

pub mod anthropic;
pub mod openai;
pub mod router;

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
    /// Instructions that are the same for every run: providers cache them.
    pub system: &'a str,
    /// What this run is told about its realm, owner and workspace; follows
    /// `system`. May be empty.
    pub context: &'a str,
    pub messages: &'a [Message],
    pub tools: &'a [ToolSpec],
    /// A short, simple answer (e.g. a one-word classification): little
    /// reasoning and a small output limit.
    pub quick: bool,
}

impl<'a> ModelRequest<'a> {
    pub fn new(system: &'a str, messages: &'a [Message], tools: &'a [ToolSpec]) -> Self {
        Self {
            system,
            context: "",
            messages,
            tools,
            quick: false,
        }
    }
}

/// Output limit of a `quick` request.
pub const QUICK_MAX_TOKENS: u32 = 2048;

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
    /// All input tokens, cached ones included.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Of `input_tokens`: read from the provider's prompt cache.
    pub cache_read_tokens: u64,
    /// Of `input_tokens`: written to the provider's prompt cache.
    pub cache_write_tokens: u64,
}

pub trait Model: Send + Sync {
    fn name(&self) -> String;
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse>;

    /// Keep the provider's prompt cache of `request` (a request already sent)
    /// alive without generating anything, so the next request after a long
    /// pause reads it instead of writing it again. Returns the call's usage,
    /// or None if the provider cannot do this.
    fn keep_warm(&self, _request: &ModelRequest) -> Result<Option<ModelResponse>> {
        Ok(None)
    }

    /// The conversation was replaced (compacted): what the model sees next
    /// starts fresh.
    fn history_replaced(&self) {}
}

/// Build the configured provider from the environment.
pub fn from_env() -> Result<Box<dyn Model>> {
    from_settings(&|key| std::env::var(key).ok())
}

/// The providers a run can be pointed at: id, name, and the credential that
/// enables it (None: local, always available).
pub fn provider_catalog() -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    let mut out = vec![("anthropic", "Anthropic (Claude)", Some("ANTHROPIC_API_KEY"))];
    for p in openai::providers() {
        let name = match p {
            "openai" => "OpenAI",
            "xai" => "xAI (Grok)",
            "deepseek" => "DeepSeek",
            "mistral" => "Mistral",
            "openrouter" => "OpenRouter",
            "ollama" => "Ollama (local)",
            other => other,
        };
        out.push((p, name, openai::preset(p).and_then(|(_, key)| key)));
    }
    out
}

/// Settings for a model chosen for one request (`provider/model`), on top of
/// the realm's: only that provider's own credential is used, so a key for
/// one service is never sent to another (no `OPENAI_API_KEY` or
/// `OPENAI_BASE_URL` fallback for other providers).
pub fn choice_settings<'a>(
    choice: &'a str,
    get: &'a dyn Fn(&str) -> Option<String>,
) -> Result<impl Fn(&str) -> Option<String> + 'a> {
    let (provider, model) = choice
        .split_once('/')
        .filter(|(p, m)| !p.is_empty() && !m.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("a model choice looks like provider/model, e.g. deepseek/deepseek-chat")
        })?;
    if !provider_catalog().iter().any(|(id, _, _)| *id == provider) {
        anyhow::bail!("unknown provider '{provider}'");
    }
    let configured = get("AIRTH_PROVIDER").map(|p| p.trim().to_lowercase());
    let same = configured.as_deref() == Some(provider)
        || (configured.is_none() && provider == "anthropic");
    Ok(move |key: &str| match key {
        "AIRTH_PROVIDER" => Some(provider.to_string()),
        "AIRTH_MODEL" => Some(model.to_string()),
        // Effort and base URL are tied to the realm's own provider.
        "AIRTH_EFFORT" | "OPENAI_BASE_URL" | "ANTHROPIC_BASE_URL" if !same => None,
        "OPENAI_API_KEY" if provider != "openai" && !same => None,
        _ => get(key),
    })
}

/// The models a provider offers, from its live `/models` list (chat models
/// only, best guess by name).
pub fn list_models(provider: &str, get: &dyn Fn(&str) -> Option<String>) -> Result<Vec<String>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let non_empty = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    let req = if provider == "anthropic" {
        let key = non_empty("ANTHROPIC_API_KEY")
            .ok_or_else(|| anyhow::anyhow!("no ANTHROPIC_API_KEY"))?;
        client
            .get("https://api.anthropic.com/v1/models?limit=100")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
    } else {
        let (base, key_var) =
            openai::preset(provider).ok_or_else(|| anyhow::anyhow!("unknown provider"))?;
        let mut r = client.get(format!("{base}/models"));
        if let Some(var) = key_var {
            r = r.bearer_auth(non_empty(var).ok_or_else(|| anyhow::anyhow!("no {var}"))?);
        }
        r
    };
    let resp = req.send()?;
    if !resp.status().is_success() {
        anyhow::bail!("{provider} answered {}", resp.status());
    }
    let body: serde_json::Value = resp.json()?;
    const NOT_CHAT: &[&str] = &[
        "embed",
        "tts",
        "whisper",
        "dall-e",
        "image",
        "audio",
        "moderation",
        "transcribe",
        "realtime",
        "search",
        "babbage",
        "davinci",
        "ocr",
    ];
    let mut models: Vec<String> = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| !NOT_CHAT.iter().any(|n| id.to_lowercase().contains(n)))
        .map(String::from)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

/// Build the provider named by `AIRTH_PROVIDER` (default `anthropic`) from
/// settings looked up by name.
pub fn from_settings(get: &dyn Fn(&str) -> Option<String>) -> Result<Box<dyn Model>> {
    let provider = get("AIRTH_PROVIDER")
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "anthropic".into());
    match provider.as_str() {
        "anthropic" => Ok(Box::new(anthropic::Anthropic::from_settings(get)?)),
        p if openai::providers().contains(&p) => {
            Ok(Box::new(openai::OpenAiCompatible::from_settings(p, get)?))
        }
        other => anyhow::bail!(
            "unknown provider '{other}' (supported: anthropic, {})",
            openai::providers().join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_model_only_uses_its_own_providers_key() {
        let realm = |k: &str| match k {
            "AIRTH_PROVIDER" => Some("openai".to_string()),
            "AIRTH_MODEL" => Some("gpt-x".to_string()),
            "OPENAI_API_KEY" => Some("sk-openai".to_string()),
            "OPENAI_BASE_URL" => Some("https://proxy.example/v1".to_string()),
            _ => None,
        };
        let s = choice_settings("deepseek/deepseek-chat", &realm).unwrap();
        assert_eq!(s("AIRTH_PROVIDER").as_deref(), Some("deepseek"));
        assert_eq!(
            s("OPENAI_API_KEY"),
            None,
            "the OpenAI key never goes to DeepSeek"
        );
        assert_eq!(s("OPENAI_BASE_URL"), None);
        let err = from_settings(&s).err().unwrap().to_string();
        assert!(err.contains("DEEPSEEK_API_KEY"), "{err}");
        let with_key = |k: &str| {
            if k == "DEEPSEEK_API_KEY" {
                Some("sk-ds".into())
            } else {
                realm(k)
            }
        };
        let s = choice_settings("deepseek/deepseek-chat", &with_key).unwrap();
        assert_eq!(from_settings(&s).unwrap().name(), "deepseek/deepseek-chat");
        // Same provider as the realm's: its settings still apply.
        let s = choice_settings("openai/gpt-y", &realm).unwrap();
        assert_eq!(
            s("OPENAI_BASE_URL").as_deref(),
            Some("https://proxy.example/v1")
        );
        assert!(choice_settings("deepseek", &realm).is_err());
        assert!(choice_settings("nope/x", &realm).is_err());
    }

    #[test]
    fn picks_the_provider_by_name() {
        let get = |k: &str| match k {
            "AIRTH_PROVIDER" => Some("XAI".to_string()),
            "XAI_API_KEY" => Some("k".to_string()),
            "AIRTH_MODEL" => Some("grok".to_string()),
            _ => None,
        };
        assert_eq!(from_settings(&get).unwrap().name(), "xai/grok");
        let unknown = |k: &str| (k == "AIRTH_PROVIDER").then(|| "nope".to_string());
        let err = from_settings(&unknown).err().unwrap().to_string();
        assert!(err.contains("ollama"), "{err}");
    }
}
