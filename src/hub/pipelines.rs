//! Business pipelines run from the hub (decision 30): the engine is
//! `crate::pipeline`; here its reads get the realm's runtime, so each one
//! is checked against the realm's rules, redacted and recorded like any
//! tool call, and its judge steps ask the realm's model.

use super::*;
use crate::business::store::Store;
use crate::permissions::{Answer, Grant};
use crate::pipeline::run::{Fetched, Io, Outcome};
use crate::pipeline::schedule::{Tick, tick};
use crate::state::{Event, EventStatus};
use serde_json::json;

impl Hub {
    /// Run one of the realm's pipelines now. `approver` answers what the
    /// realm's rules leave to the owner, and `grants` is what was approved in
    /// advance.
    pub fn run_pipeline(
        &self,
        realm: &Realm,
        name: &str,
        approver: Box<dyn Approver>,
        grants: Vec<Grant>,
    ) -> Result<Outcome> {
        let root = realm.root()?;
        let pipeline = crate::pipeline::load(&crate::business::dir(&root), name)?;
        self.with_business_io(realm, name, approver, grants, |store, io| {
            crate::pipeline::run::run(name, &pipeline, store, io, Utc::now())
        })
    }

    /// Call `f` with the realm's store and what a business run reaches
    /// outside: reads through the realm's runtime (checked, redacted,
    /// recorded like any tool call) and the realm's model, asked only if a
    /// step needs it.
    pub(super) fn with_business_io<T>(
        &self,
        realm: &Realm,
        name: &str,
        approver: Box<dyn Approver>,
        grants: Vec<Grant>,
        f: impl FnOnce(&mut Store, &mut Io) -> Result<T>,
    ) -> Result<T> {
        let root = realm.root()?;
        let workspace = Workspace::new(&root)?
            .with_secrets(realm.command_env()?, self.redactions(realm)?)
            .with_network(!realm.config.offline);
        let mut runtime = Runtime::new(
            tools::pipeline_tools(realm.config.integrations.clone()),
            PermissionGate::new(realm.config.policy.clone(), approver).with_grants(grants),
            workspace,
            CancelToken::default(),
        );
        let mut store = Store::of(realm)?;
        let mut calls = 0;
        let mut fetch = |url: &str| {
            calls += 1;
            #[cfg(test)]
            if let Some(body) = TEST_FEEDS.lock().unwrap().get(url) {
                return Ok(Fetched {
                    body: body.clone(),
                    truncated: false,
                });
            }
            fetched(runtime.invoke(
                &format!("{name}-{calls}"),
                "http_request",
                &json!({"method": "GET", "url": url}),
            ))
        };
        let mut model = None;
        let mut ask = |request: &crate::models::ModelRequest| {
            if model.is_none() {
                model = Some(self.model_for(realm).context("no model available")?);
            }
            model.as_ref().expect("set above").complete(request)
        };
        let mut io = Io {
            fetch: &mut fetch,
            ask: &mut ask,
            prices: crate::prices::Prices::load(self.home()),
        };
        f(&mut store, &mut io)
    }
}

impl Hub {
    /// Tell the founder a scheduled run is late (Airth was not running at
    /// its time) or that the one before left no trace.
    pub(super) fn tell_late(
        &self,
        realm: &Realm,
        name: &str,
        missed: Option<DateTime<Utc>>,
        silent: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let inbox = self.home.inbox();
        if let Some(at) = silent {
            inbox.push(
                Kind::Failed,
                realm.name(),
                None,
                &format!("{name} left no trace of its last run"),
                &format!("The run started at {} never recorded its end: Airth stopped during it. Running it again now.", local(at)),
            )?;
        }
        if let Some(at) = missed {
            inbox.push(
                Kind::Failed,
                realm.name(),
                None,
                &format!("{name} missed its run at {}", local(at)),
                "Airth was not running then. Running it now, once.",
            )?;
        }
        Ok(())
    }
}

/// Nobody is there: what the realm's rules leave to the owner is not done.
pub(super) struct Unattended;

impl Approver for Unattended {
    fn ask(&mut self, _: &crate::permissions::PermissionRequest) -> Result<Answer> {
        Ok(Answer::No(
            "a scheduled pipeline runs unattended: allow network requests in the realm's rules, or run it by hand (airth pipeline run)".into(),
        ))
    }
}

impl Hub {
    /// The scheduled pipelines due at `now`, across realms, each claimed so
    /// the next call does not return it again. A late or silent run is
    /// told to the owner here.
    pub fn due_pipelines(&self, now: DateTime<Utc>) -> Result<Vec<(Realm, String)>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived {
                continue;
            }
            let Ok(root) = realm.root() else { continue };
            let scheduled: Vec<(String, Schedule)> =
                crate::pipeline::load_all(&crate::business::dir(&root))?
                    .into_iter()
                    .filter_map(|(name, p)| Some((name, Schedule::parse(&p.ok()?.schedule?).ok()?)))
                    .collect();
            if scheduled.is_empty() {
                continue;
            }
            let mut store = Store::of(&realm)?;
            for (name, schedule) in scheduled {
                let Tick::Run { missed, silent } = tick(&mut store, &name, &schedule, now)? else {
                    continue;
                };
                self.tell_late(&realm, &name, missed, silent)?;
                out.push((realm.clone(), name));
            }
        }
        Ok(out)
    }

    /// Run a scheduled pipeline with nobody there, and tell the owner how it went.
    pub fn run_pipeline_unattended(&self, realm: &Realm, name: &str) {
        let (kind, title, body) =
            match self.run_pipeline(realm, name, Box::new(Unattended), Vec::new()) {
                Ok(o) => (
                    Kind::Completed,
                    match o.queued {
                        0 => format!("{name}: {} new of {} read", o.kept.len(), o.read),
                        n => format!("{name}: {n} draft(s) to review"),
                    },
                    [o.report.unwrap_or_default(), o.notes.join("\n")]
                        .join("\n")
                        .trim()
                        .to_string(),
                ),
                Err(e) => (Kind::Failed, format!("{name} failed"), format!("{e:#}")),
            };
        if let Err(e) = self
            .home
            .inbox()
            .push(kind, realm.name(), None, &title, &body)
        {
            eprintln!("pipeline {}/{name}: could not notify: {e:#}", realm.name());
        }
    }
}

fn local(at: DateTime<Utc>) -> String {
    at.with_timezone(&chrono::Local)
        .format("%a %d %H:%M")
        .to_string()
}

/// A feed's body from the `http_request` event: its status must be 2xx, and
/// the tool's truncation mark is taken off.
fn fetched(event: Event) -> Result<Fetched> {
    if event.status != EventStatus::Ok {
        bail!("{}", event.result);
    }
    let (head, body) = event
        .result
        .split_once("\n\n")
        .unwrap_or((&event.result, ""));
    let status = head.lines().next().unwrap_or_default();
    if !status
        .split_whitespace()
        .nth(1)
        .is_some_and(|code| code.starts_with('2'))
    {
        // Redirects are not followed: the feed's address is the business's to fix.
        if let Some(to) = head.lines().find_map(|l| l.strip_prefix("location: ")) {
            bail!("{status}: the feed moved to {to}");
        }
        bail!("{status}");
    }
    let (body, truncated) = match body.rsplit_once("\n... [truncated ") {
        Some((kept, _)) => (kept, true),
        None => (body, false),
    };
    Ok(Fetched {
        body: body.to_string(),
        truncated,
    })
}

/// Tests: feeds served without the network, by URL (a public read refuses
/// the local servers tests use).
#[cfg(test)]
pub(crate) static TEST_FEEDS: std::sync::Mutex<std::collections::BTreeMap<String, String>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

#[cfg(test)]
mod tests {
    use super::*;

    fn event(status: EventStatus, result: &str) -> Event {
        Event {
            timestamp: Utc::now(),
            action: "http_request".into(),
            arguments: json!({}),
            result: result.into(),
            status,
            tool_use_id: None,
        }
    }

    #[test]
    fn reads_the_body_of_a_successful_response_only() {
        let ok = fetched(event(
            EventStatus::Ok,
            "HTTP 200 OK\ncontent-type: application/rss+xml\n\n<rss>\n\n</rss>",
        ))
        .unwrap();
        assert_eq!(ok.body, "<rss>\n\n</rss>");
        assert!(!ok.truncated);
        let cut = fetched(event(
            EventStatus::Ok,
            "HTTP 200 OK\n\n<rss><item>\n... [truncated 10 of 100 bytes]",
        ))
        .unwrap();
        assert_eq!(cut.body, "<rss><item>");
        assert!(cut.truncated);
        let moved = fetched(event(
            EventStatus::Ok,
            "HTTP 308 Permanent Redirect\nlocation: https://new.example/rss\n\n",
        ));
        assert!(format!("{:#}", moved.unwrap_err()).contains("moved to https://new.example/rss"));
        let missing = fetched(event(EventStatus::Ok, "HTTP 404 Not Found\n\nnope"));
        assert!(format!("{:#}", missing.unwrap_err()).contains("404"));
        let denied = fetched(event(EventStatus::Denied, "permission denied: no"));
        assert!(format!("{:#}", denied.unwrap_err()).contains("permission denied"));
    }
}
