//! Anthropic Messages API provider (raw HTTP; there is no official Rust SDK).

use super::{
    ContentBlock, Message, Model, ModelRequest, ModelResponse, QUICK_MAX_TOKENS, Role, StopReason,
};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::Duration;

const DEFAULT_MODEL: &str = "claude-opus-5-5";
const API_VERSION: &str = "2023-06-01";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const MAX_RETRIES: u32 = 3;

/// Models that accept `fallbacks: "default"`.
const FALLBACK_MODELS: &[&str] = &[
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-fable-5-1",
    "claude-sonnet-5-5",
];

enum Auth {
    ApiKey(String),
    Bearer(String),
}

pub struct Anthropic {
    client: reqwest::blocking::Client,
    base_url: String,
    auth: Auth,
    /// For API keys not scoped to a workspace.
    workspace_id: Option<String>,
    model: String,
    max_tokens: u32,
    effort: String,
}

impl Anthropic {
    /// Configure from settings looked up by name (environment variables, or a
    /// realm's secrets falling back to them).
    pub fn from_settings(get: &dyn Fn(&str) -> Option<String>) -> Result<Self> {
        let non_empty_env = |key: &str| get(key).filter(|v| !v.trim().is_empty());
        let auth = match (
            non_empty_env("ANTHROPIC_API_KEY"),
            non_empty_env("ANTHROPIC_AUTH_TOKEN"),
        ) {
            (Some(key), _) => Auth::ApiKey(key),
            (None, Some(token)) => Auth::Bearer(token),
            (None, None) => bail!(
                "set ANTHROPIC_API_KEY (or ANTHROPIC_AUTH_TOKEN) to use the Anthropic provider"
            ),
        };
        let max_tokens = match non_empty_env("AIRTH_MAX_TOKENS") {
            Some(v) => v.parse().context("AIRTH_MAX_TOKENS must be an integer")?,
            None => 16_000,
        };
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(600))
                .build()?,
            base_url: non_empty_env("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|| "https://api.anthropic.com".into()),
            auth,
            workspace_id: non_empty_env("ANTHROPIC_WORKSPACE_ID"),
            model: non_empty_env("AIRTH_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
            max_tokens,
            effort: non_empty_env("AIRTH_EFFORT").unwrap_or_else(|| "high".into()),
        })
    }

    fn body(&self, request: &ModelRequest) -> Value {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
            .collect();
        // The instructions are the same for every run and the tools for every
        // run of a realm: a breakpoint after them lets a new run read them
        // from the cache. The run's own context comes after it.
        let mut system = vec![json!({
            "type": "text",
            "text": request.system,
            "cache_control": {"type": "ephemeral"},
        })];
        if !request.context.is_empty() {
            system.push(json!({"type": "text", "text": request.context}));
        }
        let (max_tokens, effort) = if request.quick {
            (self.max_tokens.min(QUICK_MAX_TOKENS), "low")
        } else {
            (self.max_tokens, self.effort.as_str())
        };
        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": request.messages.iter().map(encode_message).collect::<Vec<_>>(),
            "tools": tools,
            // Automatic prompt caching: the history prefix is re-sent every iteration.
            "cache_control": {"type": "ephemeral"},
        });
        if adaptive_thinking(&self.model) {
            body["thinking"] = json!({"type": "adaptive", "display": "summarized"});
            body["output_config"] = json!({"effort": effort});
        }
        if self.uses_fallbacks() {
            body["fallbacks"] = json!("default");
        }
        body
    }

    fn uses_fallbacks(&self) -> bool {
        FALLBACK_MODELS.contains(&self.model.as_str())
    }

    /// The request for `body`, with this connection's credentials.
    fn request(&self, body: &Value) -> reqwest::blocking::RequestBuilder {
        let mut betas = Vec::new();
        if self.uses_fallbacks() {
            betas.push(FALLBACK_BETA);
        }
        let mut req = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        req = match &self.auth {
            Auth::ApiKey(key) => req.header("x-api-key", key),
            Auth::Bearer(token) => {
                betas.push(OAUTH_BETA);
                req.bearer_auth(token)
            }
        };
        if let Some(id) = &self.workspace_id {
            req = req.header("anthropic-workspace-id", id);
        }
        if !betas.is_empty() {
            req = req.header("anthropic-beta", betas.join(","));
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
            let resp = result.context("request to Anthropic API failed")?;
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
                bail!("Anthropic API error {status}: {text}");
            }
            return serde_json::from_str(&text).context("invalid JSON from Anthropic API");
        }
    }
}

impl Model for Anthropic {
    fn name(&self) -> String {
        format!("anthropic/{}", self.model)
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
        let resp = super::stream("Anthropic", || self.request(&body), on_text, read_stream)?;
        decode_response(&resp)
    }

    /// The same request with `max_tokens: 0`: it only reads the cached
    /// prefix, which restarts its lifetime, and generates nothing.
    fn keep_warm(&self, request: &ModelRequest) -> Result<Option<ModelResponse>> {
        let mut body = self.body(request);
        body["max_tokens"] = json!(0);
        decode_response(&self.send(&body)?).map(Some)
    }
}

fn encode_message(msg: &Message) -> Value {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let content: Vec<Value> = msg
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => json!({"type": "text", "text": text}),
            ContentBlock::ToolUse { id, name, input } => {
                json!({"type": "tool_use", "id": id, "name": name, "input": input})
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "is_error": is_error,
            }),
            ContentBlock::Reasoning { raw, .. } | ContentBlock::Opaque { raw } => raw.clone(),
        })
        .collect();
    json!({"role": role, "content": content})
}

/// A streamed answer put back together as the API would have sent it whole.
fn read_stream(r: &mut dyn std::io::BufRead, on_text: &mut dyn FnMut(&str)) -> Result<Value> {
    let mut message = json!({"content": []});
    // Each tool call's input arrives as pieces of JSON text.
    let mut inputs: std::collections::HashMap<usize, String> = Default::default();
    let whole = super::events(r, |data| {
        let event: Value = serde_json::from_str(data).context("invalid JSON from Anthropic API")?;
        let index = event["index"].as_u64().unwrap_or(0) as usize;
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                message = event["message"].clone();
                message["content"] = json!([]);
            }
            "content_block_start" => {
                if let Some(content) = message["content"].as_array_mut() {
                    content.resize(content.len().max(index), json!({}));
                    content.insert(index, event["content_block"].clone());
                }
            }
            "content_block_delta" => {
                let delta = &event["delta"];
                let Some(block) = message["content"].get_mut(index) else {
                    bail!("Anthropic API sent a piece of a block it never started");
                };
                let mut add = |field: &str, piece: &Value| {
                    let so_far = block[field].as_str().unwrap_or_default();
                    block[field] = json!(format!("{so_far}{}", piece.as_str().unwrap_or_default()));
                };
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        add("text", &delta["text"]);
                        on_text(&text_of(&message));
                    }
                    "thinking_delta" => add("thinking", &delta["thinking"]),
                    "signature_delta" => add("signature", &delta["signature"]),
                    "input_json_delta" => inputs
                        .entry(index)
                        .or_default()
                        .push_str(delta["partial_json"].as_str().unwrap_or_default()),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let (Some(json), Some(block)) =
                    (inputs.remove(&index), message["content"].get_mut(index))
                    && !json.trim().is_empty()
                {
                    // Malformed input reaches the tool, which reports it to the model.
                    block["input"] = serde_json::from_str(&json).unwrap_or(Value::String(json));
                }
            }
            "message_delta" => {
                for (key, value) in event["delta"].as_object().into_iter().flatten() {
                    message[key] = value.clone();
                }
                for (key, value) in event["usage"].as_object().into_iter().flatten() {
                    message["usage"][key] = value.clone();
                }
            }
            "message_stop" => return Ok(true),
            "error" => bail!("Anthropic API error: {}", event["error"]),
            _ => {}
        }
        Ok(false)
    })?;
    if !whole {
        bail!("the answer broke off");
    }
    Ok(message)
}

/// The words of a message being written, its text blocks one after another.
fn text_of(message: &Value) -> String {
    message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b["text"].as_str().filter(|_| b["type"] == "text"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn decode_response(resp: &Value) -> Result<ModelResponse> {
    let stop_reason = match resp["stop_reason"].as_str() {
        Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        _ => StopReason::Other,
    };
    let blocks = resp["content"]
        .as_array()
        .ok_or_else(|| anyhow!("response has no content array: {resp}"))?;
    let content = blocks
        .iter()
        .map(|b| match b["type"].as_str() {
            Some("text") => ContentBlock::Text {
                text: b["text"].as_str().unwrap_or_default().to_string(),
            },
            Some("tool_use") => ContentBlock::ToolUse {
                id: b["id"].as_str().unwrap_or_default().to_string(),
                name: b["name"].as_str().unwrap_or_default().to_string(),
                input: b["input"].clone(),
            },
            Some("thinking") => ContentBlock::Reasoning {
                summary: b["thinking"].as_str().unwrap_or_default().to_string(),
                raw: b.clone(),
            },
            _ => ContentBlock::Opaque { raw: b.clone() },
        })
        .collect();
    let usage = &resp["usage"];
    let cache_read = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
    let cache_write = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
    Ok(ModelResponse {
        content,
        stop_reason,
        model: resp["model"].as_str().unwrap_or_default().to_string(),
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0) + cache_read + cache_write,
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
    })
}

/// Whether `model` takes adaptive thinking and an effort: Claude 4.6 and
/// later (Haiku from 5). Older ones, like Haiku 4.5, refuse them. Names Airth
/// does not recognize (a proxy's, say) get them.
fn adaptive_thinking(model: &str) -> bool {
    let Some(rest) = model.strip_prefix("claude-") else {
        return true;
    };
    let mut parts = rest.split('-');
    let family = parts.next().unwrap_or_default();
    let number = |p: Option<&str>| {
        p.filter(|p| p.len() <= 2)
            .and_then(|p| p.parse::<u32>().ok())
    };
    let Some(major) = number(parts.next()) else {
        return true;
    };
    let minor = number(parts.next()).unwrap_or(0);
    match family {
        "haiku" => major >= 5,
        "opus" | "sonnet" => (major, minor) >= (4, 6),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_the_instructions_and_keeps_quick_requests_small() {
        let model =
            Anthropic::from_settings(&|k| (k == "ANTHROPIC_API_KEY").then(|| "k".into())).unwrap();
        let messages = [Message::user_text("hi")];
        let request = ModelRequest {
            context: "realm: work",
            ..ModelRequest::new("instructions", &messages, &[])
        };
        let body = model.body(&request);
        assert_eq!(body["system"][0]["text"], "instructions");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(
            body["system"][1],
            json!({"type": "text", "text": "realm: work"})
        );
        assert_eq!(body["cache_control"]["type"], "ephemeral");
        assert_eq!(body["output_config"]["effort"], "high");
        assert_eq!(body["max_tokens"], 16_000);
        let quick = model.body(&ModelRequest {
            quick: true,
            ..ModelRequest::new("triage", &messages, &[])
        });
        assert_eq!(quick["system"].as_array().unwrap().len(), 1);
        assert_eq!(quick["output_config"]["effort"], "low");
        assert_eq!(quick["max_tokens"], QUICK_MAX_TOKENS);
    }

    #[test]
    fn older_models_get_no_adaptive_thinking() {
        for model in [
            "claude-opus-5-5",
            "claude-sonnet-4-6",
            "claude-fable-5-1",
            "custom",
        ] {
            assert!(adaptive_thinking(model), "{model}");
        }
        for model in [
            "claude-haiku-4-5",
            "claude-haiku-4-5-20251001",
            "claude-sonnet-4-5",
            "claude-opus-4-20250514",
        ] {
            assert!(!adaptive_thinking(model), "{model}");
        }
        let haiku = Anthropic::from_settings(&|k| match k {
            "ANTHROPIC_API_KEY" => Some("k".into()),
            "AIRTH_MODEL" => Some("claude-haiku-4-5".into()),
            _ => None,
        })
        .unwrap();
        let messages = [Message::user_text("hi")];
        let body = haiku.body(&ModelRequest::new("x", &messages, &[]));
        assert!(body.get("thinking").is_none() && body.get("output_config").is_none());
    }

    /// A streamed answer comes back as the whole one would have: thinking
    /// and its signature, words heard as they come, a tool call's input sent
    /// in pieces, and the usage of its last events.
    #[test]
    fn a_streamed_answer_is_rebuilt_as_the_whole_one() {
        let events = [
            json!({"type": "message_start", "message": {"model": "claude-opus-5-5", "content": [], "stop_reason": null, "usage": {"input_tokens": 10, "cache_read_input_tokens": 90, "output_tokens": 1}}}),
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "look "}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "first"}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "sig"}}),
            json!({"type": "content_block_stop", "index": 0}),
            json!({"type": "ping"}),
            json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Read"}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "ing."}}),
            json!({"type": "content_block_stop", "index": 1}),
            json!({"type": "content_block_start", "index": 2, "content_block": {"type": "tool_use", "id": "t1", "name": "read_file", "input": {}}}),
            json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "{\"path\": "}}),
            json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "\"x\"}"}}),
            json!({"type": "content_block_stop", "index": 2}),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 5}}),
            json!({"type": "message_stop"}),
        ];
        let text: String = events
            .iter()
            .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
            .collect();
        let mut heard = Vec::new();
        let message =
            read_stream(&mut text.as_bytes(), &mut |t| heard.push(t.to_string())).unwrap();
        assert_eq!(heard, ["Read", "Reading."]);
        let whole = decode_response(&message).unwrap();
        assert_eq!(whole.stop_reason, StopReason::ToolUse);
        assert!(
            matches!(&whole.content[0], ContentBlock::Reasoning { summary, raw } if summary == "look first" && raw["signature"] == "sig")
        );
        assert!(matches!(&whole.content[1], ContentBlock::Text { text } if text == "Reading."));
        assert!(
            matches!(&whole.content[2], ContentBlock::ToolUse { input, .. } if input == &json!({"path": "x"}))
        );
        assert_eq!(
            (
                whole.input_tokens,
                whole.output_tokens,
                whole.cache_read_tokens
            ),
            (100, 5, 90)
        );
        // Cut before its end, it is not an answer.
        let cut = &text[..text.find("message_delta").unwrap()];
        assert!(read_stream(&mut cut.as_bytes(), &mut |_| {}).is_err());
    }

    #[test]
    fn decodes_and_reencodes_blocks_losslessly() {
        let resp = json!({
            "model": "claude-opus-5-5",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "content": [
                {"type": "thinking", "thinking": "look first", "signature": "sig"},
                {"type": "text", "text": "Reading."},
                {"type": "tool_use", "id": "t1", "name": "read_file", "input": {"path": "x"}},
                {"type": "fallback", "from": {"model": "a"}, "to": {"model": "b"}}
            ]
        });
        let decoded = decode_response(&resp).unwrap();
        assert_eq!(decoded.stop_reason, StopReason::ToolUse);
        assert!(
            matches!(&decoded.content[0], ContentBlock::Reasoning { summary, .. } if summary == "look first")
        );
        assert!(matches!(&decoded.content[3], ContentBlock::Opaque { .. }));

        let encoded = encode_message(&Message {
            role: Role::Assistant,
            content: decoded.content,
        });
        assert_eq!(encoded["content"], resp["content"]);
    }
}
