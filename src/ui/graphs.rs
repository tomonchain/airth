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
        request: &mut Request,
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
            // Run now, from the app: refused at once when the file does not
            // hold; otherwise the request waits for the run, as a dry run's.
            (Method::Post, ["realms", r, "graphs", name, "run"]) => {
                let realm = self.realm(r)?;
                self.hub
                    .with_routine_checks(&realm, |c| crate::routine::load(c, name))?;
                match self.hub.run_routine_by_hand(&realm, name) {
                    Ok(report) => json_response(200, json!({"report": report})),
                    Err(e) => json_response(422, json!({"error": format!("{e:#}")})),
                }
            }
            // A new routine from the editor (decision 91): checked as a
            // save is, and never over an existing file.
            (Method::Post, ["realms", r, "graphs"]) => {
                let body = read_json(request)?;
                let str_of = |k: &str| body[k].as_str().unwrap_or_default();
                match self.create_graph(&self.realm(r)?, str_of("name"), str_of("text")) {
                    Ok(version) => json_response(200, json!({ "version": version })),
                    Err(e) => json_response(422, json!({"error": format!("{e:#}")})),
                }
            }
            // The routine editor's save (decision 69): the founder's write,
            // checked as loading checks it, never over a newer file.
            (Method::Put, ["realms", r, "graphs", name]) => {
                let body = read_json(request)?;
                let str_of = |k: &str| body[k].as_str().unwrap_or_default();
                match self.save_graph(&self.realm(r)?, name, str_of("text"), str_of("version")) {
                    Ok(version) => json_response(200, json!({ "version": version })),
                    Err(crate::documents::SaveError::Changed(version)) => json_response(
                        409,
                        json!({ "error": "the routine changed since you opened it", "changed": true, "version": version }),
                    ),
                    Err(crate::documents::SaveError::Other(e)) => {
                        json_response(422, json!({"error": format!("{e:#}")}))
                    }
                }
            }
            _ => return Ok(None),
        }))
    }

    /// Write a new routine the founder drew, once it passes the loader's
    /// checks. Returns the file's version.
    fn create_graph(&self, realm: &Realm, name: &str, text: &str) -> Result<String> {
        let file = crate::routine::file(name)?;
        let path = realm.root()?.join(&file);
        if path.exists() {
            bail!("{file} already exists: choose another name");
        }
        self.hub
            .with_routine_checks(realm, |c| crate::routine::check_text(c, name, text))?;
        crate::realm::write_atomic(&path, text)?;
        crate::documents::version(&path)
    }

    /// Write a routine the founder edited, once it passes the loader's
    /// checks; nothing is written otherwise. Returns the file's new version.
    fn save_graph(
        &self,
        realm: &Realm,
        name: &str,
        text: &str,
        version: &str,
    ) -> Result<String, crate::documents::SaveError> {
        self.hub
            .with_routine_checks(realm, |c| crate::routine::check_text(c, name, text))?;
        let file = crate::routine::file(name)?;
        crate::documents::save_text(&realm.root()?, &file, text, version)
    }

    /// Each routine, loaded or with why not, with its agent, its last runs,
    /// its next time and what its model calls cost this week.
    fn graphs(&self, realm: &Realm) -> Result<Value> {
        // What the editor offers: the declared actions and the prompts.
        let (all, actions) = self.hub.with_routine_checks(realm, |c| {
            let actions: Vec<Value> = c
                .actions
                .iter()
                .map(|a| json!({"id": a.id, "class": a.class, "ceiling": a.ceiling}))
                .collect();
            Ok((crate::routine::load_all(c)?, actions))
        })?;
        let root = realm.root()?;
        let mut prompts: Vec<String> = std::fs::read_dir(root.join("prompts"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                name.ends_with(".md").then(|| format!("prompts/{name}"))
            })
            .collect();
        prompts.sort();
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
            // The file as written, for the editor to change in place. The
            // version is taken first: a change in between makes it stale.
            let path = root.join(crate::routine::file(&name)?);
            let version = crate::documents::version(&path).unwrap_or_default();
            let mut routine = json!({
                "name": name,
                "source": std::fs::read_to_string(&path).unwrap_or_default(),
                "version": version,
                "runs": runs,
                "next_at": store.next_run(&key)?,
                "cost_usd": cost.map_or(0.0, |c| c.cost_usd),
                "unpriced": cost.map_or(0, |c| c.unpriced),
                // What its drafting block hears from the founder (decision 116).
                "feedback": store.founder_feedback(
                    &name,
                    chrono::Utc::now() - chrono::Duration::days(crate::routine::run::FEEDBACK_DAYS),
                    crate::routine::run::FEEDBACK_MAX,
                )?,
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
        Ok(json!({"routines": routines, "actions": actions, "prompts": prompts}))
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
        assert!(
            page["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "airth.report")
        );
        assert_eq!(page["prompts"], json!([]));

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

    /// The routine editor writes only a graph the loader accepts, and
    /// never over a file that changed since the page read it.
    #[test]
    fn the_editor_saves_only_a_checked_routine_over_what_it_read() {
        use crate::documents::SaveError;
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
        std::fs::create_dir_all(root.join("routines")).unwrap();
        std::fs::write(root.join("agents/chief.md"), "Report the week.").unwrap();
        let file = root.join("routines/weekly.json");
        let graph = |schedule: &str, act: &str| {
            format!(
                r#"{{"agent": "chief", "trigger": {{"schedule": "{schedule}"}}, "blocks": [
                {{"id": "week", "type": "read", "source": "business", "records": "week"}},
                {{"id": "out", "type": "act", "action": "{act}",
                 "fields": {{"title": "Week", "text": "{{{{summary}}}}"}}}}]}}"#
            )
        };
        let original = graph("mon 08:00", "airth.report");
        std::fs::write(&file, &original).unwrap();
        let app = super::super::tests::test_app(home, 0);
        let page = app.graphs(&realm).unwrap();
        let listed = &page["routines"][0];
        assert_eq!(listed["source"], original.as_str());
        let version = listed["version"].as_str().unwrap().to_string();

        // Refused by the loader's checks: nothing is written.
        for (text, why) in [
            (graph("someday", "airth.report"), "schedule"),
            (graph("mon 08:00", "ghost.send"), "ghost.send"),
            (original.replace("\"agent\"", "\"secret\": 1, \"agent\""), "unknown field"),
            // Content from outside, straight to an act: the security rule.
            (
                r#"{"agent": "chief", "trigger": {"manual": true}, "blocks": [
                {"id": "news", "type": "read", "source": "rss", "feeds": ["https://example.com/feed"]},
                {"id": "go", "type": "act", "action": "airth.launch_mission",
                 "fields": {"brief": "{{title}}"}}]}"#
                    .to_string(),
                "without an approval",
            ),
        ] {
            let Err(SaveError::Other(e)) = app.save_graph(&realm, "weekly", &text, &version) else {
                panic!("{why}: saved");
            };
            assert!(format!("{e:#}").contains(why), "{e:#}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        }
        let Err(SaveError::Other(_)) = app.save_graph(&realm, "../weekly", &original, &version)
        else {
            panic!("a path, not a name, was saved");
        };

        // Checked: written, with a new version.
        let edited = graph("fri 09:00", "airth.report");
        let Ok(new) = app.save_graph(&realm, "weekly", &edited, &version) else {
            panic!("a checked routine was refused");
        };
        assert_ne!(new, version);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), edited);

        // A new routine: checked the same way, never over an existing one,
        // and the editor's layout must place real blocks.
        let new = root.join("routines/drawn.json");
        let laid = |at: &str| {
            edited.replacen(
                r#"{"agent""#,
                &format!(r#"{{"layout": {{"trigger": [0, 0], "{at}": [240, 0]}}, "agent""#),
                1,
            )
        };
        assert!(
            app.create_graph(&realm, "weekly", &edited).is_err(),
            "over an existing file"
        );
        let named = edited.replace(r#""id": "week""#, r#""id": "trigger""#);
        let e = app.create_graph(&realm, "drawn", &named).unwrap_err();
        assert!(format!("{e:#}").contains("kept for the trigger"), "{e:#}");
        let e = app
            .create_graph(&realm, "drawn", &laid("ghost"))
            .unwrap_err();
        assert!(format!("{e:#}").contains("not a block"), "{e:#}");
        assert!(!new.exists());
        app.create_graph(&realm, "drawn", &laid("week")).unwrap();
        assert_eq!(std::fs::read_to_string(&new).unwrap(), laid("week"));

        // The page's version is now stale: refused, not overwritten.
        let Err(SaveError::Changed(_)) = app.save_graph(&realm, "weekly", &original, &version)
        else {
            panic!("saved over a newer file");
        };
        assert_eq!(std::fs::read_to_string(&file).unwrap(), edited);
    }
}
