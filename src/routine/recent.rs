//! The read of what a business did lately, for an agent's ideas (decision
//! 119): the day and the season, the goal, what the founder approved or saw
//! in the last 30 days, what is coming, and the ideas already given with
//! how the founder took them. Put into words here, by code; a model then
//! decides whether one thing deserves the founder's attention.

use crate::business::store::Store;
use crate::business::store::queue::Item as QueueItem;
use crate::pipeline::run::Item;
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use std::path::Path;

/// How far back the business's work is read.
const DAYS: i64 = 30;
/// At most this many pieces of work, newest first: enough to see a year's
/// end coming, not a prompt of every item.
const MAX_DONE: usize = 15;
const MAX_IDEAS: usize = 10;

/// One item for the day: its source id is the day, so an idea is given at
/// most once a day, however often the routine is retried.
pub fn read(root: &Path, store: &Store, now: DateTime<Utc>) -> Result<Item> {
    let since = now - Duration::days(DAYS);
    let recent = |i: &&QueueItem| {
        DateTime::parse_from_rfc3339(&i.created_at).is_ok_and(|at| at.with_timezone(&Utc) >= since)
    };
    let mut done: Vec<QueueItem> = store
        .queue(&["approved", "done"])?
        .into_iter()
        .filter(|i| i.kind != "idea")
        .collect();
    done.retain(|i| recent(&i));
    let mut ideas: Vec<QueueItem> = store
        .queue(&["pending", "done", "rejected", "expired"])?
        .into_iter()
        .filter(|i| i.kind == "idea")
        .collect();
    ideas.retain(|i| recent(&i));
    let coming: Vec<QueueItem> = store
        .queue(&["pending"])?
        .into_iter()
        .filter(|i| i.kind == "deadline")
        .collect();

    let day = now.date_naive();
    let mut item = Item::new();
    let mut set = |k: &str, v: String| {
        item.insert(k.into(), v.into());
    };
    set(super::run::SOURCE, format!("day/{day}"));
    set("today", now.format("%A %-d %B %Y").to_string());
    set("season", season(day));
    set("goal", goal(root));
    set("done", lines(done.iter().rev().take(MAX_DONE), work));
    set("coming", lines(coming.iter(), |i| i.title.clone()));
    set("ideas", lines(ideas.iter().rev().take(MAX_IDEAS), idea));
    Ok(item)
}

/// Where the day sits in the year, in words a model cannot get wrong.
fn season(day: NaiveDate) -> String {
    let year_end = NaiveDate::from_ymd_opt(day.year(), 12, 31).unwrap_or(day);
    let left = (year_end - day).num_days();
    let quarter = (day.month() - 1) / 3 + 1;
    format!(
        "{left} days to the end of {}; quarter {quarter}, {} days to its end",
        day.year(),
        days_to_quarter_end(day)
    )
}

fn days_to_quarter_end(day: NaiveDate) -> i64 {
    let month = ((day.month() - 1) / 3 + 1) * 3;
    let (y, m) = if month == 12 {
        (day.year() + 1, 1)
    } else {
        (day.year(), month + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1)
        .map(|first| (first - day).num_days() - 1)
        .unwrap_or_default()
}

/// The goal and its milestones, from `business/goal.md`.
fn goal(root: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(root.join("business/goal.md")) else {
        return "none written".into();
    };
    let doc = crate::frontmatter::parse(&text);
    ["goal", "target", "by", "milestones"]
        .iter()
        .filter_map(|k| doc.text(k).map(|v| format!("{k}: {v}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A piece of work in one line: when, who, what.
fn work(i: &QueueItem) -> String {
    let what = shown(i);
    format!(
        "{} · {} · {}",
        i.created_at.get(..10).unwrap_or_default(),
        i.agent.as_deref().unwrap_or("Airth"),
        what
    )
}

/// An idea already given, and how the founder took it.
fn idea(i: &QueueItem) -> String {
    let taken = match i.status.as_str() {
        "done" => "useful",
        "rejected" => "not for them",
        "expired" => "no answer",
        _ => "not answered yet",
    };
    format!(
        "{} · {} · {} ({taken})",
        i.created_at.get(..10).unwrap_or_default(),
        i.agent.as_deref().unwrap_or("Airth"),
        i.title
    )
}

/// What an item says, its first lines: the field Needs you shows, else
/// its title.
fn shown(i: &QueueItem) -> String {
    let text = i.source["show"]
        .as_str()
        .and_then(|path| path.split('.').try_fold(&i.args, |v, k| v.get(k)))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&i.title);
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = one.chars().take(160).collect();
    if cut.len() < one.len() {
        format!("{cut}…")
    } else {
        cut
    }
}

fn lines<'a>(
    items: impl Iterator<Item = &'a QueueItem>,
    f: impl Fn(&QueueItem) -> String,
) -> String {
    let all: Vec<String> = items.map(|i| format!("- {}", f(i))).collect();
    if all.is_empty() {
        "none".into()
    } else {
        all.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_season_counts_the_days_to_the_year_and_quarter_end() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        assert_eq!(
            season(day),
            "50 days to the end of 2026; quarter 4, 50 days to its end"
        );
        let day = NaiveDate::from_ymd_opt(2026, 2, 27).unwrap();
        assert_eq!(days_to_quarter_end(day), 32);
    }

    #[test]
    fn a_day_reads_the_goal_the_work_and_the_ideas_given() {
        use crate::business::store::autonomy::NewItem;
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("business")).unwrap();
        std::fs::write(
            dir.path().join("business/goal.md"),
            "---\ngoal: Grow\ntarget: 10 firms\n---\n",
        )
        .unwrap();
        let mut store = Store::open(&dir.path().join("business.db")).unwrap();
        let propose = |store: &mut Store, kind: &str, title: &str| {
            store
                .propose(&NewItem {
                    kind,
                    title,
                    agent: Some("writer"),
                    routine: None,
                    run: None,
                    action: None,
                    args: r#"{"text": "The year in five steps."}"#,
                    execute_at: None,
                    source: r#"{"show": "text"}"#,
                    model_call: None,
                })
                .unwrap()
        };
        use crate::business::store::queue::Answer;
        let post = propose(&mut store, "approval", "A post");
        store.answer(post, None, Answer::Approve, "app", 0).unwrap();
        store.close_item(post, "done", "published").unwrap();
        let idea = propose(&mut store, "idea", "A year-end recap");
        store
            .answer(idea, None, Answer::Reject("not now".into()), "app", 0)
            .unwrap();
        propose(&mut store, "deadline", "Confirmation statement due 20 Nov");
        let day = read(dir.path(), &store, Utc::now()).unwrap();
        let text = |k: &str| day[k].as_str().unwrap().to_string();
        assert!(text(super::super::run::SOURCE).starts_with("day/"));
        assert!(text("goal").contains("target: 10 firms"));
        assert!(text("done").contains("writer · The year in five steps."));
        assert!(!text("done").contains("year-end"));
        assert!(text("ideas").contains("A year-end recap (not for them)"));
        assert_eq!(text("coming"), "- Confirmation statement due 20 Nov");
    }
}
