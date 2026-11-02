//! Team and Business in the new interface, read-only: the agents by
//! department with their routines and what they cost, and the business's
//! config (profile and legal identity, goal, lines, repositories). The
//! config is written in the business folder, never from here (decision 44).

use super::*;
use crate::frontmatter::Doc;

impl App {
    /// Team and Business's routes; None when the request is not one of them.
    pub(super) fn company_routes(
        &self,
        _request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "team"]) => json_response(200, self.team(&self.realm(r)?)?),
            (Method::Get, ["realms", r, "company"]) => {
                json_response(200, self.company(&self.realm(r)?)?)
            }
            _ => return Ok(None),
        }))
    }

    /// The agents, each with its routines, their last run, what waits for
    /// the founder, and the last 30 days' model cost and founder time.
    fn team(&self, realm: &Realm) -> Result<Value> {
        let config = match crate::business::config::load(&realm.root()?) {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("{e:#}"), "agents": []})),
        };
        let graphs = self
            .hub
            .with_routine_checks(realm, crate::routine::load_all)?;
        let store = Store::of(realm)?;
        let since = chrono::Utc::now() - chrono::Duration::days(30);
        let costs = store.pipeline_costs(since)?;
        let seconds = store.owner_seconds_by_agent(since)?;
        let waiting = store.queue(&["pending"])?;
        let mut agents = Vec::new();
        for (name, doc) in &config.agents {
            let mine: Vec<(&String, crate::routine::Graph)> = graphs
                .iter()
                .filter_map(|(n, g)| {
                    g.as_ref()
                        .ok()
                        .filter(|g| g.agent == *name)
                        .map(|g| (n, g.clone()))
                })
                .collect();
            let mut routines = Vec::new();
            let (mut cost, mut unpriced) = (0.0, 0);
            for (routine, graph) in &mine {
                let last = store.pipeline_runs(&format!("routine:{routine}"), 1)?;
                routines.push(json!({
                    "name": routine,
                    "schedule": graph.trigger.schedule,
                    "last": last.first().map(|r| json!({"at": r.started_at, "ok": r.ok})),
                }));
                for c in costs.iter().filter(|c| &c.pipeline == *routine) {
                    cost += c.cost_usd;
                    unpriced += c.unpriced;
                }
            }
            agents.push(json!({
                "name": name,
                "title": doc.text("title"),
                "department": doc.text("department"),
                "job": doc.body.lines().map(str::trim).find(|l| !l.is_empty()),
                "routines": routines,
                "waiting": waiting.iter().filter(|i| i.agent.as_deref() == Some(name)).count(),
                "cost_usd": cost,
                "unpriced": unpriced,
                "seconds": seconds.iter().find(|(a, _)| a.as_deref() == Some(name)).map_or(0, |(_, s)| *s),
            }));
        }
        Ok(json!({"agents": agents}))
    }

    /// The business's config as it stands, with what is left to fill.
    fn company(&self, realm: &Realm) -> Result<Value> {
        let repos: Vec<Value> = realm
            .repos()
            .iter()
            .map(|r| json!({"name": r.name, "url": r.url}))
            .collect();
        let config = match crate::business::config::load(&realm.root()?) {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("{e:#}"), "repos": repos})),
        };
        Ok(json!({
            "profile": fields(&config.profile),
            "goal": fields(&config.goal),
            "lines": config.lines.iter().map(|(n, d)| json!({"name": n, "fields": fields(d)})).collect::<Vec<_>>(),
            "repos": repos,
            "missing": config.missing,
        }))
    }
}

/// A config file's front matter as JSON: text, a list, or null when not
/// known yet.
fn fields(doc: &Doc) -> Value {
    let mut out = serde_json::Map::new();
    for (key, value) in &doc.fields {
        out.insert(
            key.clone(),
            match value {
                Some(crate::frontmatter::Value::Text(t)) => json!(t),
                Some(crate::frontmatter::Value::List(l)) => json!(l),
                None => Value::Null,
            },
        );
    }
    Value::Object(out)
}
