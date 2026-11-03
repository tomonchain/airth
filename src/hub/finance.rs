//! The business's bank figures for the HQ (decisions 53 and 54): read at
//! most once an hour, kept in the realm's Airth home, and shown as unknown,
//! never as stale figures, when the consent is gone or the last read is
//! more than a day old. A refused consent is one incident in Needs you and
//! on Telegram, not one an hour.

use super::*;
use crate::bank::{Finance, Reauthorize, Refused};
use crate::business::store::Store;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const FILE: &str = "finance.json";
/// The incident a refused consent raises.
pub const REAUTHORIZE: &str = "Revolut access needs re-authorizing";
/// The incident another refusal raises, with Revolut's message.
pub const REFUSED: &str = "Revolut refused a request";

/// What Airth last knew of the bank.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Kept {
    Read { finance: Finance },
    Unknown { why: String, at: DateTime<Utc> },
}

fn kept(realm: &Realm) -> Option<Kept> {
    let text = std::fs::read_to_string(realm.dir().join(FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

fn keep(realm: &Realm, kept: &Kept) -> Result<()> {
    crate::realm::write_private(&realm.dir().join(FILE), &serde_json::to_string(kept)?)
}

impl Hub {
    /// Read each connected bank whose figures are an hour old, across
    /// realms; what happened, by realm. Only a bank the founder connected
    /// is ever called.
    pub fn refresh_finance(&self, now: DateTime<Utc>) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            let skills = realm.skills_dir();
            if realm.config.archived || !crate::bank::connected(&skills) {
                continue;
            }
            let last = match kept(&realm) {
                Some(Kept::Read { finance }) => Some(finance.read_at),
                Some(Kept::Unknown { at, .. }) => Some(at),
                None => None,
            };
            if last.is_some_and(|at| now - at < Duration::hours(1)) {
                continue;
            }
            match crate::bank::finance(&skills, now) {
                Ok(finance) => {
                    keep(&realm, &Kept::Read { finance })?;
                    out.push(format!("{}: bank read", realm.name()));
                }
                Err(e) if e.downcast_ref::<Reauthorize>().is_some() => {
                    keep(
                        &realm,
                        &Kept::Unknown {
                            why: REAUTHORIZE.into(),
                            at: now,
                        },
                    )?;
                    let detail = format!(
                        "{e:#}\n\nThe business's finance shows as unknown until you consent again: airth bank connect --realm {}.",
                        realm.name()
                    );
                    tell(&realm, REAUTHORIZE, &detail)?;
                    out.push(format!("{}: {REAUTHORIZE}", realm.name()));
                }
                // Another refusal: an ordinary incident with Revolut's
                // message; the last figures stay until a day old.
                Err(e) if e.downcast_ref::<Refused>().is_some() => {
                    tell(&realm, REFUSED, &format!("{e:#}"))?;
                    out.push(format!("{}: {e:#}", realm.name()));
                }
                // Revolut down or the network: the last figures stay, and
                // turn unknown once a day old.
                Err(e) => out.push(format!("{}: bank not read: {e:#}", realm.name())),
            }
        }
        Ok(out)
    }
}

/// An incident titled `title`, unless one is open already: once, not once
/// an hour.
pub(super) fn tell(realm: &Realm, title: &str, detail: &str) -> Result<()> {
    let open = Store::of(realm)?
        .queue(&["pending"])?
        .iter()
        .any(|i| i.kind == "incident" && i.title == title);
    if !open {
        super::incidents::record(
            realm,
            &super::incidents::Incident {
                title,
                detail,
                agent: None,
                routine: None,
                action: None,
                demote: false,
                retry: None,
            },
        )?;
    }
    Ok(())
}

/// What the HQ shows of a business's bank: None with no bank connected;
/// else its figures as words, or why they are unknown.
pub fn shown(realm: &Realm, now: DateTime<Utc>) -> Option<Value> {
    if !crate::bank::connected(&realm.skills_dir()) {
        return None;
    }
    Some(match kept(realm) {
        Some(Kept::Read { finance }) if now - finance.read_at < Duration::days(1) => {
            let money = |amount: f64| money(amount, &finance.currency);
            json!({
                "source": format!("Revolut Business · read-only · {}", finance.read_at.with_timezone(&chrono::Local).format("%H:%M")),
                "cash": money(finance.cash),
                "in": money(finance.money_in),
                "out": money(finance.money_out),
                "runway": finance.runway_months.map_or("No spending".to_string(), |m| format!("{} months", m.floor())),
                "runway_months": finance.runway_months,
            })
        }
        Some(Kept::Read { .. }) => json!({"unknown": "Last read more than a day ago"}),
        Some(Kept::Unknown { why, .. }) => json!({"unknown": why}),
        None => json!({"unknown": "Not read yet"}),
    })
}

/// An amount as the founder reads it: "£12,300".
fn money(amount: f64, currency: &str) -> String {
    let whole = amount.round() as i64;
    let digits = whole.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let sign = if whole < 0 { "-" } else { "" };
    match currency {
        "GBP" => format!("{sign}£{grouped}"),
        "EUR" => format!("{sign}€{grouped}"),
        "USD" => format!("{sign}${grouped}"),
        other => format!("{sign}{grouped} {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_read_like_money() {
        assert_eq!(money(12300.4, "GBP"), "£12,300");
        assert_eq!(money(0.0, "EUR"), "€0");
        assert_eq!(money(-1234567.0, "USD"), "-$1,234,567");
        assert_eq!(money(950.0, "CHF"), "950 CHF");
    }
}
