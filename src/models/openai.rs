//! OpenAI-compatible Chat Completions provider (raw HTTP).
//!
//! One wire format covers many services: OpenAI, xAI (Grok), and local
//! servers such as Ollama or LM Studio. Presets pick the base URL and the
//! key variable; `OPENAI_BASE_URL` points it anywhere else.

use super::{
    ContentBlock, Message, Model, ModelRequest, ModelResponse, QUICK_MAX_TOKENS, Role, StopReason,
};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::Duration;

const MAX_RETRIES: u32 = 3;

/// A known OpenAI-compatible service.
struct Preset {
    provider: &'static str,
    base_url: &'static str,
    /// Variable holding the API key; None for local servers that need none.
    key_var: Option<&'static str>,
    /// Field capping the output: OpenAI's reasoning models reject `max_tokens`,
    /// some local servers only know `max_tokens`.
    max_tokens_field: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        provider: "openai",
        base_url: "https://api.openai.com/v1",
        key_var: Some("OPENAI_API_KEY"),
        max_tokens_field: "max_completion_tokens",
    },
    Preset {
        provider: "xai",
        base_url: "https://api.x.ai/v1",
        key_var: Some("XAI_API_KEY"),
        max_tokens_field: "max_completion_tokens",
    },
    Preset {
        provider: "ollama",
        base_url: "http://localhost:11434/v1",
        key_var: None,
        max_tokens_field: "max_tokens",
    },
    Preset {
        provider: "deepseek",
        base_url: "https://api.deepseek.com/v1",
        key_var: Some("DEEPSEEK_API_KEY"),
        max_tokens_field: "max_tokens",
    },
    Preset {
        provider: "mistral",
        base_url: "https://api.mistral.ai/v1",
        key_var: Some("MISTRAL_API_KEY"),
        max_tokens_field: "max_tokens",
    },
    Preset {
        provider: "openrouter",
        base_url: "https://openrouter.ai/api/v1",
        key_var: Some("OPENROUTER_API_KEY"),
        max_tokens_field: "max_tokens",
    },
];

/// Provider names this module serves.
pub fn providers() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.provider).collect()
}

/// A preset's default endpoint and key variable.
pub fn preset(provider: &str) -> Option<(&'static str, Option<&'static str>)> {
    PRESETS
        .iter()
        .find(|p| p.provider == provider)
        .map(|p| (p.base_url, p.key_var))
}

pub struct OpenAiCompatible {
    client: reqwest::blocking::Client,
    provider: &'static str,
    base_url: String,
    api_key: Option<String>,
    model: String,
    max_tokens: u32,
    max_tokens_field: &'static str,
    /// Sent only when set: models without reasoning reject it.
    effort: Option<String>,
}

impl OpenAiCompatible {
    /// Configure the named preset from settings looked up by name.
    pub fn from_settings(provider: &str, get: &dyn Fn(&str) -> Option<String>) -> Result<Self> {
        let preset = PRESETS
            .iter()
            .find(|p| p.provider == provider)
            .ok_or_else(|| anyhow!("unknown OpenAI-compatible provider '{provider}'"))?;
        let non_empty = |key: &str| get(key).filter(|v| !v.trim().is_empty());
        let base_url = non_empty("OPENAI_BASE_URL")
            .unwrap_or_else(|| preset.base_url.into())
            .trim_end_matches('/')
            .to_string();
        let api_key = preset
            .key_var
            .and_then(non_empty)
            // A custom endpoint can still use the generic variable.
            .or_else(|| non_empty("OPENAI_API_KEY"));
        if let (Some(var), None) = (preset.key_var, &api_key) {
            bail!("set {var} to use the {provider} provider");
        }
        let model = non_empty("AIRTH_MODEL").ok_or_else(|| {
            anyhow!("set AIRTH_MODEL to the model to use with the {provider} provider")
        })?;
        let max_tokens = match non_empty("AIRTH_MAX_TOKENS") {
            Some(v) => v.parse().context("AIRTH_MAX_TOKENS must be an integer")?,
            None => 16_000,
        };
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(600))
                .build()?,
            provider: preset.provider,
            base_url,
            api_key,
            model,
            max_tokens,
            max_tokens_field: preset.max_tokens_field,
            effort: non_empty("AIRTH_EFFORT"),
        })
    }

    fn body(&self, request: &ModelRequest) -> Value {
        // Instructions first, then the run's context: providers cache the
        // longest prefix that repeats.
        let system = if request.context.is_empty() {
            request.system.to_string()
        } else {
            format!("{}\n\n{}", request.system, request.context)
        };
        let mut messages = vec![json!({"role": "system", "content": system})];
        for msg in request.messages {
            messages.extend(encode_message(msg));
        }
        let mut body = json!({
            "model": self.model,
            "messages": messages,
        });
        body[self.max_tokens_field] = json!(if request.quick {
            self.max_tokens.min(QUICK_MAX_TOKENS)
        } else {
            self.max_tokens
        });
        if !request.tools.is_empty() {
            body["tools"] = request
                .tools
                .iter()
                .map(|t| {
                    json!({"type": "function", "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }})
                })
                .collect();
        }
        if let Some(effort) = &self.effort {
            body["reasoning_effort"] = json!(if request.quick { "low" } else { effort });
        }
        body
    }

    /// The request for `body`, with this connection's key.
    fn request(&self, body: &Value) -> reqwest::blocking::RequestBuilder {
        let mut req = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("content-type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req.json(body)
    }

    fn send(&self, body: &Value) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let result = self.request(body).send();
            let retryable = match &result {
                Ok(resp) => {
                    let s = resp.status().as_u16();
                    s == 408 || s == 409 || s == 429 || s >= 500
                }
                Err(e) => super::worth_retrying(e),
            };
            if retryable && attempt < MAX_RETRIES {
                attempt += 1;
                std::thread::sleep(super::backoff(attempt));
                continue;
            }
            let resp = result.with_context(|| format!("request to {} failed", self.base_url))?;
            let status = resp.status();
            // The answer can break off while it is read: ask again then too.
            let text = match resp.text() {
                Ok(text) => text,
                Err(e) if attempt < MAX_RETRIES && super::worth_retrying(&e) => {
                    attempt += 1;
                    std::thread::sleep(super::backoff(attempt));
                    continue;
                }
                Err(e) => return Err(e).context("reading the model's answer failed"),
            };
            if !status.is_success() {
                bail!("{} API error {status}: {text}", self.provider);
            }
            return serde_json::from_str(&text)
                .with_context(|| format!("invalid JSON from {} API", self.provider));
        }
    }
}

impl Model for OpenAiCompatible {
    fn name(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }

    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse> {
        let resp = self.send(&self.body(request))?;
        decode_response(&resp)
    }

    fn stream(
        &self,
        request: &ModelRequest,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ModelResponse> {
        let mut body = self.body(request);
        body["stream"] = json!(true);
        // The usage comes in a last event, when asked for; Mistral refuses
        // the field and sends it anyway.
        if self.provider != "mistral" {
            body["stream_options"] = json!({"include_usage": true});
        }
        let resp = super::stream(self.provider, || self.request(&body), on_text, read_stream)?;
        decode_response(&resp)
    }
}

/// A streamed answer put back together as the API would have sent it whole.
fn read_stream(r: &mut dyn std::io::BufRead, on_text: &mut dyn FnMut(&str)) -> Result<Value> {
    let (mut model, mut usage, mut finish) = (Value::Null, Value::Null, Value::Null);
    let (mut text, mut refusal) = (String::new(), String::new());
    let mut calls: Vec<Value> = Vec::new();
    let done = super::events(r, |data| {
        if data == "[DONE]" {
            return Ok(true);
        }
        let event: Value = serde_json::from_str(data).context("invalid JSON in the answer")?;
        if !event["error"].is_null() {
            bail!("API error: {}", event["error"]);
        }
        if model.is_null() {
            model = event["model"].clone();
        }
        if event["usage"].is_object() {
            usage = event["usage"].clone();
        }
        let choice = &event["choices"][0];
        if !choice["finish_reason"].is_null() {
            finish = choice["finish_reason"].clone();
        }
        let delta = &choice["delta"];
        if let Some(piece) = delta["content"].as_str().filter(|p| !p.is_empty()) {
            text.push_str(piece);
            on_text(&text);
        }
        refusal.push_str(delta["refusal"].as_str().unwrap_or_default());
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            // A call's pieces share its index; its id and name come first.
            let i = match call["index"].as_u64() {
                Some(i) => i as usize,
                None if call["id"].as_str().is_some_and(|id| !id.is_empty()) => calls.len(),
                None => calls.len().saturating_sub(1),
            };
            if calls.len() <= i {
                calls.resize(
                    i + 1,
                    json!({"id": "", "function": {"name": "", "arguments": ""}}),
                );
            }
            let c = &mut calls[i];
            let first = |piece: &Value, at: &Value| at == "" && piece.is_string();
            if first(&call["id"], &c["id"]) {
                c["id"] = call["id"].clone();
            }
            if first(&call["function"]["name"], &c["function"]["name"]) {
                c["function"]["name"] = call["function"]["name"].clone();
            }
            let so_far = c["function"]["arguments"].as_str().unwrap_or_default();
            let piece = call["function"]["arguments"].as_str().unwrap_or_default();
            c["function"]["arguments"] = json!(format!("{so_far}{piece}"));
        }
        Ok(false)
    })?;
    // Some servers end without [DONE] once the answer is finished.
    if !done && finish.is_null() {
        bail!("the answer broke off");
    }
    Ok(json!({
        "model": model,
        "usage": usage,
        "choices": [{
            "finish_reason": finish,
            "message": {
                "content": text,
                "refusal": (!refusal.is_empty()).then_some(refusal),
                "tool_calls": calls,
            },
        }],
    }))
}

/// One Airth message becomes one or more chat messages: tool results are
/// messages of their own, and must come right after the assistant's calls.
fn encode_message(msg: &Message) -> Vec<Value> {
    let mut out = Vec::new();
    let mut text = Vec::new();
    let mut calls = Vec::new();
    for block in &msg.content {
        match block {
            ContentBlock::Text { text: t } => text.push(t.as_str()),
            ContentBlock::ToolUse { id, name, input } => calls.push(json!({
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": input.to_string()},
            })),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let content = if *is_error {
                    format!("Error: {content}")
                } else {
                    content.clone()
                };
                out.push(json!({"role": "tool", "tool_call_id": tool_use_id, "content": content}));
            }
            // Other providers' reasoning and markers mean nothing here.
            ContentBlock::Reasoning { .. } | ContentBlock::Opaque { .. } => {}
        }
    }
    let text = text.join("\n\n");
    match msg.role {
        Role::User => {
            if !text.is_empty() || out.is_empty() {
                out.push(json!({"role": "user", "content": text}));
            }
        }
        Role::Assistant => {
            let mut m = json!({"role": "assistant", "content": text});
            if !calls.is_empty() {
                if text.is_empty() {
                    m["content"] = Value::Null;
                }
                m["tool_calls"] = Value::Array(calls);
            }
            out.push(m);
        }
    }
    out
}

fn decode_response(resp: &Value) -> Result<ModelResponse> {
    let choice = &resp["choices"][0];
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow!("response has no message: {resp}"))?;
    let mut content = Vec::new();
    if let Some(text) = message["content"].as_str().filter(|t| !t.is_empty()) {
        content.push(ContentBlock::Text { text: text.into() });
    }
    let refusal = message["refusal"].as_str().filter(|t| !t.is_empty());
    if let Some(text) = refusal {
        content.push(ContentBlock::Text { text: text.into() });
    }
    let mut has_calls = false;
    for (i, call) in message["tool_calls"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        has_calls = true;
        let raw = call["function"]["arguments"].as_str().unwrap_or("{}");
        let input = if raw.trim().is_empty() {
            json!({})
        } else {
            // Malformed arguments reach the tool, which reports them to the model.
            serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.into()))
        };
        let id = match call["id"].as_str().filter(|id| !id.is_empty()) {
            Some(id) => id.to_string(),
            // Some local servers omit ids; results are matched within the turn.
            None => format!("call_{}_{i}", chrono::Utc::now().timestamp_millis()),
        };
        content.push(ContentBlock::ToolUse {
            id,
            name: call["function"]["name"].as_str().unwrap_or_default().into(),
            input,
        });
    }
    // Some servers report "stop" alongside tool calls; the calls decide.
    let stop_reason = if has_calls {
        StopReason::ToolUse
    } else if refusal.is_some() {
        StopReason::Refusal
    } else {
        match choice["finish_reason"].as_str() {
            Some("stop") => StopReason::EndTurn,
            Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            Some("content_filter") => StopReason::Refusal,
            _ => StopReason::Other,
        }
    };
    Ok(ModelResponse {
        content,
        stop_reason,
        model: resp["model"].as_str().unwrap_or_default().to_string(),
        input_tokens: resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: resp["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        // prompt_tokens already counts them; there is no separate cache write.
        cache_read_tokens: resp["usage"]["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            // DeepSeek reports them under its own name.
            .or_else(|| resp["usage"]["prompt_cache_hit_tokens"].as_u64())
            .unwrap_or(0),
        cache_write_tokens: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ToolSpec;
    use std::collections::HashMap;

    fn settings(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    /// Read one HTTP request whole: its head, then as many bytes as it says.
    fn read_request(s: &mut std::net::TcpStream) {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") && s.read(&mut byte).unwrap() == 1 {
            buf.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&buf).to_lowercase();
        let length: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .map_or(0, |v| v.trim().parse().unwrap());
        let mut body = vec![0u8; length];
        s.read_exact(&mut body).unwrap();
    }

    /// A model call has no side effect: a connection dropped before the
    /// answer, or an answer cut while it is read, is asked again. On 1
    /// November both failed DeepSeek runs at the first try.
    #[test]
    fn a_dropped_or_cut_answer_is_asked_again() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let body = r#"{"choices": [{"message": {"content": "fine"}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 1, "completion_tokens": 1}, "model": "m"}"#;
            let mut served = 0;
            for stream in listener.incoming().take(3) {
                let mut s = stream.unwrap();
                read_request(&mut s);
                served += 1;
                match served {
                    // Closed before any answer.
                    1 => {}
                    // The answer breaks off halfway.
                    2 => {
                        let cut = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 500\r\n\r\n{\"choi";
                        s.write_all(cut.as_bytes()).unwrap();
                    }
                    _ => {
                        let whole = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        s.write_all(whole.as_bytes()).unwrap();
                    }
                }
            }
            served
        });
        let url = format!("http://127.0.0.1:{port}/v1");
        let model = OpenAiCompatible::from_settings(
            "ollama",
            &settings(&[("AIRTH_MODEL", "m"), ("OPENAI_BASE_URL", &url)]),
        )
        .unwrap();
        let answer = model
            .complete(&ModelRequest::new("s", &[Message::user_text("hi")], &[]))
            .unwrap();
        assert!(matches!(&answer.content[0], ContentBlock::Text { text } if text == "fine"));
        assert_eq!(server.join().unwrap(), 3);
    }

    /// A streamed answer: its words heard as they come, the tool call it
    /// sends in pieces put together, and its usage from the last event. An
    /// answer that breaks off is asked again, the words heard from nothing.
    #[test]
    fn a_streamed_answer_is_heard_as_it_comes_and_rebuilt_whole() {
        use std::io::Write;
        let events = [
            r#"{"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"Look"}}]}"#,
            r#"{"model":"m","choices":[{"index":0,"delta":{"content":"ing."}}]}"#,
            r#"{"model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"read_file","arguments":""}}]}}]}"#,
            r#"{"model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]}}]}"#,
            r#"{"model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":" \"x\"}"}}]}}]}"#,
            r#"{"model":"m","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
            r#"{"model":"m","choices":[],"usage":{"prompt_tokens":12,"completion_tokens":7,"prompt_cache_hit_tokens":4}}"#,
        ];
        let stream: String = events
            .iter()
            .map(|e| format!("data: {e}\n\n"))
            .chain([": keep-alive\n\n".into(), "data: [DONE]\n\n".into()])
            .collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            for (served, s) in listener.incoming().take(2).enumerate() {
                let mut s = s.unwrap();
                read_request(&mut s);
                let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                // The first answer breaks off after its first words.
                let body = if served == 0 {
                    &stream[..stream.find("ing.").unwrap()]
                } else {
                    &stream
                };
                s.write_all(format!("{head}{body}").as_bytes()).unwrap();
            }
        });
        let url = format!("http://127.0.0.1:{port}/v1");
        let model = OpenAiCompatible::from_settings(
            "deepseek",
            &settings(&[
                ("AIRTH_MODEL", "m"),
                ("OPENAI_BASE_URL", &url),
                ("DEEPSEEK_API_KEY", "k"),
            ]),
        )
        .unwrap();
        let mut heard = Vec::new();
        let answer = model
            .stream(
                &ModelRequest::new("s", &[Message::user_text("hi")], &[]),
                &mut |t| heard.push(t.to_string()),
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(heard, ["Look", "", "Look", "Looking."]);
        assert_eq!(answer.stop_reason, StopReason::ToolUse);
        assert!(matches!(&answer.content[0], ContentBlock::Text { text } if text == "Looking."));
        assert!(matches!(
            &answer.content[1],
            ContentBlock::ToolUse { id, name, input } if id == "c1" && name == "read_file" && input == &json!({"path": "x"})
        ));
        assert_eq!(
            (
                answer.input_tokens,
                answer.output_tokens,
                answer.cache_read_tokens
            ),
            (12, 7, 4)
        );
    }

    #[test]
    fn presets_pick_url_and_key() {
        let xai = OpenAiCompatible::from_settings(
            "xai",
            &settings(&[("XAI_API_KEY", "k"), ("AIRTH_MODEL", "grok-x")]),
        )
        .unwrap();
        assert_eq!(xai.base_url, "https://api.x.ai/v1");
        assert_eq!(xai.api_key.as_deref(), Some("k"));
        assert_eq!(xai.name(), "xai/grok-x");

        // Local servers need no key, and the URL can be overridden.
        let local = OpenAiCompatible::from_settings(
            "ollama",
            &settings(&[
                ("AIRTH_MODEL", "qwen"),
                ("OPENAI_BASE_URL", "http://127.0.0.1:1234/v1/"),
            ]),
        )
        .unwrap();
        assert_eq!(local.base_url, "http://127.0.0.1:1234/v1");
        assert!(local.api_key.is_none());
        assert_eq!(local.max_tokens_field, "max_tokens");
    }

    #[test]
    fn missing_key_or_model_is_explained() {
        let err = OpenAiCompatible::from_settings("openai", &settings(&[("AIRTH_MODEL", "m")]))
            .err()
            .unwrap();
        assert!(err.to_string().contains("OPENAI_API_KEY"));
        let err = OpenAiCompatible::from_settings("ollama", &settings(&[]))
            .err()
            .unwrap();
        assert!(err.to_string().contains("AIRTH_MODEL"));
    }

    #[test]
    fn encodes_tool_turns_in_chat_order() {
        let assistant = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Reasoning {
                    summary: "x".into(),
                    raw: json!({"type": "thinking"}),
                },
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "read_file".into(),
                    input: json!({"path": "a"}),
                },
            ],
        };
        let encoded = encode_message(&assistant);
        assert_eq!(encoded.len(), 1);
        assert!(encoded[0]["content"].is_null());
        assert_eq!(encoded[0]["tool_calls"][0]["id"], "t1");
        assert_eq!(
            encoded[0]["tool_calls"][0]["function"]["arguments"],
            r#"{"path":"a"}"#
        );

        let user = Message {
            role: Role::User,
            content: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "denied".into(),
                    is_error: true,
                },
                ContentBlock::Text {
                    text: "Also check b.".into(),
                },
            ],
        };
        let encoded = encode_message(&user);
        assert_eq!(encoded[0]["role"], "tool");
        assert_eq!(encoded[0]["tool_call_id"], "t1");
        assert_eq!(encoded[0]["content"], "Error: denied");
        assert_eq!(
            encoded[1],
            json!({"role": "user", "content": "Also check b."})
        );
    }

    #[test]
    fn body_has_system_tools_and_optional_effort() {
        let model = OpenAiCompatible::from_settings(
            "openai",
            &settings(&[("OPENAI_API_KEY", "k"), ("AIRTH_MODEL", "m")]),
        )
        .unwrap();
        let tools = [ToolSpec {
            name: "read_file".into(),
            description: "Read".into(),
            input_schema: json!({"type": "object"}),
        }];
        let messages = [Message::user_text("hi")];
        let body = model.body(&ModelRequest::new("sys", &messages, &tools));
        assert_eq!(
            body["messages"][0],
            json!({"role": "system", "content": "sys"})
        );
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
        assert_eq!(body["max_completion_tokens"], 16_000);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn decodes_tool_calls_even_when_finish_reason_says_stop() {
        let resp = json!({
            "model": "qwen",
            "choices": [{"finish_reason": "stop", "message": {
                "role": "assistant",
                "content": "Reading.",
                "tool_calls": [
                    {"id": "c1", "type": "function",
                     "function": {"name": "read_file", "arguments": "{\"path\":\"x\"}"}},
                    {"type": "function", "function": {"name": "list_directory", "arguments": ""}}
                ]
            }}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3}
        });
        let decoded = decode_response(&resp).unwrap();
        assert_eq!(decoded.stop_reason, StopReason::ToolUse);
        assert_eq!(decoded.input_tokens, 12);
        assert!(matches!(&decoded.content[0], ContentBlock::Text { text } if text == "Reading."));
        assert!(
            matches!(&decoded.content[1], ContentBlock::ToolUse { id, input, .. } if id == "c1" && input["path"] == "x")
        );
        assert!(
            matches!(&decoded.content[2], ContentBlock::ToolUse { id, input, .. } if !id.is_empty() && *input == json!({}))
        );
    }

    #[test]
    fn decodes_end_turn_length_and_refusal() {
        let end = json!({"choices": [{"finish_reason": "stop", "message": {"content": "Done."}}]});
        assert_eq!(
            decode_response(&end).unwrap().stop_reason,
            StopReason::EndTurn
        );
        let cut = json!({"choices": [{"finish_reason": "length", "message": {"content": "Do"}}]});
        assert_eq!(
            decode_response(&cut).unwrap().stop_reason,
            StopReason::MaxTokens
        );
        let refused = json!({"choices": [{"finish_reason": "stop",
            "message": {"content": null, "refusal": "I can't help with that."}}]});
        let decoded = decode_response(&refused).unwrap();
        assert_eq!(decoded.stop_reason, StopReason::Refusal);
        assert_eq!(decoded.content.len(), 1);
        assert!(decode_response(&json!({"error": "x"})).is_err());
    }
}
