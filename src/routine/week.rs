//! The read of a business's own records for its weekly report: how its
//! routines ran in the last seven days, from `business.db`, as one item.
//! Its figures are put into words here, by code, never by a model, so the
//! founder can rely on them.

use crate::business::store::Store;
use crate::business::store::status::RoutinesWeek;
use crate::business::store::time::human;
use crate::pipeline::run::Item;
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Utc};
use std::path::Path;

/// The seven days up to `now`, as one item. Its source id is the week, so a
/// report acts once per week, however often its routine is retried.
pub fn read(root: &Path, store: &Store, now: DateTime<Utc>) -> Result<Item> {
    let since = now - Duration::days(7);
    let w = store.routines_week(since)?;
    let baseline = baseline_minutes(root);
    let iso = now.iso_week();
    let mut item = Item::new();
    let mut set = |k: &str, v: String| {
        item.insert(k.into(), v.into());
    };
    set(
        super::run::SOURCE,
        format!("week/{}-W{:02}", iso.year(), iso.week()),
    );
    set("week", format!("{}-W{:02}", iso.year(), iso.week()));
    set("from", since.format("%-d %b").to_string());
    set("to", now.format("%-d %b").to_string());
    set("founder_time", human(w.founder_seconds as u64));
    set("baseline", baseline_text(&w, baseline));
    set("answers", answers(&w));
    set("cost", cost(&w));
    set(
        "actions",
        format!("{} done, {} failed", w.actions_done, w.actions_failed),
    );
    set("incidents", w.incidents.to_string());
    set("routines", routines(&w));
    set("late", late(&w));
    set("dropped", dropped(&w));
    let summary = summary(&item);
    item.insert("summary".into(), summary.into());
    Ok(item)
}

/// The founder's weekly minutes before Airth, from `business/goal.md`; none
/// when it is not a number yet.
fn baseline_minutes(root: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(root.join("business/goal.md")).ok()?;
    crate::frontmatter::parse(&text)
        .text("baseline_minutes")?
        .trim()
        .parse()
        .ok()
}

fn baseline_text(w: &RoutinesWeek, baseline: Option<u64>) -> String {
    match baseline {
        None | Some(0) => "not set (baseline_minutes in business/goal.md)".into(),
        Some(m) => format!(
            "{} a week, so this week took {}% of it",
            human(m * 60),
            (w.founder_seconds as u64 * 100).div_ceil(m * 60)
        ),
    }
}

fn answers(w: &RoutinesWeek) -> String {
    format!(
        "{} approved, {} edited, {} rejected",
        w.approved, w.edited, w.rejected
    )
}

fn cost(w: &RoutinesWeek) -> String {
    let unpriced = match w.unpriced {
        0 => String::new(),
        n => format!(", and {n} to a model without a known price"),
    };
    format!(
        "${:.2} for {} model call(s){unpriced}",
        w.cost_usd, w.model_calls
    )
}

fn routines(w: &RoutinesWeek) -> String {
    if w.routines.is_empty() {
        return "none ran".into();
    }
    w.routines
        .iter()
        .map(|r| {
            format!(
                "- {}: {} run(s), {} failed, {} late, {} dropped, ${:.2}, {} of yours",
                r.routine,
                r.runs,
                r.failed,
                r.late,
                r.dropped,
                r.cost_usd,
                human(r.founder_seconds as u64)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs that started late, apart: a late start is the Mac or Airth, not the
/// routine, and must not pass for slowness.
fn late(w: &RoutinesWeek) -> String {
    if w.late.is_empty() {
        return "none".into();
    }
    let runs: Vec<String> = w
        .late
        .iter()
        .map(|l| {
            format!(
                "- {}: due {}, started {} later",
                l.routine,
                l.due_at.format("%a %-d %b %H:%M UTC"),
                human((l.started_at - l.due_at).num_seconds().max(0) as u64)
            )
        })
        .collect();
    format!("{}\n{}", w.late.len(), runs.join("\n"))
}

/// What was dropped, each reason once with how often (decision 61): a
/// missing post the founder expected, not a failure.
fn dropped(w: &RoutinesWeek) -> String {
    let total: i64 = w.dropped.iter().map(|d| d.count).sum();
    if total == 0 {
        return "none".into();
    }
    let reasons: Vec<String> = w
        .dropped
        .iter()
        .map(|d| format!("- {}: {} ({}×)", d.routine, d.reason, d.count))
        .collect();
    format!("{total}\n{}", reasons.join("\n"))
}

fn summary(item: &Item) -> String {
    let f = |k: &str| crate::pipeline::run::field_text(item, k);
    format!(
        "Week {} ({} to {})\nYour time: {} (baseline: {})\nYour answers: {}\nModel cost: {}\nActions: {}\nIncidents: {}\nBy routine:\n{}\nStarted late: {}\nDropped: {}",
        f("week"),
        f("from"),
        f("to"),
        f("founder_time"),
        f("baseline"),
        f("answers"),
        f("cost"),
        f("actions"),
        f("incidents"),
        f("routines"),
        f("late"),
        f("dropped")
    )
}
