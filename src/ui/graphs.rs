//! Routines in the new interface: each routine's graph as its file says,
//! checked, with its last runs and its week's cost, and a dry run on demand
//! (decision 49). Read-only: a routine is changed in the business folder
//! (decision 44); the editor comes at milestone 5.

use super::*;
use crate::permissions::{Answer, Approver, PermissionRequest};

/// A dry run from the app asks nothing on the way: what the realm's rules
/// leave to the founder is not done, and the report says so.
struct AsksNothing;

impl Approver for AsksNothing {
    fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
        Ok(Answer::No(
            "a dry run from the app asks nothing: allow network requests in the business's rules, or run it in the terminal (airth routines dry-run)".into(),
        ))
    }
}

impl App {
    /// Routines' routes; None when the request is not one of them.
    pub(super) fn graph_routes(
        &self,
        _request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "graphs"]) => {
                json_response(200, self.graphs(&self.realm(r)?)?)
            }
            (Method::Post, ["realms", r, "graphs", name, "dry-run"]) => {
                let realm = self.realm(r)?;
                match self
                    .hub
                    .dry_run_routine(&realm, name, Box::new(AsksNothing))
                {
                    Ok(report) => json_response(200, json!(report)),
                    Err(e) => json_response(422, json!({"error": format!("{e:#}")})),
                }
            }
            _ => return Ok(None),
        }))
    }

    /// Each routine, loaded or with why not, with its agent, its last runs,
    /// its next time and what its model calls cost this week.
    fn graphs(&self, realm: &Realm) -> Result<Value> {
        let all = self
            .hub
            .with_routine_checks(realm, crate::routine::load_all)?;
        let root = realm.root()?;
        let store = Store::of(realm)?;
        let costs = store.pipeline_costs(chrono::Utc::now() - chrono::Duration::days(7))?;
        let mut routines = Vec::new();
        for (name, graph) in all {
            let key = format!("routine:{name}");
            let runs: Vec<Value> = store
                .pipeline_runs(&key, 5)?
                .into_iter()
                .map(|r| json!({"at": r.started_at, "due_at": r.due_at, "ok": r.ok, "error": r.error}))
                .collect();
            let cost = costs.iter().find(|c| c.pipeline == name);
            let mut routine = json!({
                "name": name,
                "runs": runs,
                "next_at": store.next_run(&key)?,
                "cost_usd": cost.map_or(0.0, |c| c.cost_usd),
                "unpriced": cost.map_or(0, |c| c.unpriced),
            });
            match graph {
                Ok(g) => {
                    // The graph check made sure the agent's file is there.
                    routine["agent_title"] =
                        std::fs::read_to_string(root.join(format!("agents/{}.md", g.agent)))
                            .ok()
                            .and_then(|t| crate::frontmatter::parse(&t).text("title"))
                            .into();
                    routine["graph"] = json!(g);
                }
                Err(e) => routine["error"] = format!("{e:#}").into(),
            }
            routines.push(routine);
        }
        Ok(json!({"routines": routines}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Routines page: each routine's graph as data, a wrong one with
    /// why, and a dry run's report block by block.
    #[test]
    fn routines_show_their_graph_and_run_dry() {
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
                "---\ntitle: Chief of staff\n---\nReport the week.",
            ),
            (
                "routines/weekly-report.json",
                r#"{"agent": "chief", "trigger": {"schedule": "mon 08:00"}, "blocks": [
                    {"id": "week", "type": "read", "source": "business", "records": "week"},
                    {"id": "report", "type": "act", "action": "airth.report",
                     "fields": {"title": "Week {{week}}", "text": "{{summary}}"}}]}"#,
            ),
            ("routines/broken.json", r#"{"agent": "ghost"}"#),
        ] {
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), text).unwrap();
        }
        let app = super::super::tests::test_app(home, 0);
        let page = app.graphs(&realm).unwrap();
        let list = page["routines"].as_array().unwrap();
        assert_eq!(list[0]["name"], "broken");
        assert!(list[0]["graph"].is_null());
        assert!(list[0]["error"].as_str().unwrap().contains("broken.json"));
        let weekly = &list[1];
        assert_eq!(weekly["agent_title"], "Chief of staff");
        assert_eq!(weekly["graph"]["trigger"]["schedule"], "mon 08:00");
        assert_eq!(weekly["graph"]["blocks"][0]["type"], "read");
        assert_eq!(weekly["graph"]["blocks"][0]["records"], "week");
        assert_eq!(weekly["graph"]["blocks"][1]["action"], "airth.report");
        assert_eq!(weekly["runs"], json!([]));

        let report = app
            .hub
            .dry_run_routine(&realm, "weekly-report", Box::new(AsksNothing))
            .unwrap();
        let report = json!(report);
        assert_eq!(report["blocks"][0]["id"], "week");
        assert_eq!(report["blocks"][0]["items_out"], 1);
        let act = report["blocks"][1]["lines"][0].as_str().unwrap();
        assert!(
            act.starts_with("would airth.report [weekly-report/week/"),
            "{act}"
        );
        assert!(act.contains("Your time: 0 s"), "{act}");
        // Nothing was reported: a dry run only shows its acts.
        assert!(app.hub.home().inbox().list().unwrap().is_empty());
    }
}
