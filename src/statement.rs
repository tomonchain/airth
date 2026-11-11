//! A Revolut statement exported as CSV (decision 123): its completed lines,
//! read by the names in its header, so the personal and the business
//! exports, and a column Revolut adds, all read the same. Exchanges between
//! the business's own currencies are neither in nor out.

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::collections::BTreeMap;

/// One completed line of the statement, signed: negative went out.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Revolut's id for the transaction; a hash of the line when the export
    /// has none, so the same line imported twice is kept once.
    pub id: String,
    pub account: String,
    pub currency: String,
    /// `YYYY-MM-DDTHH:MM:SS`, as completed.
    pub at: String,
    pub kind: String,
    pub description: String,
    pub amount: f64,
    pub balance: Option<f64>,
}

/// What a statement held: its completed lines, oldest first, and how many
/// were left out because they were not completed.
#[derive(Debug, PartialEq)]
pub struct Statement {
    pub lines: Vec<Line>,
    pub not_completed: usize,
}

/// A statement is refused past this size: a year of a small business is far
/// below it.
pub const MAX_BYTES: usize = 8 << 20;

pub fn parse(text: &str) -> Result<Statement> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or_default();
    let separator = if first.matches(';').count() > first.matches(',').count() {
        ';'
    } else {
        ','
    };
    let mut rows = records(text, separator).into_iter();
    let Some(header) = rows.next() else {
        bail!("the file is empty");
    };
    let names: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let find = |wanted: &[&str]| {
        wanted.iter().find_map(|w| {
            names
                .iter()
                .position(|n| n == w || n.starts_with(&format!("{w} (")))
        })
    };
    let date = find(&[
        "date completed",
        "completed date",
        "date started",
        "started date",
        "date",
    ]);
    let amount = find(&["total amount", "amount"]);
    let fee = find(&["fee"]).filter(|_| find(&["total amount"]).is_none());
    let currency = find(&["payment currency", "currency"]);
    let balance = find(&["balance"]);
    let missing: Vec<&str> = [("Date", date), ("Amount", amount), ("Currency", currency)]
        .into_iter()
        .filter(|(_, at)| at.is_none())
        .map(|(name, _)| name)
        .collect();
    if !missing.is_empty() {
        bail!(
            "this is not a Revolut statement: it has no {} column. Export the CSV from Revolut, Statements, Transaction statement",
            missing.join(", ")
        );
    }
    let (date, amount, currency) = (
        date.unwrap_or(0),
        amount.unwrap_or(0),
        currency.unwrap_or(0),
    );
    let id = find(&["id", "transaction id"]);
    let state = find(&["state"]);
    let kind = find(&["type"]);
    let description = find(&["description"]);
    let account = find(&["account", "product"]);

    let mut lines = Vec::new();
    let mut not_completed = 0;
    for (n, row) in rows.enumerate() {
        if row.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let field = |i: Option<usize>| i.and_then(|i| row.get(i)).map_or("", |f| f.trim());
        if state.is_some() && !field(state).eq_ignore_ascii_case("completed") {
            not_completed += 1;
            continue;
        }
        let row_no = n + 2;
        let Some(at) = moment(field(Some(date))) else {
            bail!(
                "line {row_no}: the date {:?} is not a date",
                field(Some(date))
            );
        };
        let Some(mut value) = number(field(Some(amount))) else {
            bail!(
                "line {row_no}: the amount {:?} is not a number",
                field(Some(amount))
            );
        };
        value -= number(field(fee)).unwrap_or(0.0);
        let currency = field(Some(currency)).to_uppercase();
        let account = match field(account) {
            "" => currency.clone(),
            a => a.to_string(),
        };
        let balance = number(field(balance));
        let description = field(description).to_string();
        let id = match field(id) {
            "" => format!(
                "line:{}",
                &crate::update::sha256_hex(
                    format!("{account}|{at}|{description}|{value}|{balance:?}").as_bytes()
                )[..32]
            ),
            id => id.to_string(),
        };
        lines.push(Line {
            id,
            account,
            currency,
            at,
            kind: field(kind).to_lowercase().replace(' ', "_"),
            description,
            amount: value,
            balance,
        });
    }
    // Revolut lists the newest first: the lines are kept in time order, so
    // the last of the same second is the latest balance.
    if lines
        .first()
        .zip(lines.last())
        .is_some_and(|(a, b)| a.at > b.at)
    {
        lines.reverse();
    }
    Ok(Statement {
        lines,
        not_completed,
    })
}

/// The fields of each record, quotes and line breaks inside quotes kept.
fn records(text: &str, separator: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut field) = (Vec::new(), String::new());
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == separator && !quoted => row.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// A date or a date and time, as `YYYY-MM-DDTHH:MM:SS`.
fn moment(text: &str) -> Option<String> {
    let day = NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()?;
    let time: String = text.get(11..).unwrap_or_default().chars().take(8).collect();
    let time = if time.len() == 8 && chrono::NaiveTime::parse_from_str(&time, "%H:%M:%S").is_ok() {
        time
    } else if time.len() >= 5 && chrono::NaiveTime::parse_from_str(&time[..5], "%H:%M").is_ok() {
        format!("{}:00", &time[..5])
    } else {
        "00:00:00".into()
    };
    Some(format!("{day}T{time}"))
}

/// "-1,234.50", "1234,5" or "1 234.50": a number, whatever the separators.
fn number(text: &str) -> Option<f64> {
    let mut t: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
        .collect();
    if t.contains('.') {
        t.retain(|c| c != ',');
    } else {
        t = t.replace(',', ".");
    }
    if t.is_empty() {
        return None;
    }
    t.parse().ok()
}

/// The figures the HQ shows from the lines kept: in the main currency (the
/// profile's when the business holds it, else the one most cash is in),
/// cash as the latest balance of each account, money in and out this
/// month, and runway on the last three months' spending (fewer when the
/// statements cover less).
pub fn figures(
    lines: &[Line],
    currency: Option<&str>,
    now: DateTime<Utc>,
) -> Option<crate::bank::Finance> {
    let mut latest: BTreeMap<&str, &Line> = BTreeMap::new();
    for line in lines.iter().filter(|l| l.balance.is_some()) {
        latest.insert(&line.account, line);
    }
    let mut by_currency: BTreeMap<&str, f64> = BTreeMap::new();
    for line in latest.values() {
        *by_currency.entry(&line.currency).or_default() += line.balance.unwrap_or(0.0);
    }
    let main = match currency.map(str::to_uppercase) {
        Some(c) if by_currency.contains_key(c.as_str()) => c,
        _ => by_currency
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))?
            .0
            .to_string(),
    };
    let cash = by_currency.get(main.as_str()).copied().unwrap_or(0.0);
    let month = now.format("%Y-%m").to_string();
    let since = (now - Duration::days(90)).format("%Y-%m-%d").to_string();
    let (mut money_in, mut money_out, mut out_90) = (0.0, 0.0, 0.0);
    let mut out_by_day = BTreeMap::new();
    let mut first = None::<&str>;
    for line in lines
        .iter()
        .filter(|l| l.currency == main && l.kind != "exchange")
    {
        first = Some(first.map_or(&line.at[..], |f| f.min(&line.at[..])));
        if line.at.starts_with(&month) {
            if line.amount < 0.0 {
                money_out -= line.amount;
            } else {
                money_in += line.amount;
            }
        }
        if line.amount < 0.0 && line.at[..10] >= *since {
            out_90 -= line.amount;
            *out_by_day.entry(line.at[..10].to_string()).or_insert(0.0) -= line.amount;
        }
    }
    // Months covered, between one and three: a statement of six weeks is not
    // spread over three months.
    let covered = first
        .and_then(|f| NaiveDate::parse_from_str(&f[..10], "%Y-%m-%d").ok())
        .map_or(3.0, |f| {
            let days = (now.date_naive() - f).num_days() as f64;
            (days / 30.0).clamp(1.0, 3.0)
        });
    let monthly_out = out_90 / covered;
    Some(crate::bank::Finance {
        currency: main,
        cash,
        money_in,
        money_out,
        runway_months: (monthly_out > 0.0).then(|| cash / monthly_out),
        read_at: now,
        out_by_day,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUSINESS: &str = "\u{feff}Date started (UTC),Date completed (UTC),ID,Type,State,Description,Reference,Payer,Card number,Orig currency,Orig amount,Payment currency,Amount,Total amount,Exchange rate,Fee,Fee currency,Balance,Account\n\
2026-11-09 10:00:00,2026-11-09 10:00:05,t5,CARD_PAYMENT,PENDING,Anthropic,,,,GBP,-20,GBP,-20,-20,,0,GBP,,Main\n\
2026-11-08 09:00:00,2026-11-08 09:00:02,t4,CARD_PAYMENT,COMPLETED,\"Typefully, Inc.\",,,,USD,-15,GBP,-12.00,-12.30,,0.30,GBP,4987.70,Main\n\
2026-11-03 12:00:00,2026-11-03 12:00:00,t3,TRANSFER,COMPLETED,Client invoice 12,INV-12,Acme,,GBP,1000,GBP,1000.00,1000.00,,0,GBP,5000.00,Main\n\
2026-11-02 08:00:00,2026-11-02 08:00:00,t2,EXCHANGE,COMPLETED,To EUR,,,,GBP,-500,GBP,-500.00,-500.00,,0,GBP,4000.00,Main\n\
2026-11-02 08:00:00,2026-11-02 08:00:00,t2e,EXCHANGE,COMPLETED,From GBP,,,,EUR,575,EUR,575.00,575.00,,0,EUR,575.00,Euro\n\
2026-10-15 08:00:00,2026-10-15 08:00:00,t1,CARD_PAYMENT,COMPLETED,OVH,,,,GBP,-500,GBP,-500.00,-500.00,,0,GBP,4500.00,Main\n";

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-11T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn a_business_statement_reads_its_completed_lines_oldest_first() {
        let s = parse(BUSINESS).unwrap();
        assert_eq!(s.not_completed, 1);
        let ids: Vec<&str> = s.lines.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["t1", "t2e", "t2", "t3", "t4"]);
        let card = &s.lines[4];
        assert_eq!(card.description, "Typefully, Inc.");
        assert_eq!(card.amount, -12.30);
        assert_eq!(card.balance, Some(4987.70));
        assert_eq!(card.at, "2026-11-08T09:00:02");
        assert_eq!(card.kind, "card_payment");
    }

    #[test]
    fn cash_is_the_latest_balance_and_exchanges_are_neither_in_nor_out() {
        let s = parse(BUSINESS).unwrap();
        let f = figures(&s.lines, Some("gbp"), now()).unwrap();
        assert_eq!(f.currency, "GBP");
        assert_eq!(f.cash, 4987.70);
        assert_eq!(f.money_in, 1000.0);
        assert!((f.money_out - 12.30).abs() < 1e-9);
        // Out since 15 October: 512.30 over one month covered.
        let runway = f.runway_months.unwrap();
        assert!((runway - 4987.70 / 512.30).abs() < 1e-9, "{runway}");
        assert_eq!(f.out_by_day.get("2026-10-15"), Some(&500.0));
        // Without the profile's currency, the one most cash is in.
        assert_eq!(
            figures(&s.lines, Some("CHF"), now()).unwrap().currency,
            "GBP"
        );
    }

    #[test]
    fn a_personal_export_without_ids_reads_too_and_its_lines_keep_one_id() {
        let text = "Type,Product,Started Date,Completed Date,Description,Amount,Fee,Currency,State,Balance\n\
Card Payment,Current,2026-11-01 10:00:00,2026-11-02 10:00:00,Coffee,-3.50,0.00,EUR,COMPLETED,96.50\n\
Topup,Current,2026-10-01 10:00:00,2026-10-01 10:00:00,Top-up,100.00,0.00,EUR,COMPLETED,100.00\n";
        let a = parse(text).unwrap();
        let b = parse(text).unwrap();
        assert_eq!(a.lines.len(), 2);
        assert!(a.lines[0].id.starts_with("line:"));
        assert_eq!(a.lines, b.lines);
        assert_eq!(a.lines[1].account, "Current");
        assert_eq!(a.lines[1].amount, -3.50);
    }

    #[test]
    fn a_file_that_is_not_a_statement_says_what_it_lacks() {
        let err = parse("Name,Total\nA,1\n").unwrap_err().to_string();
        assert!(err.contains("no Date, Amount, Currency column"), "{err}");
        let err = parse("Date,Amount,Currency\n2026-11-02,abc,EUR\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 2: the amount"), "{err}");
    }

    #[test]
    fn separators_and_quotes_are_read_as_written() {
        let text = "Date;Amount;Currency;Description\n2026-11-02;-1 234,50;EUR;\"Rent; \"\"November\"\"\nflat\"\n";
        let s = parse(text).unwrap();
        assert_eq!(s.lines[0].amount, -1234.50);
        assert_eq!(s.lines[0].description, "Rent; \"November\"\nflat");
        assert_eq!(number("-1,234.50"), Some(-1234.50));
    }
}
