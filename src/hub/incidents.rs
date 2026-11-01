//! Incidents (`CLAUDE.md`, decision 45): what went wrong without the
//! founder, put in the queue where it waits until the founder has seen it,
//! and so on Telegram. One about an agent's action also takes that action
//! down one level, at once.

use super::*;
use crate::business::store::Store;
use crate::business::store::autonomy::NewItem;
use serde_json::json;

pub struct Incident<'a> {
    pub title: &'a str,
    pub detail: &'a str,
    pub agent: Option<&'a str>,
    pub routine: Option<&'a str>,
    /// The action that failed: demoted for `agent`.
    pub action: Option<&'a str>,
}

/// Whether an HTTP call's result says it worked: a 2xx status.
pub fn succeeded(result: &str) -> bool {
    result.starts_with("HTTP 2")
}

/// Record `incident` in `realm`'s queue; when it names an agent's action,
/// take that action down one level. Its id in the queue.
pub fn record(realm: &Realm, incident: &Incident) -> Result<i64> {
    let mut store = Store::of(realm)?;
    let mut detail = incident.detail.to_string();
    if let (Some(agent), Some(action)) = (incident.agent, incident.action) {
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        let declared = crate::actions::all(&realm.config.integrations, &servers);
        let (ceiling, start) = declared
            .iter()
            .find(|d| d.id == action)
            .map_or((1, 1), |d| (d.ceiling, d.start));
        let (from, to) = store.demote(agent, action, ceiling, start, incident.title)?;
        if to < from {
            detail.push_str(&format!("\n\n{agent} now takes {action} at level {to} (was {from}): raise it again with airth actions level."));
        }
    }
    store.propose(&NewItem {
        kind: "incident",
        title: incident.title,
        agent: incident.agent,
        routine: incident.routine,
        run: None,
        action: incident.action,
        args: &json!({ "detail": detail }).to_string(),
        execute_at: None,
        source: r#"{"show": "detail"}"#,
        model_call: None,
    })
}
