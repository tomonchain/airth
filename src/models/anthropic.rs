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
            "thinking": {"type": "adaptive", "display": "summarized"},
            "output_config": {"effort": effort},
            // Automatic prompt caching: the history prefix is re-sent every iteration.
            "cache_control": {"type": "ephemeral"},
        });
        if self.uses_fallbacks() {
            body["fallbacks"] = json!("default");
        }
        body
    }

    fn uses_fallbacks(&self) -> bool {
        FALLBACK_MODELS.contains(&self.model.as_str())
    }

    fn send(&self, body: &Value) -> Result<Value> {
        let mut betas = Vec::new();
        if self.uses_fallbacks() {
            betas.push(FALLBACK_BETA);
        }
        let mut attempt = 0;
        loop {
            let mut req = self
                .client
                .post(format!("{}/v1/messages", self.base_url))
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json");
            let mut req_betas = betas.clone();
            req = match &self.auth {
                Auth::ApiKey(key) => req.header("x-api-key", key),
                Auth::Bearer(token) => {
                    req_betas.push(OAUTH_BETA);
                    req.bearer_auth(token)
                }
            };
            if let Some(id) = &self.workspace_id {
                req = req.header("anthropic-workspace-id", id);
            }
            if !req_betas.is_empty() {
                req = req.header("anthropic-beta", req_betas.join(","));
            }

            let result = req.json(body).send();
            let retryable = match &result {
                Ok(resp) => {
                    let s = resp.status().as_u16();
                    s == 408 || s == 409 || s == 429 || s >= 500
                }
                Err(e) => e.is_timeout() || e.is_connect(),
            };
            if retryable && attempt < MAX_RETRIES {
                attempt += 1;
                std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
                continue;
            }
            let resp = result.context("request to Anthropic API failed")?;
            let status = resp.status();
            let text = resp.text()?;
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
