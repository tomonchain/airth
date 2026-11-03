//! The command bar (decision 64): the founder's order, routed to an agent
//! or given to the one named, then started as a task nobody watches.

use super::*;

impl App {
    /// The command bar's routes; None when the request is not one of them.
    pub(super) fn order_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            // Which agent: the one named at the order's start, else routed.
            (Method::Post, ["realms", r, "orders", "route"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let text = body["text"].as_str().unwrap_or_default();
                let agents = self.agent_names(&realm)?;
                let routed = match crate::hub::orders::named(text, &agents) {
                    Some((agent, order)) => {
                        Ok(json!({"agent": agent, "why": "you named it", "order": order}))
                    }
                    None => self
                        .hub
                        .route_order(&realm, text)
                        .map(|r| json!({"agent": r.agent, "why": r.why, "order": text.trim()})),
                };
                match routed {
                    Ok(mut route) => {
                        route["agents"] = json!(agents);
                        json_response(200, route)
                    }
                    // No agent fits: the founder picks among them.
                    Err(e) => json_response(
                        200,
                        json!({"error": format!("{e:#}"), "agents": agents, "order": text.trim()}),
                    ),
                }
            }
            // Start it: a task of that agent, unwatched (decision 56).
            (Method::Post, ["realms", r, "orders"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let agent = body["agent"].as_str().context("pick the agent")?;
                let order = body["text"].as_str().unwrap_or_default();
                self.hub.model_for(&realm).context("no model available")?;
                let state = self.hub.start_task(&realm, agent, order)?;
                let id = state.id.clone();
                self.spawn(realm, state, Job::Alone);
                json_response(200, json!({"id": id, "agent": agent}))
            }
            _ => return Ok(None),
        }))
    }

    fn agent_names(&self, realm: &Realm) -> Result<Vec<String>> {
        Ok(crate::business::bots(&realm.root()?)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }
}
