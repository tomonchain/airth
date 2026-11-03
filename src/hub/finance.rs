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

/// What the HQ shows of a business's finance: a connected bank's figures,
/// else those the founder entered by hand (decision 68), else None; as
/// words, or why they are unknown.
pub fn shown(realm: &Realm, now: DateTime<Utc>) -> Option<Value> {
    if !crate::bank::connected(&realm.skills_dir()) {
        return by_hand(realm, now);
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

/// Figures entered by hand are shown for this long after their date, then
/// as unknown: never stale (decision 68).
const BY_HAND_DAYS: i64 = 35;

/// `business/finance.md`'s figures, as the HQ shows a bank's; None without
/// the file.
fn by_hand(realm: &Realm, now: DateTime<Utc>) -> Option<Value> {
    let root = realm.root().ok()?;
    let doc = match crate::business::config::finance(&root) {
        Ok(doc) => doc?,
        Err(e) => return Some(json!({"unknown": format!("{e:#}")})),
    };
    let number = |key| doc.text(key).and_then(|v| v.parse::<f64>().ok());
    let as_of = doc
        .text("as_of")
        .and_then(|v| chrono::NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok());
    let (Some(cash), Some(spent), Some(as_of)) = (number("cash"), number("spent"), as_of) else {
        return Some(json!({"unknown": "business/finance.md needs cash, spent and as_of"}));
    };
    if (now.with_timezone(&chrono::Local).date_naive() - as_of).num_days() > BY_HAND_DAYS {
        return Some(
            json!({"unknown": format!("Entered by hand more than {BY_HAND_DAYS} days ago")}),
        );
    }
    let currency = doc.text("currency").or_else(|| {
        let profile = std::fs::read_to_string(root.join("business/profile.md")).ok()?;
        crate::frontmatter::parse(&profile).text("currency")
    });
    let money = |amount: f64| money(amount, currency.as_deref().unwrap_or_default());
    let runway = (spent > 0.0).then(|| cash / spent);
    Some(json!({
        "source": format!("Entered by hand · as of {}", as_of.format("%-d %b")),
        "cash": money(cash),
        "in": null,
        "out": money(spent),
        "runway": runway.map_or("No spending".to_string(), |m| format!("{} months", m.floor())),
        "runway_months": runway,
    }))
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
        "" => format!("{sign}{grouped}"),
        other => format!("{sign}{grouped} {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision 68: figures entered by hand show with their date and a
    /// runway, until 35 days old; a wrong value says why; a connected bank
    /// takes precedence.
    #[test]
    fn figures_by_hand_until_a_bank_is_connected() {
        let dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(dir.path()).unwrap();
        let realm = home
            .create_realm(crate::home::NewRealm {
                name: "r".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let root = realm.root().unwrap();
        let now = Utc::now();
        assert_eq!(shown(&realm, now), None, "no file, no figures");
        std::fs::create_dir_all(root.join("business")).unwrap();
        std::fs::write(
            root.join("business/profile.md"),
            "---\ncurrency: GBP\n---\n",
        )
        .unwrap();
        let enter = |as_of: chrono::NaiveDate, cash: &str| {
            std::fs::write(
                root.join("business/finance.md"),
                format!(
                    "---\ncash: {cash}\nspent: 1000\nas_of: {as_of}\n---\nFrom the bank's app.\n"
                ),
            )
            .unwrap();
        };
        let today = now.with_timezone(&chrono::Local).date_naive();
        enter(today - Duration::days(35), "8000");
        let f = shown(&realm, now).unwrap();
        assert!(
            f["source"]
                .as_str()
                .unwrap()
                .starts_with("Entered by hand · as of ")
        );
        assert_eq!(
            (f["cash"].as_str(), f["out"].as_str()),
            (Some("£8,000"), Some("£1,000"))
        );
        assert!(f["in"].is_null(), "money in is not entered by hand");
        assert_eq!(f["runway_months"], 8.0);
        enter(today - Duration::days(36), "8000");
        assert_eq!(
            shown(&realm, now).unwrap()["unknown"],
            "Entered by hand more than 35 days ago"
        );
        enter(today, "lots");
        assert!(
            shown(&realm, now).unwrap()["unknown"]
                .as_str()
                .unwrap()
                .contains("cash 'lots'")
        );
        enter(today, "8000");
        let access = realm.skills_dir().join("mcp-auth/revolut.json");
        std::fs::create_dir_all(access.parent().unwrap()).unwrap();
        std::fs::write(&access, "{}").unwrap();
        assert_eq!(
            shown(&realm, now).unwrap(),
            json!({"unknown": "Not read yet"}),
            "the bank, once connected, takes precedence"
        );
    }

    #[test]
    fn amounts_read_like_money() {
        assert_eq!(money(12300.4, "GBP"), "£12,300");
        assert_eq!(money(0.0, "EUR"), "€0");
        assert_eq!(money(-1234567.0, "USD"), "-$1,234,567");
        assert_eq!(money(950.0, "CHF"), "950 CHF");
    }
}
