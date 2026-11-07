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
                // Named by its file name, or by its first name (decision 94).
                let root = realm.root()?;
                let by_first = text.split_once(':').and_then(|(head, rest)| {
                    let head = head.trim().to_lowercase();
                    agents
                        .iter()
                        .find(|a| {
                            crate::business::first_name(&root, a)
                                .is_some_and(|n| n.to_lowercase() == head)
                        })
                        .map(|a| (a.clone(), rest.trim()))
                });
                let routed = match crate::hub::orders::named(text, &agents).or(by_first) {
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
                // Written in the agent's chat, the founder is there to answer
                // its questions as they come (decision 75); from the command
                // bar, it runs unwatched (decision 64).
                let job = match body["watched"].as_bool() {
                    Some(true) => Job::Drive,
                    _ => Job::Alone,
                };
                self.spawn(realm, state, job);
                json_response(200, json!({"id": id, "agent": agent}))
            }
            (Method::Get, ["realms", r, "tasks"]) => {
                json_response(200, self.tasks(&self.realm(r)?)?)
            }
            _ => return Ok(None),
        }))
    }

    /// The founder's tasks (decision 64), grouped: waiting for the founder
    /// in Needs you, to review (a change to adopt), running, and the last
    /// ones done.
    fn tasks(&self, realm: &Realm) -> Result<Value> {
        let waiting: std::collections::HashMap<String, i64> = Store::of(realm)?
            .queue(&["pending"])?
            .into_iter()
            .filter_map(|i| i.run.map(|r| (r, i.id)))
            .collect();
        let (mut groups, mut done) = (
            json!({"waiting": [], "review": [], "running": []}),
            Vec::new(),
        );
        for run in self
            .hub
            .runs(realm)?
            .into_iter()
            .filter(|r| r.agent.is_some())
        {
            let status = run.display_status();
            let line = run.report.clone().unwrap_or_else(|| match run.plan_total {
                0 => String::new(),
                n => format!("{} of {n} steps done", run.plan_done),
            });
            let task = json!({
                "id": run.id,
                "order": run.objective,
                "agent": run.agent,
                "status": status,
                "line": line,
                "at": run.updated_at,
                "item": waiting.get(&run.id),
                // A change adopted or discarded closes the task: the chat's
                // next message starts a new one.
                "change": run.change.map(|c| c.as_str()),
            });
            let group = match (status, waiting.contains_key(&run.id), run.change) {
                (_, true, _) => "waiting",
                ("running", _, _) => "running",
                ("completed", _, Some(crate::isolation::ChangeStatus::Open)) => "review",
                _ => {
                    done.push(task);
                    continue;
                }
            };
            groups[group].as_array_mut().expect("an array").push(task);
        }
        done.truncate(10);
        groups["done"] = done.into();
        Ok(groups)
    }

    fn agent_names(&self, realm: &Realm) -> Result<Vec<String>> {
        Ok(crate::business::bots(&realm.root()?)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::autonomy::NewItem;

    /// Tasks lists the founder's orders only, each in its group: one waiting
    /// on an item in Needs you, the others done; a chat is not a task.
    #[test]
    fn tasks_are_grouped_by_what_they_wait_for() {
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
        std::fs::create_dir_all(root.join("agents")).unwrap();
        std::fs::write(root.join("agents/writer.md"), "Write.").unwrap();
        let app = super::super::tests::test_app(home, 0);
        let asking = app
            .hub
            .start_task(&realm, "writer", "Check today's rates")
            .unwrap();
        // Run ids are times to the millisecond: one apart each.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut finished = app
            .hub
            .start_task(&realm, "writer", "Outline the week")
            .unwrap();
        finished.status = crate::state::Status::Completed;
        finished.report = Some("\nThe outline is in the run.\nMore.".into());
        // Its change adopted: the task is closed, the chat starts anew.
        finished.change = Some(
            serde_json::from_value(serde_json::json!({
                "repo": "/r", "path": "/r/wt", "branch": "airth/x", "base": "a", "head": "b",
                "status": "adopted", "files": [], "checks": null,
            }))
            .unwrap(),
        );
        finished.save().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.hub.create_run_in(&realm, "Just a chat", None).unwrap();
        let item = Store::of(&realm)
            .unwrap()
            .propose(&NewItem {
                kind: "approval",
                title: "writer asks to use network",
                agent: Some("writer"),
                routine: None,
                run: Some(&asking.id),
                action: None,
                args: "{}",
                execute_at: None,
                source: "{}",
                model_call: None,
            })
            .unwrap();
        let page = app.tasks(&realm).unwrap();
        assert_eq!(page["waiting"][0]["order"], "Check today's rates");
        assert_eq!(page["waiting"][0]["item"], item);
        assert_eq!(page["done"].as_array().unwrap().len(), 1);
        assert_eq!(page["done"][0]["line"], "The outline is in the run.");
        assert_eq!(page["done"][0]["agent"], "writer");
        assert_eq!(page["done"][0]["change"], "adopted");
        assert!(page["waiting"][0]["change"].is_null());
        let all = page.to_string();
        assert!(!all.contains("Just a chat"), "a chat is not a task");
    }
}
