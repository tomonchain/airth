//! Business pipelines run from the hub (decision 30): the engine is
//! `crate::pipeline`; here its reads get the realm's runtime, so each one
//! is checked against the realm's rules, redacted and recorded like any
//! tool call.

use super::*;
use crate::business::store::Store;
use crate::permissions::Grant;
use crate::pipeline::run::{Fetched, Outcome};
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
            fetched(runtime.invoke(
                &format!("{name}-{calls}"),
                "http_request",
                &json!({"method": "GET", "url": url}),
            ))
        };
        crate::pipeline::run::run(name, &pipeline, &mut store, &mut fetch, Utc::now())
    }
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
