//! The business's Calendar in the new interface: its routines' runs over the
//! seven days from today, from their schedules, with what already became
//! of today's, and the scheduled runs of the last seven days that started
//! late. Deadlines come with the legal calendars.

use super::*;
use crate::business::store::status::RunRow;
use crate::objective::Schedule;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};

/// More runs than this in a week is a schedule like `every 5m`: the page
/// shows the first ones and says how many there are.
const MAX_RUNS: usize = 200;

impl App {
    /// Calendar's route; None when the request is not it.
    pub(super) fn calendar_routes(
        &self,
        _request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "calendar"]) => {
                json_response(200, self.calendar(&self.realm(r)?, Utc::now())?)
            }
            _ => return Ok(None),
        }))
    }

    fn calendar(&self, realm: &Realm, now: DateTime<Utc>) -> Result<Value> {
        let store = Store::of(realm)?;
        let mut routines = Vec::new();
        for (name, graph) in self
            .hub
            .with_routine_checks(realm, crate::routine::load_all)?
        {
            let Ok(graph) = graph else { continue };
            let Some(schedule) = graph.trigger.schedule.as_deref() else {
                continue;
            };
            let Ok(schedule) = Schedule::parse(schedule) else {
                continue;
            };
            let runs = store.pipeline_runs(&format!("routine:{name}"), 50)?;
            routines.push((name, graph.agent, schedule, runs));
        }
        let late: Vec<Value> = store
            .routines_week(now - Duration::days(7))?
            .late
            .iter()
            .map(|l| {
                json!({
                    "routine": l.routine,
                    "due": local(l.due_at, "%a %-d %b, %H:%M"),
                    "late_by": (l.started_at - l.due_at).num_minutes(),
                })
            })
            .collect();
        let mut page = week(&routines, now);
        page["late"] = late.into();
        Ok(page)
    }
}

fn local(at: DateTime<Utc>, format: &str) -> String {
    at.with_timezone(&Local).format(format).to_string()
}

/// Each routine's runs from the start of today to seven days later, by
/// day: `next` for each routine's next one, `planned` after it; before now,
/// what became of it: `done`, `late`, `failed`, or `not run`.
fn week(routines: &[(String, String, Schedule, Vec<RunRow>)], now: DateTime<Utc>) -> Value {
    let today = now.with_timezone(&Local).date_naive();
    let start = Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap_or_default())
        .earliest()
        .map_or(now, |t| t.with_timezone(&Utc));
    let end = start + Duration::days(7);
    let grace = Duration::minutes(crate::pipeline::schedule::GRACE_MINUTES);
    let mut runs = Vec::new();
    for (name, agent, schedule, done) in routines {
        let mut at = start - Duration::seconds(1);
        let mut next = true;
        loop {
            at = schedule.next_after(at);
            if at >= end || runs.len() > MAX_RUNS {
                break;
            }
            let ran = done.iter().find(|r| {
                r.due_at
                    .as_deref()
                    .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                    .is_some_and(|d| d.with_timezone(&Utc) == at)
            });
            let started = ran.and_then(|r| DateTime::parse_from_rfc3339(&r.started_at).ok());
            let state = match (at > now, ran, started) {
                (true, _, _) if next => {
                    next = false;
                    "next"
                }
                (true, _, _) => "planned",
                (false, Some(r), _) if !r.ok => "failed",
                (false, Some(_), Some(s)) if s.with_timezone(&Utc) - at > grace => "late",
                (false, Some(_), _) => "done",
                (false, None, _) => "not run",
            };
            runs.push((at, name, agent, state));
        }
    }
    let total = runs.len();
    runs.sort_by_key(|r| r.0);
    runs.truncate(MAX_RUNS);
    let days: Vec<Value> = (0..7)
        .map(|d| {
            let date = today + Duration::days(d);
            let of_day: Vec<Value> = runs
                .iter()
                .filter(|r| r.0.with_timezone(&Local).date_naive() == date)
                .map(|(at, name, agent, state)| {
                    json!({"routine": name, "agent": agent, "time": local(*at, "%H:%M"), "state": state})
                })
                .collect();
            json!({
                "weekday": date.format("%a").to_string(),
                "day": date.format("%-d").to_string(),
                "today": d == 0,
                "runs": of_day,
            })
        })
        .collect();
    let title = match total {
        _ if total > MAX_RUNS => format!("This week: more than {MAX_RUNS} routine runs."),
        0 => "No routine runs this week.".to_string(),
        1 => "This week: 1 routine run.".to_string(),
        n => format!("This week: {n} routine runs."),
    };
    json!({"title": title, "total": total, "days": days})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(due: DateTime<Utc>, started: DateTime<Utc>, ok: bool) -> RunRow {
        RunRow {
            started_at: started.to_rfc3339(),
            finished_at: started.to_rfc3339(),
            ok,
            items_read: 0,
            items_kept: 0,
            report: None,
            notes: String::new(),
            error: None,
            due_at: Some(due.to_rfc3339()),
        }
    }

    fn at(day: i64, hh: u32, mm: u32, now: DateTime<Utc>) -> DateTime<Utc> {
        let date = now.with_timezone(&Local).date_naive() + Duration::days(day);
        Local
            .from_local_datetime(&date.and_hms_opt(hh, mm, 0).unwrap())
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
    }

    /// The week from today: each run of each schedule on its day and time;
    /// today's past ones say what became of them (done, late past the
    /// grace, failed, not run); each routine's next one is marked.
    #[test]
    fn the_week_shows_each_run_and_what_became_of_todays() {
        let now = at(0, 12, 0, Utc::now());
        let daily = |hhmm: &str| Schedule::parse(&format!("daily {hhmm}")).unwrap();
        let routines = vec![
            (
                "watch".into(),
                "watcher".into(),
                daily("07:00"),
                vec![run(at(0, 7, 0, now), at(0, 7, 1, now), true)],
            ),
            (
                "posts".into(),
                "post-writer".into(),
                daily("07:30"),
                vec![run(at(0, 7, 30, now), at(0, 9, 0, now), true)],
            ),
            (
                "noon".into(),
                "watcher".into(),
                daily("11:00"),
                vec![run(at(0, 11, 0, now), at(0, 11, 2, now), false)],
            ),
            ("missed".into(), "watcher".into(), daily("10:00"), vec![]),
            (
                "evening".into(),
                "post-writer".into(),
                daily("18:00"),
                vec![],
            ),
        ];
        let page = week(&routines, now);
        assert_eq!(page["total"], 35);
        assert_eq!(page["title"], "This week: 35 routine runs.");
        let today: Vec<(String, String)> = page["days"][0]["runs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["routine"].as_str().unwrap().into(),
                    r["state"].as_str().unwrap().into(),
                )
            })
            .collect();
        let state = |name: &str| today.iter().find(|(n, _)| n == name).unwrap().1.as_str();
        assert_eq!(state("watch"), "done");
        assert_eq!(state("posts"), "late");
        assert_eq!(state("noon"), "failed");
        assert_eq!(state("missed"), "not run");
        assert_eq!(state("evening"), "next");
        assert_eq!(page["days"][0]["runs"][0]["time"], "07:00");
        assert_eq!(page["days"][0]["today"], true);
        // Tomorrow: the morning routines are next, the evening one planned.
        let tomorrow = &page["days"][1]["runs"];
        assert_eq!(tomorrow[0]["routine"], "watch");
        assert_eq!(tomorrow[0]["state"], "next");
        let evening = tomorrow
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["routine"] == "evening")
            .unwrap();
        assert_eq!(evening["state"], "planned");
        assert_eq!(page["days"].as_array().unwrap().len(), 7);
    }

    /// A schedule like `every 5m` is cut at the bound, and says how many.
    #[test]
    fn a_dense_schedule_is_cut_and_counted() {
        let now = at(0, 0, 30, Utc::now());
        let routines = vec![(
            "tick".into(),
            "watcher".into(),
            Schedule::parse("every 5m").unwrap(),
            vec![],
        )];
        let page = week(&routines, now);
        let shown: usize = page["days"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["runs"].as_array().unwrap().len())
            .sum();
        assert_eq!(shown, MAX_RUNS);
        assert!(page["total"].as_u64().unwrap() > MAX_RUNS as u64);
        assert_eq!(page["title"], "This week: more than 200 routine runs.");
    }

    /// The page: the business's scheduled routines on their days, and the
    /// late starts of the last seven days (#70).
    #[test]
    fn the_calendar_reads_the_routines_and_the_late_starts() {
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
        for (path, text) in [
            (
                "agents/chief.md",
                "---\ntitle: Chief of staff\n---\nReport.",
            ),
            (
                "routines/weekly-report.json",
                r#"{"agent": "chief", "trigger": {"schedule": "daily 08:00"}, "blocks": [
                    {"id": "week", "type": "read", "source": "business", "records": "week"},
                    {"id": "report", "type": "act", "action": "airth.report", "fields": {"title": "Week", "text": "{{summary}}"}}]}"#,
            ),
        ] {
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), text).unwrap();
        }
        let now = Utc::now();
        let due = now - Duration::days(1);
        Store::of(&realm)
            .unwrap()
            .record_run(
                &crate::business::store::pipelines::Heartbeat {
                    pipeline: "routine:weekly-report",
                    due: Some(due),
                    started: due + Duration::hours(2),
                    finished: due + Duration::hours(2),
                    items_read: 0,
                    items_kept: 0,
                    report: None,
                    notes: "",
                    error: None,
                },
                &[],
            )
            .unwrap();
        let app = super::super::tests::test_app(home, 0);
        let page = app.calendar(&realm, now).unwrap();
        assert_eq!(page["total"], 7);
        assert_eq!(page["days"][1]["runs"][0]["routine"], "weekly-report");
        assert_eq!(page["days"][1]["runs"][0]["agent"], "chief");
        assert_eq!(page["late"][0]["routine"], "weekly-report");
        assert_eq!(page["late"][0]["late_by"], 120);
    }
}
