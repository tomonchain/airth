//! Token usage: what every run's model calls consumed, across realms, by day,
//! realm, model, purpose and run, with its estimated cost. Shown to the owner
//! in the app; runs never see it.

use crate::hub::Hub;
use crate::prices::{Price, Prices};
use crate::state::{Purpose, State, Usage};
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;
use std::collections::HashMap;

/// The model name for tokens of runs from before each call was recorded.
pub const UNRECORDED: &str = "unrecorded";

/// Estimated USD from list prices, by what was billed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Cost {
    /// Input neither read from nor written to the cache.
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
}

impl Cost {
    fn of(u: &Usage, p: Price) -> Self {
        let m = 1_000_000.0;
        Cost {
            input: fresh(u) as f64 * p.input / m,
            cache_read: u.cache_read as f64 * p.cache_read / m,
            cache_write: u.cache_write as f64 * p.cache_write / m,
            output: u.output as f64 * p.output / m,
        }
    }

    fn add(&mut self, c: Cost) {
        self.input += c.input;
        self.cache_read += c.cache_read;
        self.cache_write += c.cache_write;
        self.output += c.output;
    }

    pub fn total(&self) -> f64 {
        self.input + self.cache_read + self.cache_write + self.output
    }
}

/// Input tokens billed at the full price.
fn fresh(u: &Usage) -> u64 {
    u.input.saturating_sub(u.cache_read + u.cache_write)
}

/// Token counts. `input` includes the cached input (`cache_read`, `cache_write`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub calls: u64,
    /// The largest input of a single call: the context at its biggest.
    pub peak_input: u64,
    /// The calls' cost; calls of a model without a known price are left out.
    pub cost: Cost,
    /// Calls left out of `cost`.
    pub unpriced: u64,
}

impl Tokens {
    fn add(&mut self, u: &Usage, price: Option<Price>) {
        self.input += u.input;
        self.output += u.output;
        self.cache_read += u.cache_read;
        self.cache_write += u.cache_write;
        self.calls += 1;
        // The unrecorded remainder lumps many calls together.
        if u.model != UNRECORDED {
            self.peak_input = self.peak_input.max(u.input);
        }
        match price {
            Some(p) => self.cost.add(Cost::of(u, p)),
            None => self.unpriced += 1,
        }
    }

    pub fn total(&self) -> u64 {
        self.input + self.output
    }
}

#[derive(Debug, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    #[serde(flatten)]
    pub tokens: Tokens,
}

/// A realm's or a model's part of the window.
#[derive(Debug, Serialize)]
pub struct Share {
    pub name: String,
    #[serde(flatten)]
    pub tokens: Tokens,
}

#[derive(Debug, Serialize)]
pub struct RunUsage {
    pub realm: String,
    pub id: String,
    pub objective: String,
    /// Started by a realm objective rather than by hand.
    pub scheduled: bool,
    pub compactions: u32,
    #[serde(flatten)]
    pub tokens: Tokens,
}

#[derive(Debug, Serialize)]
pub struct Report {
    /// The window: the last `days` days, today included.
    pub days: u32,
    pub window: Tokens,
    pub today: Tokens,
    /// The current calendar month, whatever the window.
    pub month: Tokens,
    /// One entry per day of the window, oldest first, empty days included.
    pub by_day: Vec<Day>,
    pub by_realm: Vec<Share>,
    pub by_model: Vec<Share>,
    /// Why the calls were made: work, compaction, warm, triage, title.
    pub by_purpose: Vec<Share>,
    /// Chats started by hand, and runs started by a realm objective.
    pub by_source: Vec<Share>,
    /// By the routine (objective) that started the run; "" for runs
    /// started by hand.
    pub by_routine: Vec<Share>,
    /// The runs that cost the most in the window (used the most tokens,
    /// when no price is known).
    pub top_runs: Vec<RunUsage>,
}

/// Usage of every realm's runs (or only `only`'s) over the last `days` days,
/// in `now`'s time zone.
pub fn report<Tz: TimeZone>(
    hub: &Hub,
    days: u32,
    now: DateTime<Tz>,
    only: Option<&str>,
) -> Result<Report> {
    let mut runs = Vec::new();
    for realm in hub.home().realms()? {
        if only.is_some_and(|name| name != realm.name()) {
            continue;
        }
        for state in hub.all_runs(&realm)? {
            runs.push((realm.name().to_string(), state));
        }
    }
    let prices = Prices::load(hub.home());
    Ok(aggregate(&runs, days, now, &prices))
}

/// A run's model calls. Runs from before calls were recorded (or the part of
/// a run that predates it) count as one call when the run started.
fn calls(state: &State) -> Vec<Usage> {
    let mut calls = state.usage.clone();
    let input: u64 = calls.iter().map(|u| u.input).sum();
    let output: u64 = calls.iter().map(|u| u.output).sum();
    let (input, output) = (
        state.input_tokens.saturating_sub(input),
        state.output_tokens.saturating_sub(output),
    );
    if input + output > 0 {
        calls.insert(
            0,
            Usage {
                at: started(state),
                model: UNRECORDED.into(),
                input,
                output,
                cache_read: 0,
                cache_write: 0,
                purpose: Purpose::Work,
            },
        );
    }
    calls
}

/// When the run started: its id is its creation time.
fn started(state: &State) -> DateTime<Utc> {
    NaiveDateTime::parse_from_str(state.id.trim_end_matches('Z'), "%Y%m%dT%H%M%S%.3f")
        .map(|t| t.and_utc())
        .unwrap_or(state.updated_at)
}

fn first_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default().trim();
    if line.chars().count() > 120 {
        format!("{}…", line.chars().take(119).collect::<String>())
    } else {
        line.to_string()
    }
}

fn purpose_name(p: Purpose) -> &'static str {
    match p {
        Purpose::Work => "work",
        Purpose::Compaction => "compaction",
        Purpose::Warm => "warm",
        Purpose::Triage => "triage",
        Purpose::Title => "title",
    }
}

/// Costliest first, then the most tokens.
fn by_weight(a: &Tokens, b: &Tokens) -> std::cmp::Ordering {
    b.cost
        .total()
        .total_cmp(&a.cost.total())
        .then(b.total().cmp(&a.total()))
}

fn shares(map: HashMap<String, Tokens>) -> Vec<Share> {
    let mut v: Vec<Share> = map
        .into_iter()
        .map(|(name, tokens)| Share { name, tokens })
        .collect();
    v.sort_by(|a, b| by_weight(&a.tokens, &b.tokens).then(a.name.cmp(&b.name)));
    v
}

pub fn aggregate<Tz: TimeZone>(
    runs: &[(String, State)],
    days: u32,
    now: DateTime<Tz>,
    prices: &Prices,
) -> Report {
    let days = days.clamp(1, 366);
    let tz = now.timezone();
    let today = now.date_naive();
    let first = today - Duration::days(i64::from(days) - 1);
    let mut by_day: Vec<Day> = (0..days)
        .map(|i| Day {
            date: first + Duration::days(i64::from(i)),
            tokens: Tokens::default(),
        })
        .collect();
    let mut report = Report {
        days,
        window: Tokens::default(),
        today: Tokens::default(),
        month: Tokens::default(),
        by_day: Vec::new(),
        by_realm: Vec::new(),
        by_model: Vec::new(),
        by_purpose: Vec::new(),
        by_source: Vec::new(),
        by_routine: Vec::new(),
        top_runs: Vec::new(),
    };
    let mut realms: HashMap<String, Tokens> = HashMap::new();
    let mut models: HashMap<String, Tokens> = HashMap::new();
    let mut purposes: HashMap<String, Tokens> = HashMap::new();
    let mut sources: HashMap<String, Tokens> = HashMap::new();
    let mut routines: HashMap<String, Tokens> = HashMap::new();
    let mut top = Vec::new();
    for (realm, state) in runs {
        let mut run = Tokens::default();
        let routine = state
            .origin
            .as_ref()
            .map_or(String::new(), |o| o.objective.clone());
        let source = if state.origin.is_some() {
            "objectives"
        } else {
            "chats"
        };
        for call in calls(state) {
            let price = prices.get(&call.model);
            let date = call.at.with_timezone(&tz).date_naive();
            if date.year() == today.year() && date.month() == today.month() && date <= today {
                report.month.add(&call, price);
            }
            if date < first || date > today {
                continue;
            }
            if date == today {
                report.today.add(&call, price);
            }
            let index = (date - first).num_days() as usize;
            by_day[index].tokens.add(&call, price);
            report.window.add(&call, price);
            run.add(&call, price);
            realms.entry(realm.clone()).or_default().add(&call, price);
            models
                .entry(call.model.clone())
                .or_default()
                .add(&call, price);
            purposes
                .entry(purpose_name(call.purpose).into())
                .or_default()
                .add(&call, price);
            sources.entry(source.into()).or_default().add(&call, price);
            routines
                .entry(routine.clone())
                .or_default()
                .add(&call, price);
        }
        if run.calls > 0 {
            top.push(RunUsage {
                realm: realm.clone(),
                id: state.id.clone(),
                objective: first_line(&state.objective),
                scheduled: state.origin.is_some(),
                compactions: state.compaction.as_ref().map_or(0, |c| c.count),
                tokens: run,
            });
        }
    }
    top.sort_by(|a, b| by_weight(&a.tokens, &b.tokens));
    top.truncate(10);
    report.by_day = std::mem::take(&mut by_day);
    report.by_realm = shares(realms);
    report.by_model = shares(models);
    report.by_purpose = shares(purposes);
    report.by_source = shares(sources);
    report.by_routine = shares(routines);
    report.top_runs = top;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap()
    }

    fn call(day: u32, model: &str, input: u64, output: u64, cache_read: u64) -> Usage {
        Usage {
            at: at(day, 12),
            model: model.into(),
            input,
            output,
            cache_read,
            cache_write: 0,
            purpose: Purpose::Work,
        }
    }

    fn run(id: &str, objective: &str, calls: Vec<Usage>) -> State {
        let mut s = State::new(objective, None);
        s.id = id.into();
        s.input_tokens = calls.iter().map(|u| u.input).sum();
        s.output_tokens = calls.iter().map(|u| u.output).sum();
        s.usage = calls;
        s
    }

    #[test]
    fn adds_up_by_day_realm_model_and_run() {
        let runs = vec![
            (
                "acme".to_string(),
                run(
                    "20261010T080000.000Z",
                    "Build the kit\nwith details",
                    vec![
                        call(10, "opus", 1000, 100, 800),
                        call(12, "opus", 2000, 200, 1500),
                    ],
                ),
            ),
            (
                "personal".to_string(),
                run(
                    "20261012T080000.000Z",
                    "Plan the week",
                    vec![call(12, "sonnet", 300, 30, 0)],
                ),
            ),
        ];
        let r = aggregate(&runs, 7, at(12, 18), &Prices::default());
        assert_eq!(r.by_day.len(), 7);
        assert_eq!(r.by_day.last().unwrap().date, at(12, 0).date_naive());
        assert_eq!(r.by_day[6].tokens.input, 2300);
        assert_eq!(r.by_day[4].tokens.calls, 1); // the 10th
        assert_eq!(r.window.total(), 3630);
        assert_eq!(r.window.cache_read, 2300);
        assert_eq!(r.window.calls, 3);
        assert_eq!(r.today.total(), 2530);
        assert_eq!(r.month.total(), 3630);
        assert_eq!(r.by_realm[0].name, "acme");
        assert_eq!(r.by_realm[0].tokens.total(), 3300);
        assert_eq!(r.by_model[1].name, "sonnet");
        assert_eq!(r.top_runs[0].objective, "Build the kit");
        assert_eq!(r.top_runs[1].realm, "personal");
    }

    #[test]
    fn the_window_excludes_older_days_but_the_month_keeps_them() {
        let runs = vec![(
            "acme".to_string(),
            run(
                "20261001T080000.000Z",
                "Old",
                vec![call(1, "opus", 500, 50, 0), call(12, "opus", 10, 1, 0)],
            ),
        )];
        let r = aggregate(&runs, 7, at(12, 18), &Prices::default());
        assert_eq!(r.window.total(), 11);
        assert_eq!(r.month.total(), 561);
        assert_eq!(r.top_runs[0].tokens.total(), 11);
    }

    #[test]
    fn runs_from_before_recording_count_when_they_started() {
        let mut old = run("20261011T093000.000Z", "Before", vec![]);
        old.input_tokens = 4000;
        old.output_tokens = 400;
        // A run resumed after the upgrade: only part of it was recorded.
        let mut mixed = run(
            "20261009T100000.000Z",
            "Resumed",
            vec![call(12, "opus", 100, 10, 0)],
        );
        mixed.input_tokens += 900;
        mixed.output_tokens += 90;
        let runs = vec![("acme".to_string(), old), ("acme".to_string(), mixed)];
        let r = aggregate(&runs, 7, at(12, 18), &Prices::default());
        assert_eq!(r.window.total(), 4400 + 1100);
        let unrecorded = r.by_model.iter().find(|m| m.name == UNRECORDED).unwrap();
        assert_eq!(unrecorded.tokens.total(), 4400 + 990);
        assert_eq!(r.by_day[5].tokens.total(), 4400); // the 11th
        assert_eq!(r.by_day[3].tokens.total(), 990); // the 9th
        assert_eq!(r.window.peak_input, 100);
    }

    #[test]
    fn estimates_costs_by_what_was_billed_and_why() {
        let mut work = call(
            12,
            "anthropic/claude-sonnet-5-5",
            1_000_000,
            100_000,
            400_000,
        );
        work.cache_write = 100_000;
        let mut warm = call(12, "claude-sonnet-5-5", 500_000, 0, 500_000);
        warm.purpose = Purpose::Warm;
        let mut scheduled = run("20261012T080000.000Z", "Digest", vec![work, warm]);
        scheduled.origin = Some(crate::state::Origin {
            objective: "digest".into(),
            trigger: "schedule: daily 08:00".into(),
            job_version: None,
        });
        let runs = vec![
            ("acme".to_string(), scheduled),
            (
                "acme".to_string(),
                run(
                    "20261012T090000.000Z",
                    "Local",
                    vec![call(12, "qwen3", 10, 1, 0)],
                ),
            ),
        ];
        let r = aggregate(&runs, 7, at(12, 18), &Prices::default());
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // Sonnet 5.5: $2 in, $10 out, $0.20 cache reads, $2.50 cache writes.
        assert!(close(r.window.cost.input, 1.0));
        assert!(close(r.window.cost.cache_read, 0.08 + 0.1));
        assert!(close(r.window.cost.cache_write, 0.25));
        assert!(close(r.window.cost.output, 1.0));
        assert_eq!(r.window.unpriced, 1);
        assert_eq!(r.window.peak_input, 1_000_000);
        assert_eq!(r.by_purpose[0].name, "work");
        assert!(close(r.by_purpose[1].tokens.cost.total(), 0.1));
        assert_eq!(r.by_source[0].name, "objectives");
        assert_eq!(r.by_source[1].name, "chats");
        let routines: Vec<&str> = r.by_routine.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(routines, vec!["digest", ""]);
        assert!(r.top_runs[0].scheduled);
        assert_eq!(r.top_runs[1].objective, "Local");
    }
}
