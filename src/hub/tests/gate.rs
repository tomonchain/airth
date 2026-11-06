//! Milestone 3's gate: a routine goes from what it collects to the
//! founder's approval on Telegram, and every step is recorded.

use super::*;
use crate::business::store::Store;
use crate::hub::pipelines::TEST_FEEDS;
use crate::telegram::{self, Api};
use serde_json::Value;
use std::cell::RefCell;

/// Telegram, as the founder's phone sees it: what the bot sent.
#[derive(Default)]
struct Phone {
    sent: RefCell<Vec<Value>>,
}

impl Api for Phone {
    fn call(&self, method: &str, body: &Value) -> Result<Value> {
        if method == "sendMessage" {
            self.sent.borrow_mut().push(body.clone());
        }
        Ok(json!({"message_id": 500 + self.sent.borrow().len()}))
    }
}

/// Each request a service got: its path and body.
type Requests = Arc<Mutex<Vec<(String, String)>>>;

/// Typefully's API v2, keeping each request's path and body.
fn typefully() -> (String, Requests) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = got.clone();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            req.as_reader().read_to_string(&mut body).unwrap();
            kept.lock().unwrap().push((req.url().to_string(), body));
            req.respond(tiny_http::Response::from_string("{\"id\": 77}"))
                .unwrap();
        }
    });
    (base, got)
}

#[test]
fn a_routine_goes_from_its_feed_to_the_founders_approval_on_telegram() {
    let (base, drafts) = typefully();
    let feed = "https://news.example/feed.xml";
    TEST_FEEDS.lock().unwrap().insert(
        feed.into(),
        "<rss><channel><item><title>ETH unstaking queue empties</title>\
         <link>https://news.example/eth?utm_source=rss</link></item></channel></rss>"
            .into(),
    );
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/post-writer.md".to_string(), "Draft Acme's posts.".to_string()),
        ("prompts/post.md".into(), "Draft a post about the item.".into()),
        ("prompts/post.schema.json".into(), r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string"}}}"#.into()),
        ("routines/posts.json".into(), json!({"agent": "post-writer", "trigger": {"schedule": "every 1h"}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": [feed], "dedupe": "link"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft"},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"text": "{{answer.draft}}", "publish_at": "now"}}
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
            base_url: base,
            headers: Default::default(),
            actions: vec![serde_json::from_value(json!({
                "name": "create_draft", "method": "POST", "class": "reversible",
                "path": "/v2/social-sets/{social_set_id}/drafts",
                "fields": [{"name": "social_set_id", "place": "path", "value": "42"}, {"name": "text", "place": "body"}],
                "body": {"platforms": {"x": {"enabled": true, "posts": [{"text": "{text}"}]}}}
            }))
            .unwrap()],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![done(r#"{"draft": "The ETH unstaking queue is empty."}"#)],
    );
    let store = || Store::of(&f.realm).unwrap();

    // 1. The schedule: armed when first seen, due an hour later, once.
    let now = Utc::now();
    assert!(hub.due_routines(now).unwrap().is_empty());
    let due = hub
        .due_routines(now + chrono::Duration::minutes(61))
        .unwrap();
    assert_eq!(
        due.iter().map(|(_, n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["posts"]
    );

    // 2. The run: read, drafted, waiting for the founder; its heartbeat left.
    hub.run_scheduled_routine(&f.realm, "posts", due[0].2);
    let beats = store().heartbeats().unwrap();
    assert_eq!(beats.len(), 1);
    assert!(beats[0].0 == "routine:posts" && beats[0].1.ok, "{beats:?}");
    // With the time it was due, to tell a late start from a slow one.
    assert_eq!(beats[0].1.due_at, Some(due[0].2.to_rfc3339()));
    let calls = store().model_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].valid && !calls[0].dry_run);
    let waiting = store().queue(&["pending"]).unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].args["source_id"], "https://news.example/eth");
    assert!(
        drafts.lock().unwrap().is_empty(),
        "nothing sent before the founder"
    );

    // 3. Telegram: the item reaches the founder's phone, whole, with buttons.
    let phone = Phone::default();
    let mut bot = telegram::Config {
        chat: Some(5),
        user: Some(7),
        ..Default::default()
    };
    telegram::push(&hub, &phone, &mut bot).unwrap();
    let message = phone.sent.borrow()[0].clone();
    assert!(
        message["text"]
            .as_str()
            .unwrap()
            .contains("The ETH unstaking queue is empty.")
    );
    let item_message = bot.sent["work/1"].messages[0];

    // 4. The founder edits, as a reply to the item, then approves.
    let tap = |data: &str| json!({"update_id": 1, "callback_query": {"id": "q", "from": {"id": 7}, "message": {"chat": {"id": 5}}, "data": data}});
    telegram::handle(&hub, &phone, &mut bot, &tap("e/work/1/1")).unwrap();
    let reply = json!({"update_id": 2, "message": {"chat": {"id": 5}, "from": {"id": 7},
        "text": "ETH's unstaking queue is empty: exits are instant again.", "reply_to_message": {"message_id": item_message}}});
    telegram::handle(&hub, &phone, &mut bot, &reply).unwrap();
    telegram::handle(&hub, &phone, &mut bot, &tap("a/work/1/2")).unwrap();
    assert!(
        phone.sent.borrow().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .starts_with("work · done")
    );

    // 5. Typefully got the edited draft once, in its set, never published.
    let got = drafts.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "/v2/social-sets/42/drafts");
    let body: Value = serde_json::from_str(&got[0].1).unwrap();
    assert_eq!(
        body,
        json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "ETH's unstaking queue is empty: exits are instant again."}]}}})
    );

    // 6. Every step on record.
    let log: Vec<String> = store()
        .queue_log(1)
        .unwrap()
        .into_iter()
        .map(|(a, by, _)| format!("{a} by {by}"))
        .collect();
    assert_eq!(
        log,
        vec![
            "proposed by airth",
            "edited by founder",
            "approved by founder",
            "done by airth"
        ]
    );
    let answers = store().answers().unwrap();
    assert_eq!(answers.len(), 2);
    assert!(
        answers
            .iter()
            .all(|(item, channel)| *item == Some(1) && channel.as_deref() == Some("telegram"))
    );
    assert_eq!(
        store().journal().unwrap(),
        vec![(
            "posts/https://news.example/eth/send".to_string(),
            "confirmed".to_string(),
            true
        )]
    );
    assert_eq!(
        store().model_calls().unwrap()[0].outcome.as_deref(),
        Some("edited")
    );
    // Each block's execution, in order, for the founder to follow the run:
    // waiting at the approval, then the act once the founder answered.
    let run = waiting[0].run.clone().unwrap();
    let blocks: Vec<(String, String, i64, i64)> = store()
        .run_blocks("posts", &run)
        .unwrap()
        .into_iter()
        .map(|b| (b.block, b.outcome, b.items_in, b.items_out))
        .collect();
    let row = |b: &str, o: &str, i, out| (b.to_string(), o.to_string(), i, out);
    assert_eq!(
        blocks,
        vec![
            row("news", "ok", 1, 1),
            row("draft", "ok", 1, 1),
            row("ok", "waiting", 1, 0),
            row("send", "ok", 1, 1)
        ]
    );
    // The link is remembered: the next run does not draft it again.
    assert_eq!(
        store()
            .seen("posts", &["https://news.example/eth".into()])
            .unwrap()
            .len(),
        1
    );
}

/// Run now, from the Routines page: the routine runs as at its time (what
/// needs the founder waits in the queue) and is recorded the same way,
/// with no time it was due; a routine that does not load is refused.
#[test]
fn a_routine_run_by_hand_runs_and_is_recorded_as_at_its_time() {
    let feed = "https://news.example/by-hand.xml";
    TEST_FEEDS.lock().unwrap().insert(
        feed.into(),
        "<rss><channel><item><title>Stablecoin supply at a high</title>\
         <link>https://news.example/supply</link></item></channel></rss>"
            .into(),
    );
    let f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        (
            "agents/watcher.md".to_string(),
            "Watch the news.".to_string(),
        ),
        (
            "routines/watch.json".into(),
            json!({"agent": "watcher", "trigger": {"manual": true}, "blocks": [
                {"id": "news", "type": "read", "source": "rss", "feeds": [feed], "dedupe": "link"},
                {"id": "ok", "type": "approval", "show": "title"}
            ]})
            .to_string(),
        ),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let store = || Store::of(&f.realm).unwrap();

    hub.run_routine_by_hand(&f.realm, "watch").unwrap();
    let beats = store().heartbeats().unwrap();
    assert_eq!(beats.len(), 1);
    assert!(beats[0].0 == "routine:watch" && beats[0].1.ok, "{beats:?}");
    assert_eq!(beats[0].1.due_at, None, "run by hand, not due");
    assert_eq!(store().queue(&["pending"]).unwrap().len(), 1);

    assert!(hub.run_routine_by_hand(&f.realm, "ghost").is_err());
}

/// Decision 81: a routine that fails at its time is tried again before it
/// becomes an incident, and its heartbeat says how many times; run by hand,
/// it is tried once.
#[test]
fn a_scheduled_routine_that_fails_is_tried_three_times_then_is_an_incident() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/watcher.md".to_string(), "Watch the news.".to_string()),
        // A feed it may not reach: network requests ask, and nobody answers.
        ("routines/watch.json".into(), json!({"agent": "watcher", "trigger": {"schedule": "every 1h"}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["https://news.example/unreachable.xml"]},
            {"id": "ok", "type": "approval", "show": "title"}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let store = || Store::of(&f.realm).unwrap();

    hub.run_scheduled_routine(&f.realm, "watch", Utc::now());
    let beats = store().heartbeats().unwrap();
    assert_eq!(beats.len(), 1);
    assert!(!beats[0].1.ok, "{beats:?}");
    assert_eq!(beats[0].1.notes, "tried 3 times");
    let incidents: Vec<_> = store()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident")
        .collect();
    assert_eq!(incidents.len(), 1, "one incident, not one a try");

    assert!(hub.run_routine_by_hand(&f.realm, "watch").is_err());
    let beats = store().heartbeats().unwrap();
    assert_eq!(beats.len(), 2);
    assert_eq!(beats[1].1.notes, "", "by hand: tried once, {beats:?}");
}

/// Decision 82: a routine's model call is told the business's strategy and
/// memory before its prompt, read at each run, and the journal records the
/// prompt file's version and the essentials' together.
#[test]
fn a_routine_is_told_the_strategy_and_memory_the_founder_keeps() {
    let feed = "https://news.example/essentials.xml";
    TEST_FEEDS.lock().unwrap().insert(
        feed.into(),
        "<rss><channel><item><title>Fed holds rates</title>\
         <link>https://news.example/fed</link></item></channel></rss>"
            .into(),
    );
    let f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/writer.md".to_string(), "Write.".to_string()),
        ("prompts/post.md".into(), "Draft a post about the item.".into()),
        ("prompts/post.schema.json".into(), r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string"}}}"#.into()),
        ("routines/posts.json".into(), json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": [feed], "dedupe": "link"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft"}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    crate::business::set_strategy(&root, "Twelve articles by June.").unwrap();
    f.realm
        .memory()
        .add("No posts on weekends.", "owner")
        .unwrap();
    let (hub, _, systems) = make_hub(&f.home, vec![done(r#"{"draft": "Rates held."}"#)]);

    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    let system = systems.lock().unwrap().last().cloned().unwrap();
    assert!(system.contains("Twelve articles by June."), "{system}");
    assert!(system.contains("- No posts on weekends."), "{system}");
    let (said, task) = (
        system.find("No posts on weekends").unwrap(),
        system.find("Draft a post").unwrap(),
    );
    assert!(said < task, "the essentials come before the prompt");
    let calls = Store::of(&f.realm).unwrap().model_calls().unwrap();
    let file = crate::update::sha256_hex(b"Draft a post about the item.");
    assert!(
        calls[0].prompt_version.starts_with(&format!("{file}+")),
        "{}",
        calls[0].prompt_version
    );
}

/// A task nobody watches files what commits the business under its agent,
/// so the question shows in that agent's chat (where the founder answers it
/// and the task goes on), titled by the request it would make.
#[test]
fn an_unwatched_tasks_approval_is_filed_under_its_agent() {
    use crate::runtime::{Authority, Decision};
    let f = fixture();
    let mut state = crate::state::State::new("Optimise the landing page", None);
    state.agent = Some("developer".into());
    let mut authority = crate::hub::autonomy::FounderAuthority::for_run(&f.realm, &state).unwrap();
    let decision = authority
        .decide(
            "http_request",
            "github.undeclared",
            &json!({"integration": "github", "url": "/user/repos"}),
            "c1",
            false,
        )
        .unwrap();
    assert!(matches!(decision, Decision::Stop(_)));
    let items = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].agent.as_deref(), Some("developer"));
    assert_eq!(items[0].title, "github.undeclared: GET /user/repos");
}
