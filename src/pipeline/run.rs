//! Running a pipeline: its steps in order, as plain code. Reading a feed
//! goes through `fetch`, which the hub routes through the runtime (the
//! permission check, redaction and event of every tool call). Every run,
//! failed or not, leaves its row in `business.db`: the pipeline's heartbeat.

use super::{Pipeline, Step, rss};
use crate::business::store::Store;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// A feed's body, and whether it was cut before its end.
#[derive(Debug)]
pub struct Fetched {
    pub body: String,
    pub truncated: bool,
}

pub type Item = Map<String, Value>;

#[derive(Debug, Default)]
pub struct Outcome {
    pub read: usize,
    pub kept: Vec<Item>,
    pub report: Option<String>,
    /// What went wrong without stopping the run (a feed down, a feed cut).
    pub notes: Vec<String>,
}

/// Run `pipeline` (named `name`) now, and record it.
pub fn run(
    name: &str,
    pipeline: &Pipeline,
    store: &mut Store,
    fetch: &mut dyn FnMut(&str) -> Result<Fetched>,
    now: DateTime<Utc>,
) -> Result<Outcome> {
    let mut seen_now = Vec::new();
    let result = steps(name, pipeline, store, fetch, &mut seen_now);
    let finished = Utc::now();
    match result {
        Ok(outcome) => {
            store.record_run(name, now, finished, &outcome, None, &seen_now)?;
            Ok(outcome)
        }
        Err(e) => {
            let error = format!("{e:#}");
            store.record_run(name, now, finished, &Outcome::default(), Some(&error), &[])?;
            Err(e)
        }
    }
}

fn steps(
    name: &str,
    pipeline: &Pipeline,
    store: &Store,
    fetch: &mut dyn FnMut(&str) -> Result<Fetched>,
    seen_now: &mut Vec<String>,
) -> Result<Outcome> {
    if pipeline
        .steps
        .iter()
        .any(|s| matches!(s, Step::Judge { .. }))
    {
        bail!("this Airth cannot run judge steps yet");
    }
    let mut out = Outcome::default();
    let mut items: Vec<Item> = Vec::new();
    for step in &pipeline.steps {
        match step {
            Step::Rss { feeds } => {
                let mut failures = Vec::new();
                for feed in feeds {
                    match fetch(feed) {
                        Ok(fetched) => {
                            let parsed = rss::parse(&fetched.body);
                            if fetched.truncated || !parsed.complete {
                                out.notes.push(format!(
                                    "{feed} was cut or malformed: kept the {} item(s) before the break",
                                    parsed.items.len()
                                ));
                            }
                            for mut item in parsed.items {
                                item.insert("feed".into(), feed.clone().into());
                                items.push(item);
                            }
                        }
                        Err(e) => failures.push(format!("{feed}: {e:#}")),
                    }
                }
                if failures.len() == feeds.len() {
                    bail!("no feed could be read: {}", failures.join("; "));
                }
                out.notes.extend(failures);
                out.read = items.len();
            }
            Step::Dedupe { by } => {
                let key = |item: &Item| match item.get(by) {
                    Some(Value::String(s)) => Some(s.clone()),
                    Some(Value::Null) | None => None,
                    Some(other) => Some(other.to_string()),
                };
                let keys: Vec<String> = items.iter().filter_map(key).collect();
                let already = store.seen(name, &keys)?;
                let mut this_run = HashSet::new();
                let before = items.len();
                items.retain(|item| match key(item) {
                    Some(k) => !already.contains(&k) && this_run.insert(k),
                    None => false,
                });
                let missing = before - keys.len();
                if missing > 0 {
                    out.notes
                        .push(format!("{missing} item(s) without {by} were dropped"));
                }
                seen_now.extend(this_run);
            }
            Step::Judge { .. } => unreachable!("refused above"),
            Step::Report { title } => out.report = Some(report(title, &items)),
        }
    }
    out.kept = items;
    Ok(out)
}

/// The report, in Markdown: one line per item, linked.
fn report(title: &str, items: &[Item]) -> String {
    let mut out = format!("# {title}\n\n");
    if items.is_empty() {
        out.push_str("Nothing new.\n");
    }
    let text = |item: &Item, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    for item in items {
        let title = text(item, "title");
        let title = if title.is_empty() {
            text(item, "link")
        } else {
            title
        };
        match text(item, "link") {
            link if link.is_empty() => out.push_str(&format!("- {title}\n")),
            link => out.push_str(&format!("- [{title}]({link})\n")),
        }
        let summary = text(item, "summary");
        if !summary.is_empty() {
            out.push_str(&format!("  {summary}\n"));
        }
    }
    out
}

impl Store {
    /// Which of `keys` an earlier run of `pipeline` already kept.
    fn seen(&self, pipeline: &str, keys: &[String]) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM pipeline_seen WHERE pipeline = ?1 AND key = ?2")?;
        let mut out = HashSet::new();
        for key in keys {
            if stmt.exists(params![pipeline, key])? {
                out.insert(key.clone());
            }
        }
        Ok(out)
    }

    /// One run's heartbeat, and what it saw, in one transaction: a run that
    /// fails to record leaves its items to the next one.
    fn record_run(
        &mut self,
        pipeline: &str,
        started: DateTime<Utc>,
        finished: DateTime<Utc>,
        outcome: &Outcome,
        error: Option<&str>,
        seen: &[String],
    ) -> Result<()> {
        let (read, kept) = (outcome.read as i64, outcome.kept.len() as i64);
        self.write(|tx| {
            tx.execute(
                "INSERT INTO pipeline_runs (pipeline, started_at, finished_at, ok, items_read, items_kept, report, notes, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    pipeline,
                    started.to_rfc3339(),
                    finished.to_rfc3339(),
                    error.is_none(),
                    read,
                    kept,
                    outcome.report,
                    outcome.notes.join("\n"),
                    error,
                ],
            )?;
            for key in seen {
                tx.execute(
                    "INSERT OR IGNORE INTO pipeline_seen (pipeline, key, first_seen) VALUES (?1, ?2, ?3)",
                    params![pipeline, key, finished.to_rfc3339()],
                )?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;

    fn pipeline(steps: Vec<Step>) -> Pipeline {
        Pipeline {
            description: String::new(),
            schedule: None,
            steps,
            prompts: Vec::new(),
        }
    }

    fn rss(feeds: &[&str]) -> Step {
        Step::Rss {
            feeds: feeds.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn feed(links: &[&str]) -> String {
        let items: String = links
            .iter()
            .map(|l| format!("<item><title>About {l}</title><link>{l}</link></item>"))
            .collect();
        format!("<rss><channel>{items}</channel></rss>")
    }

    /// Serves `body` for every feed, or fails the ones listed in `down`.
    fn serve(body: String, down: &'static [&'static str]) -> impl FnMut(&str) -> Result<Fetched> {
        move |url| {
            if down.contains(&url) {
                bail!("HTTP 503");
            }
            Ok(Fetched {
                body: body.clone(),
                truncated: false,
            })
        }
    }

    fn runs(store: &Store) -> Vec<(bool, i64, i64, Option<String>)> {
        let mut stmt = store
            .conn
            .prepare("SELECT ok, items_read, items_kept, error FROM pipeline_runs ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn keeps_only_what_no_earlier_run_kept_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let p = pipeline(vec![
            rss(&["https://a.com/feed", "https://b.com/feed"]),
            Step::Dedupe { by: "link".into() },
            Step::Report {
                title: "Daily watch".into(),
            },
        ]);
        // Both feeds carry the same two items: kept once each.
        let mut fetch = serve(feed(&["https://e.com/1", "https://e.com/2"]), &[]);
        let first = run("watch", &p, &mut store, &mut fetch, Utc::now()).unwrap();
        assert_eq!(first.read, 4);
        assert_eq!(first.kept.len(), 2);
        assert_eq!(first.kept[0]["feed"], "https://a.com/feed");
        let report = first.report.unwrap();
        assert!(report.starts_with("# Daily watch\n"));
        assert!(report.contains("- [About https://e.com/1](https://e.com/1)"));
        // The next run only keeps what is new.
        let mut fetch = serve(feed(&["https://e.com/2", "https://e.com/3"]), &[]);
        let second = run("watch", &p, &mut store, &mut fetch, Utc::now()).unwrap();
        let links: Vec<_> = second.kept.iter().map(|i| i["link"].clone()).collect();
        assert_eq!(links, vec!["https://e.com/3"]);
        // Another pipeline has its own memory of what it saw.
        let mut fetch = serve(feed(&["https://e.com/1"]), &[]);
        let other = run("other", &p, &mut store, &mut fetch, Utc::now()).unwrap();
        assert_eq!(other.kept.len(), 1);
        assert_eq!(
            runs(&store),
            vec![(true, 4, 2, None), (true, 4, 1, None), (true, 2, 1, None)]
        );
    }

    #[test]
    fn a_feed_down_is_noted_and_all_down_fails_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let p = pipeline(vec![
            rss(&["https://a.com/feed", "https://b.com/feed"]),
            Step::Dedupe { by: "link".into() },
        ]);
        let mut fetch = serve(feed(&["https://e.com/1"]), &["https://a.com/feed"]);
        let outcome = run("watch", &p, &mut store, &mut fetch, Utc::now()).unwrap();
        assert_eq!(outcome.kept.len(), 1);
        assert!(outcome.notes[0].contains("https://a.com/feed: HTTP 503"));
        // Every feed down: the run fails, is recorded, and marks nothing as
        // seen, so the next run still gets the items.
        let mut fetch = serve(
            feed(&["https://e.com/9"]),
            &["https://a.com/feed", "https://b.com/feed"],
        );
        let err = run("watch", &p, &mut store, &mut fetch, Utc::now()).unwrap_err();
        assert!(format!("{err:#}").contains("no feed could be read"));
        let recorded = runs(&store);
        assert!(!recorded[1].0);
        assert!(recorded[1].3.as_deref().unwrap().contains("HTTP 503"));
        let mut fetch = serve(feed(&["https://e.com/9"]), &[]);
        let next = run("watch", &p, &mut store, &mut fetch, Utc::now()).unwrap();
        assert_eq!(next.kept.len(), 1);
    }

    #[test]
    fn notes_cut_feeds_and_items_without_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let p = pipeline(vec![
            rss(&["https://a.com/feed"]),
            Step::Dedupe {
                by: "published".into(),
            },
        ]);
        let mut fetch = |_: &str| {
            Ok(Fetched {
                body: "<rss><item><title>a</title><pubDate>Fri</pubDate></item><item><title>b</title></item><item><title>c</title>".into(),
                truncated: true,
            })
        };
        let outcome = run("watch", &p, &mut store, &mut fetch, Utc::now()).unwrap();
        assert_eq!(outcome.kept.len(), 1);
        assert!(outcome.notes.iter().any(|n| n.contains("cut or malformed")));
        assert!(
            outcome
                .notes
                .iter()
                .any(|n| n.contains("1 item(s) without published"))
        );
    }

    #[test]
    fn judge_steps_are_refused_until_they_can_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let p = pipeline(vec![
            rss(&["https://a.com/feed"]),
            Step::Judge {
                prompt: "p.md".into(),
                schema: "s.json".into(),
                keep: None,
            },
        ]);
        let mut fetch = |_: &str| -> Result<Fetched> { panic!("nothing is read") };
        assert!(run("watch", &p, &mut store, &mut fetch, Utc::now()).is_err());
        assert!(!runs(&store)[0].0);
    }
}
