//! Decision 45: a routine's agent acts alone only where its level allows,
//! reading the levels before each action.

use super::*;
use crate::business::store::Store;
use crate::objective::{Objective, Trigger};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A service that counts what reaches it; `on_hit` runs on each request.
pub(super) fn service(on_hit: impl Fn(&str) + Send + 'static) -> (String, Arc<AtomicUsize>) {
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
pub(super) fn business(base: String) -> Fixture {
    let mut f = fixture();
    let action = |name: &str, path: &str, class: &str, max: Option<u8>| {
        serde_json::from_value(json!({"name": name, "method": "POST", "path": path,
            "class": class, "max_level": max}))
        .unwrap()
    };
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
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

pub(super) fn post(id: &str, path: &str) -> ModelResponse {
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

/// Decision 45: what nobody declared commits, so an agent trusted with one
/// action cannot act through an undeclared integration or a public URL;
/// and level 2 waits out its window.
#[test]
fn undeclared_calls_are_proposed_and_level_two_waits() {
    let (base, hits) = service(|_| {});
    let mut f = business(base.clone());
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
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
    // The cancellation took drafts down a level for the writer.
    let level = Store::of(&f.realm)
        .unwrap()
        .level("writer", "blog.create_draft", 3, 1)
        .unwrap();
    assert_eq!(level, 1);
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

/// A mission writing its own body cannot slip `publish_at` into a draft:
/// the runtime refuses any body that is not the declared one's shape.
#[test]
fn a_mission_cannot_add_to_a_declared_body() {
    let (base, hits) = service(|_| {});
    let mut f = business(base.clone());
    let draft: crate::actions::HttpAction = serde_json::from_value(json!({
        "name": "create_draft", "method": "POST", "class": "reversible",
        "path": "/v2/social-sets/{social_set_id}/drafts",
        "fields": [{"name": "social_set_id", "place": "path", "value": "123"}, {"name": "text", "place": "body"}],
        "body": {"platforms": {"x": {"enabled": true, "posts": [{"text": "{text}"}]}}}
    }))
    .unwrap();
    draft.validate().unwrap();
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
            name: "typefully".into(),
            description: String::new(),
            base_url: base,
            headers: Default::default(),
            actions: vec![draft],
        })
        .unwrap();
    Store::of(&f.realm)
        .unwrap()
        .set_level("writer", "typefully.create_draft", 3, 3, "founder", "")
        .unwrap();
    let body = |extra: bool| {
        let mut b = json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "Hi"}]}}});
        if extra {
            b["publish_at"] = json!("now");
        }
        call(
            "1",
            "http_request",
            json!({"integration": "typefully", "method": "POST",
            "url": "v2/social-sets/123/drafts", "body": b.to_string()}),
        )
    };
    let state = routine_run(&f, vec![body(true), body(false), done("done")]);
    assert!(
        state.events[0]
            .result
            .contains("takes only its declared body"),
        "{}",
        state.events[0].result
    );
    assert_eq!(
        state.events[1].status,
        crate::state::EventStatus::Ok,
        "{}",
        state.events[1].result
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1, "only the plain draft went");
}

/// An action's outcome that cannot be recorded is an incident: the journal
/// no longer says what happened.
#[test]
fn an_unrecorded_outcome_is_an_incident() {
    use crate::runtime::Authority;
    let (base, _) = service(|_| {});
    let f = business(base);
    let mut authority =
        crate::hub::autonomy::BusinessAuthority::for_graph(&f.realm, "weekly", "writer", "r1")
            .unwrap();
    authority.finish(9999, None);
    let queued = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        (queued[0].kind.as_str(), queued[0].title.as_str()),
        ("incident", "An action's outcome was not recorded")
    );
}

/// Decision 103: a call of an MCP tool the founder approved in Needs you is
/// done by Airth, once, with the arguments proposed; one that fails is an
/// incident, never tried again.
#[test]
fn an_approved_mcp_call_is_done_once_and_a_failed_one_is_not_retried() {
    let f = fixture();
    let root = f.realm.root().unwrap();
    // A stdio server that answers initialize and tools/call, and notes each
    // call it gets in calls.txt.
    std::fs::write(
        root.join("server.sh"),
        r#"while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"blog","version":"1"}}}\n' "$id" ;;
    *'"method":"tools/call"'*'"fail"'*)
      echo call >> calls.txt
      printf '{"jsonrpc":"2.0","id":%s,"result":{"isError":true,"content":[{"type":"text","text":"refused"}]}}\n' "$id" ;;
    *'"method":"tools/call"'*)
      echo call >> calls.txt
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"published"}]}}\n' "$id" ;;
  esac
done
"#,
    )
    .unwrap();
    let mcp = crate::tools::mcp::McpStore::new(&f.realm.skills_dir());
    mcp.save("blog", "sh server.sh", "").unwrap();
    mcp.update("blog", |s| {
        s.tools = vec![crate::tools::mcp::McpToolSpec {
            name: "publish".into(),
            description: "Publish".into(),
            input_schema: json!({"type": "object"}),
        }];
    })
    .unwrap();
    let propose = |text: &str| {
        Store::of(&f.realm)
            .unwrap()
            .propose(&crate::business::store::autonomy::NewItem {
                kind: "approval",
                title: "writer: blog.publish",
                agent: Some("writer"),
                routine: None,
                run: Some("r1"),
                action: Some("blog.publish"),
                args: &json!({"text": text}).to_string(),
                execute_at: None,
                source: r#"{"tool": "mcp__blog__publish"}"#,
                model_call: None,
            })
            .unwrap()
    };
    let calls = || {
        std::fs::read_to_string(root.join("calls.txt"))
            .unwrap_or_default()
            .lines()
            .count()
    };
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let approve = |id: i64| {
        hub.answer_item(
            &f.realm,
            id,
            None,
            crate::business::store::queue::Answer::Approve,
            "app",
            5,
        )
        .unwrap()
    };

    let ok = propose("Hello");
    let (item, report) = approve(ok);
    assert_eq!(item.status, "done", "{report:?}");
    assert!(report.unwrap().contains("blog.publish: published"));
    assert_eq!(calls(), 1);
    let (item, again) = approve(ok);
    assert_eq!((item.status.as_str(), again), ("done", None));
    assert_eq!(calls(), 1, "done once");

    let bad = propose("fail");
    let (item, _) = approve(bad);
    assert_eq!(item.status, "failed");
    assert_eq!(calls(), 2);
    let incidents: Vec<_> = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident")
        .collect();
    assert_eq!(incidents.len(), 1);
    assert_eq!(
        incidents[0].args["retry"],
        serde_json::Value::Null,
        "never retried"
    );
    assert!(
        Store::of(&f.realm)
            .unwrap()
            .due_retries(chrono::Utc::now() + chrono::Duration::days(1))
            .unwrap()
            .is_empty()
    );
}
