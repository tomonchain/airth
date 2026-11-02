//! Needs you in the new interface: what waits for the founder in a business
//! (the queue, decisions 45 and 47), and the founder's answers. An answer
//! goes through `Hub::answer_item`, as on Telegram: the channel decides who
//! answers, never what an answer may do.

use super::*;
use crate::business::store::queue::{Answer, Item};

impl App {
    /// The queue's routes; None when the request is not one of them.
    pub(super) fn queue_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "queue"]) => {
                json_response(200, self.needs_you(&self.realm(r)?)?)
            }
            // The run an item came from, block by block.
            (Method::Get, ["realms", r, "queue", id, "run"]) => {
                let realm = self.realm(r)?;
                let id: i64 = id.parse().context("no such item")?;
                let store = Store::of(&realm)?;
                let item = store
                    .queue(&["pending", "approved", "rejected", "done", "failed"])?
                    .into_iter()
                    .find(|i| i.id == id)
                    .context("no such item")?;
                let blocks = match (&item.routine, &item.run) {
                    (Some(routine), Some(run)) => store.run_blocks(routine, run)?,
                    _ => Vec::new(),
                };
                json_response(
                    200,
                    json!({"routine": item.routine, "run": item.run, "blocks": blocks}),
                )
            }
            (Method::Post, ["realms", r, "queue", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let id: i64 = id.parse().context("no such item")?;
                let text = body["text"].as_str().unwrap_or_default().to_string();
                let answer = match body["answer"].as_str() {
                    Some("approve") => Answer::Approve,
                    Some("edit") => Answer::Edit(text),
                    Some("reject") => Answer::Reject(text),
                    _ => bail!("answer approve, edit or reject"),
                };
                let seconds = body["seconds"].as_u64().unwrap_or(0);
                let (item, report) = self.hub.answer_item(
                    &realm,
                    id,
                    body["revision"].as_i64(),
                    answer,
                    "app",
                    seconds,
                )?;
                json_response(200, json!({"item": shown(&item), "report": report}))
            }
            _ => return Ok(None),
        }))
    }

    /// What waits for the founder in `realm`, oldest first, and the agents'
    /// departments to group it by.
    fn needs_you(&self, realm: &Realm) -> Result<Value> {
        let items = Store::of(realm)?.queue(&["pending"])?;
        // A business still filling its config shows its items all the same.
        let agents: Vec<Value> = crate::business::config::load(&realm.root()?)
            .map(|c| c.agents)
            .unwrap_or_default()
            .iter()
            .map(|(name, doc)| {
                json!({"name": name, "title": doc.text("title"), "department": doc.text("department")})
            })
            .collect();
        // An action a task of the founder's proposed (decision 56): the task
        // it came from, to open it.
        let titles = realm.run_titles();
        let items: Vec<Value> = items
            .iter()
            .map(|i| {
                let mut page = shown(i);
                if let (None, Some(run)) = (&i.routine, &i.run) {
                    let title = titles
                        .get(run)
                        .cloned()
                        .or_else(|| self.hub.load_run(realm, run).ok().map(|s| s.objective));
                    page["task"] = json!({"id": run, "title": title});
                }
                page
            })
            .collect();
        Ok(json!({"items": items, "agents": agents}))
    }
}

/// An item as the page shows it: what is asked, in plain text, and where
/// it comes from.
fn shown(item: &Item) -> Value {
    let at = |path: &str| {
        path.split('.')
            .try_fold(&item.args, |v, key| v.get(key))
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
    };
    let text = match (item.source["show"].as_str(), item.kind.as_str()) {
        (Some(path), _) => at(path),
        (None, "incident") => at("detail"),
        _ if item.source["tool"] == "http_request" => call_text(&item.args),
        _ => None,
    }
    .unwrap_or_else(|| serde_json::to_string_pretty(&item.args).unwrap_or_default());
    // Only a web address is ever a link: a feed may put anything in `link`.
    let link = ["link", "source_id"]
        .iter()
        .filter_map(|k| item.args[*k].as_str())
        .find(|l| l.starts_with("https://") || l.starts_with("http://"));
    json!({
        "id": item.id,
        "kind": item.kind,
        "title": item.title,
        "agent": item.agent,
        "routine": item.routine,
        "run": item.run,
        "action": item.action,
        "created_at": item.created_at,
        "execute_at": item.execute_at,
        "status": item.status,
        "revision": item.revision,
        "text": text,
        "editable": item.source["editable"].is_string(),
        // A call shown as it would be made, not prose.
        "call": item.source["show"].is_null() && item.source["tool"] == "http_request",
        "source": {"title": item.args["title"].as_str(), "link": link},
    })
}

/// A proposed HTTP call as it would be made: method, address, body.
fn call_text(args: &Value) -> Option<String> {
    let url = args["url"].as_str()?;
    let method = args["method"].as_str().unwrap_or("GET").to_uppercase();
    let at = match args["integration"].as_str().filter(|i| !i.is_empty()) {
        Some(integration) => format!("{integration}: {url}"),
        None => url.to_string(),
    };
    Some(match args["body"].as_str().filter(|b| !b.is_empty()) {
        Some(body) => {
            let body = serde_json::from_str::<Value>(body)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or_else(|| body.to_string());
            format!("{method} {at}\n\n{body}")
        }
        None => format!("{method} {at}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, args: Value, source: Value) -> Item {
        Item {
            id: 1,
            created_at: String::new(),
            kind: kind.into(),
            title: "A post".into(),
            agent: None,
            routine: None,
            run: None,
            action: None,
            original_args: args.clone(),
            args,
            source,
            model_call: None,
            execute_at: None,
            status: "pending".into(),
            revision: 1,
        }
    }

    #[test]
    fn an_item_shows_what_is_asked_and_only_a_web_link() {
        let draft = item(
            "approval",
            json!({"title": "Flows", "link": "javascript:alert(1)", "source_id": "https://e.com/1", "answer": {"draft": "ETH left the queue."}}),
            json!({"block": "ok", "show": "answer.draft", "editable": "answer.draft"}),
        );
        let page = shown(&draft);
        assert_eq!(page["text"], "ETH left the queue.");
        assert_eq!(page["editable"], true);
        assert_eq!(
            page["source"]["link"], "https://e.com/1",
            "never a javascript: link"
        );
        let incident = item(
            "incident",
            json!({"detail": "It failed."}),
            json!({"show": "detail"}),
        );
        assert_eq!(shown(&incident)["text"], "It failed.");
        assert_eq!(shown(&incident)["editable"], false);
        // A mission's proposal shows its call as it would be made.
        let call = item(
            "approval",
            json!({"integration": "blog", "method": "post", "url": "posts", "body": "{\"text\":\"Hi\"}"}),
            json!({"tool": "http_request"}),
        );
        assert_eq!(
            shown(&call)["text"],
            "POST blog: posts\n\n{\n  \"text\": \"Hi\"\n}"
        );
        let public = item(
            "approval",
            json!({"method": "DELETE", "url": "https://x"}),
            json!({"tool": "http_request"}),
        );
        assert_eq!(shown(&public)["text"], "DELETE https://x");
        assert_eq!(shown(&public)["call"], true);
        assert_eq!(shown(&draft)["call"], false);
    }

    /// An action a task of the founder's proposed shows the task, to open it.
    #[test]
    fn an_action_from_a_task_shows_that_task() {
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
        let app = super::super::tests::test_app(home, 0);
        let run = app
            .hub
            .create_run(&realm, "Publish the launch post")
            .unwrap();
        Store::of(&realm)
            .unwrap()
            .propose(&crate::business::store::autonomy::NewItem {
                kind: "approval",
                title: "blog.publish, from a task you started",
                agent: None,
                routine: None,
                run: Some(&run.id),
                action: Some("blog.publish"),
                args: r#"{"integration": "blog", "method": "POST", "url": "posts"}"#,
                execute_at: None,
                source: r#"{"tool": "http_request"}"#,
                model_call: None,
            })
            .unwrap();
        let page = app.needs_you(&realm).unwrap();
        let item = &page["items"][0];
        assert_eq!(item["task"]["id"], run.id.as_str());
        assert_eq!(item["task"]["title"], "Publish the launch post");
        assert_eq!(item["text"], "POST blog: posts");
    }
}
