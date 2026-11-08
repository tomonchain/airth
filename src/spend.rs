//! What each model provider costs, and how long its money lasts (decision
//! 107): the spending from Airth's own records, a balance where the provider
//! tells it, a monthly budget the founder sets where it does not.
//!
//! Spending is estimated from list prices (`prices.rs`), as on the Usage
//! page; a balance is the provider's own figure.

use crate::home::Home;
use crate::hub::Hub;
use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

const FILE: &str = "budgets.json";
/// Days of spending the rate is taken over.
const RATE_DAYS: i64 = 7;
/// A balance is read again after this long.
const BALANCE_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

/// A monthly budget for a provider paid by use, reset on a day of the month.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub monthly_usd: f64,
    pub reset_day: u32,
}

/// What the provider says is left on the account.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Balance {
    pub amount: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// More than a week left at the current rate.
    Ok,
    /// Less than a week.
    Warn,
    /// Less than two days, or nothing left.
    Low,
    /// Nothing to measure against: no balance, no budget.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub provider: String,
    pub name: String,
    pub month_usd: f64,
    pub daily_usd: f64,
    pub budget: Option<Budget>,
    /// Spent since the budget's last reset.
    pub since_reset_usd: Option<f64>,
    pub resets_on: Option<NaiveDate>,
    pub balance: Option<Balance>,
    /// Dollars left: the balance, or the budget less what was spent.
    pub left_usd: Option<f64>,
    pub days_left: Option<f64>,
    /// Of the budget, how much is spent (0 to 1).
    pub used: Option<f64>,
    pub level: Level,
}

/// The provider a model belongs to, from its name: model calls record the
/// model, not who served it.
pub fn provider_of(model: &str) -> &'static str {
    let m = model.to_ascii_lowercase();
    let m = m.rsplit('/').next().unwrap_or(&m).to_string();
    let starts = |p: &[&str]| p.iter().any(|p| m.starts_with(p));
    if starts(&["claude"]) {
        "anthropic"
    } else if starts(&["deepseek"]) {
        "deepseek"
    } else if starts(&["gpt", "o1", "o3", "o4", "chatgpt"]) {
        "openai"
    } else if starts(&["grok"]) {
        "xai"
    } else if starts(&[
        "mistral",
        "ministral",
        "codestral",
        "magistral",
        "devstral",
        "pixtral",
    ]) {
        "mistral"
    } else {
        "other"
    }
}

pub fn budgets(home: &Home) -> BTreeMap<String, Budget> {
    std::fs::read_to_string(home.dir().join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Set (or, with `None`, remove) a provider's budget.
pub fn set_budget(home: &Home, provider: &str, budget: Option<Budget>) -> Result<()> {
    if let Some(b) = budget {
        anyhow::ensure!(b.monthly_usd > 0.0, "a budget is more than $0");
        anyhow::ensure!((1..=28).contains(&b.reset_day), "the reset day is 1 to 28");
    }
    let mut all = budgets(home);
    match budget {
        Some(b) => all.insert(provider.to_string(), b),
        None => all.remove(provider),
    };
    std::fs::write(home.dir().join(FILE), serde_json::to_string_pretty(&all)?)?;
    Ok(())
}

/// The last reset of a budget on `today` or before.
fn last_reset(day: u32, today: NaiveDate) -> NaiveDate {
    let this_month = today.with_day(day).unwrap_or(today);
    if this_month <= today {
        this_month
    } else {
        let prev = today.with_day(1).unwrap() - Duration::days(1);
        prev.with_day(day).unwrap_or(prev)
    }
}

fn next_reset(day: u32, today: NaiveDate) -> NaiveDate {
    let last = last_reset(day, today);
    let (y, m) = if last.month() == 12 {
        (last.year() + 1, 1)
    } else {
        (last.year(), last.month() + 1)
    };
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or(last)
}

/// Judge each provider from its spending by day, its budget and balance.
pub fn assess(
    spending: &BTreeMap<String, BTreeMap<NaiveDate, f64>>,
    budgets: &BTreeMap<String, Budget>,
    balances: &BTreeMap<String, Balance>,
    names: &BTreeMap<String, String>,
    today: NaiveDate,
) -> Vec<Status> {
    let empty = BTreeMap::new();
    let month_start = today.with_day(1).unwrap();
    let mut providers: Vec<&String> = spending
        .keys()
        .chain(budgets.keys())
        .chain(balances.keys())
        .collect();
    providers.sort();
    providers.dedup();
    providers
        .into_iter()
        .map(|p| {
            let days = spending.get(p).unwrap_or(&empty);
            let sum_since = |from: NaiveDate| days.range(from..).map(|(_, c)| c).sum::<f64>();
            let month_usd = sum_since(month_start);
            let daily_usd = sum_since(today - Duration::days(RATE_DAYS - 1)) / RATE_DAYS as f64;
            let budget = budgets.get(p).copied();
            let since_reset_usd = budget.map(|b| sum_since(last_reset(b.reset_day, today)));
            let balance = balances.get(p).cloned();
            let left_usd = match (&balance, budget, since_reset_usd) {
                (Some(b), _, _) if b.currency == "USD" => Some(b.amount),
                (_, Some(b), Some(spent)) => Some(b.monthly_usd - spent),
                _ => None,
            };
            let days_left = left_usd.map(|left| {
                if daily_usd > 0.0 {
                    left.max(0.0) / daily_usd
                } else {
                    f64::INFINITY
                }
            });
            let level = match (left_usd, days_left) {
                (Some(left), _) if left <= 0.0 => Level::Low,
                (_, Some(d)) if d < 2.0 => Level::Low,
                (_, Some(d)) if d < 7.0 => Level::Warn,
                (Some(_), _) => Level::Ok,
                _ => Level::Unknown,
            };
            Status {
                provider: p.clone(),
                name: names.get(p).cloned().unwrap_or_else(|| p.clone()),
                month_usd,
                daily_usd,
                budget,
                since_reset_usd,
                resets_on: budget.map(|b| next_reset(b.reset_day, today)),
                balance,
                left_usd,
                days_left: days_left.filter(|d| d.is_finite()),
                used: budget
                    .zip(since_reset_usd)
                    .map(|(b, s)| (s / b.monthly_usd).clamp(0.0, 1.0)),
                level,
            }
        })
        .collect()
}

/// What each provider's calls cost each UTC day since `since`: the tasks'
/// calls (their run records) and the routines' (each business's journal).
fn spending(hub: &Hub, since: DateTime<Utc>) -> Result<BTreeMap<String, BTreeMap<NaiveDate, f64>>> {
    let prices = crate::prices::Prices::load(hub.home());
    let mut out: BTreeMap<String, BTreeMap<NaiveDate, f64>> = BTreeMap::new();
    let mut add = |model: &str, day: NaiveDate, cost: f64| {
        *out.entry(provider_of(model).to_string())
            .or_default()
            .entry(day)
            .or_default() += cost;
    };
    for realm in hub.home().realms()? {
        for state in hub.all_runs(&realm)? {
            for call in state.usage.iter().filter(|c| c.at >= since) {
                add(
                    &call.model,
                    call.at.date_naive(),
                    crate::usage::call_cost(call, &prices),
                );
            }
        }
        if realm.dir().join(crate::business::store::FILE).exists() {
            let store = crate::business::store::Store::of(&realm)?;
            for (day, model, cost) in store.model_costs_since(since)? {
                if let Ok(day) = NaiveDate::parse_from_str(&day, "%Y-%m-%d") {
                    add(&model, day, cost);
                }
            }
        }
    }
    Ok(out)
}

/// DeepSeek's own figure for what is left on the account.
fn deepseek_balance(key: &str) -> Result<Balance> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let body: serde_json::Value = client
        .get("https://api.deepseek.com/user/balance")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    let infos = body["balance_infos"]
        .as_array()
        .context("no balance in DeepSeek's answer")?;
    // USD first: spending is in dollars.
    let info = infos
        .iter()
        .find(|i| i["currency"] == "USD")
        .or_else(|| infos.first())
        .context("no balance in DeepSeek's answer")?;
    Ok(Balance {
        amount: info["total_balance"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .context("unreadable balance")?,
        currency: info["currency"].as_str().unwrap_or("USD").to_string(),
    })
}

static BALANCES: Mutex<Option<(Instant, BTreeMap<String, Balance>)>> = Mutex::new(None);

/// The balances providers tell, read at most once an hour. Each request goes
/// only to the provider's own address, with its own key, from Airth itself:
/// no model and no run sees the key.
fn balances(hub: &Hub) -> BTreeMap<String, Balance> {
    let mut cache = BALANCES.lock().unwrap();
    if let Some((at, b)) = cache.as_ref()
        && at.elapsed() < BALANCE_TTL
    {
        return b.clone();
    }
    let mut out = BTreeMap::new();
    if let Some(key) = hub.provider_key("DEEPSEEK_API_KEY") {
        match deepseek_balance(&key) {
            Ok(b) => {
                out.insert("deepseek".to_string(), b);
            }
            Err(e) => eprintln!("spend: DeepSeek's balance: {e:#}"),
        }
    }
    *cache = Some((Instant::now(), out.clone()));
    out
}

/// Every provider Airth spent on this month, or has a budget or balance for.
pub fn status(hub: &Hub, now: DateTime<Utc>) -> Result<Vec<Status>> {
    let today = now.date_naive();
    let from = (today.with_day(1).unwrap() - Duration::days(31))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let names: BTreeMap<String, String> = crate::models::provider_catalog()
        .into_iter()
        .map(|(id, name, _)| (id.to_string(), name.to_string()))
        .collect();
    Ok(assess(
        &spending(hub, from)?,
        &budgets(hub.home()),
        &balances(hub),
        &names,
        today,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn spent(days: &[(u32, f64)]) -> BTreeMap<NaiveDate, f64> {
        days.iter().map(|&(day, c)| (d(day), c)).collect()
    }

    #[test]
    fn a_model_is_put_with_its_provider() {
        assert_eq!(provider_of("claude-sonnet-4-6"), "anthropic");
        assert_eq!(provider_of("deepseek/deepseek-v4-pro"), "deepseek");
        assert_eq!(provider_of("gpt-5.1"), "openai");
        assert_eq!(provider_of("grok-4"), "xai");
        assert_eq!(provider_of("mistral-large"), "mistral");
        assert_eq!(provider_of("llama3"), "other");
    }

    #[test]
    fn a_balance_is_what_is_left_and_the_rate_says_how_long_it_lasts() {
        let spending = BTreeMap::from([(
            "deepseek".to_string(),
            spent(&[(2, 1.0), (8, 1.0), (9, 1.0), (10, 2.0)]),
        )]);
        let balances = BTreeMap::from([(
            "deepseek".to_string(),
            Balance {
                amount: 6.0,
                currency: "USD".into(),
            },
        )]);
        let s = &assess(
            &spending,
            &BTreeMap::new(),
            &balances,
            &BTreeMap::new(),
            d(10),
        )[0];
        assert_eq!(s.month_usd, 5.0);
        // The last seven days (4 to 10): $4, so $4/7 a day; $6 lasts 10.5 days.
        assert!((s.days_left.unwrap() - 10.5).abs() < 1e-9);
        assert_eq!(s.level, Level::Ok);
        assert_eq!(s.used, None);
    }

    #[test]
    fn a_budget_counts_from_its_reset_day_and_warns_before_it_runs_out() {
        let spending = BTreeMap::from([(
            "anthropic".to_string(),
            spent(&[(3, 20.0), (6, 10.0), (9, 20.0)]),
        )]);
        let budgets = BTreeMap::from([(
            "anthropic".to_string(),
            Budget {
                monthly_usd: 50.0,
                reset_day: 5,
            },
        )]);
        let s = &assess(
            &spending,
            &budgets,
            &BTreeMap::new(),
            &BTreeMap::new(),
            d(10),
        )[0];
        // Since the 5th: $30 of $50, $20 left; $30 over the last 7 days,
        // so $20 lasts 4.7 days.
        assert_eq!(s.since_reset_usd, Some(30.0));
        assert_eq!(s.left_usd, Some(20.0));
        assert_eq!(
            s.resets_on,
            Some(NaiveDate::from_ymd_opt(2026, 12, 5).unwrap())
        );
        assert!((s.used.unwrap() - 0.6).abs() < 1e-9);
        assert_eq!(s.level, Level::Warn, "{:?}", s.days_left);
    }

    #[test]
    fn nothing_left_is_low_and_nothing_to_measure_is_unknown() {
        let spending = BTreeMap::from([
            ("anthropic".to_string(), spent(&[(9, 60.0)])),
            ("openai".to_string(), spent(&[(9, 3.0)])),
        ]);
        let budgets = BTreeMap::from([(
            "anthropic".to_string(),
            Budget {
                monthly_usd: 50.0,
                reset_day: 1,
            },
        )]);
        let all = assess(
            &spending,
            &budgets,
            &BTreeMap::new(),
            &BTreeMap::new(),
            d(10),
        );
        assert_eq!(all[0].level, Level::Low);
        assert_eq!(all[0].used, Some(1.0));
        assert_eq!(all[1].level, Level::Unknown);
        assert_eq!(all[1].month_usd, 3.0);
    }

    #[test]
    fn a_budget_set_on_a_day_not_yet_reached_counts_from_last_month() {
        assert_eq!(
            last_reset(15, d(10)),
            NaiveDate::from_ymd_opt(2026, 10, 15).unwrap()
        );
        assert_eq!(next_reset(15, d(10)), d(15));
        assert_eq!(last_reset(1, d(10)), d(1));
    }

    #[test]
    fn budgets_are_kept_and_checked() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        let b = Budget {
            monthly_usd: 50.0,
            reset_day: 1,
        };
        set_budget(&home, "anthropic", Some(b)).unwrap();
        assert_eq!(budgets(&home)["anthropic"], b);
        assert!(
            set_budget(
                &home,
                "openai",
                Some(Budget {
                    monthly_usd: 0.0,
                    reset_day: 1
                })
            )
            .is_err()
        );
        assert!(
            set_budget(
                &home,
                "openai",
                Some(Budget {
                    monthly_usd: 10.0,
                    reset_day: 31
                })
            )
            .is_err()
        );
        set_budget(&home, "anthropic", None).unwrap();
        assert!(budgets(&home).is_empty());
    }
}
