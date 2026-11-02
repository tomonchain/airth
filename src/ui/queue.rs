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
        Ok(json!({"items": items.iter().map(shown).collect::<Vec<_>>(), "agents": agents}))
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
        "source": {"title": item.args["title"].as_str(), "link": link},
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
            json!({"method": "POST", "url": "https://x"}),
            json!({"tool": "http_request"}),
        );
        assert!(
            shown(&call)["text"]
                .as_str()
                .unwrap()
                .contains("\"method\": \"POST\"")
        );
    }
}
