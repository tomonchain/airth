//! Decision 49: a routine runs for real, waits for the founder at an
//! approval, and resumes with the founder's answer; its acts are done once.

use super::*;
use crate::business::store::Store;
use crate::business::store::queue::Answer;

/// A feed at `/feed`, and a drafts endpoint that keeps each body it gets.
fn services() -> (String, Arc<Mutex<Vec<String>>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let posted = Arc::new(Mutex::new(Vec::new()));
    let kept = posted.clone();
    let link = format!("{base}/news/1?utm_source=x");
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let reply = if req.url() == "/feed" {
                format!(
                    "<rss><channel><item><title>Flows</title><link>{link}</link></item></channel></rss>"
                )
            } else {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                kept.lock().unwrap().push(body);
                "{\"id\": 1}".into()
            };
            req.respond(tiny_http::Response::from_string(reply))
                .unwrap();
        }
    });
    (base, posted)
}

#[test]
fn a_routine_waits_for_the_founder_and_acts_once_on_the_approval() {
    let (base, posted) = services();
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/post-writer.md".to_string(), "Draft posts.".to_string()),
        ("prompts/post.md".into(), "Draft a post.".into()),
        ("prompts/post.schema.json".into(), r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string"}}}"#.into()),
        ("routines/posts.json".into(), json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": [format!("{base}/feed")], "dedupe": "link"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft"},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"content": "{{answer.draft}}"}}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    f.realm
        .set_integration(crate::integration::Integration {
            name: "typefully".into(),
            description: String::new(),
            base_url: base.clone(),
            headers: Default::default(),
            actions: vec![
                serde_json::from_value(
                    json!({"name": "create_draft", "method": "POST", "path": "/v1/drafts",
                "fields": [{"name": "content", "place": "body"}], "class": "reversible"}),
                )
                .unwrap(),
            ],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    // Where the engine leaves an item at the approval (its own test is in
    // routine::run): the item, its block, the field the founder may edit.
    let link = format!("{base}/news/1");
    let item = json!({"title": "Flows", "link": link, "source_id": link, "answer": {"draft": "ETH left the queue."}});
    Store::of(&f.realm)
        .unwrap()
        .propose(&crate::business::store::autonomy::NewItem {
            kind: "approval",
            title: "ETH left the queue.",
            agent: Some("post-writer"),
            routine: Some("posts"),
            run: Some("r1"),
            action: None,
            args: &item.to_string(),
            execute_at: None,
            source: r#"{"block": "ok", "editable": "answer.draft"}"#,
            model_call: None,
        })
        .unwrap();
    assert!(
        posted.lock().unwrap().is_empty(),
        "nothing done before the founder answers"
    );

    hub.answer_item(
        &f.realm,
        1,
        None,
        Answer::Edit("ETH left. Calmly.".into()),
        "app",
        5,
    )
    .unwrap();
    let (item, report) = hub
        .answer_item(&f.realm, 1, None, Answer::Approve, "app", 5)
        .unwrap();
    assert_eq!(item.status, "done");
    let report = report.unwrap();
    let key = format!("posts/{base}/news/1/send");
    assert!(
        report.contains(&format!("typefully.create_draft [{key}]")),
        "{report}"
    );
    assert_eq!(
        *posted.lock().unwrap(),
        vec![r#"{"content":"ETH left. Calmly."}"#]
    );

    // Approved again: it is done already, nothing runs twice.
    let (item, again) = hub
        .answer_item(&f.realm, 1, None, Answer::Approve, "app", 5)
        .unwrap();
    assert_eq!((item.status.as_str(), again), ("done", None));
    assert_eq!(posted.lock().unwrap().len(), 1);
}

/// A scheduled routine is armed the first time it is seen, then runs at its
/// time, once.
#[test]
fn a_scheduled_routine_runs_at_its_time() {
    let f = fixture();
    let root = f.root.path();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::create_dir_all(root.join("routines")).unwrap();
    std::fs::write(root.join("agents/watcher.md"), "Watch.").unwrap();
    std::fs::write(
        root.join("routines/hourly.json"),
        json!({"agent": "watcher", "trigger": {"schedule": "every 1h"}, "blocks": [
            {"id": "tell", "type": "act", "action": "airth.report", "fields": {"title": "Hourly"}}
        ]})
        .to_string(),
    )
    .unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let now = Utc::now();
    assert!(hub.due_routines(now).unwrap().is_empty(), "armed, not run");
    let due = hub
        .due_routines(now + chrono::Duration::minutes(61))
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].1, "hourly");
    assert!(
        hub.due_routines(now + chrono::Duration::minutes(62))
            .unwrap()
            .is_empty()
    );
}

/// `CLAUDE.md`: an act the service refuses for what the agent wrote becomes
/// an incident in Needs you, and its agent takes that action down a level
/// (decision 55); the founder's "seen" closes it.
#[test]
fn a_failed_act_is_an_incident_and_demotes_its_agent() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let refused = tiny_http::Response::from_string("{\"error\": \"text too long\"}")
                .with_status_code(422);
            req.respond(refused).unwrap();
        }
    });
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::create_dir_all(root.join("routines")).unwrap();
    std::fs::write(root.join("agents/post-writer.md"), "Draft.").unwrap();
    std::fs::write(
        root.join("routines/posts.json"),
        json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "ok", "type": "approval", "show": "title"},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"content": "{{title}}"}}
        ]})
        .to_string(),
    )
    .unwrap();
    f.realm
        .set_integration(crate::integration::Integration {
            name: "typefully".into(),
            description: String::new(),
            base_url: base,
            headers: Default::default(),
            actions: vec![
                serde_json::from_value(
                    json!({"name": "create_draft", "method": "POST", "path": "/v1/drafts",
                "fields": [{"name": "content", "place": "body"}], "class": "reversible"}),
                )
                .unwrap(),
            ],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    let mut store = Store::of(&f.realm).unwrap();
    store
        .set_level("post-writer", "typefully.create_draft", 3, 3, "founder", "")
        .unwrap();
    store
        .propose(&crate::business::store::autonomy::NewItem {
            kind: "approval",
            title: "A post",
            agent: Some("post-writer"),
            routine: Some("posts"),
            run: Some("r1"),
            action: None,
            args: &json!({"title": "A post", "source_id": "https://e.com/1"}).to_string(),
            execute_at: None,
            source: r#"{"block": "ok"}"#,
            model_call: None,
        })
        .unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (item, report) = hub
        .answer_item(&f.realm, 1, None, Answer::Approve, "app", 5)
        .unwrap();
    assert!(
        report.unwrap().contains("failed"),
        "the act's 422 is a failure"
    );
    assert_eq!(item.status, "done", "the routine ran; its act failed");
    let incidents: Vec<_> = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident")
        .collect();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].title, "typefully.create_draft failed in posts");
    assert!(
        incidents[0].args["detail"]
            .as_str()
            .unwrap()
            .contains("now takes typefully.create_draft at level 2 (was 3)")
    );
    assert_eq!(
        Store::of(&f.realm)
            .unwrap()
            .level("post-writer", "typefully.create_draft", 3, 1)
            .unwrap(),
        2
    );
    let (seen, _) = hub
        .answer_item(
            &f.realm,
            incidents[0].id,
            None,
            Answer::Approve,
            "telegram",
            3,
        )
        .unwrap();
    assert_eq!(seen.status, "approved");
    assert!(
        Store::of(&f.realm)
            .unwrap()
            .queue(&["pending"])
            .unwrap()
            .is_empty()
    );
}
