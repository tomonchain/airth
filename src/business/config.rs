//! The business's config (decision 43): `business/profile.md`, `goal.md`,
//! `lines/<name>.md` and `agents/<name>.md`. A field not known yet is listed
//! in `missing`; a wrong one (unknown key, bad date, no such department or
//! line) refuses the config, naming the file. What agents may do is not here.

use crate::frontmatter::{Doc, parse};
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
const LINE: &[&str] = &["title", "kind", "status"];
#[rustfmt::skip]
const AGENT: &[&str] = &[
    "title", "department", "reports_to", "lines", "mailbox",
    // What its tasks may use (decision 64), and its character (decision 65).
    "integrations", "skills", "repos", "look", "accessory",
];

/// The seven departments of `CLAUDE.md`, fixed in Airth.
#[rustfmt::skip]
pub const DEPARTMENTS: &[&str] = &[
    "leadership", "marketing", "sales", "support", "product", "operations", "finance",
];

#[derive(Debug, Default)]
pub struct Config {
    pub profile: Doc,
    pub goal: Doc,
    pub lines: Vec<(String, Doc)>,
    pub agents: Vec<(String, Doc)>,
    /// What is left to fill, as `file: field`.
    pub missing: Vec<String>,
}

pub fn load(root: &Path) -> Result<Config> {
    super::migrate(root)?;
    let mut c = Config::default();
    let m = &mut c.missing;
    let profile = read(root, "business/profile.md", PROFILE, m, check_profile)?;
    let goal = read(root, "business/goal.md", GOAL, m, check_goal)?;
    let lines = folder(root, "business/lines", LINE, m, check_line)?;
    let agents = folder(root, "agents", AGENT, m, check_agent)?;
    (c.profile, c.goal, c.lines, c.agents) = (profile, goal, lines, agents);
    for (name, agent) in &c.agents {
        let file = format!("agents/{name}.md");
        if let Some(boss) = agent.text("reports_to")
            && boss != "founder"
            && !c.agents.iter().any(|(n, _)| *n == boss)
        {
            bail!("{file}: reports_to '{boss}' is neither founder nor an agent");
        }
        for line in agent.list("lines") {
            if !c.lines.iter().any(|(n, _)| *n == line) {
                bail!("{file}: line '{line}' has no file in business/lines/");
            }
        }
    }
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

/// Every `<name>.md` of the folder `shown`, by name.
fn folder(
    root: &Path,
    shown: &str,
    keys: &[&str],
    missing: &mut Vec<String>,
    check: impl Fn(&Doc) -> Result<Vec<&'static str>>,
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
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            bail!("{file}: a name is lowercase letters, digits, dashes");
        }
        out.push((name.to_string(), read(root, &file, keys, missing, &check)?));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
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
    for m in d.list("milestones") {
        let day = m.split_whitespace().next().unwrap_or("");
        if NaiveDate::parse_from_str(day, "%Y-%m-%d").is_err() || m.trim() == day {
            bail!("milestone '{m}' is not a date then what: 2026-12-31 four weeks alone");
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

fn check_agent(d: &Doc) -> Result<Vec<&'static str>> {
    one_of(d, "department", DEPARTMENTS)?;
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
        let config = load(r).unwrap();
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
            load(r)
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
        assert!(format!("{:#}", load(r).unwrap_err()).contains("confirmation_date 'soon'"));
        write(
            r,
            "business/profile.md",
            "---\nname: Acme\nreminders: [30, a week]\n---\n",
        );
        assert!(format!("{:#}", load(r).unwrap_err()).contains("reminders 'a week'"));
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
        let c = load(r).unwrap();
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
        let c = load(root.path()).unwrap();
        assert!(c.agents.is_empty() && c.profile.fields.is_empty());
        assert!(c.missing.contains(&"business/profile.md: name".to_string()));
        write(
            root.path(),
            "business/profile.md",
            "---\nname: Numi\ncountry: FR\nstatus: planned\n---\n",
        );
        let c = load(root.path()).unwrap();
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
                "is not a date then what",
            ),
            (
                "business/goal.md",
                "---\nbaseline_minutes: lots\n---\n",
                "not a number",
            ),
            (
                "agents/x.md",
                "---\ndepartment: editorial\n---\n",
                "department 'editorial'",
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
            let err = format!("{:#}", load(root.path()).unwrap_err());
            assert!(err.contains(why), "{path}: {err}");
            assert!(err.contains(path), "{path}: {err}");
        }
    }
}
