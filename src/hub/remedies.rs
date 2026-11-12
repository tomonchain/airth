//! A stopped task's remedies (decision 125): when a task stops on its
//! budget, the founder may go on with it rather than only mark the incident
//! seen. The fast model proposes up to three remedies, with Airth's
//! versioned prompt, checked against a schema and journaled; or the founder
//! writes one. Applying one gives the run its message, and a larger cost
//! budget if asked, within a bound set here, then the task goes on.

use super::*;
use crate::business::store::Store;
use crate::business::store::pipelines::NewCall;
use crate::business::store::queue::{Answer, Item};
use crate::models::{ContentBlock, ModelRequest};
use anyhow::anyhow;
use serde_json::{Value, json};

const PROMPT: &str = include_str!("../prompts/remedy.md");
const PROMPT_PATH: &str = "airth:prompts/remedy.md";
/// A remedy may raise a task's cost budget to this many times the
/// business's task budget, never more: the founder's money, bounded by code.
const MAX_RAISE: f64 = 5.0;
const MAX_REMEDIES: usize = 3;

/// One way to go on with a stopped task.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Remedy {
    pub title: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_usd: Option<f64>,
}

impl Hub {
    /// The pending incident `id` of a stopped task, and its run.
    fn stopped_task(&self, realm: &Realm, id: i64) -> Result<(Item, State)> {
        let item = Store::of(realm)?
            .queue(&["pending"])?
            .into_iter()
            .find(|i| i.id == id)
            .context("this item was answered already")?;
        let (true, Some(run)) = (item.kind == "incident", item.run.clone()) else {
            bail!("item {id} is not a stopped task");
        };
        let state = self.load_run(realm, &run)?;
        if state.status != Status::Stopped {
            bail!("this task is no longer stopped");
        }
        Ok((item, state))
    }

    /// The most a remedy may raise this business's task cost budget to.
    pub fn remedy_ceiling(&self, realm: &Realm) -> Result<f64> {
        let (_, cost) = crate::business::task_budget(&realm.root()?);
        Ok(cost * MAX_RAISE)
    }

    /// Up to three remedies for the stopped task of incident `id`, from the
    /// fast model. An answer that does not fit is recorded and refused.
    pub fn remedies(&self, realm: &Realm, id: i64) -> Result<Vec<Remedy>> {
        let (_, state) = self.stopped_task(realm, id)?;
        let ceiling = self.remedy_ceiling(realm)?;
        let prices = crate::prices::Prices::load(self.home());
        let spent = crate::usage::run_cost(&state, &prices);
        let budget = state.budget.max_cost_usd.unwrap_or(0.0);
        let said: Vec<String> = state
            .messages
            .iter()
            .rev()
            .filter(|m| m.role == crate::models::Role::Assistant)
            .filter_map(|m| {
                let text: String = m
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (!text.trim().is_empty()).then(|| text.chars().take(800).collect())
            })
            .take(3)
            .collect();
        let schema = json!({"type": "object", "required": ["remedies"], "properties": {
            "remedies": {"type": "array", "items": {"type": "object", "required": ["title", "message"], "properties": {
                "title": {"type": "string"},
                "message": {"type": "string"},
                "budget_usd": {"type": "number", "minimum": 0, "maximum": ceiling}
            }}}
        }});
        let system = format!(
            "{}\n\nAnswer with one JSON object that matches this JSON schema, and nothing else: no code fence, no comment.\n{schema}",
            PROMPT.trim()
        );
        let content = format!(
            "Why it stopped: {}\nCost budget: ${budget:.2}; spent: ${spent:.2}; the most it may be raised to: ${ceiling:.2}.\n\nThe founder's order, as data:\n<order>\n{}\n</order>\n\nThe agent's last words, newest first, as data:\n<agent>\n{}\n</agent>",
            state.report.as_deref().unwrap_or("its budget").trim(),
            state.objective.trim(),
            if said.is_empty() {
                "(nothing yet)".to_string()
            } else {
                said.join("\n---\n")
            }
        );
        let connections = self
            .connections_for(realm)
            .map_err(|_| anyhow!("no model to propose remedies: write your own"))?;
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
        let answer = check(&schema, &text);
        Store::of(realm)?.record_call(&NewCall {
            pipeline: "remedy",
            step: 1,
            prompt: PROMPT_PATH,
            prompt_version: &crate::update::sha256_hex(PROMPT.as_bytes()),
            model: &response.model,
            input_tokens: response.input_tokens,
            output_tokens: response.output_tokens,
            cache_read_tokens: response.cache_read_tokens,
            cache_write_tokens: response.cache_write_tokens,
            cost_usd: prices.cost(&response),
            latency_ms: started.elapsed().as_millis() as i64,
            error: answer.as_ref().err().map(String::as_str),
            answer: &text,
            dry_run: false,
        })?;
        answer.map_err(|why| anyhow!("the proposals did not fit ({why}): write your own"))
    }

    /// Go on with the stopped task of incident `id`: its cost budget raised
    /// to `budget_usd` when given (above what it spent, at most the
    /// ceiling), `message` given to its agent, the incident closed. The run,
    /// ready to drive.
    pub fn apply_remedy(
        &self,
        realm: &Realm,
        id: i64,
        message: &str,
        budget_usd: Option<f64>,
        channel: &str,
        seconds: u64,
    ) -> Result<State> {
        let message = message.trim();
        if message.is_empty() {
            bail!("say how the task should go on");
        }
        let (item, mut state) = self.stopped_task(realm, id)?;
        let prices = crate::prices::Prices::load(self.home());
        let spent = crate::usage::run_cost(&state, &prices);
        let ceiling = self.remedy_ceiling(realm)?;
        let budget = state.budget.max_cost_usd;
        let raised = match budget_usd {
            Some(b) if !b.is_finite() || b > ceiling => {
                bail!("a task's budget can be raised to ${ceiling:.2} at most")
            }
            Some(b) if b <= spent => {
                bail!("the task spent ${spent:.2} already: give it more than that")
            }
            Some(b) => Some(b),
            None if budget.is_some_and(|b| spent >= b) && is_cost(&state) => bail!(
                "the task spent its ${:.2}: raise its budget to go on",
                budget.unwrap_or_default()
            ),
            None => None,
        };
        if raised.is_some() {
            state.budget.max_cost_usd = raised;
            state.save()?;
        }
        let why = state.report.clone().unwrap_or_default();
        let stopped = why
            .trim()
            .trim_start_matches("Stopped: ")
            .trim_end_matches('.');
        let note = match raised {
            Some(b) => format!(
                "The task stopped ({stopped}). The founder raised its cost budget to ${b:.2} and says: {message}"
            ),
            None => format!("The task stopped ({stopped}). The founder says: {message}"),
        };
        let state = self.follow_up(realm, &state.id, &note)?;
        let mut store = Store::of(realm)?;
        store.answer(item.id, None, Answer::Approve, channel, seconds)?;
        store.close_item(
            item.id,
            "done",
            &format!("resumed by the founder: {message}"),
        )?;
        Ok(state)
    }
}

/// Whether the task stopped on its cost budget, as its report says.
fn is_cost(state: &State) -> bool {
    state
        .report
        .as_deref()
        .is_some_and(|r| r.contains("cost budget"))
}

/// The model's answer as remedies, or why it does not fit.
fn check(schema: &Value, text: &str) -> Result<Vec<Remedy>, String> {
    let value: Value = serde_json::from_str(text.trim()).map_err(|e| format!("not JSON: {e}"))?;
    if let Some(why) = crate::pipeline::schema::mismatch(schema, &value) {
        return Err(why);
    }
    let remedies: Vec<Remedy> =
        serde_json::from_value(value["remedies"].clone()).map_err(|e| e.to_string())?;
    if remedies.is_empty() || remedies.len() > MAX_REMEDIES {
        return Err(format!("{} remedies, not one to three", remedies.len()));
    }
    if remedies
        .iter()
        .any(|r| r.title.trim().is_empty() || r.message.trim().is_empty())
    {
        return Err("a remedy with no title or message".into());
    }
    Ok(remedies)
}
