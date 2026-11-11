//! The read of a business's morning, for its interlocutor's brief (decision
//! 121): what waits for the founder, what ran since yesterday, what runs
//! today, and the idea waiting. Put into words here, by code, never by a
//! model: the founder relies on these figures before anything else.

use crate::business::store::Store;
use crate::business::store::queue::Item as QueueItem;
use crate::pipeline::run::Item;
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Utc};
use std::path::Path;

/// One item for the day: its source id is the day, so a brief is given
/// once a day, however often its routine is retried.
pub fn read(root: &Path, store: &Store, now: DateTime<Utc>) -> Result<Item> {
    let pending = store.queue(&["pending"])?;
    let week = store.routines_week(now - Duration::hours(24))?;
    let mut item = Item::new();
    let mut set = |k: &str, v: String| {
        item.insert(k.into(), v.into());
    };
    let day = now.format("%A %-d %B").to_string();
    set(super::run::SOURCE, format!("morning/{}", now.date_naive()));
    set("day", day.clone());
    let lines = [
        format!("Good morning. Where things stand on {day}."),
        waiting(&pending),
        overnight(&week.routines),
        today(root, now),
        idea(&pending),
    ];
    set(
        "brief",
        lines
            .into_iter()
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    Ok(item)
}

/// What waits for the founder, by kind, the most pressing first.
fn waiting(pending: &[QueueItem]) -> String {
    let count = |f: &dyn Fn(&QueueItem) -> bool| pending.iter().filter(|i| f(i)).count();
    let parts: Vec<String> = [
        (count(&|i| i.kind == "incident"), "incident", "incidents"),
        (count(&|i| i.kind == "deadline"), "deadline", "deadlines"),
        (
            count(&|i| i.action.as_deref() == Some("airth.dispatch")),
            "plan to approve",
            "plans to approve",
        ),
        (
            count(&|i| {
                matches!(i.kind.as_str(), "approval" | "delayed")
                    && i.action.as_deref() != Some("airth.dispatch")
            }),
            "draft or action to approve",
            "drafts or actions to approve",
        ),
        (
            count(&|i| i.kind == "prepared"),
            "thing to do yourself",
            "things to do yourself",
        ),
        (
            count(&|i| i.kind == "report"),
            "report to read",
            "reports to read",
        ),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
    .collect();
    match parts.as_slice() {
        [] => "Nothing waits for you.".into(),
        _ => format!("Waiting for you: {}.", parts.join(", ")),
    }
}

/// How the routines ran in the last 24 hours: their runs, and what failed.
fn overnight(routines: &[crate::business::store::status::RoutineWeek]) -> String {
    // A routine listed for the founder's time on its items may not have run.
    if routines.iter().all(|r| r.runs == 0) {
        return "Since yesterday: no routine ran.".into();
    }
    let failed: Vec<String> = routines
        .iter()
        .filter(|r| r.failed > 0)
        .map(|r| format!("{} ({} failed)", r.routine, r.failed))
        .collect();
    let runs: i64 = routines.iter().map(|r| r.runs).sum();
    match failed.as_slice() {
        [] => format!("Since yesterday: {runs} routine run(s), none failed."),
        _ => format!(
            "Since yesterday: {runs} routine run(s); failed: {}.",
            failed.join(", ")
        ),
    }
}

/// The routines scheduled for the rest of `now`'s day, in time order.
fn today(root: &Path, now: DateTime<Utc>) -> String {
    let weekday = now.weekday().to_string().to_lowercase();
    let current = now.format("%H:%M").to_string();
    let mut runs: Vec<(String, String)> = std::fs::read_dir(root.join("routines"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_string();
            let graph: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
            let schedule = graph["trigger"]["schedule"].as_str()?.to_lowercase();
            let (when, time) = schedule.split_once(' ')?;
            (when == "daily" || weekday.starts_with(when)).then(|| (time.to_string(), name))
        })
        .filter(|(time, _)| *time > current)
        .collect();
    runs.sort();
    match runs.as_slice() {
        [] => String::new(),
        _ => format!(
            "Today: {}.",
            runs.iter()
                .map(|(time, name)| format!("{time} {name}"))
                .collect::<Vec<_>>()
                .join(" · ")
        ),
    }
}

/// The newest idea still waiting, if any (decision 119).
fn idea(pending: &[QueueItem]) -> String {
    pending
        .iter()
        .rev()
        .find(|i| i.kind == "idea")
        .map(|i| {
            format!(
                "An idea waits in {}'s conversation: {}.",
                i.agent.as_deref().unwrap_or("an agent"),
                i.title.trim_end_matches('.')
            )
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::autonomy::NewItem;

    #[test]
    fn the_brief_says_what_waits_what_ran_and_what_runs_today() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("routines")).unwrap();
        for (name, schedule) in [
            ("daily-posts", "daily 07:30"),
            ("writer-ideas", "daily 09:40"),
            ("weekly-report", "mon 08:00"),
            ("legal-review", "wed 10:00"),
        ] {
            std::fs::write(
                dir.path().join(format!("routines/{name}.json")),
                format!(r#"{{"trigger": {{"schedule": "{schedule}"}}}}"#),
            )
            .unwrap();
        }
        let mut store = Store::open(&dir.path().join("business.db")).unwrap();
        let propose = |store: &mut Store, kind: &str, action: Option<&str>, title: &str| {
            store
                .propose(&NewItem {
                    kind,
                    title,
                    agent: Some("writer"),
                    routine: None,
                    run: None,
                    action,
                    args: "{}",
                    execute_at: None,
                    source: "{}",
                    model_call: None,
                })
                .unwrap();
        };
        propose(&mut store, "approval", None, "A post");
        propose(&mut store, "approval", None, "Another post");
        propose(&mut store, "approval", Some("airth.dispatch"), "Year-end");
        propose(&mut store, "deadline", None, "Confirmation statement");
        propose(&mut store, "idea", None, "A year-end recap thread");
        // Wednesday 11 November 2026, 08:05 UTC.
        let now = DateTime::parse_from_rfc3339("2026-11-11T08:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let day = read(dir.path(), &store, now).unwrap();
        let brief = day["brief"].as_str().unwrap();
        assert_eq!(day[super::super::run::SOURCE], "morning/2026-11-11");
        assert!(
            brief.starts_with("Good morning. Where things stand on Wednesday 11 November."),
            "{brief}"
        );
        assert!(
            brief.contains(
                "Waiting for you: 1 deadline, 1 plan to approve, 2 drafts or actions to approve."
            ),
            "{brief}"
        );
        assert!(
            brief.contains("Since yesterday: no routine ran."),
            "{brief}"
        );
        // What already ran this morning, and other days' routines, are left out.
        assert!(
            brief.contains("Today: 09:40 writer-ideas · 10:00 legal-review."),
            "{brief}"
        );
        assert!(
            brief.contains("An idea waits in writer's conversation: A year-end recap thread."),
            "{brief}"
        );
    }

    #[test]
    fn an_empty_morning_says_so() {
        assert_eq!(waiting(&[]), "Nothing waits for you.");
        assert_eq!(idea(&[]), "");
    }
}
