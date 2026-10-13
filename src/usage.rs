//! Token usage: what every run's model calls consumed, across realms, by day,
//! realm, model and run. Shown to the owner in the app; runs never see it.

use crate::hub::Hub;
use crate::state::{State, Usage};
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;
use std::collections::HashMap;

/// The model name for tokens of runs from before each call was recorded.
pub const UNRECORDED: &str = "unrecorded";

/// Token counts. `input` includes the cached input (`cache_read`, `cache_write`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub calls: u64,
}

impl Tokens {
    fn add(&mut self, u: &Usage) {
        self.input += u.input;
        self.output += u.output;
        self.cache_read += u.cache_read;
        self.cache_write += u.cache_write;
        self.calls += 1;
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
    /// The runs that used the most tokens in the window.
    pub top_runs: Vec<RunUsage>,
}

/// Usage of every realm's runs over the last `days` days, in `now`'s time zone.
pub fn report<Tz: TimeZone>(hub: &Hub, days: u32, now: DateTime<Tz>) -> Result<Report> {
    let mut runs = Vec::new();
    for realm in hub.home().realms()? {
        for state in hub.all_runs(&realm)? {
            runs.push((realm.name().to_string(), state));
        }
    }
    Ok(aggregate(&runs, days, now))
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

fn shares(map: HashMap<String, Tokens>) -> Vec<Share> {
    let mut v: Vec<Share> = map
        .into_iter()
        .map(|(name, tokens)| Share { name, tokens })
        .collect();
    v.sort_by(|a, b| {
        b.tokens
            .total()
            .cmp(&a.tokens.total())
            .then(a.name.cmp(&b.name))
    });
    v
}

pub fn aggregate<Tz: TimeZone>(runs: &[(String, State)], days: u32, now: DateTime<Tz>) -> Report {
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
        top_runs: Vec::new(),
    };
    let mut realms: HashMap<String, Tokens> = HashMap::new();
    let mut models: HashMap<String, Tokens> = HashMap::new();
    let mut top = Vec::new();
    for (realm, state) in runs {
        let mut run = Tokens::default();
        for call in calls(state) {
            let date = call.at.with_timezone(&tz).date_naive();
            if date.year() == today.year() && date.month() == today.month() && date <= today {
                report.month.add(&call);
            }
            if date < first || date > today {
                continue;
            }
            if date == today {
                report.today.add(&call);
            }
            let index = (date - first).num_days() as usize;
            by_day[index].tokens.add(&call);
            report.window.add(&call);
            run.add(&call);
            realms.entry(realm.clone()).or_default().add(&call);
            models.entry(call.model.clone()).or_default().add(&call);
        }
        if run.calls > 0 {
            top.push(RunUsage {
                realm: realm.clone(),
                id: state.id.clone(),
                objective: first_line(&state.objective),
                tokens: run,
            });
        }
    }
    top.sort_by_key(|r| std::cmp::Reverse(r.tokens.total()));
    top.truncate(10);
    report.by_day = std::mem::take(&mut by_day);
    report.by_realm = shares(realms);
    report.by_model = shares(models);
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
        let r = aggregate(&runs, 7, at(12, 18));
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
        let r = aggregate(&runs, 7, at(12, 18));
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
        let r = aggregate(&runs, 7, at(12, 18));
        assert_eq!(r.window.total(), 4400 + 1100);
        let unrecorded = r.by_model.iter().find(|m| m.name == UNRECORDED).unwrap();
        assert_eq!(unrecorded.tokens.total(), 4400 + 990);
        assert_eq!(r.by_day[5].tokens.total(), 4400); // the 11th
        assert_eq!(r.by_day[3].tokens.total(), 990); // the 9th
    }
}
