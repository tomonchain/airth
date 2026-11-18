//! Decision 151: a routine's AI blocks asked again about what the founder
//! answered, as a dry run, and compared by code.

use super::*;
use crate::business::store::Store;
use crate::business::store::queue::Answer;

/// A posts routine that picks, drafts and checks, its act aimed at a
/// server that keeps every request it gets.
pub(super) fn routine(f: &mut Fixture) -> Arc<Mutex<Vec<String>>> {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = got.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            kept.lock().unwrap().push(req.url().to_string());
            req.respond(tiny_http::Response::from_string("{}")).unwrap();
        }
    });
    let root = f.root.path().to_path_buf();
    let schema = |field: &str, kind: &str| {
        json!({"type": "object", "required": [field], "properties": {field: {"type": kind}}})
            .to_string()
    };
    for (path, text) in [
        ("agents/post-writer.md".to_string(), "Draft posts.".to_string()),
        ("prompts/pick.md".into(), "Keep what matters.".into()),
        ("prompts/pick.schema.json".into(), schema("relevant", "boolean")),
        ("prompts/post.md".into(), "Draft a post.".into()),
        ("prompts/post.schema.json".into(), schema("draft", "string")),
        ("prompts/check.md".into(), "Check the draft.".into()),
        ("prompts/check.schema.json".into(), schema("flag", "string")),
        ("routines/posts.json".into(), json!({"agent": "post-writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": [format!("{base}/feed")], "dedupe": "link"},
            {"id": "pick", "type": "ai", "prompt": "prompts/pick.md", "keep": "relevant"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "check", "type": "ai", "prompt": "prompts/check.md", "as": "check"},
            {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft", "flag": "check"},
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
    got
}

/// An item the routine sent to Needs you, as the engine leaves it there.
pub(super) fn drafted(store: &mut Store, id: &str, draft: &str) -> i64 {
    let args = json!({"title": id, "link": format!("https://example.com/{id}"), "source_id": id,
        "answer": {"draft": draft}, "check": {"flag": "none"}, "model_call": 7});
    store
        .propose(&crate::business::store::autonomy::NewItem {
            kind: "approval",
            title: draft,
            agent: Some("post-writer"),
            routine: Some("posts"),
            run: Some("r1"),
            action: None,
            args: &args.to_string(),
            execute_at: None,
            source: r#"{"block": "ok", "show": "answer.draft", "editable": "answer.draft"}"#,
            model_call: None,
        })
        .unwrap()
}

#[test]
fn a_routines_ai_blocks_are_measured_against_what_the_founder_decided() {
    let mut f = fixture();
    let sent = routine(&mut f);
    let mut store = Store::of(&f.realm).unwrap();
    let a = drafted(&mut store, "a", "Draft A");
    store.answer(a, None, Answer::Approve, "app", 1).unwrap();
    let b = drafted(&mut store, "b", "Draft B");
    store
        .answer(b, None, Answer::Edit("Mine B".into()), "app", 1)
        .unwrap();
    store.answer(b, None, Answer::Approve, "app", 1).unwrap();
    let c = drafted(&mut store, "c", "Draft C");
    store
        .answer(c, None, Answer::Reject("off topic".into()), "app", 1)
        .unwrap();
    let before = store
        .queue(&["pending", "approved", "done", "failed", "rejected"])
        .unwrap();
    // Newest first, each block in turn: the pick drops C and A, keeps B;
    // then three new drafts. The check is not asked.
    let (hub, seen, systems) = make_hub(
        &f.home,
        vec![
            done(r#"{"relevant": false}"#),
            done(r#"{"relevant": true}"#),
            done(r#"{"relevant": false}"#),
            done(r#"{"draft": "New C"}"#),
            done(r#"{"draft": "New B"}"#),
            done(r#"{"draft": "New A"}"#),
        ],
    );

    let eval = hub.evaluate_routine(&f.realm, "posts", 20).unwrap();

    assert_eq!(eval.cases, 3);
    assert_eq!(eval.blocks.len(), 2, "{eval:?}");
    let pick = &eval.blocks[0];
    assert_eq!((pick.id.as_str(), pick.kind), ("pick", "keeps"));
    let score = pick.score.as_ref().unwrap();
    assert_eq!(
        (
            score.wanted,
            score.wanted_kept,
            score.rejected,
            score.rejected_dropped
        ),
        (2, 1, 1, 1)
    );
    assert!((score.rate.unwrap() - 2.0 / 3.0).abs() < 1e-9);
    assert!(
        (score.before.unwrap() - 2.0 / 3.0).abs() < 1e-9,
        "before, it kept them all"
    );
    let draft = &eval.blocks[1];
    assert_eq!((draft.id.as_str(), draft.kind), ("draft", "drafts"));
    assert!(draft.score.is_none(), "a draft is not scored");
    let b_case = &draft.cases[1];
    assert_eq!(
        (
            b_case.item,
            b_case.founder,
            b_case.draft.as_str(),
            b_case.edited.as_str(),
            b_case.new.as_deref()
        ),
        (b, "edit", "Draft B", "Mine B", Some("New B"))
    );
    assert_eq!(draft.cases[0].said, "off topic");
    assert!(
        eval.skipped.iter().any(|s| s.starts_with("check:")),
        "{:?}",
        eval.skipped
    );

    // Each block was asked about the item as it was read, without what the
    // blocks wrote.
    let first: String = seen.lock().unwrap()[0]
        .iter()
        .map(|m| format!("{m:?}"))
        .collect();
    assert!(first.contains(r#"\"title\":\"c\""#), "{first}");
    for written in ["Draft C", "check", "model_call"] {
        assert!(!first.contains(written), "{written}: {first}");
    }
    // The founder's words on these very drafts are not told to the block
    // being measured against them.
    assert!(
        systems
            .lock()
            .unwrap()
            .iter()
            .all(|s| !s.contains("What the founder said")),
    );

    // Nothing sent, nothing new in Needs you, every call a dry run.
    assert!(sent.lock().unwrap().is_empty());
    let store = Store::of(&f.realm).unwrap();
    assert_eq!(
        store
            .queue(&["pending", "approved", "done", "failed", "rejected"])
            .unwrap(),
        before
    );
    let calls = store.model_calls().unwrap();
    assert_eq!(calls.len(), 6);
    assert!(calls.iter().all(|c| c.dry_run && c.pipeline == "posts"));

    // At most `max` cases, the newest.
    let (hub, _, _) = make_hub(
        &f.home,
        vec![done(r#"{"relevant": true}"#), done(r#"{"draft": "New C"}"#)],
    );
    let eval = hub.evaluate_routine(&f.realm, "posts", 1).unwrap();
    assert_eq!(eval.cases, 1);
    assert_eq!(eval.blocks[1].cases[0].item, c);
}

/// A keeping block that writes its answer elsewhere lets an item go on
/// when the model cannot answer: the evaluation leaves that case out of
/// the score, and says why.
#[test]
fn a_case_the_model_could_not_answer_is_not_scored() {
    let mut f = fixture();
    routine(&mut f);
    let file = f.root.path().join("routines/posts.json");
    let graph = std::fs::read_to_string(&file).unwrap().replace(
        r#""keep":"relevant""#,
        r#""keep":"relevant","as":"verdict""#,
    );
    assert!(graph.contains("verdict"), "{graph}");
    std::fs::write(&file, graph).unwrap();
    let mut store = Store::of(&f.realm).unwrap();
    let a = drafted(&mut store, "a", "Draft A");
    store.answer(a, None, Answer::Approve, "app", 1).unwrap();
    let c = drafted(&mut store, "c", "Draft C");
    store
        .answer(c, None, Answer::Reject("off topic".into()), "app", 1)
        .unwrap();
    // C's answer does not fit, twice; A is kept. Then two drafts.
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            done("not json"),
            done("still not json"),
            done(r#"{"relevant": true}"#),
            done(r#"{"draft": "New C"}"#),
            done(r#"{"draft": "New A"}"#),
        ],
    );

    let eval = hub.evaluate_routine(&f.realm, "posts", 20).unwrap();

    let pick = &eval.blocks[0];
    assert_eq!(pick.id, "pick");
    assert_eq!(
        pick.cases.iter().map(|c| c.item).collect::<Vec<_>>(),
        vec![a]
    );
    let score = pick.score.as_ref().unwrap();
    assert_eq!((score.wanted, score.wanted_kept, score.rejected), (1, 1, 0));
    assert!(
        eval.skipped
            .iter()
            .any(|s| s.starts_with(&format!("pick: item {c}: ")) && s.contains("twice")),
        "{:?}",
        eval.skipped
    );
}

#[test]
fn a_routine_with_no_answer_yet_has_no_evaluation() {
    let mut f = fixture();
    routine(&mut f);
    let (hub, seen, _) = make_hub(&f.home, vec![]);
    let eval = hub.evaluate_routine(&f.realm, "posts", 20).unwrap();
    assert_eq!(eval.cases, 0);
    assert!(eval.blocks.is_empty());
    assert!(eval.skipped[0].contains("nothing answered"), "{eval:?}");
    assert!(seen.lock().unwrap().is_empty(), "no model call");
}
