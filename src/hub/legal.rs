//! Legal deadlines reach the founder (decision 66): at each reminder before
//! a deadline, then the day after it passes unfiled, one item in Needs you
//! (and so on Telegram). Filed records it; Seen closes one reminder. No
//! model is involved.

use super::*;
use crate::business::store::Store;
use crate::business::store::autonomy::NewItem;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde_json::json;

/// Days ahead of a deadline its reminders come, unless the profile says.
const REMINDERS: &[i64] = &[30, 7];
/// A deadline passed longer ago than this was there before Airth looked:
/// it raises no overdue item.
const OVERDUE_DAYS: i64 = 30;
/// The incident a failed check raises, once.
const FAILED: &str = "The legal calendar could not be checked";

impl Hub {
    /// For each business, the deadline items now due; what happened, by
    /// business.
    pub fn remind_deadlines(&self, now: DateTime<Utc>) -> Result<Vec<String>> {
        let today = now.with_timezone(&Local).date_naive();
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived {
                continue;
            }
            match remind(&realm, today) {
                Ok(0) => {}
                Ok(n) => out.push(format!("{}: {n} legal deadline item(s)", realm.name())),
                Err(e) => {
                    super::finance::tell(&realm, FAILED, &format!("{e:#}"))?;
                    out.push(format!("{}: {FAILED}: {e:#}", realm.name()));
                }
            }
        }
        Ok(out)
    }
}

/// The items due today for one business; how many were proposed. A profile
/// that does not give the dates yet raises nothing: Calendar says what is
/// missing.
fn remind(realm: &Realm, today: NaiveDate) -> Result<usize> {
    let root = realm.root()?;
    let Ok(text) = std::fs::read_to_string(root.join("business/profile.md")) else {
        return Ok(0);
    };
    let profile = crate::frontmatter::parse(&text);
    if !crate::legal::missing(&profile).is_empty() {
        return Ok(0);
    }
    let rules = crate::legal::rules(
        &profile.text("country").unwrap_or_default(),
        &profile.text("legal_form").unwrap_or_default(),
    )?;
    let company = crate::legal::Company::of(&profile)?;
    let leads: Vec<i64> = match profile.list("reminders") {
        days if days.is_empty() => REMINDERS.to_vec(),
        days => days.iter().filter_map(|d| d.parse().ok()).collect(),
    };
    let mut store = Store::of(realm)?;
    let filed = store.filings()?;
    let mut proposed = 0;
    let until = today + Duration::days(400);
    for d in crate::legal::deadlines(&company, &rules, until) {
        let period_end = d.period_end.to_string();
        if filed.iter().any(|(r, p)| r == d.rule && *p == period_end) {
            continue;
        }
        let overdue = d.due < today;
        // The nearest reminder whose day has come: one item, not one per
        // reminder missed while Airth was off.
        let lead = match overdue {
            true if (today - d.due).num_days() > OVERDUE_DAYS => continue,
            true => "overdue".to_string(),
            false => match leads
                .iter()
                .filter(|&&l| today >= d.due - Duration::days(l))
                .min()
            {
                Some(l) => l.to_string(),
                None => continue,
            },
        };
        let key = format!("{}/{period_end}/{lead}", d.rule);
        if store.deadline_proposed(&key)? {
            continue;
        }
        let due = d.due.format("%a %-d %b %Y");
        let title = match overdue {
            true => format!("Overdue: {} was due {due}", d.title),
            false => format!("{} due {due}", d.title),
        };
        let then = match overdue {
            true => "It stays here until you answer Filed.",
            false => {
                "Filed records it, and no more reminders come for it. Seen closes this reminder only."
            }
        };
        let detail = format!(
            "{}, for the period to {}. {then}",
            d.authority,
            d.period_end.format("%-d %b %Y")
        );
        let link = d.source.iter().find(|s| s.starts_with("https://"));
        store.propose(&NewItem {
            kind: "deadline",
            title: &title,
            agent: None,
            routine: None,
            run: None,
            action: None,
            args: &json!({"detail": detail, "link": link, "overdue": overdue}).to_string(),
            execute_at: None,
            source:
                &json!({"key": key, "rule": d.rule, "period_end": period_end, "show": "detail"})
                    .to_string(),
            model_call: None,
        })?;
        proposed += 1;
    }
    Ok(proposed)
}
