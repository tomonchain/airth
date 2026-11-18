//! Decision 153: a routine's approval step follows the founder-set level of
//! the acts it guards, end to end: a delayed item released after its window
//! or cancelled, an item that goes at once, and what still waits.

use super::*;
use crate::business::store::Store;
use crate::business::store::queue::Answer;

/// A business whose posts routine reads `titles` from a feed, has an
/// approval step, and sends each to Typefully, declared `class` with
/// `daily_limit`; and the bodies Typefully got.
fn posts(titles: &[&str], class: &str, daily_limit: Option<u32>) -> (Fixture, Mutexed) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let posted = Arc::new(Mutex::new(Vec::new()));
    let kept = posted.clone();
    let feed: String = titles
        .iter()
        .map(|t| format!("<item><title>{t}</title><link>{base}/news/{t}</link></item>"))
        .collect();
    crate::hub::pipelines::TEST_FEEDS.lock().unwrap().insert(
        format!("{base}/feed"),
        format!("<rss><channel>{feed}</channel></rss>"),
    );
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            req.as_reader().read_to_string(&mut body).unwrap();
            kept.lock().unwrap().push(body);
            req.respond(tiny_http::Response::from_string("{\"id\": 1}"))
                .unwrap();
        }
    });
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/post-writer.md".to_string(), "Draft posts.".to_string()),
        ("routines/posts.json".into(), json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": [format!("{base}/feed")], "dedupe": "link"},
            {"id": "ok", "type": "approval", "show": "title", "editable": "title"},
            {"id": "send", "type": "act", "action": "typefully.create_draft", "fields": {"content": "{{title}}"}}
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
            actions: vec![
                serde_json::from_value(
                    json!({"name": "create_draft", "method": "POST", "path": "/v1/drafts",
                "fields": [{"name": "content", "place": "body"}], "class": class,
                "daily_limit": daily_limit}),
                )
                .unwrap(),
            ],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    (f, posted)
}

type Mutexed = Arc<Mutex<Vec<String>>>;

fn set(f: &Fixture, level: u8, ceiling: u8) {
    Store::of(&f.realm)
        .unwrap()
        .set_level(
            "post-writer",
            "typefully.create_draft",
            level,
            ceiling,
            "founder",
            "",
        )
        .unwrap();
}

fn pending(f: &Fixture) -> Vec<crate::business::store::queue::Item> {
    Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap()
}

fn mode(hub: &Hub, f: &Fixture) -> serde_json::Value {
    let graph = hub
        .with_routine_checks(&f.realm, |c| crate::routine::load(c, "posts"))
        .unwrap();
    let levels = hub.step_levels(&f.realm).unwrap();
    json!(hub.approval_steps(&graph, &levels).unwrap())["ok"].clone()
}

/// Level 2, set by the founder: the item waits its window in Needs you,
/// the founder's edit stays, and once the window passes the routine goes on
/// as if approved, the act counted as done alone.
#[test]
fn a_step_at_level_two_goes_after_its_window_with_the_founders_edit() {
    let (f, posted) = posts(&["Flows"], "reversible", None);
    set(&f, 2, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    assert_eq!(mode(&hub, &f), json!({"mode": "delay", "minutes": 30}));
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    let queued = pending(&f);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].kind, "delayed");
    assert!(posted.lock().unwrap().is_empty(), "not before the window");

    hub.answer_item(
        &f.realm,
        queued[0].id,
        None,
        Answer::Edit("Flows, calmly.".into()),
        "app",
        4,
    )
    .unwrap();
    assert_eq!(pending(&f)[0].kind, "delayed", "an edit does not end it");
    assert!(hub.release_delayed(Utc::now()).unwrap().is_empty());
    let done = hub
        .release_delayed(Utc::now() + chrono::Duration::minutes(31))
        .unwrap();
    assert_eq!(done.len(), 1, "{done:?}");
    assert!(done[0].contains(": done:"), "{done:?}");
    assert_eq!(
        *posted.lock().unwrap(),
        vec![r#"{"content":"Flows, calmly."}"#]
    );
    // Done alone, not as the founder's approval: it counts on the limit.
    let journal = Store::of(&f.realm).unwrap().journal().unwrap();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0].1, "confirmed");
    assert!(!journal[0].2, "not approved by the founder");
    assert!(pending(&f).is_empty());
    assert!(
        hub.release_delayed(Utc::now() + chrono::Duration::minutes(62))
            .unwrap()
            .is_empty(),
        "once"
    );
}

/// Cancel is a rejection, and an incident: what the step guards comes
/// down a level (decision 45).
#[test]
fn a_cancelled_step_does_nothing_and_demotes_what_it_guards() {
    let (f, posted) = posts(&["Flows"], "reversible", None);
    set(&f, 2, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    let id = pending(&f)[0].id;
    let (item, said) = hub
        .answer_item(&f.realm, id, None, Answer::Reject("no".into()), "app", 2)
        .unwrap();
    assert_eq!(item.status, "rejected");
    assert_eq!(
        said.as_deref(),
        Some("cancelled; post-writer now takes typefully.create_draft at level 1 (was 2)")
    );
    assert!(
        hub.release_delayed(Utc::now() + chrono::Duration::minutes(31))
            .unwrap()
            .is_empty()
    );
    assert!(posted.lock().unwrap().is_empty());
    assert_eq!(mode(&hub, &f), json!({"mode": "ask"}));
}

/// Two acts of one action after the step are one action: a cancel brings
/// it down one level, not two.
#[test]
fn a_cancel_demotes_an_action_once_however_many_acts_use_it() {
    let (f, posted) = posts(&["Flows"], "reversible", None);
    let path = f.root.path().join("routines/posts.json");
    let mut graph: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    graph["blocks"].as_array_mut().unwrap().push(json!(
        {"id": "again", "type": "act", "action": "typefully.create_draft", "fields": {"content": "{{title}} (2)"}}
    ));
    std::fs::write(&path, graph.to_string()).unwrap();
    set(&f, 2, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    let queued = pending(&f);
    assert_eq!(queued[0].kind, "delayed");
    assert_eq!(
        queued[0].source["guards"],
        json!(["typefully.create_draft", "typefully.create_draft"]),
        "both acts guarded"
    );
    let (_, said) = hub
        .answer_item(
            &f.realm,
            queued[0].id,
            None,
            Answer::Reject("no".into()),
            "app",
            2,
        )
        .unwrap();
    assert_eq!(
        said.as_deref(),
        Some("cancelled; post-writer now takes typefully.create_draft at level 1 (was 2)")
    );
    let level = Store::of(&f.realm)
        .unwrap()
        .level("post-writer", "typefully.create_draft", 3, 3)
        .unwrap();
    assert_eq!(level, 1);
    assert!(posted.lock().unwrap().is_empty());
}

/// Level 3, set by the founder: no item waits; the act is done at once
/// and journaled, and the connector's daily limit still holds what goes
/// past it for the founder.
#[test]
fn a_step_at_level_three_goes_at_once_under_the_daily_limit() {
    let (f, posted) = posts(&["Flows", "Ebbs"], "reversible", Some(1));
    set(&f, 3, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    assert_eq!(mode(&hub, &f), json!({"mode": "go"}));
    let report = hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    assert!(report.contains("went on without asking"), "{report}");
    assert_eq!(posted.lock().unwrap().len(), 1, "{report}");
    // Done alone, at the agent's level: not as the founder's approval.
    let journal = Store::of(&f.realm).unwrap().journal().unwrap();
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].1, "confirmed");
    assert!(!journal[0].2, "not approved by the founder");
    let queued = pending(&f);
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert_eq!(queued[0].kind, "approval");
    assert!(
        queued[0].title.contains("daily limit reached"),
        "{}",
        queued[0].title
    );
}

/// Decision 45: an item its step let go after the window still counts on
/// the connector's daily limit, so what passes it waits for the founder.
#[test]
fn a_released_step_still_meets_the_daily_limit() {
    let (f, posted) = posts(&["Flows", "Ebbs"], "reversible", Some(1));
    set(&f, 2, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    assert_eq!(pending(&f).len(), 2);
    let done = hub
        .release_delayed(Utc::now() + chrono::Duration::minutes(31))
        .unwrap();
    assert_eq!(done.len(), 2, "{done:?}");
    assert_eq!(posted.lock().unwrap().len(), 1, "{done:?}");
    let queued = pending(&f);
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert_eq!(queued[0].kind, "approval");
    assert!(queued[0].title.contains("daily limit reached"));
}

/// The act reads its level again once the window passes: one the founder
/// lowered meanwhile holds it for their approval (decision 49).
#[test]
fn a_level_lowered_during_the_window_holds_the_act() {
    let (f, posted) = posts(&["Flows"], "reversible", None);
    set(&f, 2, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    set(&f, 1, 3);
    let done = hub
        .release_delayed(Utc::now() + chrono::Duration::minutes(31))
        .unwrap();
    assert_eq!(done.len(), 1, "{done:?}");
    assert!(posted.lock().unwrap().is_empty(), "{done:?}");
    let queued = pending(&f);
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert_eq!(queued[0].kind, "approval");
    assert_eq!(queued[0].action.as_deref(), Some("typefully.create_draft"));
}

/// Decision 96: a reversible action starts at level 3, but a starting
/// level never frees an approval step; nor does a level above a ceiling
/// that came down since.
#[test]
fn a_starting_level_or_a_lowered_ceiling_keeps_the_step_waiting() {
    let (f, posted) = posts(&["Flows"], "reversible", None);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    assert_eq!(mode(&hub, &f), json!({"mode": "ask"}));
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    assert_eq!(pending(&f)[0].kind, "approval");
    assert!(posted.lock().unwrap().is_empty());

    // Set at 3 while it was reversible, now classed as committing.
    let (f, posted) = posts(&["Flows"], "commits", None);
    set(&f, 3, 3);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    assert_eq!(mode(&hub, &f), json!({"mode": "ask"}));
    hub.run_routine_by_hand(&f.realm, "posts").unwrap();
    assert_eq!(pending(&f)[0].kind, "approval");
    assert!(posted.lock().unwrap().is_empty());
}
