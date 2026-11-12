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
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "team"]) => json_response(200, self.team(&self.realm(r)?)?),
            (Method::Get, ["realms", r, "spend"]) => {
                let days = param(query, "days").parse().unwrap_or(30);
                let realm = self.realm(r)?;
                json_response(
                    200,
                    crate::hub::finance::spend(&self.hub, &realm, days, chrono::Utc::now())?,
                )
            }
            // The Home's "Today": the morning brief's words, as of now
            // (decision 121), its greeting left out.
            (Method::Get, ["realms", r, "today"]) => {
                let realm = self.realm(r)?;
                let day = crate::routine::morning::read(
                    &realm.root()?,
                    &Store::of(&realm)?,
                    chrono::Utc::now(),
                )?;
                let lines: Vec<&str> = day["brief"]
                    .as_str()
                    .unwrap_or_default()
                    .lines()
                    .skip(1)
                    .collect();
                json_response(200, json!({ "lines": lines }))
            }
            // A Revolut statement the founder imports (decision 123): CSV,
            // as exported.
            (Method::Post, ["realms", r, "finance", "statement"]) => {
                let bytes = read_body(request, crate::statement::MAX_BYTES, "the statement")?;
                let text = String::from_utf8_lossy(&bytes);
                json_response(
                    200,
                    crate::hub::finance::import_statement(
                        &self.realm(r)?,
                        &text,
                        chrono::Utc::now(),
                    )?,
                )
            }
            // A lesson the founder gives an agent (decision 127): kept in
            // its file, read by its tasks and its routines.
            (Method::Post, ["realms", r, "agents", name, "lessons"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                crate::business::add_lesson(
                    &root,
                    name,
                    body["text"].as_str().unwrap_or_default(),
                )?;
                json_response(
                    200,
                    json!({"lessons": crate::business::lessons(&root, name)}),
                )
            }
            // "Keep this look" (decision 65): the founder's gesture.
            (Method::Post, ["realms", r, "agents", name, "look"]) => {
                let seed = crate::business::keep_look(&self.realm(r)?.root()?, name)?;
                json_response(200, json!({"look": seed}))
            }
            (Method::Get, ["realms", r, "company"]) => {
                json_response(200, self.company(&self.realm(r)?)?)
            }
            // The projects (decision 129), the one touched last first.
            (Method::Get, ["realms", r, "projects"]) => {
                let realm = self.realm(r)?;
                let mut projects = serde_json::to_value(Store::of(&realm)?.projects()?)?;
                // Each task says where it stands, read from its run.
                for p in projects.as_array_mut().into_iter().flatten() {
                    for t in p["tasks"].as_array_mut().into_iter().flatten() {
                        let task: crate::business::store::projects::Task =
                            serde_json::from_value(t.clone())?;
                        t["state"] = self.hub.project_task_state(&realm, &task).into();
                    }
                }
                json_response(200, json!({ "projects": projects }))
            }
            // The founder renames a project, rewrites its description, or
            // says it is done or open again.
            (Method::Put, ["realms", r, "projects", id]) => {
                let id: i64 = id
                    .parse()
                    .map_err(|_| anyhow::anyhow!("no project '{id}'"))?;
                let body = read_json(request)?;
                let mut store = Store::of(&self.realm(r)?)?;
                store.edit_project(
                    id,
                    body["name"].as_str(),
                    body["description"].as_str(),
                    body["status"].as_str(),
                    chrono::Utc::now(),
                )?;
                json_response(200, json!({ "project": store.project(id)? }))
            }
            _ => return Ok(None),
        }))
    }

    /// The agents, each with its routines, their last run, what waits for
    /// the founder, and the last 30 days' model cost and founder time.
    pub(super) fn team(&self, realm: &Realm) -> Result<Value> {
        let config = match crate::business::config::load(&realm.root()?, &realm.config.departments)
        {
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
        // An idea waits for no answer (decision 119): it is not counted.
        let mut waiting = store.queue(&["pending"])?;
        waiting.retain(|i| i.kind != "idea");
        let tasks: Vec<_> = self
            .hub
            .runs(realm)?
            .into_iter()
            .filter(|r| r.agent.is_some())
            .collect();
        let root = realm.root()?;
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
            let waits = waiting
                .iter()
                .filter(|i| i.agent.as_deref() == Some(name))
                .count();
            let mine_tasks: Vec<_> = tasks
                .iter()
                .filter(|t| t.agent.as_deref() == Some(name))
                .collect();
            let failed = routines.iter().any(|r| r["last"]["ok"] == false)
                || mine_tasks
                    .first()
                    .is_some_and(|t| t.display_status() == "failed");
            let scheduled = routines.iter().any(|r| !r["schedule"].is_null());
            // Its state, from Airth's records (decision 65).
            let state = match (
                waits > 0,
                failed,
                mine_tasks.iter().any(|t| t.live),
                scheduled,
            ) {
                (true, ..) => "needs",
                (_, true, ..) => "incident",
                (_, _, true, _) => "working",
                (_, _, _, false) => "asleep",
                _ => "idle",
            };
            let (look, look_saved) = crate::business::look(&root, name);
            let accessory = doc
                .text("accessory")
                .filter(|a| ["pen", "lens", "book"].contains(&a.trim()));
            agents.push(json!({
                "name": name,
                "look": look,
                "look_saved": look_saved,
                "accessory": accessory,
                "state": state,
                "title": doc.text("title"),
                "first_name": doc.text("first_name").map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
                // The founder's one interlocutor (decision 120).
                "dispatch": doc.text("dispatch").as_deref() == Some("true"),
                "department": doc.text("department"),
                "job": job(&doc.body),
                // The whole job, as written, to edit it.
                "body": doc.body.trim(),
                // What its tasks may use (decision 64).
                "integrations": doc.text("integrations").map(|t| split_names(&t)).unwrap_or_default(),
                "skills": doc.text("skills").map(|t| split_names(&t)).unwrap_or_default(),
                // The repositories its tasks work in (decision 64).
                "repos": doc.text("repos").map(|t| split_names(&t)).unwrap_or_default(),
                "routines": routines,
                "waiting": waits,
                "cost_usd": cost,
                "unpriced": unpriced,
                "seconds": seconds.iter().find(|(a, _)| a.as_deref() == Some(name)).map_or(0, |(_, s)| *s),
            }));
        }
        Ok(json!({"agents": agents, "broken": config.broken, "departments": departments(realm)}))
    }

    /// The business's config as it stands, with what is left to fill.
    fn company(&self, realm: &Realm) -> Result<Value> {
        let repos: Vec<Value> = realm
            .repos()
            .iter()
            .map(|r| json!({"name": r.name, "url": r.url}))
            .collect();
        let config = match crate::business::config::load(&realm.root()?, &realm.config.departments)
        {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("{e:#}"), "repos": repos})),
        };
        Ok(json!({
            "profile": fields(&config.profile),
            "goal": fields(&config.goal),
            "lines": config.lines.iter().map(|(n, d)| json!({"name": n, "fields": fields(d)})).collect::<Vec<_>>(),
            "repos": repos,
            "missing": config.missing,
            "broken": config.broken,
        }))
    }
}

/// The business's departments, in the order shown: the nine, then the ones
/// the founder added (decision 79).
pub(super) fn departments(realm: &Realm) -> Value {
    let nine = crate::business::config::DEPARTMENTS
        .iter()
        .map(|(key, name)| json!({"key": key, "name": name, "own": false}));
    let own = realm.config.departments.iter().map(|key| {
        let mut name = key.replace('-', " ");
        if let Some(first) = name.get(0..1) {
            name = first.to_uppercase() + &name[1..];
        }
        json!({"key": key, "name": name, "own": true})
    });
    Value::Array(nine.chain(own).collect())
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

/// The card's one line: the body's first line of prose. Agent files open
/// with headings ("## Mission"), which say nothing on a card.
fn job(body: &str) -> Option<&str> {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
}

/// A front-matter list written comma-separated (decision 64).
fn split_names(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::autonomy::NewItem;

    #[test]
    fn a_cards_job_line_skips_headings() {
        assert_eq!(
            job("\n## Mission\n\nDraft the morning posts.\nMore."),
            Some("Draft the morning posts.")
        );
        assert_eq!(job("Write.\n"), Some("Write."));
        assert_eq!(job("## Mission\n"), None);
    }

    /// Decision 65 on Team: each agent's look (derived until kept) and its
    /// state from Airth's records: one with an item waiting needs the
    /// founder; one with nothing scheduled sleeps.
    #[test]
    fn team_shows_each_agents_look_and_state() {
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
        std::fs::write(
            root.join("agents/writer.md"),
            "---\ntitle: Writer\ndepartment: marketing\naccessory: book\n---\nWrite.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("agents/clerk.md"),
            "---\ntitle: Clerk\nlook: 0a1b2c3d\naccessory: wand\n---\nFile.\n",
        )
        .unwrap();
        Store::of(&realm)
            .unwrap()
            .propose(&NewItem {
                kind: "approval",
                title: "A draft",
                agent: Some("writer"),
                routine: None,
                run: None,
                action: None,
                args: "{}",
                execute_at: None,
                source: "{}",
                model_call: None,
            })
            .unwrap();
        let app = super::super::tests::test_app(home, 0);
        let team = app.team(&realm).unwrap();
        let agent = |name: &str| {
            team["agents"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["name"] == name)
                .unwrap()
                .clone()
        };
        let (writer, clerk) = (agent("writer"), agent("clerk"));
        assert_eq!(writer["state"], "needs");
        assert_eq!(writer["look_saved"], false);
        assert_eq!(writer["look"].as_str().unwrap().len(), 8);
        assert_eq!(writer["accessory"], "book");
        assert_eq!(clerk["state"], "asleep", "nothing scheduled");
        assert_eq!(
            (clerk["look"].as_str(), clerk["look_saved"].as_bool()),
            (Some("0a1b2c3d"), Some(true))
        );
        assert!(
            clerk["accessory"].is_null(),
            "only the accessories Airth draws"
        );
    }
}
