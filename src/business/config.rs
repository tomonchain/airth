//! The business's config (decision 43): `business/profile.md`, `goal.md`,
//! `lines/<name>.md` and `agents/<name>.md`. A field not known yet is listed
//! in `missing`; a wrong one (unknown key, bad date, no such department or
//! line) refuses the config, naming the file. What agents may do is not here.

use crate::frontmatter::{Doc, Value, parse};
use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use std::path::Path;

#[rustfmt::skip]
const PROFILE: &[&str] = &[
    "name", "legal_name", "legal_form", "country", "status", "registration_number",
    "registered_office", "incorporated_on", "fiscal_year_end", "vat_number", "tax_regime",
    "currency", "bank", "domain", "links",
    // A task's budget (decision 64): optional, with defaults.
    "task_max_minutes", "task_max_cost_usd",
    // The last confirmation statement's date, when filed early, and how
    // many days ahead deadlines are reminded (decision 66).
    "confirmation_date", "reminders",
];
/// Fields Airth has a default for: never listed as left to fill.
const PROFILE_OPTIONAL: &[&str] = &[
    "task_max_minutes",
    "task_max_cost_usd",
    "confirmation_date",
    "reminders",
];
const GOAL: &[&str] = &[
    "goal",
    "measure",
    "target",
    "by",
    "milestones",
    "baseline_minutes",
];
/// Finance entered by hand until a bank is connected (decision 68).
const FINANCE: &[&str] = &["cash", "spent", "as_of", "currency"];
const LINE: &[&str] = &["title", "kind", "status"];
#[rustfmt::skip]
const AGENT: &[&str] = &[
    "title", "first_name", "department", "reports_to", "lines", "mailbox",
    // What its tasks may use (decision 64), and its character (decision 65).
    "integrations", "skills", "repos", "look", "accessory",
];

#[derive(Debug, Default)]
pub struct Config {
    pub profile: Doc,
    pub goal: Doc,
    pub lines: Vec<(String, Doc)>,
    pub agents: Vec<(String, Doc)>,
    /// What is left to fill, as `file: field`.
    pub missing: Vec<String>,
    /// Agent files set aside, each with why: one wrong file (an agent may
    /// write them) must not hide the rest of the team.
    pub broken: Vec<String>,
}

pub fn load(root: &Path, own: &[String]) -> Result<Config> {
    super::migrate(root)?;
    let mut c = Config::default();
    let m = &mut c.missing;
    let profile = read(root, "business/profile.md", PROFILE, m, check_profile)?;
    let goal = read(root, "business/goal.md", GOAL, m, check_goal)?;
    let lines = folder(root, "business/lines", LINE, m, check_line, None)?;
    let b = &mut c.broken;
    let check_agent = |d: &Doc| check_agent(d, own);
    let mut agents = folder(root, "agents", AGENT, m, check_agent, Some(b))?;
    // A department said in other words is read as the department.
    for (_, doc) in &mut agents {
        for (k, v) in &mut doc.fields {
            if k == "department"
                && let Some(Value::Text(t)) = v
                && let Ok(d) = department(t, own)
            {
                *t = d;
            }
        }
    }
    (c.profile, c.goal, c.lines) = (profile, goal, lines);
    // A boss set aside is still an agent: only the wrong file is.
    let mut names: Vec<String> = agents.iter().map(|(n, _)| n.clone()).collect();
    names.extend(c.broken.iter().filter_map(|b| {
        b.strip_prefix("agents/")?
            .split_once(".md:")
            .map(|(n, _)| n.to_string())
    }));
    let named = |n: &str| names.iter().any(|a| a == n);
    let mut broken = Vec::new();
    for (name, agent) in agents {
        let file = format!("agents/{name}.md");
        let boss = agent.text("reports_to");
        let line = agent
            .list("lines")
            .into_iter()
            .find(|l| !c.lines.iter().any(|(n, _)| n == l));
        if let Some(boss) = boss.filter(|b| *b != "founder" && !named(b)) {
            broken.push(format!(
                "{file}: reports_to '{boss}' is neither founder nor an agent"
            ));
        } else if let Some(line) = line {
            broken.push(format!(
                "{file}: line '{line}' has no file in business/lines/"
            ));
        } else {
            c.agents.push((name, agent));
        }
    }
    c.broken.extend(broken);
    c.broken.sort();
    Ok(c)
}

/// Read one file, shown to the founder as `name`: refuse unknown keys,
/// check its values, and list the expected fields not known yet. A missing
/// file is all missing.
fn read(
    root: &Path,
    name: &str,
    keys: &[&str],
    missing: &mut Vec<String>,
    check: impl Fn(&Doc) -> Result<Vec<&'static str>>,
) -> Result<Doc> {
    let doc = match std::fs::read_to_string(root.join(name)) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Doc::default(),
        Err(e) => return Err(e).with_context(|| format!("reading {name}")),
    };
    doc.only(keys).context(name.to_string())?;
    let expected = check(&doc).context(name.to_string())?;
    missing.extend(
        doc.unknown(&expected)
            .iter()
            .map(|k| format!("{name}: {k}")),
    );
    Ok(doc)
}

/// Every `<name>.md` of the folder `shown`, by name. With `set_aside`, a
/// wrong file is listed there, with why, instead of refusing them all.
fn folder(
    root: &Path,
    shown: &str,
    keys: &[&str],
    missing: &mut Vec<String>,
    check: impl Fn(&Doc) -> Result<Vec<&'static str>>,
    mut set_aside: Option<&mut Vec<String>>,
) -> Result<Vec<(String, Doc)>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(root.join(shown)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e).with_context(|| format!("reading {shown}")),
    };
    for entry in entries {
        let path = entry?.path();
        let (Some(name), true) = (
            path.file_stem().and_then(|s| s.to_str()),
            path.extension().is_some_and(|e| e == "md"),
        ) else {
            continue;
        };
        let file = format!("{shown}/{name}.md");
        let doc = if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            Err(anyhow::anyhow!(
                "{file}: a name is lowercase letters, digits, dashes"
            ))
        } else {
            read(root, &file, keys, missing, &check)
        };
        match (doc, set_aside.as_deref_mut()) {
            (Ok(doc), _) => out.push((name.to_string(), doc)),
            (Err(e), Some(aside)) => aside.push(format!("{e:#}")),
            (Err(e), None) => return Err(e),
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// `business/finance.md`, the figures the founder enters by hand
/// (decision 68): None without the file; refused, naming the field, when a
/// value is wrong.
pub fn finance(root: &Path) -> Result<Option<Doc>> {
    const FILE: &str = "business/finance.md";
    if !root.join(FILE).exists() {
        return Ok(None);
    }
    read(root, FILE, FINANCE, &mut Vec::new(), |d| {
        for key in ["cash", "spent"] {
            if let Some(v) = d.text(key)
                && v.parse::<f64>().map_or(true, |n| !n.is_finite())
            {
                bail!("{key} '{v}' is not a number like 12300.50");
            }
        }
        if d.text("spent")
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|n| n < 0.0)
        {
            bail!("spent is what went out this month: a number of zero or more");
        }
        date(d, "as_of")?;
        Ok(Vec::new())
    })
    .map(Some)
}

/// The departments every business has, in the order shown, with the names
/// the founder reads (decision 79).
#[rustfmt::skip]
pub const DEPARTMENTS: &[(&str, &str)] = &[
    ("leadership", "Leadership"), ("marketing", "Marketing"), ("sales", "Sales"),
    ("support", "Support"), ("tech", "Product & tech"), ("operations", "Operations"),
    ("finance", "Finance & admin"), ("legal", "Legal & compliance"), ("data", "Data & research"),
];

/// Other words for them, read as the department: what an agent naming a
/// department in its own words most likely means.
#[rustfmt::skip]
const SYNONYMS: &[(&str, &str)] = &[
    ("management", "leadership"), ("direction", "leadership"), ("executive", "leadership"),
    ("content", "marketing"), ("editorial", "marketing"), ("growth", "marketing"),
    ("communication", "marketing"), ("social", "marketing"), ("community", "marketing"),
    ("brand", "marketing"),
    ("business-development", "sales"), ("partnerships", "sales"),
    ("customer-success", "support"), ("customer-service", "support"),
    ("product", "tech"), ("development", "tech"), ("engineering", "tech"), ("dev", "tech"),
    ("it", "tech"),
    ("ops", "operations"), ("logistics", "operations"),
    ("accounting", "finance"), ("admin", "finance"),
    ("compliance", "legal"),
    ("research", "data"), ("analytics", "data"), ("intelligence", "data"),
];

/// A department as written, as a key: lowercase, words joined by dashes.
pub fn key(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The department `value` means: one of the nine, said in other words, or
/// one the founder added for the business (`own`). Anything else is refused,
/// naming what is allowed, so an agent can correct itself.
pub fn department(value: &str, own: &[String]) -> Result<String> {
    let k = key(value);
    if let Some((d, _)) = DEPARTMENTS.iter().find(|(d, _)| *d == k) {
        return Ok(d.to_string());
    }
    if let Some((_, d)) = SYNONYMS.iter().find(|(s, _)| *s == k) {
        return Ok(d.to_string());
    }
    if own.iter().any(|o| key(o) == k) {
        return Ok(k);
    }
    let mut allowed: Vec<&str> = DEPARTMENTS.iter().map(|(d, _)| *d).collect();
    allowed.extend(own.iter().map(String::as_str));
    bail!(
        "department '{}' is not one of: {} (the founder adds departments in Settings)",
        value.trim(),
        allowed.join(", ")
    )
}

/// A department the founder adds for the business: a new name, not one of
/// the nine nor another word for one.
pub fn new_department(value: &str) -> Result<String> {
    let k = key(value);
    if k.is_empty() || k.chars().count() > 40 {
        bail!("a department is a short name, at most 40 characters");
    }
    if !k
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("a department is letters, digits and spaces");
    }
    if let Ok(d) = department(&k, &[]) {
        bail!("'{}' is already the department {d}", value.trim());
    }
    Ok(k)
}

fn one_of(d: &Doc, key: &str, allowed: &[&str]) -> Result<()> {
    match d.text(key) {
        Some(v) if !allowed.contains(&v.as_str()) => {
            bail!("{key} '{v}' is not one of: {}", allowed.join(", "))
        }
        _ => Ok(()),
    }
}

fn date(d: &Doc, key: &str) -> Result<()> {
    if let Some(v) = d.text(key) {
        NaiveDate::parse_from_str(&v, "%Y-%m-%d")
            .with_context(|| format!("{key} '{v}' is not a date like 2025-11-30"))?;
    }
    Ok(())
}

/// Each check refuses a wrong value and says which fields are expected.
fn check_profile(d: &Doc) -> Result<Vec<&'static str>> {
    one_of(d, "status", &["planned", "registered"])?;
    if let Some(c) = d.text("country")
        && !(c.len() == 2 && c.chars().all(|c| c.is_ascii_uppercase()))
    {
        bail!("country '{c}' is not an ISO code like GB or FR");
    }
    date(d, "incorporated_on")?;
    date(d, "confirmation_date")?;
    for days in d.list("reminders") {
        if days.parse::<u16>().is_err() {
            bail!("reminders '{days}' is not a number of days: [30, 7]");
        }
    }
    if let Some(v) = d.text("fiscal_year_end") {
        NaiveDate::parse_from_str(&format!("2024-{v}"), "%Y-%m-%d")
            .with_context(|| format!("fiscal_year_end '{v}' is not a month and day like 03-31"))?;
    }
    // Before registration, only who it is and where.
    Ok(match d.text("status").as_deref() {
        Some("registered") => PROFILE
            .iter()
            .copied()
            .filter(|k| !PROFILE_OPTIONAL.contains(k))
            .collect(),
        _ => vec!["name", "country", "status"],
    })
}

fn check_goal(d: &Doc) -> Result<Vec<&'static str>> {
    date(d, "by")?;
    // Its date first ("2026-12-31 four weeks alone") or last ("four weeks
    // alone by 2026-12-31"), with something said besides.
    let is_date = |w: &str| NaiveDate::parse_from_str(w, "%Y-%m-%d").is_ok();
    for m in d.list("milestones") {
        let words: Vec<&str> = m.split_whitespace().collect();
        let first = words.len() > 1 && is_date(words[0]);
        let last =
            words.len() > 2 && words[words.len() - 2] == "by" && is_date(words[words.len() - 1]);
        if !first && !last {
            bail!(
                "milestone '{m}' has no date: 2026-12-31 four weeks alone, or four weeks alone by 2026-12-31"
            );
        }
    }
    if let Some(v) = d.text("baseline_minutes")
        && v.parse::<u32>().is_err()
    {
        bail!("baseline_minutes '{v}' is not a number of minutes");
    }
    Ok(GOAL.to_vec())
}

fn check_line(d: &Doc) -> Result<Vec<&'static str>> {
    one_of(d, "kind", &["editorial", "range", "brand"])?;
    one_of(d, "status", &["active", "paused"])?;
    Ok(vec!["title", "kind"])
}

fn check_agent(d: &Doc, own: &[String]) -> Result<Vec<&'static str>> {
    if let Some(v) = d.text("department") {
        department(&v, own)?;
    }
    Ok(vec!["title", "department"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Decisions 64 and 65's fields load: an agent's scope and look, the
    /// profile's task budget.
    /// Decision 68: finance by hand is optional; a value that is not a
    /// number or a date, money out below zero, or an unknown key is refused.
    #[test]
    fn finance_by_hand_is_checked() {
        let root = tempfile::tempdir().unwrap();
        assert!(finance(root.path()).unwrap().is_none());
        std::fs::create_dir_all(root.path().join("business")).unwrap();
        let write =
            |text: &str| std::fs::write(root.path().join("business/finance.md"), text).unwrap();
        write("---\ncash: 8000.50\nspent: 0\nas_of: 2026-11-02\ncurrency: EUR\n---\n");
        assert_eq!(
            finance(root.path()).unwrap().unwrap().text("cash").unwrap(),
            "8000.50"
        );
        for (text, error) in [
            ("---\ncash: lots\n---\n", "cash 'lots' is not a number"),
            ("---\nspent: -5\n---\n", "zero or more"),
            ("---\nas_of: 2 Nov\n---\n", "as_of '2 Nov' is not a date"),
            ("---\nbalance: 1\n---\n", "balance"),
        ] {
            write(text);
            let e = format!("{:#}", finance(root.path()).unwrap_err());
            assert!(
                e.contains("business/finance.md") && e.contains(error),
                "{e}"
            );
        }
    }

    /// A milestone says its date first or last ("2026-12-31 what" or "what
    /// by 2026-12-31"), written one per line as agents do; without a date it
    /// is refused.
    #[test]
    fn milestones_take_their_date_first_or_last() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\n---\nA business.\n",
        );
        write(
            r,
            "business/goal.md",
            "---\ngoal: Be the reference\nby: 2027-12-31\nmilestones:\n  - 2026-12-31 four weeks alone\n  \
             - First firm signed by 2027-03-31\n---\nStrategy.\n",
        );
        let config = load(r, &[]).unwrap();
        assert_eq!(config.goal.list("milestones").len(), 2);
        assert!(!config.missing.iter().any(|m| m.contains("milestones")));
        write(
            r,
            "business/goal.md",
            "---\nmilestones:\n  - First firm signed soon\n---\n",
        );
        let e = format!("{:#}", load(r, &[]).unwrap_err());
        assert!(e.contains("First firm signed soon"), "{e}");
    }

    #[test]
    fn task_and_character_fields_load() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\ntask_max_minutes: 10\ntask_max_cost_usd: 0.5\n---\nA business.\n",
        );
        write(
            r,
            "agents/writer.md",
            "---\ntitle: Writer\ndepartment: marketing\nintegrations: typefully\nskills: \nrepos: site\nlook: 0a1b2c3d\naccessory: book\n---\nWrite.\n",
        );
        let config = load(r, &[]).unwrap();
        assert_eq!(
            config.agents[0].1.text("integrations").as_deref(),
            Some("typefully")
        );
        assert_eq!(
            config.profile.text("task_max_minutes").as_deref(),
            Some("10")
        );
    }

    /// Decision 66: the last confirmation date is optional, and a date.
    #[test]
    fn the_confirmation_date_is_a_date() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\nconfirmation_date: 2026-10-01\n---\n",
        );
        assert!(
            load(r, &[])
                .unwrap()
                .missing
                .iter()
                .all(|m| !m.contains("confirmation_date"))
        );
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\nconfirmation_date: soon\n---\n",
        );
        assert!(format!("{:#}", load(r, &[]).unwrap_err()).contains("confirmation_date 'soon'"));
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\nreminders: [30, a week]\n---\n",
        );
        assert!(format!("{:#}", load(r, &[]).unwrap_err()).contains("reminders 'a week'"));
    }
    #[test]
    fn a_business_loads_with_what_is_left_to_fill() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\nlegal_name: Acme Ltd\nlegal_form: ltd\ncountry: GB\nstatus: registered\nregistration_number: 123\nincorporated_on: 2025-11-03\nfiscal_year_end: 11-30\nvat_number: TODO\nbank: A bank\ndomain: acme.test\nlinks: [https://acme.test]\n---\n# Acme\n",
        );
        write(
            r,
            "business/goal.md",
            "---\ngoal: Run alone\nmeasure: weeks\ntarget: 4\nby: 2026-12-31\nmilestones: [2026-11-30 first week]\nbaseline_minutes: 300\n---\nStay calm.\n",
        );
        write(
            r,
            "business/lines/news.md",
            "---\ntitle: News\nkind: editorial\nstatus: active\n---\n",
        );
        write(
            r,
            "agents/watcher.md",
            "---\ntitle: Watcher\ndepartment: marketing\nreports_to: founder\nlines: [news]\nmailbox:\n---\nWatch.\n",
        );
        write(
            r,
            "agents/writer.md",
            "---\ntitle: Writer\ndepartment: marketing\nreports_to: watcher\n---\nWrite.\n",
        );
        write(r, "agents/notes.txt", "not an agent");
        let c = load(r, &[]).unwrap();
        assert_eq!(c.profile.text("name").as_deref(), Some("Acme"));
        assert_eq!(c.goal.body, "Stay calm.\n");
        assert_eq!(c.goal.text("baseline_minutes").as_deref(), Some("300"));
        let agents: Vec<&str> = c.agents.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(agents, vec!["watcher", "writer"]);
        assert_eq!(
            c.missing,
            vec![
                "business/profile.md: registered_office",
                "business/profile.md: vat_number",
                "business/profile.md: tax_regime",
                "business/profile.md: currency",
            ]
        );
    }

    #[test]
    fn a_planned_business_needs_little_and_an_empty_folder_loads() {
        let root = tempfile::tempdir().unwrap();
        let c = load(root.path(), &[]).unwrap();
        assert!(c.agents.is_empty() && c.profile.fields.is_empty());
        assert!(c.missing.contains(&"business/profile.md: name".to_string()));
        write(
            root.path(),
            "business/profile.md",
            "---\nname: Numi\ncountry: FR\nstatus: planned\n---\n",
        );
        let c = load(root.path(), &[]).unwrap();
        assert!(
            !c.missing
                .iter()
                .any(|m| m.starts_with("business/profile.md")),
            "{:?}",
            c.missing
        );
    }

    #[test]
    fn wrong_fields_refuse_the_config_naming_the_file() {
        let cases = [
            (
                "business/profile.md",
                "---\nnmae: typo\n---\n",
                "unknown field 'nmae'",
            ),
            (
                "business/profile.md",
                "---\ncountry: France\n---\n",
                "not an ISO code",
            ),
            (
                "business/profile.md",
                "---\nincorporated_on: November 2025\n---\n",
                "not a date",
            ),
            (
                "business/goal.md",
                "---\nmilestones: [soon]\n---\n",
                "has no date",
            ),
            (
                "business/goal.md",
                "---\nbaseline_minutes: lots\n---\n",
                "not a number",
            ),
            (
                "agents/x.md",
                "---\ndepartment: astrology\n---\n",
                "department 'astrology' is not one of: leadership,",
            ),
            (
                "agents/x.md",
                "---\nreports_to: ghost\n---\n",
                "neither founder nor an agent",
            ),
            ("agents/x.md", "---\nlines: [nope]\n---\n", "line 'nope'"),
            ("agents/Bad Name.md", "---\n---\n", "lowercase"),
            (
                "business/lines/l.md",
                "---\nkind: blog\n---\n",
                "kind 'blog'",
            ),
        ];
        for (path, text, why) in cases {
            let root = tempfile::tempdir().unwrap();
            write(root.path(), path, text);
            // A wrong agent is set aside; any other wrong file refuses all.
            let err = match load(root.path(), &[]) {
                Ok(c) if path.starts_with("agents/") => {
                    assert!(c.agents.is_empty(), "{path}");
                    c.broken.join("\n")
                }
                Ok(_) => panic!("{path}: loaded"),
                Err(e) => format!("{e:#}"),
            };
            assert!(err.contains(why), "{path}: {err}");
            assert!(err.contains(path), "{path}: {err}");
        }
    }

    /// Decision 79: a department is one of the nine, said in other words
    /// or not, or one the founder added; anything else names the list.
    #[test]
    fn departments_are_the_nine_their_other_words_and_the_founders() {
        let own = ["events".to_string()];
        for (written, read) in [
            ("tech", "tech"),
            ("Development", "tech"),
            ("product", "tech"),
            ("Customer service", "support"),
            ("customer_success", "support"),
            ("Research", "data"),
            ("compliance", "legal"),
            ("Events", "events"),
        ] {
            assert_eq!(department(written, &own).unwrap(), read, "{written}");
        }
        let err = format!("{:#}", department("astrology", &own).unwrap_err());
        assert!(err.contains("one of: leadership, marketing"), "{err}");
        assert!(err.contains("data, events"), "{err}");
        assert!(err.contains("Settings"), "{err}");
        // The file keeps what was written; the team reads the department.
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "agents/dev.md",
            "---\ntitle: Developer\ndepartment: development\n---\n",
        );
        let c = load(root.path(), &[]).unwrap();
        assert!(c.broken.is_empty(), "{:?}", c.broken);
        assert_eq!(c.agents[0].1.text("department").unwrap(), "tech");
    }

    /// The founder adds a department with a new name only.
    #[test]
    fn a_department_the_founder_adds_is_a_new_name() {
        assert_eq!(
            new_department("Customer events").unwrap(),
            "customer-events"
        );
        assert!(new_department("tech").is_err());
        assert!(
            new_department("Development").is_err(),
            "another word for tech"
        );
        assert!(new_department(" ").is_err());
        assert!(new_department("R&D").is_err());
        assert!(new_department(&"x".repeat(41)).is_err());
    }

    /// An agent writing a wrong file leaves the rest of the team shown,
    /// including those who report to the agent set aside.
    #[test]
    fn a_wrong_agent_file_is_set_aside_and_the_team_stays() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(
            r,
            "agents/chief.md",
            "---\ntitle: Chief\ndepartment: leadership\n---\n",
        );
        write(
            r,
            "agents/dev.md",
            "---\ntitle: Dev\ndepartment: product\nreports: chief\n---\n",
        );
        write(
            r,
            "agents/intern.md",
            "---\ntitle: Intern\ndepartment: product\nreports_to: dev\n---\n",
        );
        let c = load(r, &[]).unwrap();
        let names: Vec<_> = c.agents.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["chief", "intern"]);
        assert_eq!(c.broken.len(), 1);
        assert!(c.broken[0].starts_with("agents/dev.md: "), "{:?}", c.broken);
        assert!(
            c.broken[0].contains("unknown field 'reports'"),
            "{:?}",
            c.broken
        );
    }
}
