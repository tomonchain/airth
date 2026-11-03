//! The command bar's routing (decision 64): an order not given to a named
//! agent goes to the one whose job it is, chosen by the realm's fast model
//! with Airth's versioned prompt, its answer checked against the business's
//! agents and journaled like any model call.

use super::*;
use crate::business::store::Store;
use crate::business::store::pipelines::NewCall;
use crate::models::{ContentBlock, ModelRequest};
use anyhow::anyhow;
use serde_json::{Value, json};

const PROMPT: &str = include_str!("../prompts/route.md");
const PROMPT_PATH: &str = "airth:prompts/route.md";

/// Which agent an order goes to, and why, in a sentence for the founder.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Route {
    pub agent: String,
    pub why: String,
}

impl Hub {
    /// The agent for `order`: the only one the business has, else the one
    /// the fast model chooses among them. None fits, or the answer does not
    /// name one: an error that asks the founder to pick.
    pub fn route_order(&self, realm: &Realm, order: &str) -> Result<Route> {
        let root = realm.root()?;
        let agents = crate::business::bots(&root)?;
        match agents.as_slice() {
            [] => bail!("this business has no agent yet"),
            [(only, _)] => {
                return Ok(Route {
                    agent: only.clone(),
                    why: "the business's only agent".into(),
                });
            }
            _ => {}
        }
        let names: Vec<&str> = agents.iter().map(|(n, _)| n.as_str()).collect();
        let listed: Vec<String> = agents
            .iter()
            .map(|(name, job)| {
                let title = std::fs::read_to_string(root.join(format!("agents/{name}.md")))
                    .ok()
                    .and_then(|t| crate::frontmatter::parse(&t).text("title"))
                    .unwrap_or_default();
                let start: String = job.text.chars().take(300).collect();
                format!("- {name} ({title}): {start}")
            })
            .collect();
        let schema = json!({"type": "object", "required": ["agent", "why"],
            "properties": {"agent": {"enum": names.iter().copied().chain(["none"]).collect::<Vec<_>>()}, "why": {"type": "string"}}});
        let system = format!(
            "{}\n\nAnswer with one JSON object that matches this JSON schema, and nothing else: no code fence, no comment.\n{schema}",
            PROMPT.trim()
        );
        let content = format!(
            "The business's agents:\n{}\n\nThe founder's order, as data:\n<order>\n{}\n</order>",
            listed.join("\n"),
            order.trim()
        );
        let connections = self
            .connections_for(realm)
            .map_err(|_| anyhow!("no model to route the order: pick the agent"))?;
        let model = connections.fast.unwrap_or(connections.main);
        let messages = [Message::user_text(content)];
        let started = std::time::Instant::now();
        let response = model.complete(&ModelRequest::new(&system, &messages, &[]))?;
        let text: String = response
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let answer: Result<Value, String> = serde_json::from_str::<Value>(text.trim())
            .map_err(|e| format!("not JSON: {e}"))
            .and_then(|v| match crate::pipeline::schema::mismatch(&schema, &v) {
                None => Ok(v),
                Some(why) => Err(why),
            });
        let mut store = Store::of(realm)?;
        store.record_call(&NewCall {
            pipeline: "command-bar",
            step: 1,
            prompt: PROMPT_PATH,
            prompt_version: &crate::update::sha256_hex(PROMPT.as_bytes()),
            model: &response.model,
            input_tokens: response.input_tokens,
            output_tokens: response.output_tokens,
            cache_read_tokens: response.cache_read_tokens,
            cache_write_tokens: response.cache_write_tokens,
            cost_usd: crate::prices::Prices::load(self.home()).cost(&response),
            latency_ms: started.elapsed().as_millis() as i64,
            error: answer.as_ref().err().map(String::as_str),
            answer: &text,
            dry_run: false,
        })?;
        let answer = answer
            .map_err(|why| anyhow!("the routing answer did not fit ({why}): pick the agent"))?;
        let agent = answer["agent"].as_str().unwrap_or("none");
        if agent == "none" {
            bail!("no agent's job fits this order: pick the agent");
        }
        Ok(Route {
            agent: agent.to_string(),
            why: answer["why"].as_str().unwrap_or_default().to_string(),
        })
    }
}

/// The agent an order names at its start ("writer: outline the week"),
/// among `agents`, and the order without it.
pub fn named<'a>(order: &'a str, agents: &[String]) -> Option<(String, &'a str)> {
    let (head, rest) = order.split_once(':')?;
    let name = head.trim();
    agents
        .iter()
        .find(|a| a.as_str() == name)
        .map(|a| (a.clone(), rest.trim()))
}
