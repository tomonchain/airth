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
            free_reads: false,
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
            free_reads: false,
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
    // Approved, but the send failed: the item says so in one line, with
    // the service's own words, never as done and never as raw JSON.
    let report = report.unwrap();
    assert_eq!(item.status, "failed", "the routine ran; its act failed");
    assert!(
        report.starts_with("typefully.create_draft: HTTP 422")
            && report.ends_with(": text too long"),
        "{report}"
    );
    assert!(!report.contains('{') && !report.contains('\n'), "{report}");
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

/// Decision 59: what a run's `airth.report` gathers is one report in
/// Needs you, not a notification per item; a retried run reports nothing
/// twice; Seen closes it.
#[test]
fn a_runs_reports_are_one_item_in_needs_you_and_never_repeat() {
    let feed = "https://feeds.example.com/watch.xml";
    let items: String = ["ETH", "BTC", "SOL"]
        .iter()
        .map(|t| format!("<item><title>{t}</title><link>https://e.com/{t}</link></item>"))
        .collect();
    crate::hub::pipelines::TEST_FEEDS.lock().unwrap().insert(
        feed.into(),
        format!("<rss><channel>{items}</channel></rss>"),
    );
    let f = fixture();
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::create_dir_all(root.join("routines")).unwrap();
    std::fs::write(root.join("agents/watcher.md"), "Watch.").unwrap();
    std::fs::write(
        root.join("routines/watch.json"),
        json!({"agent": "watcher", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": [feed]},
            {"id": "tell", "type": "act", "action": "airth.report",
             "fields": {"title": "Daily watch", "line": "{{title}}"}}
        ]})
        .to_string(),
    )
    .unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let start = || crate::routine::run::Start::Trigger;
    let report = hub
        .run_routine(&f.realm, "watch", "r1", start(), None)
        .unwrap()
        .text;
    assert!(
        report.contains("tell: reported in Needs you (item 1)"),
        "{report}"
    );
    let store = Store::of(&f.realm).unwrap();
    let items = store.queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 1, "one report for the run, not one per item");
    assert_eq!(
        (items[0].kind.as_str(), items[0].title.as_str()),
        ("report", "Daily watch")
    );
    assert_eq!(items[0].args["text"], "ETH\n\nBTC\n\nSOL");
    assert_eq!(items[0].agent.as_deref(), Some("watcher"));
    assert!(
        f.home.inbox().list().unwrap().is_empty(),
        "nothing in the old inbox"
    );
    // Retried: the same items, the same keys, nothing new.
    let again = hub
        .run_routine(&f.realm, "watch", "r2", start(), None)
        .unwrap()
        .text;
    assert!(
        again.contains("tell: already reported, nothing new"),
        "{again}"
    );
    assert_eq!(store.queue(&["pending"]).unwrap().len(), 1);
    // Seen closes it.
    let (_, said) = hub
        .answer_item(&f.realm, 1, None, Answer::Approve, "telegram", 4)
        .unwrap();
    assert_eq!(said.as_deref(), Some("seen"));
    assert!(store.queue(&["pending", "approved"]).unwrap().is_empty());
    assert_eq!(store.queue(&["done"]).unwrap()[0].id, 1);
}

/// A drafts service that answers each request with the next code of
/// `codes`, and keeps each path it was asked.
fn answering(codes: Vec<u16>) -> (String, Arc<Mutex<Vec<String>>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let paths = Arc::new(Mutex::new(Vec::new()));
    let kept = paths.clone();
    std::thread::spawn(move || {
        let mut codes = codes.into_iter();
        for req in server.incoming_requests() {
            kept.lock().unwrap().push(req.url().to_string());
            let code = codes.next().unwrap_or(500);
            let reply =
                tiny_http::Response::from_string("{\"detail\": \"no\"}").with_status_code(code);
            req.respond(reply).unwrap();
        }
    });
    (base, paths)
}

/// A routine whose approved act calls `base` at `path`, with `class`; its
/// one item approved by the founder, so the act has run once.
fn approved_and_failed(f: &mut Fixture, base: &str, path: &str, class: &str) -> Hub {
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::create_dir_all(root.join("routines")).unwrap();
    std::fs::write(root.join("agents/post-writer.md"), "Draft.").unwrap();
    std::fs::write(
        root.join("routines/posts.json"),
        json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "ok", "type": "approval", "show": "title"},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"text": "{{title}}"}}
        ]})
        .to_string(),
    )
    .unwrap();
    declare(f, base, path, class);
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    Store::of(&f.realm)
        .unwrap()
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
    let (item, _) = hub
        .answer_item(&f.realm, 1, None, Answer::Approve, "telegram", 5)
        .unwrap();
    assert_eq!(item.status, "failed");
    hub
}

fn declare(f: &mut Fixture, base: &str, path: &str, class: &str) {
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
            name: "typefully".into(),
            description: String::new(),
            base_url: base.into(),
            headers: Default::default(),
            actions: vec![
                serde_json::from_value(
                    json!({"name": "create_draft", "method": "POST", "path": path,
                    "fields": [{"name": "text", "place": "body"}], "class": class}),
                )
                .unwrap(),
            ],
        })
        .unwrap();
}

fn incident(f: &Fixture) -> crate::business::store::queue::Item {
    Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .find(|i| i.kind == "incident")
        .unwrap()
}

/// Decision 62: a refusal that proves nothing was done offers Retry; only
/// the founder's channels can take it; the action is rebuilt from the
/// approved item and the declaration as it is now, under the same key,
/// and done once.
#[test]
fn the_founder_retries_a_refused_action_once_with_the_fixed_declaration() {
    let (base, paths) = answering(vec![403, 201]);
    let mut f = fixture();
    let hub = approved_and_failed(&mut f, &base, "/v1/drafts", "reversible");
    let refused = incident(&f);
    assert_eq!(refused.args["retry"], "clear");
    assert_eq!(refused.args["key"], "posts/https://e.com/1/send");
    // The founder fixes the declaration, then retries.
    declare(&mut f, &base, "/v2/drafts", "reversible");
    let e = hub
        .answer_item(
            &f.realm,
            refused.id,
            None,
            Answer::Retry { checked: false },
            "cli",
            1,
        )
        .unwrap_err();
    assert!(
        format!("{e:#}").contains("only from the app or Telegram"),
        "{e:#}"
    );
    let (closed, report) = hub
        .answer_item(
            &f.realm,
            refused.id,
            None,
            Answer::Retry { checked: false },
            "telegram",
            3,
        )
        .unwrap();
    assert_eq!(closed.status, "done");
    assert_eq!(
        report.as_deref(),
        Some("retried: typefully.create_draft done")
    );
    assert_eq!(*paths.lock().unwrap(), ["/v1/drafts", "/v2/drafts"]);
    let journal = Store::of(&f.realm).unwrap().journal().unwrap();
    assert_eq!(
        journal,
        [(
            "posts/https://e.com/1/send".to_string(),
            "confirmed".to_string(),
            true
        )]
    );
    // Done once: asked again, nothing is sent.
    hub.answer_item(
        &f.realm,
        refused.id,
        None,
        Answer::Retry { checked: false },
        "telegram",
        1,
    )
    .unwrap();
    assert_eq!(paths.lock().unwrap().len(), 2);
}

/// Decision 62: when the action may have been done (a 502), Retry waits for
/// the founder to say they checked; what commits is never retried.
#[test]
fn an_ambiguous_failure_asks_first_and_what_commits_is_never_retried() {
    let (base, paths) = answering(vec![502, 201]);
    let mut f = fixture();
    let hub = approved_and_failed(&mut f, &base, "/drafts", "reversible");
    let unsure = incident(&f);
    assert_eq!(unsure.args["retry"], "ambiguous");
    let e = hub
        .answer_item(
            &f.realm,
            unsure.id,
            None,
            Answer::Retry { checked: false },
            "app",
            1,
        )
        .unwrap_err();
    assert!(
        format!("{e:#}").contains("Check that nothing was created"),
        "{e:#}"
    );
    assert_eq!(paths.lock().unwrap().len(), 1);
    let (closed, _) = hub
        .answer_item(
            &f.realm,
            unsure.id,
            None,
            Answer::Retry { checked: true },
            "app",
            1,
        )
        .unwrap();
    assert_eq!(closed.status, "done");
    assert_eq!(paths.lock().unwrap().len(), 2);

    let (base, paths) = answering(vec![403, 201]);
    let mut f = fixture();
    let hub = approved_and_failed(&mut f, &base, "/pay", "commits");
    let committing = incident(&f);
    assert!(committing.args["retry"].is_null());
    let e = hub
        .answer_item(
            &f.realm,
            committing.id,
            None,
            Answer::Retry { checked: true },
            "app",
            1,
        )
        .unwrap_err();
    assert!(format!("{e:#}").contains("offers no retry"), "{e:#}");
    assert_eq!(paths.lock().unwrap().len(), 1);
}

/// Decision 63: a post link from the founder runs the business's reply
/// routine on that post, the rest of the message as their angle; a business
/// without one, or a post too thin to answer, says why and drafts nothing.
#[test]
fn a_post_link_runs_the_reply_routine() {
    let link = "https://x.com/jack/status/2026";
    let oembed = |text: &str| {
        json!({"url": link, "author_name": "jack",
            "html": format!("<blockquote><p>{text}</p>&mdash; jack <a href=\"x\">November 3, 2026</a></blockquote>")})
        .to_string()
    };
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let e = hub
        .reply_to_post(&f.realm, &format!("{link} push back"))
        .unwrap_err();
    assert!(format!("{e:#}").contains("no routine for replies"), "{e:#}");
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::create_dir_all(root.join("routines")).unwrap();
    std::fs::write(root.join("agents/writer.md"), "Reply.").unwrap();
    std::fs::write(
        root.join("routines/replies.json"),
        json!({"agent": "writer", "trigger": {"telegram": "x_post"}, "blocks": [
            {"id": "post", "type": "read", "source": "x_post"},
            {"id": "ok", "type": "approval", "show": "summary"}
        ]})
        .to_string(),
    )
    .unwrap();
    let url = crate::routine::x_post::oembed_url(link);
    crate::hub::pipelines::TEST_FEEDS.lock().unwrap().insert(
        url.clone(),
        oembed("Staking yields fell to 2.8% this week and withdrawals doubled."),
    );
    hub.reply_to_post(
        &f.realm,
        "https://twitter.com/jack/status/2026?s=46 push back on the yield claim",
    )
    .unwrap();
    let items = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].routine.as_deref(), Some("replies"));
    assert_eq!(items[0].args["link"], link);
    assert_eq!(items[0].args["source_id"], link);
    assert_eq!(
        items[0].args["founder_angle"],
        "push back on the yield claim"
    );
    crate::hub::pipelines::TEST_FEEDS
        .lock()
        .unwrap()
        .insert(url, oembed("gm https://t.co/abc"));
    let e = hub.reply_to_post(&f.realm, link).unwrap_err();
    assert!(
        format!("{e:#}").contains("not drafted: the post is mostly a link or a quoted post"),
        "{e:#}"
    );
    assert!(hub.reply_to_post(&f.realm, "no link at all").is_err());
}

/// Decision 49: a routine hands an item to a mission at its agent's level,
/// waits, and goes on with the mission's report; an approval given before
/// the mission does not cover what follows it, and nothing runs twice.
#[test]
fn a_routine_launches_a_mission_and_goes_on_with_its_report() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/writer.md".to_string(), "Write articles.".to_string()),
        ("routines/article.json".into(), json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
            {"id": "week", "type": "read", "source": "business", "records": "week"},
            {"id": "write", "type": "act", "action": "airth.launch_mission", "fields": {"brief": "Write the article of {{week}}"}},
            {"id": "ok", "type": "approval", "show": "result.report"}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let (hub, _, _) = make_hub(&f.home, vec![done("Flows, the article: drafted.")]);
    // At level 1 (the founder lowered it from 3), the founder is asked
    // before the mission starts.
    Store::of(&f.realm)
        .unwrap()
        .set_level("writer", "airth.launch_mission", 1, 3, "founder", "")
        .unwrap();
    let start = crate::routine::run::Start::Trigger;
    hub.run_routine(&f.realm, "article", "r1", start, None)
        .unwrap();
    let asked = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].action.as_deref(), Some("airth.launch_mission"));
    let brief = asked[0].args["brief"].as_str().unwrap();
    assert!(brief.starts_with("Write the article of 20"), "{brief}");
    // What Needs you and Telegram show: the brief.
    assert_eq!(asked[0].source["show"], "brief");
    assert!(hub.take_hand_offs().unwrap().is_empty());

    let (item, _) = hub
        .answer_item(&f.realm, asked[0].id, None, Answer::Approve, "app", 5)
        .unwrap();
    assert_eq!(item.status, "done");
    let mut handed = hub.take_hand_offs().unwrap();
    assert_eq!(handed.len(), 1);
    let (realm, mut mission) = handed.remove(0);
    assert_eq!(mission.agent.as_deref(), Some("writer"));
    assert_eq!(mission.routine.as_ref().unwrap().block, "write");
    // Approved again: launched once.
    hub.answer_item(&f.realm, asked[0].id, None, Answer::Approve, "app", 5)
        .unwrap();
    assert!(hub.take_hand_offs().unwrap().is_empty());

    hub.drive(
        &realm,
        &mut mission,
        config(10),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert_eq!(mission.status, Status::Completed);
    assert!(mission.routine.as_ref().unwrap().resumed);
    let waiting = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    let article = &waiting[0];
    assert_eq!(article.source["block"], "ok");
    assert_eq!(
        article.args["result"]["report"],
        "Flows, the article: drafted."
    );
    assert_eq!(article.args["result"]["mission"], mission.id.as_str());
    assert!(
        article.args.get("approved_by_founder").is_none(),
        "{:?}",
        article.args
    );

    // A mission that ends again does not start the routine again.
    hub.mission_ended(&realm, &mut mission).unwrap();
    let after = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(after.len(), 1);
}

/// Decision 96: a reversible action starts at level 3, yet content from
/// outside still needs an approval on the way to it until the founder raises
/// the level.
#[test]
fn content_from_outside_needs_an_approval_until_the_founder_raises_the_level() {
    let mut f = fixture();
    declare(&mut f, "https://drafts.example", "/v1/drafts", "reversible");
    let root = f.root.path();
    for dir in ["agents", "routines"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("agents/post-writer.md"), "Draft posts.").unwrap();
    std::fs::write(
        root.join("routines/posts.json"),
        json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["https://news.example/feed"]},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"text": "{{title}}"}}
        ]})
        .to_string(),
    )
    .unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let load = || hub.with_routine_checks(&f.realm, |c| crate::routine::load(c, "posts"));

    let e = format!("{:#}", load().unwrap_err());
    assert!(e.contains("without an approval"), "{e}");

    Store::of(&f.realm)
        .unwrap()
        .set_level("post-writer", "typefully.create_draft", 3, 3, "founder", "")
        .unwrap();
    assert!(load().is_ok());
}

/// Decision 108: a business that fails to schedule (here, its database
/// cannot open) is said once to the founder, and the others' routines still
/// run.
#[test]
fn a_broken_business_does_not_stop_the_others_routines() {
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
    // Sorted before "work", so it is tried first.
    let other = tempfile::tempdir().unwrap();
    let broken = f
        .home
        .create_realm(NewRealm {
            name: "broken".into(),
            description: String::new(),
            root: Some(other.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    std::fs::create_dir_all(broken.dir().join(crate::business::store::FILE)).unwrap();

    let (hub, _, _) = make_hub(&f.home, vec![]);
    let now = Utc::now();
    assert!(hub.due_routines(now).unwrap().is_empty(), "armed, not run");
    let due = hub
        .due_routines(now + chrono::Duration::minutes(61))
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].0.name(), "work");
    let told: Vec<_> = f
        .home
        .inbox()
        .list()
        .unwrap()
        .into_iter()
        .filter(|n| n.realm == "broken")
        .collect();
    assert_eq!(told.len(), 1, "said once, though it failed twice");
    assert!(told[0].title.contains("Scheduling routines"));
}

/// A draft the founder did not answer within its routine's `expires_after`
/// leaves Needs you, marked expired; one still in its window, or answered,
/// stays as it is.
#[test]
fn a_draft_not_answered_in_its_window_expires() {
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/post-writer.md".to_string(), "Draft posts.".to_string()),
        ("prompts/post.md".into(), "Draft a post.".into()),
        ("prompts/post.schema.json".into(), r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string"}}}"#.into()),
        ("routines/posts.json".into(), json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["http://127.0.0.1:9/feed"], "dedupe": "link"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft", "expires_after": 24},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"content": "{{answer.draft}}"}}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
            name: "typefully".into(),
            description: String::new(),
            base_url: "http://127.0.0.1:9".into(),
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
    f.realm.save().unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    assert_eq!(
        hub.approval_windows(&f.realm)
            .get(&("posts".to_string(), "ok".to_string())),
        Some(&24)
    );
    let mut store = Store::of(&f.realm).unwrap();
    for title in ["Old draft", "Answered draft", "Fresh draft"] {
        store
            .propose(&crate::business::store::autonomy::NewItem {
                kind: "approval",
                title,
                agent: Some("post-writer"),
                routine: Some("posts"),
                run: Some("r1"),
                action: None,
                args: r#"{"answer": {"draft": "x"}}"#,
                execute_at: None,
                source: r#"{"block": "ok", "show": "answer.draft", "editable": "answer.draft"}"#,
                model_call: None,
            })
            .unwrap();
    }
    let created = |id: i64| {
        let item = store
            .queue(&["pending"])
            .unwrap()
            .into_iter()
            .find(|i| i.id == id)
            .unwrap();
        chrono::DateTime::parse_from_rfc3339(&item.created_at)
            .unwrap()
            .with_timezone(&chrono::Utc)
    };
    let at = created(1);
    store
        .answer(2, None, Answer::Reject("no".into()), "app", 5)
        .unwrap();
    // A day and an hour later, the first draft is out of its window.
    let gone = hub
        .expire_approvals(at + chrono::Duration::hours(25))
        .unwrap();
    assert_eq!(gone, vec![(f.realm.name().to_string(), 2)]);
    let left: Vec<String> = store
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert!(
        left.is_empty(),
        "both unanswered drafts were made at once: {left:?}"
    );
    let expired = store.queue(&["expired"]).unwrap();
    assert_eq!(expired.len(), 2);
    assert_eq!(
        store.queue(&["rejected"]).unwrap().len(),
        1,
        "an answered draft stays answered"
    );
    // Within the window, nothing leaves.
    assert!(hub.expire_approvals(at).unwrap().is_empty());
}

/// The founder asks for a new version of a draft: the drafting block runs
/// again with the founder's words, told apart from the item's data; the new
/// draft takes the old one's place, and the old one is rejected with the
/// request.
#[test]
fn a_draft_is_redone_with_what_the_founder_asked() {
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/post-writer.md".to_string(), "Draft posts.".to_string()),
        ("prompts/post.md".into(), "Draft a post.".into()),
        ("prompts/post.schema.json".into(), r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string"}}}"#.into()),
        ("routines/posts.json".into(), json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["http://127.0.0.1:9/feed"], "dedupe": "link"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft", "expires_after": 24},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"content": "{{answer.draft}}"}}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
            name: "typefully".into(),
            description: String::new(),
            base_url: "http://127.0.0.1:9".into(),
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
    f.realm.save().unwrap();
    let (hub, seen, _) = make_hub(&f.home, vec![done(r#"{"draft": "ETH left. Short."}"#)]);
    let item = json!({"title": "Flows", "link": "https://example.com/1", "source_id": "https://example.com/1",
        "answer": {"draft": "ETH left the staking queue, and here is a long story about it."}});
    Store::of(&f.realm)
        .unwrap()
        .propose(&crate::business::store::autonomy::NewItem {
            kind: "approval",
            title: "ETH left the staking queue",
            agent: Some("post-writer"),
            routine: Some("posts"),
            run: Some("r1"),
            action: None,
            args: &item.to_string(),
            execute_at: None,
            source: r#"{"block": "ok", "show": "answer.draft", "editable": "answer.draft"}"#,
            model_call: None,
        })
        .unwrap();
    assert!(
        hub.redo_item(&f.realm, 1, None, "  ", "app", 3).is_err(),
        "a redo says what to change"
    );
    hub.redo_item(&f.realm, 1, None, "shorter", "app", 3)
        .unwrap();

    let store = Store::of(&f.realm).unwrap();
    let pending = store.queue(&["pending"]).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].args["answer"]["draft"], "ETH left. Short.");
    assert_eq!(
        pending[0].args["title"], "Flows",
        "the same item, drafted again"
    );
    assert!(
        pending[0].args.get(crate::pipeline::run::REDO).is_none(),
        "the request served this draft only"
    );
    let old = store.queue(&["rejected"]).unwrap();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].id, 1);
    // The founder's words reach the model outside the item, with the old draft.
    let asked: String = seen.lock().unwrap()[0]
        .iter()
        .map(|m| format!("{m:?}"))
        .collect();
    let item_part = asked.split("</item>").next().unwrap();
    assert!(
        !item_part.contains("shorter"),
        "the request is not the item's data"
    );
    assert!(asked.contains("asks for a new one: shorter"), "{asked}");
    assert!(
        asked.contains("a long story about it"),
        "the draft it replaces"
    );
}
