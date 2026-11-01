//! Decision 45: a routine's agent acts alone only where its level allows,
//! reading the levels before each action.

use super::*;
use crate::business::store::Store;
use crate::objective::{Objective, Trigger};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A service that counts what reaches it; `on_hit` runs on each request.
fn service(on_hit: impl Fn(&str) + Send + 'static) -> (String, Arc<AtomicUsize>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", server.server_addr().to_ip().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            counted.fetch_add(1, Ordering::SeqCst);
            on_hit(req.url());
            req.respond(tiny_http::Response::from_string("{\"ok\":true}"))
                .unwrap();
        }
    });
    (base, hits)
}

/// A realm with a blog integration and its declared actions, and a routine
/// whose agent is the writer, at level 3 for drafts.
fn business(base: String) -> Fixture {
    let mut f = fixture();
    let action = |name: &str, path: &str, class: &str, max: Option<u8>| {
        serde_json::from_value(json!({"name": name, "method": "POST", "path": path,
            "class": class, "max_level": max}))
        .unwrap()
    };
    f.realm
        .set_integration(crate::integration::Integration {
            name: "blog".into(),
            description: String::new(),
            base_url: base,
            headers: Default::default(),
            actions: vec![
                action("create_draft", "/drafts", "reversible", None),
                action("publish", "/posts", "commits", None),
                action("reply", "/replies", "reversible", Some(0)),
            ],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    f.realm
        .objectives()
        .save(Objective {
            id: "weekly".into(),
            objective: "Write and publish".into(),
            trigger: Trigger::Manual,
            approvals: vec![],
            budget: Default::default(),
            approval_wait_minutes: 0,
            enabled: true,
            created_at: Utc::now(),
            repo: None,
            bot: Some("writer".into()),
            tools: None,
            delegates: Vec::new(),
            next_at: None,
            fingerprint: None,
            last_run: None,
            last_at: None,
            last_status: None,
        })
        .unwrap();
    std::fs::create_dir_all(f.root.path().join("agents")).unwrap();
    std::fs::write(f.root.path().join("agents/writer.md"), "Write.").unwrap();
    Store::of(&f.realm)
        .unwrap()
        .set_level("writer", "blog.create_draft", 3, 3, "founder", "")
        .unwrap();
    f
}

fn post(id: &str, path: &str) -> ModelResponse {
    call(
        id,
        "http_request",
        json!({"integration": "blog", "method": "POST", "url": path}),
    )
}

fn routine_run(f: &Fixture, script: Vec<ModelResponse>) -> State {
    let (hub, _, _) = make_hub(&f.home, script);
    let (mut state, _) = hub
        .start_objective(&f.realm, "weekly", "manual", None, Utc::now())
        .unwrap();
    hub.drive(
        &f.realm,
        &mut state,
        config(20),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    state
}

#[test]
fn a_routine_acts_alone_only_where_its_level_allows() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    let state = routine_run(
        &f,
        vec![
            post("1", "drafts"),
            post("2", "posts"),
            post("3", "replies"),
            post("4", "admin"),
            done("done"),
        ],
    );
    let result = |i: usize| state.events[i].result.clone();
    assert_eq!(
        state.events[0].status,
        crate::state::EventStatus::Ok,
        "{}",
        result(0)
    );
    // Publishing commits: proposed, never done by the agent.
    assert!(
        result(1).contains("proposed to the founder"),
        "{}",
        result(1)
    );
    // A ceiling of 0: prepared for the founder.
    assert!(
        result(2).contains("prepared for the founder"),
        "{}",
        result(2)
    );
    // Not a declared action of the blog: refused.
    assert!(
        result(3).contains("only its declared actions"),
        "{}",
        result(3)
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "only the draft reached the blog"
    );
}

#[test]
fn a_demotion_stops_a_routine_already_running() {
    let db = Arc::new(Mutex::new(None::<std::path::PathBuf>));
    let path = db.clone();
    let (base, hits) = service(move |_| {
        // The founder, or an incident, takes drafts down while the run goes.
        let p = path.lock().unwrap().clone().unwrap();
        crate::business::store::Store::open(&p)
            .unwrap()
            .set_level("writer", "blog.create_draft", 1, 3, "incident", "")
            .unwrap();
    });
    let f = business(base);
    *db.lock().unwrap() = Some(f.realm.dir().join(crate::business::store::FILE));
    let state = routine_run(
        &f,
        vec![post("1", "drafts"), post("2", "drafts"), done("done")],
    );
    assert_eq!(state.events[0].status, crate::state::EventStatus::Ok);
    assert!(
        state.events[1].result.contains("proposed to the founder"),
        "{}",
        state.events[1].result
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn a_run_the_founder_starts_is_asked_as_before() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    let (hub, _, _) = make_hub(&f.home, vec![post("1", "posts"), done("done")]);
    let state = hub
        .dispatch(
            &f.realm,
            "Publish it",
            config(10),
            hooks(&mut NoObserver, true),
        )
        .unwrap();
    assert_eq!(
        state.events[0].status,
        crate::state::EventStatus::Ok,
        "{}",
        state.events[0].result
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

/// Decision 45: what nobody declared commits, so an agent trusted with one
/// action cannot act through an undeclared integration or a public URL;
/// and level 2 waits out its window.
#[test]
fn undeclared_calls_are_proposed_and_level_two_waits() {
    let (base, hits) = service(|_| {});
    let mut f = business(base.clone());
    f.realm
        .set_integration(crate::integration::Integration {
            name: "stats".into(),
            description: String::new(),
            base_url: base.clone(),
            headers: Default::default(),
            actions: Vec::new(),
        })
        .unwrap();
    Store::of(&f.realm)
        .unwrap()
        .set_level("writer", "blog.create_draft", 2, 3, "founder", "")
        .unwrap();
    let state = routine_run(
        &f,
        vec![
            call(
                "1",
                "http_request",
                json!({"integration": "stats", "method": "POST", "url": "x"}),
            ),
            call(
                "2",
                "http_request",
                json!({"integration": "stats", "url": "y"}),
            ),
            call(
                "3",
                "http_request",
                json!({"method": "POST", "url": format!("{base}/x")}),
            ),
            post("4", "drafts"),
            done("done"),
        ],
    );
    for e in &state.events[..3] {
        assert!(e.result.contains("proposed to the founder"), "{}", e.result);
    }
    // Level 2 waits out its cancellation window.
    assert!(
        state.events[3]
            .result
            .contains("queued: done in 30 minutes"),
        "{}",
        state.events[3].result
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "nothing reached the services"
    );
}

/// Decision 51: a routine's commands are offline, so a command cannot reach
/// outside around the levels; a run the founder starts keeps the network.
#[test]
fn a_routines_commands_cannot_reach_the_network() {
    if !crate::sandbox::available() {
        return;
    }
    let (base, hits) = service(|_| {});
    let mut f = business(base.clone());
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();
    let curl = |id: &str| {
        call(
            id,
            "execute_command",
            json!({"command": format!("curl -s -m 5 -X POST {base}/drafts")}),
        )
    };
    routine_run(&f, vec![curl("1"), done("done")]);
    assert_eq!(hits.load(Ordering::SeqCst), 0, "the routine's curl got out");
    let (hub, _, _) = make_hub(&f.home, vec![curl("1"), done("done")]);
    hub.dispatch(
        &f.realm,
        "Post it",
        config(10),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the founder's run keeps the network"
    );
}

/// Decision 45: an action a routine's mission proposed is done once the
/// founder approves it, exactly as proposed, and only once.
#[test]
fn an_approved_proposal_is_done_once() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    let state = routine_run(&f, vec![post("1", "posts"), done("done")]);
    assert!(
        state.events[0]
            .result
            .contains("proposed to the founder for approval (item 1)"),
        "{}",
        state.events[0].result
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let approve = || {
        hub.answer_item(
            &f.realm,
            1,
            None,
            crate::business::store::queue::Answer::Approve,
            "app",
            5,
        )
        .unwrap()
    };
    let (item, report) = approve();
    assert_eq!(item.status, "done", "{report:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let (item, report) = approve();
    assert_eq!((item.status.as_str(), report), ("done", None));
    assert_eq!(hits.load(Ordering::SeqCst), 1, "done once");
}

/// Decision 45: level 2 is done once its window passes, unless the founder
/// cancelled it.
#[test]
fn a_delayed_action_is_done_after_its_window_unless_cancelled() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    Store::of(&f.realm)
        .unwrap()
        .set_level("writer", "blog.create_draft", 2, 3, "founder", "")
        .unwrap();
    routine_run(
        &f,
        vec![post("1", "drafts"), post("2", "drafts"), done("done")],
    );
    let (hub, _, _) = make_hub(&f.home, vec![]);
    hub.answer_item(
        &f.realm,
        2,
        None,
        crate::business::store::queue::Answer::Reject("not this one".into()),
        "app",
        3,
    )
    .unwrap();
    assert!(
        hub.release_delayed(Utc::now()).unwrap().is_empty(),
        "the window is open"
    );
    let later = Utc::now() + chrono::Duration::minutes(31);
    let done = hub.release_delayed(later).unwrap();
    assert_eq!(done.len(), 1, "{done:?}");
    assert!(done[0].contains("/1: done"), "{done:?}");
    assert!(hub.release_delayed(later).unwrap().is_empty());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "once, and not the cancelled one"
    );
}

/// Decision 45: level 0 is prepared for the founder, who does it; marking
/// it done runs nothing.
#[test]
fn a_prepared_action_is_marked_done_never_run() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    routine_run(&f, vec![post("1", "replies"), done("done")]);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (item, report) = hub
        .answer_item(
            &f.realm,
            1,
            None,
            crate::business::store::queue::Answer::Approve,
            "app",
            5,
        )
        .unwrap();
    assert_eq!(item.kind, "prepared");
    assert!(report.unwrap().contains("the founder did it"));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}
