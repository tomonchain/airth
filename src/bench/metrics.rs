//! What a benchmark measures, read from what every run already records
//! (its events and per-call usage, see state.rs): no separate
//! instrumentation. Then the suite-level scores.
//!
//! Costs are estimates from public list prices, and are labelled as such
//! wherever they are shown.

use super::storage::{BenchRun, Outcome, SecurityOutcome, TaskResult};
use super::suite::Category;
use crate::state::{EventStatus, State, Usage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One task attempt's measurements.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metrics {
    pub duration_ms: u64,
    pub model_calls: u64,
    /// All input tokens, cached ones included.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub tool_calls: u64,
    /// Tool calls that failed (an error from the tool itself).
    pub tool_failures: u64,
    pub permission_denials: u64,
    /// Tool calls made again right after the same tool failed.
    pub retries: u64,
    /// Tool calls identical to an earlier one (same tool, same arguments).
    pub repeated_actions: u64,
    pub compactions: u32,
    /// Times a call went to another model than the previous call.
    pub model_switches: u64,
    /// Input tokens of each model call, in order: the context's size over time.
    pub context: Vec<u64>,
    pub models: Vec<String>,
    /// The run stopped on its iteration, token or time budget.
    pub budget_exhausted: bool,
    /// The run was stopped before it finished (by the owner or the time watchdog).
    pub interrupted: bool,
    /// Estimated USD; None when a model's price is unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
}

impl Metrics {
    pub fn of(state: &State, duration_ms: u64, prices: &Prices) -> Self {
        let mut m = Metrics {
            duration_ms,
            model_calls: state.usage.len() as u64,
            tool_calls: state.events.len() as u64,
            compactions: state.compaction.as_ref().map_or(0, |c| c.count),
            context: state.usage.iter().map(|u| u.input).collect(),
            estimated_cost_usd: prices.cost(&state.usage),
            ..Default::default()
        };
        for u in &state.usage {
            m.input_tokens += u.input;
            m.output_tokens += u.output;
            m.cache_read_tokens += u.cache_read;
            m.cache_write_tokens += u.cache_write;
            if !m.models.contains(&u.model) {
                m.models.push(u.model.clone());
            }
        }
        m.model_switches = state
            .usage
            .windows(2)
            .filter(|w| w[0].model != w[1].model)
            .count() as u64;
        for (i, e) in state.events.iter().enumerate() {
            match e.status {
                EventStatus::Error => m.tool_failures += 1,
                EventStatus::Denied => m.permission_denials += 1,
                EventStatus::Ok => {}
            }
            let earlier = &state.events[..i];
            if earlier
                .last()
                .is_some_and(|p| p.action == e.action && p.status != EventStatus::Ok)
            {
                m.retries += 1;
            }
            if e.action != "update_plan"
                && earlier
                    .iter()
                    .any(|p| p.action == e.action && p.arguments == e.arguments)
            {
                m.repeated_actions += 1;
            }
        }
        let report = state.report.as_deref().unwrap_or_default();
        m.budget_exhausted = report.starts_with("Stopped: ");
        m
    }

    /// The run hit a failure it could have recovered from.
    pub fn had_failure(&self) -> bool {
        self.tool_failures + self.permission_denials > 0
    }
}

/// USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl Price {
    /// Cache reads at a tenth of the input price, 5-minute cache writes at
    /// 1.25 times, as Anthropic bills them.
    const fn standard(input: f64, output: f64) -> Self {
        Price {
            input,
            output,
            cache_read: input / 10.0,
            cache_write: input * 1.25,
        }
    }
}

/// List prices of Anthropic's first-party API (October 2026), by model id
/// prefix. Other providers have none: their costs show as unknown unless
/// the owner adds prices in `benchmarks/prices.json`.
const LIST_PRICES: &[(&str, Price)] = &[
    (
        "claude-fable-5-1",
        Price {
            cache_read: 0.25,
            ..Price::standard(10.0, 50.0)
        },
    ),
    ("claude-fable-5", Price::standard(10.0, 50.0)),
    (
        "claude-opus-5-5",
        Price {
            cache_read: 0.20,
            ..Price::standard(4.0, 20.0)
        },
    ),
    ("claude-opus-5", Price::standard(5.0, 25.0)),
    ("claude-opus-4-8", Price::standard(5.0, 25.0)),
    ("claude-opus-4-7", Price::standard(5.0, 25.0)),
    ("claude-opus-4-6", Price::standard(5.0, 25.0)),
    ("claude-sonnet-5-5", Price::standard(2.0, 10.0)),
    ("claude-sonnet-5", Price::standard(2.0, 10.0)),
    ("claude-sonnet-4-6", Price::standard(3.0, 15.0)),
    ("claude-haiku-5-5", Price::standard(0.10, 0.50)),
    ("claude-haiku-4-5", Price::standard(1.0, 5.0)),
];

/// Prices by model id prefix, the longest prefix winning.
#[derive(Debug, Clone)]
pub struct Prices(Vec<(String, Price)>);

impl Default for Prices {
    fn default() -> Self {
        Self::with(BTreeMap::new())
    }
}

impl Prices {
    /// The list prices, with `overrides` added (or replacing them).
    pub fn with(overrides: BTreeMap<String, Price>) -> Self {
        let mut all: Vec<(String, Price)> = LIST_PRICES
            .iter()
            .filter(|(p, _)| !overrides.contains_key(*p))
            .map(|(p, price)| (p.to_string(), *price))
            .collect();
        all.extend(overrides);
        all.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        Self(all)
    }

    pub fn get(&self, model: &str) -> Option<Price> {
        // Without the provider: "anthropic/claude-…", Bedrock's "anthropic.claude-…".
        let id = model.rsplit('/').next().unwrap_or(model);
        let id = id
            .split_once('.')
            .filter(|(_, rest)| rest.starts_with("claude"))
            .map_or(id, |(_, rest)| rest);
        self.0
            .iter()
            .find(|(p, _)| id.starts_with(p.as_str()))
            .map(|(_, price)| *price)
    }

    /// The estimated cost of these calls, if every model's price is known.
    pub fn cost(&self, usage: &[Usage]) -> Option<f64> {
        let mut total = 0.0;
        for u in usage {
            let p = self.get(&u.model)?;
            let fresh = u.input.saturating_sub(u.cache_read + u.cache_write);
            total += (fresh as f64 * p.input
                + u.cache_read as f64 * p.cache_read
                + u.cache_write as f64 * p.cache_write
                + u.output as f64 * p.output)
                / 1_000_000.0;
        }
        Some(total)
    }
}

/// A run's scores. Rates count executed tasks only: skipped tasks are
/// reported apart, never as failures or successes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub planned: usize,
    pub executed: usize,
    pub passed: usize,
    pub failed: usize,
    pub timeout: usize,
    pub error: usize,
    pub skipped: usize,
    /// passed / executed.
    pub success_rate: Option<f64>,
    /// Executed tasks that hit a tool failure or denial, and of those, the
    /// ones that still passed.
    pub recoverable: usize,
    pub recovered: usize,
    pub recovery_rate: Option<f64>,
    /// Estimated: the cost of every executed task over the tasks passed.
    pub avg_cost_usd: Option<f64>,
    pub total_cost_usd: Option<f64>,
    pub avg_duration_ms: Option<f64>,
    pub tokens: Tokens,
    pub security: SecurityCount,
    /// True when an attack succeeded: the run is not a success, whatever
    /// its other scores.
    pub security_violation: bool,
    pub by_category: BTreeMap<Category, CategoryScore>,
    /// Per task, over its attempts.
    pub tasks: Vec<TaskStats>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub model_calls: u64,
    pub tool_calls: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SecurityCount {
    pub prevented: usize,
    pub succeeded: usize,
    pub inconclusive: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CategoryScore {
    pub executed: usize,
    pub passed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskStats {
    pub task: String,
    pub category: Option<Category>,
    pub attempts: usize,
    pub executed: usize,
    pub passed: usize,
    pub mean_duration_ms: Option<f64>,
    /// Sample standard deviation, with two executed attempts or more.
    pub sd_duration_ms: Option<f64>,
    pub mean_cost_usd: Option<f64>,
}

pub fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

pub fn sd(xs: &[f64]) -> Option<f64> {
    let m = mean(xs)?;
    (xs.len() >= 2)
        .then(|| (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64).sqrt())
}

/// Sum of the estimates, None if any is unknown.
fn total_cost<'a>(results: impl Iterator<Item = &'a TaskResult>) -> Option<f64> {
    results.map(|r| r.metrics.estimated_cost_usd).sum()
}

impl Summary {
    pub fn of(run: &BenchRun) -> Self {
        let mut s = Summary {
            planned: run.planned,
            ..Default::default()
        };
        let executed: Vec<&TaskResult> = run.results.iter().filter(|r| r.executed()).collect();
        for r in &run.results {
            match r.outcome {
                Outcome::Pass => s.passed += 1,
                Outcome::Fail => s.failed += 1,
                Outcome::Timeout => s.timeout += 1,
                Outcome::Error => s.error += 1,
                Outcome::Skipped => s.skipped += 1,
            }
            let c = s.by_category.entry(r.category).or_default();
            if r.executed() {
                c.executed += 1;
                c.passed += usize::from(r.outcome == Outcome::Pass);
            } else {
                c.skipped += 1;
            }
            match r.security {
                Some(SecurityOutcome::Prevented) => s.security.prevented += 1,
                Some(SecurityOutcome::Succeeded) => s.security.succeeded += 1,
                Some(SecurityOutcome::Inconclusive) => s.security.inconclusive += 1,
                None => {}
            }
        }
        s.executed = executed.len();
        s.security_violation = s.security.succeeded > 0;
        s.success_rate = (s.executed > 0).then(|| s.passed as f64 / s.executed as f64);
        let recoverable: Vec<_> = executed
            .iter()
            .filter(|r| r.metrics.had_failure())
            .collect();
        s.recoverable = recoverable.len();
        s.recovered = recoverable
            .iter()
            .filter(|r| r.outcome == Outcome::Pass)
            .count();
        s.recovery_rate = (s.recoverable > 0).then(|| s.recovered as f64 / s.recoverable as f64);
        s.total_cost_usd = (!executed.is_empty())
            .then(|| total_cost(executed.iter().copied()))
            .flatten();
        s.avg_cost_usd = s
            .total_cost_usd
            .filter(|_| s.passed > 0)
            .map(|c| c / s.passed as f64);
        let durations: Vec<f64> = executed
            .iter()
            .map(|r| r.metrics.duration_ms as f64)
            .collect();
        s.avg_duration_ms = mean(&durations);
        for r in &run.results {
            let m = &r.metrics;
            s.tokens.input += m.input_tokens;
            s.tokens.output += m.output_tokens;
            s.tokens.cache_read += m.cache_read_tokens;
            s.tokens.cache_write += m.cache_write_tokens;
            s.tokens.model_calls += m.model_calls;
            s.tokens.tool_calls += m.tool_calls;
        }
        let mut order: Vec<&str> = Vec::new();
        for r in &run.results {
            if !order.contains(&r.task.as_str()) {
                order.push(&r.task);
            }
        }
        s.tasks = order
            .into_iter()
            .map(|task| {
                let all: Vec<&TaskResult> = run.results.iter().filter(|r| r.task == task).collect();
                let ran: Vec<&&TaskResult> = all.iter().filter(|r| r.executed()).collect();
                let durations: Vec<f64> =
                    ran.iter().map(|r| r.metrics.duration_ms as f64).collect();
                TaskStats {
                    task: task.to_string(),
                    category: all.first().map(|r| r.category),
                    attempts: all.len(),
                    executed: ran.len(),
                    passed: ran.iter().filter(|r| r.outcome == Outcome::Pass).count(),
                    mean_duration_ms: mean(&durations),
                    sd_duration_ms: sd(&durations),
                    mean_cost_usd: (!ran.is_empty())
                        .then(|| total_cost(ran.iter().map(|r| **r)))
                        .flatten()
                        .map(|c| c / ran.len() as f64),
                }
            })
            .collect();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Event;
    use chrono::Utc;
    use serde_json::json;

    fn usage(model: &str, input: u64, output: u64, cache_read: u64) -> Usage {
        Usage {
            at: Utc::now(),
            model: model.into(),
            input,
            output,
            cache_read,
            cache_write: 0,
        }
    }

    fn event(action: &str, args: serde_json::Value, status: EventStatus) -> Event {
        Event {
            timestamp: Utc::now(),
            action: action.into(),
            arguments: args,
            result: String::new(),
            status,
            tool_use_id: None,
        }
    }

    #[test]
    fn estimates_cost_from_list_prices_and_cached_input() {
        let prices = Prices::default();
        // 1M fresh input + 1M output on Sonnet 5.5: $2 + $10.
        let cost = prices
            .cost(&[usage("claude-sonnet-5-5", 1_000_000, 1_000_000, 0)])
            .unwrap();
        assert!((cost - 12.0).abs() < 1e-9);
        // Cached input costs the cache price instead.
        let cost = prices
            .cost(&[usage("claude-opus-5-5", 1_000_000, 0, 1_000_000)])
            .unwrap();
        assert!((cost - 0.20).abs() < 1e-9);
        // Prefixes: provider names are ignored, the longest prefix wins.
        assert_eq!(
            prices.get("anthropic/claude-fable-5-1").unwrap().cache_read,
            0.25
        );
        assert_eq!(prices.get("claude-fable-5").unwrap().cache_read, 1.0);
        // Unknown models make the whole estimate unknown.
        assert!(prices.cost(&[usage("gpt-x", 10, 10, 0)]).is_none());
        let custom = Prices::with(BTreeMap::from([(
            "gpt-x".to_string(),
            Price::standard(1.0, 1.0),
        )]));
        assert!(custom.cost(&[usage("openai/gpt-x", 10, 10, 0)]).is_some());
    }

    #[test]
    fn reads_harness_metrics_from_the_run() {
        let mut state = State::new("x", None);
        state.usage = vec![
            usage("a", 100, 10, 0),
            usage("a", 300, 10, 0),
            usage("b", 500, 10, 0),
        ];
        state.events = vec![
            event("read_file", json!({"path": "x"}), EventStatus::Error),
            event("read_file", json!({"path": "y"}), EventStatus::Ok),
            event("read_file", json!({"path": "y"}), EventStatus::Ok),
            event(
                "execute_command",
                json!({"command": "ls"}),
                EventStatus::Denied,
            ),
        ];
        state.report = Some("Stopped: reached the iteration limit (3).".into());
        let m = Metrics::of(&state, 1500, &Prices::default());
        assert_eq!(m.model_calls, 3);
        assert_eq!(m.context, vec![100, 300, 500]);
        assert_eq!(m.model_switches, 1);
        assert_eq!(m.models, vec!["a", "b"]);
        assert_eq!((m.tool_failures, m.permission_denials), (1, 1));
        assert_eq!(m.retries, 1);
        assert_eq!(m.repeated_actions, 1);
        assert!(m.budget_exhausted && m.had_failure());
        assert!(m.estimated_cost_usd.is_none());
    }

    #[test]
    fn sample_statistics() {
        assert_eq!(mean(&[]), None);
        assert_eq!(sd(&[3.0]), None);
        assert!((sd(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap() - 2.138).abs() < 1e-3);
    }
}
