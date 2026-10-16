//! Persistent objectives: work the owner has approved in advance, which Airth
//! starts on its own when a trigger fires: a schedule, a change to files in
//! the realm, or a local webhook.
//!
//! An objective carries everything its unattended runs may do without asking
//! (its approvals), how much they may spend (its budget), and how long a run
//! waits for the owner when it needs something more before that is denied.
//! Objectives live in the realm's `objectives.json`, out of every run's reach:
//! only the owner creates or changes them.

use crate::agent::Budget;
use crate::permissions::Grant;
use crate::realm::{validate_name, write_atomic};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

/// Serializes read-modify-write cycles within this process.
static LOCK: Mutex<()> = Mutex::new(());

const MAX_OBJECTIVES: usize = 100;
const MAX_WATCHED: usize = 5000;
/// Directories a watch ignores: build output and VCS data churn constantly.
const UNWATCHED: &[&str] = &[".git", "target", "node_modules", ".airth"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Only when the owner starts it.
    Manual,
    /// On a schedule (see `Schedule`).
    Schedule { every: String },
    /// When files under this realm-relative path change.
    Watch { path: String },
    /// When something on this machine POSTs to the objective's webhook URL.
    Webhook { token: String },
}

impl Trigger {
    pub fn describe(&self) -> String {
        match self {
            Trigger::Manual => "manual".into(),
            Trigger::Schedule { every } => format!("schedule: {every}"),
            Trigger::Watch { path } => format!("when {path} changes"),
            Trigger::Webhook { .. } => "webhook".into(),
        }
    }
}

fn yes() -> bool {
    true
}

fn default_wait() -> u32 {
    10
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Objective {
    /// Short name, unique in the realm, e.g. "daily-triage".
    pub id: String,
    /// What each run should achieve.
    pub objective: String,
    pub trigger: Trigger,
    /// What runs may do without asking (on top of the realm's allow rules).
    #[serde(default)]
    pub approvals: Vec<Grant>,
    #[serde(default)]
    pub budget: Budget,
    /// How long a run waits for the owner on anything else, before it is
    /// denied. 0 denies at once.
    #[serde(default = "default_wait")]
    pub approval_wait_minutes: u32,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// The realm repository its runs work in (None: the realm's root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// When the schedule fires next.
    #[serde(default)]
    pub next_at: Option<DateTime<Utc>>,
    /// The watched files as last seen.
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub last_run: Option<String>,
    #[serde(default)]
    pub last_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_status: Option<String>,
}

impl Objective {
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.id).context("invalid objective name")?;
        if self.objective.trim().is_empty() {
            bail!("the objective is empty");
        }
        match &self.trigger {
            Trigger::Schedule { every } => {
                Schedule::parse(every)?;
            }
            Trigger::Watch { path } => {
                watched_path(Path::new("/"), path)?;
            }
            Trigger::Webhook { token } if token.len() < 16 => bail!("webhook token too short"),
            _ => {}
        }
        Ok(())
    }
}

/// When a scheduled objective runs, in the owner's local time:
/// `every 30m`, `every 2h`, `every 1d`, `daily 08:30`, `weekdays 09:00`,
/// `mon,thu 18:00`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schedule {
    Every(Duration),
    /// On the given weekdays (Monday first) at a local time.
    At {
        days: [bool; 7],
        time: NaiveTime,
    },
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

impl Schedule {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim().to_ascii_lowercase();
        let help = "schedules look like \"every 30m\", \"every 2h\", \"daily 08:30\", \"weekdays 09:00\" or \"mon,thu 18:00\"";
        let mut words = text.split_whitespace();
        let (Some(first), Some(second), None) = (words.next(), words.next(), words.next()) else {
            bail!("{help}");
        };
        if first == "every" {
            let split = second
                .find(|c: char| !c.is_ascii_digit())
                .with_context(|| help.to_string())?;
            let n: i64 = second[..split].parse().with_context(|| help.to_string())?;
            let d = match &second[split..] {
                "m" | "min" | "mins" => Duration::minutes(n),
                "h" | "hr" | "hrs" => Duration::hours(n),
                "d" | "day" | "days" => Duration::days(n),
                _ => bail!("{help}"),
            };
            if d < Duration::minutes(1) || d > Duration::days(366) {
                bail!("the interval must be between 1 minute and a year");
            }
            return Ok(Schedule::Every(d));
        }
        let time = NaiveTime::parse_from_str(second, "%H:%M").with_context(|| help.to_string())?;
        let mut days = [false; 7];
        match first {
            "daily" => days = [true; 7],
            "weekdays" => days[..5].fill(true),
            "weekends" => days[5..].fill(true),
            list => {
                for d in list.split(',') {
                    let i = DAYS
                        .iter()
                        .position(|n| d.starts_with(n) && d.len() >= 3)
                        .with_context(|| help.to_string())?;
                    days[i] = true;
                }
            }
        }
        Ok(Schedule::At { days, time })
    }

    /// The first time strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Schedule::Every(d) => after + *d,
            Schedule::At { days, time } => {
                let local = after.with_timezone(&Local).date_naive();
                for offset in 0..=8 {
                    let date = local + Duration::days(offset);
                    if !days[date.weekday().num_days_from_monday() as usize] {
                        continue;
                    }
                    // A time skipped by a DST change has no local instant.
                    if let Some(at) = Local.from_local_datetime(&date.and_time(*time)).earliest() {
                        let at = at.with_timezone(&Utc);
                        if at > after {
                            return at;
                        }
                    }
                }
                after + Duration::days(1)
            }
        }
    }
}

/// The realm's objectives, in `objectives.json`.
#[derive(Debug, Clone)]
pub struct ObjectiveStore {
    path: PathBuf,
}

impl ObjectiveStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn list(&self) -> Result<Vec<Objective>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    pub fn get(&self, id: &str) -> Result<Objective> {
        self.list()?
            .into_iter()
            .find(|o| o.id == id)
            .with_context(|| format!("no objective '{id}' (see `airth objective list`)"))
    }

    /// Add an objective, or replace the one with the same id.
    pub fn save(&self, objective: Objective) -> Result<()> {
        objective.validate()?;
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        let count = all.len();
        match all.iter_mut().find(|o| o.id == objective.id) {
            Some(existing) => *existing = objective,
            None if count >= MAX_OBJECTIVES => {
                bail!("a realm has at most {MAX_OBJECTIVES} objectives")
            }
            None => all.push(objective),
        }
        write_atomic(&self.path, &serde_json::to_string_pretty(&all)?)
    }

    /// Change one objective in place; false if there is none with this id.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Objective)) -> Result<bool> {
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        let Some(objective) = all.iter_mut().find(|o| o.id == id) else {
            return Ok(false);
        };
        change(objective);
        write_atomic(&self.path, &serde_json::to_string_pretty(&all)?)?;
        Ok(true)
    }

    pub fn remove(&self, id: &str) -> Result<bool> {
        let _guard = LOCK.lock().unwrap();
        let mut all = self.list()?;
        let before = all.len();
        all.retain(|o| o.id != id);
        if all.len() == before {
            return Ok(false);
        }
        write_atomic(&self.path, &serde_json::to_string_pretty(&all)?)?;
        Ok(true)
    }
}

/// `path` under `root`, refusing anything that would leave it.
pub fn watched_path(root: &Path, path: &str) -> Result<PathBuf> {
    let p = Path::new(path.trim());
    if path.trim().is_empty()
        || p.components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        bail!("watch a path inside the realm's folder, like \"inbox\" or \"notes/todo.md\"");
    }
    Ok(root.join(p))
}

/// A digest of the names, sizes and modification times of the files under
/// `path`: it changes when any of them does.
pub fn fingerprint(path: &Path) -> String {
    fn walk(path: &Path, base: &Path, out: &mut Vec<String>) {
        if out.len() >= MAX_WATCHED {
            return;
        }
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        let rel = path
            .strip_prefix(base)
            .unwrap_or(path)
            .display()
            .to_string();
        if meta.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                return;
            };
            let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            children.sort();
            for child in children {
                let skip = child
                    .file_name()
                    .is_some_and(|n| UNWATCHED.iter().any(|u| n == *u));
                if !skip {
                    walk(&child, base, out);
                }
            }
        } else {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            out.push(format!("{rel}\0{}\0{mtime}", meta.len()));
        }
    }
    let mut entries = Vec::new();
    walk(path, path, &mut entries);
    // FNV-1a: stable across builds, unlike std's hasher.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in entries.join("\n").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}:{}", hash, entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn parses_schedules() {
        assert_eq!(
            Schedule::parse("every 30m").unwrap(),
            Schedule::Every(Duration::minutes(30))
        );
        assert_eq!(
            Schedule::parse("Every 2h").unwrap(),
            Schedule::Every(Duration::hours(2))
        );
        let Schedule::At { days, time } = Schedule::parse("weekdays 08:30").unwrap() else {
            panic!()
        };
        assert_eq!(days, [true, true, true, true, true, false, false]);
        assert_eq!(time, NaiveTime::from_hms_opt(8, 30, 0).unwrap());
        let Schedule::At { days, .. } = Schedule::parse("mon,thursday 18:00").unwrap() else {
            panic!()
        };
        assert_eq!(days, [true, false, false, true, false, false, false]);
        for bad in [
            "",
            "every",
            "every 0m",
            "every 5x",
            "daily 25:00",
            "funday 10:00",
            "daily 8:00 extra",
        ] {
            assert!(Schedule::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn next_times_are_in_the_future_at_the_right_local_time() {
        let now = Utc::now();
        let every = Schedule::parse("every 15m").unwrap();
        assert_eq!(every.next_after(now), now + Duration::minutes(15));

        let daily = Schedule::parse("daily 07:45").unwrap();
        let next = daily.next_after(now);
        assert!(next > now && next <= now + Duration::hours(25));
        let local = next.with_timezone(&Local);
        assert_eq!((local.hour(), local.minute()), (7, 45));
        assert_eq!(daily.next_after(next), {
            let n = daily.next_after(next);
            assert!(n > next);
            n
        });

        let weekend = Schedule::parse("sat,sun 10:00").unwrap();
        let next = weekend.next_after(now).with_timezone(&Local);
        assert!(next.weekday().num_days_from_monday() >= 5);
    }

    #[test]
    fn store_saves_updates_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let store = ObjectiveStore::new(dir.path().join("objectives.json"));
        let o = Objective {
            id: "triage".into(),
            objective: "Triage new issues".into(),
            trigger: Trigger::Schedule {
                every: "daily 08:00".into(),
            },
            approvals: vec![],
            budget: Budget::default(),
            approval_wait_minutes: 0,
            enabled: true,
            created_at: Utc::now(),
            repo: None,
            next_at: None,
            fingerprint: None,
            last_run: None,
            last_at: None,
            last_status: None,
        };
        store.save(o.clone()).unwrap();
        assert!(store.update("triage", |o| o.enabled = false).unwrap());
        assert!(!store.get("triage").unwrap().enabled);
        assert!(!store.update("nope", |_| {}).unwrap());
        let bad = Objective {
            trigger: Trigger::Watch {
                path: "../elsewhere".into(),
            },
            ..o.clone()
        };
        assert!(store.save(bad).is_err());
        assert!(store.remove("triage").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn fingerprint_changes_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = fingerprint(dir.path());
        std::fs::write(dir.path().join("x.txt"), "1").unwrap();
        let b = fingerprint(dir.path());
        assert_ne!(a, b);
        assert_eq!(b, fingerprint(dir.path()));
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), "x").unwrap();
        assert_eq!(b, fingerprint(dir.path()), "VCS data is ignored");
        std::fs::write(dir.path().join("x.txt"), "22").unwrap();
        assert_ne!(b, fingerprint(dir.path()));
    }
}
