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
    set("proof", proof(store, baseline, now)?.sentence);
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
        "Week {} ({} to {})\nYour time: {} (baseline: {})\nYour answers: {}\nModel cost: {}\nActions: {}\nIncidents: {}\nBy routine:\n{}\nStarted late: {}\nDropped: {}\nOn its own: {}",
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
        f("dropped"),
        f("proof")
    )
}

/// Stable weeks in a row the beta needs (`CLAUDE.md`: four).
pub const PROOF_WEEKS: usize = 4;
/// Weeks looked back at: past this, the count says "at least".
const LOOK_BACK: usize = 8;

/// The beta's proof so far (decision 92): stable weeks in a row, the last
/// complete week first, and why the last one was or was not stable.
#[derive(Debug, serde::Serialize)]
pub struct Proof {
    pub in_a_row: usize,
    pub of: usize,
    pub last_week: String,
    pub last_stable: bool,
    pub why: Vec<String>,
    pub sentence: String,
}

/// Whether a week was stable, and why not: its scheduled routines ran on
/// time, no incident stayed open over a day, and the founder's time was at
/// most half the baseline. A week with no scheduled run proves nothing.
pub fn judge(w: &crate::business::store::status::ProofWeek, baseline: Option<u64>) -> Vec<String> {
    let mut why = Vec::new();
    if w.scheduled == 0 {
        why.push("no scheduled run".to_string());
    }
    let off = w.late + w.missed;
    if off > 0 {
        why.push(format!(
            "{off} run{} late or missed",
            if off == 1 { "" } else { "s" }
        ));
    }
    if w.slow_incidents > 0 {
        why.push(format!(
            "{} incident{} open over 24 h",
            w.slow_incidents,
            if w.slow_incidents == 1 { "" } else { "s" }
        ));
    }
    match baseline {
        None | Some(0) => why.push("no baseline in business/goal.md".to_string()),
        Some(m) if w.founder_seconds as u64 * 2 > m * 60 => why.push(format!(
            "your time {} is over half the baseline ({})",
            human(w.founder_seconds as u64),
            human(m * 30)
        )),
        Some(_) => {}
    }
    why
}

/// The proof as of `now`, from the complete weeks (Monday to Monday, UTC)
/// before it.
pub fn proof(store: &Store, baseline: Option<u64>, now: DateTime<Utc>) -> Result<Proof> {
    let monday = (now - Duration::days(i64::from(now.weekday().num_days_from_monday())))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc())
        .unwrap_or(now);
    let mut in_a_row = 0;
    let mut last: Option<(String, Vec<String>)> = None;
    for back in 1..=LOOK_BACK {
        let from = monday - Duration::weeks(back as i64);
        let w = store.proof_week(from, from + Duration::weeks(1), now)?;
        let why = judge(&w, baseline);
        let stable = why.is_empty();
        if last.is_none() {
            let iso = from.iso_week();
            last = Some((format!("{}-W{:02}", iso.year(), iso.week()), why));
        }
        if !stable {
            break;
        }
        in_a_row += 1;
    }
    let (last_week, why) = last.unwrap_or_default();
    let last_stable = why.is_empty();
    let count = if in_a_row == LOOK_BACK {
        format!("at least {in_a_row}")
    } else {
        in_a_row.to_string()
    };
    let sentence = if last_stable {
        format!(
            "{count} stable week{} in a row, of {PROOF_WEEKS}.",
            if in_a_row == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "0 stable weeks in a row, of {PROOF_WEEKS}: week {last_week} had {}.",
            why.join(", ")
        )
    };
    Ok(Proof {
        in_a_row,
        of: PROOF_WEEKS,
        last_week,
        last_stable,
        why,
        sentence,
    })
}

/// The founder's weekly minutes before Airth, for the HQ.
pub fn baseline(root: &Path) -> Option<u64> {
    baseline_minutes(root)
}
