//! The legal calendar (decision 66): a company's filing and tax deadlines,
//! computed from its legal identity by rules written as data. Each rule is a
//! file in `src/legal/<country>/`, with its source; this module holds only
//! the date arithmetic the rules name.

use crate::frontmatter::{Doc, parse};
use anyhow::{Context, Result, bail};
use chrono::{Datelike, Duration, NaiveDate};

/// The rules Airth knows, by country, as shipped with it.
#[rustfmt::skip]
const RULES: &[(&str, &str, &str)] = &[
    ("GB", "confirmation-statement", include_str!("gb/confirmation-statement.md")),
    ("GB", "annual-accounts", include_str!("gb/annual-accounts.md")),
    ("GB", "company-tax-return", include_str!("gb/company-tax-return.md")),
    ("GB", "corporation-tax-payment", include_str!("gb/corporation-tax-payment.md")),
];

#[rustfmt::skip]
const RULE_KEYS: &[&str] = &[
    "title", "authority", "forms", "from", "after", "months",
    "first_from_incorporation", "first_after", "source", "checked",
];

/// What a deadline counts from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum End {
    /// The end of a confirmation statement's review period.
    Review,
    /// The end of a period of accounts (an accounting reference period).
    Accounts,
    /// The end of a Corporation Tax period: 12 months at most.
    Tax,
}

/// How months are counted: the Companies Act's (s443: from a month's last
/// day to the due month's last day), or from the corresponding date.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Months {
    CompaniesAct,
    Corresponding,
}

/// A delay: months, then days.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Delay {
    months: u32,
    days: i64,
}

#[derive(Debug)]
pub struct Rule {
    pub id: &'static str,
    pub title: String,
    pub authority: String,
    pub source: Vec<String>,
    from: End,
    after: Delay,
    months: Months,
    /// For a first period of accounts longer than 12 months: the later of
    /// this long after incorporation and `first_after` after its end.
    first: Option<(Delay, Delay)>,
}

/// One deadline: a rule's occurrence for one period.
#[derive(Debug, Clone, PartialEq)]
pub struct Deadline {
    pub rule: &'static str,
    pub title: String,
    pub authority: String,
    pub source: Vec<String>,
    /// The end of the period it is for: its key, with the rule.
    pub period_end: NaiveDate,
    pub due: NaiveDate,
}

/// The legal identity the rules need.
#[derive(Debug, Clone)]
pub struct Company {
    pub incorporated_on: NaiveDate,
    /// The accounting reference date: a month and a day.
    pub year_end: (u32, u32),
    /// The confirmation date on the last statement, when filed early.
    pub confirmation_date: Option<NaiveDate>,
}

impl Company {
    /// From `business/profile.md`'s fields; Err when one the rules need
    /// is not filled (see `missing`) or is not a date.
    pub fn of(profile: &Doc) -> Result<Company> {
        let gaps = missing(profile);
        if !gaps.is_empty() {
            bail!("the legal calendar needs {}", gaps.join(", "));
        }
        let text = |key| profile.text(key).unwrap_or_default();
        let year_end = text("fiscal_year_end");
        let year_end = NaiveDate::parse_from_str(&format!("2024-{year_end}"), "%Y-%m-%d")
            .with_context(|| format!("fiscal_year_end '{year_end}' is not like 11-30"))?;
        Ok(Company {
            incorporated_on: date(&text("incorporated_on"))?,
            year_end: (year_end.month(), year_end.day()),
            confirmation_date: profile
                .text("confirmation_date")
                .map(|d| date(&d))
                .transpose()?,
        })
    }
}

/// The profile's fields the rules need and the founder has not filled.
pub fn missing(profile: &Doc) -> Vec<&'static str> {
    let keys = [
        "country",
        "legal_form",
        "incorporated_on",
        "fiscal_year_end",
    ];
    keys.into_iter()
        .filter(|k| profile.text(k).is_none())
        .collect()
}

fn date(text: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .with_context(|| format!("'{text}' is not a date like 2025-11-07"))
}

/// The rules for a company's country and legal form.
pub fn rules(country: &str, form: &str) -> Result<Vec<Rule>> {
    let mut out = Vec::new();
    for (of, id, text) in RULES {
        if *of != country {
            continue;
        }
        let rule = rule(id, text).with_context(|| format!("legal rule {id}"))?;
        if parse(text).list("forms").iter().any(|f| f == form) {
            out.push(rule);
        }
    }
    Ok(out)
}

fn rule(id: &'static str, text: &str) -> Result<Rule> {
    let doc = parse(text);
    doc.only(RULE_KEYS)?;
    let text = |key: &str| doc.text(key).with_context(|| format!("no {key}"));
    let delay = |key: &str| doc.text(key).map(|v| delay(&v)).transpose();
    Ok(Rule {
        id,
        title: text("title")?,
        authority: text("authority")?,
        source: doc.list("source"),
        from: match text("from")?.as_str() {
            "review_end" => End::Review,
            "accounts_end" => End::Accounts,
            "tax_end" => End::Tax,
            other => bail!("from '{other}' is not review_end, accounts_end or tax_end"),
        },
        after: delay("after")?.context("no after")?,
        months: match text("months")?.as_str() {
            "companies-act" => Months::CompaniesAct,
            "corresponding" => Months::Corresponding,
            other => bail!("months '{other}' is not companies-act or corresponding"),
        },
        first: delay("first_from_incorporation")?.zip(delay("first_after")?),
    })
}

/// `9m1d`, `12m`, `14d`.
fn delay(text: &str) -> Result<Delay> {
    let (months, days) = match text.split_once('m') {
        Some((m, d)) => (m.parse()?, d),
        None => (0, text),
    };
    let days = match days.strip_suffix('d') {
        Some(d) => d.parse()?,
        None if days.is_empty() => 0,
        None => bail!("'{text}' is not a delay like 9m1d, 12m or 14d"),
    };
    Ok(Delay { months, days })
}

fn last_day(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("a month");
    (first + chrono::Months::new(1))
        .pred_opt()
        .expect("a day")
        .day()
}

/// `date` plus `delay`. A day that does not exist in the due month becomes
/// its last; under the Companies Act, so does a month's last day.
fn add(date: NaiveDate, delay: Delay, months: Months) -> NaiveDate {
    let total = date.month0() + delay.months;
    let (year, month) = (date.year() + (total / 12) as i32, total % 12 + 1);
    let end = last_day(year, month);
    let day = match months {
        Months::CompaniesAct if date.day() == last_day(date.year(), date.month()) => end,
        _ => date.day().min(end),
    };
    NaiveDate::from_ymd_opt(year, month, day).expect("a day") + Duration::days(delay.days)
}

const YEAR: Delay = Delay {
    months: 12,
    days: 0,
};
const SIX_MONTHS: Delay = Delay { months: 6, days: 0 };

/// The periods of accounts, from incorporation, ending on or before
/// `until`'s year: the first ends on the accounting reference date more
/// than 6 months after incorporation (Companies Act s391(5)).
fn accounts_periods(c: &Company, until: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    let (month, day) = c.year_end;
    let on =
        |year| NaiveDate::from_ymd_opt(year, month, day.min(last_day(year, month))).expect("a day");
    let six = add(c.incorporated_on, SIX_MONTHS, Months::Corresponding);
    let mut end = on(c.incorporated_on.year());
    while end <= six {
        end = on(end.year() + 1);
    }
    let (mut start, mut out) = (c.incorporated_on, Vec::new());
    while start <= until {
        out.push((start, end));
        (start, end) = (end + Duration::days(1), on(end.year() + 1));
    }
    out
}

/// Every deadline of `rules` for periods starting by `until`.
pub fn deadlines(c: &Company, rules: &[Rule], until: NaiveDate) -> Vec<Deadline> {
    let accounts = accounts_periods(c, until);
    let mut out = Vec::new();
    for rule in rules {
        let ends: Vec<(NaiveDate, NaiveDate)> = match rule.from {
            End::Accounts => accounts.clone(),
            // A Corporation Tax period is at most 12 months of a period of
            // accounts.
            End::Tax => accounts
                .iter()
                .flat_map(|&(start, end)| {
                    let mut parts = Vec::new();
                    let mut from = start;
                    while from <= end {
                        let to =
                            (add(from, YEAR, Months::Corresponding) - Duration::days(1)).min(end);
                        parts.push((from, to));
                        from = to + Duration::days(1);
                    }
                    parts
                })
                .collect(),
            // Each review period runs 12 months from the day after the last
            // confirmation date, the first from incorporation.
            End::Review => {
                let mut start = c
                    .confirmation_date
                    .map_or(c.incorporated_on, |d| d + Duration::days(1));
                let mut out = Vec::new();
                while start <= until {
                    let end = add(start, YEAR, Months::Corresponding) - Duration::days(1);
                    out.push((start, end));
                    start = end + Duration::days(1);
                }
                out
            }
        };
        for (i, &(start, end)) in ends.iter().enumerate() {
            let mut due = add(end, rule.after, rule.months);
            if let (0, Some((from_incorporation, after))) = (i, rule.first)
                && add(start, YEAR, Months::Corresponding) <= end
            {
                due = add(start, from_incorporation, rule.months).max(add(end, after, rule.months));
            }
            out.push(Deadline {
                rule: rule.id,
                title: rule.title.clone(),
                authority: rule.authority.clone(),
                source: rule.source.clone(),
                period_end: end,
                due,
            });
        }
    }
    out.sort_by_key(|d| d.due);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        date(text).unwrap()
    }

    fn company(incorporated: &str, year_end: (u32, u32)) -> Company {
        Company {
            incorporated_on: day(incorporated),
            year_end,
            confirmation_date: None,
        }
    }

    fn due(c: &Company, rule: &str, until: &str) -> Vec<(String, String)> {
        deadlines(c, &rules("GB", "ltd").unwrap(), day(until))
            .into_iter()
            .filter(|d| d.rule == rule)
            .map(|d| (d.period_end.to_string(), d.due.to_string()))
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    /// Every rule file loads, names its source and when it was checked.
    #[test]
    fn every_rule_says_its_source() {
        for (_, id, text) in RULES {
            let rule = rule(id, text).unwrap();
            assert!(!rule.source.is_empty(), "{id}");
            date(&parse(text).text("checked").unwrap()).unwrap();
        }
        assert_eq!(rules("GB", "ltd").unwrap().len(), 4);
        assert!(
            rules("FR", "sas").unwrap().is_empty(),
            "France's rules come with Numirama"
        );
    }

    /// Hedgen Ltd, as Companies House publishes it: incorporated 7 November
    /// 2025, first confirmation statement to 6 November 2026, due by 20
    /// November; first accounts to 30 November 2026, due by 7 August 2027.
    #[test]
    fn hedgen_matches_companies_house() {
        let hedgen = company("2025-11-07", (11, 30));
        assert_eq!(
            due(&hedgen, "confirmation-statement", "2027-11-06"),
            pairs(&[("2026-11-06", "2026-11-20"), ("2027-11-06", "2027-11-20")])
        );
        assert_eq!(
            due(&hedgen, "annual-accounts", "2027-11-30"),
            // From 30 November, the due month's last day (s443).
            pairs(&[("2026-11-30", "2027-08-07"), ("2027-11-30", "2028-08-31")])
        );
        assert_eq!(
            due(&hedgen, "company-tax-return", "2026-11-30"),
            pairs(&[("2026-11-30", "2027-11-30")])
        );
        // Two Corporation Tax periods in the first 12 months and 3 weeks;
        // from 30 November, the earlier reading.
        assert_eq!(
            due(&hedgen, "corporation-tax-payment", "2026-11-30"),
            pairs(&[("2026-11-06", "2027-08-07"), ("2026-11-30", "2027-08-31")])
        );
    }

    /// GOV.UK's example: set up 11 May 2024, accounts to 31 May 2025, so
    /// two Corporation Tax periods, 11 May 2024 to 10 May 2025 and 11 May
    /// 2025 to 31 May 2025.
    #[test]
    fn a_long_first_period_splits_in_two_tax_periods() {
        let c = company("2024-05-11", (5, 31));
        assert_eq!(
            due(&c, "corporation-tax-payment", "2024-12-31"),
            pairs(&[("2025-05-10", "2026-02-11"), ("2025-05-31", "2026-03-01")])
        );
        // A first period of 12 months and 3 weeks: 21 months after
        // incorporation is later than 3 months after its end.
        assert_eq!(
            due(&c, "annual-accounts", "2024-12-31"),
            pairs(&[("2025-05-31", "2026-02-11")])
        );
    }

    /// Filing a confirmation statement early moves the next review period.
    #[test]
    fn an_early_confirmation_moves_the_next_one() {
        let mut c = company("2025-11-07", (11, 30));
        c.confirmation_date = Some(day("2026-10-01"));
        assert_eq!(
            due(&c, "confirmation-statement", "2027-10-01"),
            pairs(&[("2027-10-01", "2027-10-15")])
        );
    }

    /// The Companies Act counts from a month's last day to the due month's
    /// last; otherwise a day the due month lacks is its last.
    #[test]
    fn months_are_counted_as_the_rule_says() {
        let nine = Delay { months: 9, days: 0 };
        assert_eq!(
            add(day("2026-11-30"), nine, Months::CompaniesAct),
            day("2027-08-31")
        );
        assert_eq!(
            add(day("2026-11-30"), nine, Months::Corresponding),
            day("2027-08-30")
        );
        assert_eq!(
            add(day("2026-05-31"), nine, Months::Corresponding),
            day("2027-02-28")
        );
        assert_eq!(
            add(
                day("2027-11-29"),
                Delay { months: 3, days: 0 },
                Months::CompaniesAct
            ),
            day("2028-02-29")
        );
        assert_eq!(delay("9m1d").unwrap(), Delay { months: 9, days: 1 });
        assert_eq!(
            delay("14d").unwrap(),
            Delay {
                months: 0,
                days: 14
            }
        );
        assert!(delay("nine months").is_err());
    }

    /// A company whose fields are not filled says which the rules need.
    #[test]
    fn missing_fields_are_named() {
        let profile = parse("---\ncountry: GB\nlegal_form: ltd\nincorporated_on: TODO\n---\n");
        let e = Company::of(&profile).unwrap_err().to_string();
        assert_eq!(
            e,
            "the legal calendar needs incorporated_on, fiscal_year_end"
        );
        let profile = parse(
            "---\ncountry: GB\nlegal_form: ltd\nincorporated_on: 2025-11-07\nfiscal_year_end: 11-30\n---\n",
        );
        assert_eq!(Company::of(&profile).unwrap().year_end, (11, 30));
    }
}
