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
use std::time::Duration;

/// Whether a request that failed on the way is worth sending again. A model
/// call has no side effect, so any failure to send it or to read its answer
/// is tried again; only a request Airth could not build is not.
pub(crate) fn worth_retrying(e: &reqwest::Error) -> bool {
    !e.is_builder()
}

/// How long to wait before the `attempt`-th try again: 2, 4, then 8 s.
pub(crate) fn backoff(attempt: u32) -> Duration {
    if cfg!(test) {
        return Duration::from_millis(5);
    }
    Duration::from_secs(2u64.pow(attempt))
}

/// Sends a streamed request and returns what `read` makes of its events,
/// sent again as a whole request is when it fails on the way or the answer
/// breaks off; `on_text` hears the new attempt start from nothing.
pub(crate) fn stream(
    provider: &str,
    request: impl Fn() -> reqwest::blocking::RequestBuilder,
    on_text: &mut dyn FnMut(&str),
    stopped: &dyn Fn() -> bool,
    read: impl Fn(&mut dyn std::io::BufRead, &mut dyn FnMut(&str)) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    use anyhow::Context;
    let mut attempt = 0;
    loop {
        let result = request().send();
        let retryable = match &result {
            Ok(resp) => {
                let s = resp.status().as_u16();
                s == 408 || s == 409 || s == 429 || s >= 500
            }
            Err(e) => worth_retrying(e),
        };
        if retryable && attempt < MAX_RETRIES {
            attempt += 1;
            std::thread::sleep(backoff(attempt));
            continue;
        }
        let resp = result.with_context(|| format!("request to {provider} failed"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("{provider} API error {status}: {text}");
        }
        // Each line read is a moment to give up: providers send events, or
        // keep-alive lines, every few seconds even while the model thinks.
        let mut lines = Stoppable {
            inner: std::io::BufReader::new(resp),
            stopped,
        };
        match read(&mut lines, on_text) {
            Ok(answer) => return Ok(answer),
            Err(e) if stopped() => return Err(e).context("stopped"),
            Err(_) if attempt < MAX_RETRIES => {
                on_text("");
                attempt += 1;
                std::thread::sleep(backoff(attempt));
            }
            Err(e) => return Err(e).context("reading the model's answer failed"),
        }
    }
}

/// A stream that ends, as broken off, once `stopped` says so.
struct Stoppable<'a, R> {
    inner: R,
    stopped: &'a dyn Fn() -> bool,
}

impl<R: std::io::Read> std::io::Read for Stoppable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: std::io::BufRead> std::io::BufRead for Stoppable<'_, R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if (self.stopped)() {
            return Err(std::io::Error::other("stopped"));
        }
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

/// Tries of a streamed request after the first, as the providers' own.
const MAX_RETRIES: u32 = 3;

/// Whether a failed model call may work if tried again later: the provider
/// busy or down, the network, an answer cut off (decision 81). A refused
/// key, no credit or a request the provider refuses will not.
pub fn transient(error: &str) -> bool {
    let e = error.to_lowercase();
    let lasting = [
        "401",
        "402",
        "403",
        "unauthorized",
        "forbidden",
        "api key",
        "insufficient",
        "credit",
        "billing",
        "invalid_request",
    ];
    if lasting.iter().any(|w| e.contains(w)) {
        return false;
    }
    [
        "429",
        "500",
        "502",
        "503",
        "504",
        "529",
        "overloaded",
        "rate limit",
        "timed out",
        "timeout",
        "connection",
        "error sending request",
        "broke off",
        "busy",
        "temporarily",
    ]
    .iter()
    .any(|w| e.contains(w))
}

/// Hands each server-sent event's data to `on` until it says the answer is
/// whole; false when the stream ended first.
pub(crate) fn events(
    r: &mut dyn std::io::BufRead,
    mut on: impl FnMut(&str) -> anyhow::Result<bool>,
) -> anyhow::Result<bool> {
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(false);
        }
        // Event names, ids and keep-alive comments carry nothing needed:
        // every event's data says its own type.
        if let Some(data) = line.trim_end().strip_prefix("data:")
            && on(data.trim_start())?
        {
            return Ok(true);
        }
    }
}

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

    /// Like `complete`, telling `on_text` the answer's text so far as it is
    /// written, so the founder reads it as it comes. Started again from
    /// nothing when the request is sent again. Gives up as soon as `stopped`
    /// says so. A provider that cannot stream answers all at once.
    fn stream(
        &self,
        request: &ModelRequest,
        _on_text: &mut dyn FnMut(&str),
        _stopped: &dyn Fn() -> bool,
    ) -> Result<ModelResponse> {
        self.complete(request)
    }

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

    /// Decision 81: a busy or unreachable provider is tried again later; a
    /// refused key, no credit or a refused request is not.
    #[test]
    fn a_passing_failure_is_told_from_one_that_lasts() {
        for passing in [
            "DeepSeek API error 503 Service Unavailable: overloaded",
            "Anthropic API error 529: {\"type\": \"overloaded_error\"}",
            "OpenAI API error 429 Too Many Requests: rate limit reached",
            "error sending request for url (https://api.deepseek.com/chat/completions)",
            "operation timed out",
            "the answer broke off",
        ] {
            assert!(transient(passing), "{passing}");
        }
        for lasting in [
            "ollama API error 401 Unauthorized: Authentication Fails, your api key is invalid",
            "DeepSeek API error 402 Payment Required: Insufficient Balance",
            "OpenAI API error 400 Bad Request: invalid_request_error: context too long",
            "Stopped by the user.",
            "write_file: permission denied",
        ] {
            assert!(!transient(lasting), "{lasting}");
        }
    }

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
