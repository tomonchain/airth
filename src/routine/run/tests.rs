use super::*;
use crate::business::store::FILE;
use crate::models::{ContentBlock, ModelResponse, StopReason};
use crate::prices::Prices;
use serde_json::json;

/// The same article reached through two campaigns is one source.
#[test]
fn a_link_is_known_without_its_tracking() {
    let a = "https://e.com/news/1?utm_source=x&utm_medium=social&id=7#comments";
    let b = "https://e.com/news/1?id=7&fbclid=abc&utm_campaign=z";
    assert_eq!(canonical(a), "https://e.com/news/1?id=7");
    assert_eq!(canonical(a), canonical(b));
    assert_eq!(
        canonical("https://e.com/x?utm_source=feed"),
        "https://e.com/x"
    );
    assert_eq!(canonical("urn:uuid:42"), "urn:uuid:42");
    let item = |link: &str| -> Item { serde_json::from_value(json!({"link": link})).unwrap() };
    let mut one = item(a);
    one.insert(SOURCE.into(), source_id(&item(a)).into());
    let mut two = item(b);
    two.insert(SOURCE.into(), source_id(&item(b)).into());
    assert_eq!(key("posts", &one, "send"), key("posts", &two, "send"));
}

#[test]
fn an_acts_key_is_the_source_not_the_content() {
    let item = |draft: &str| -> Item {
        serde_json::from_value(json!({"source_id": "https://e.com/1", "answer": {"draft": draft}}))
            .unwrap()
    };
    assert_eq!(
        key("posts", &item("one"), "send"),
        "posts/https://e.com/1/send"
    );
    assert_eq!(
        key("posts", &item("one"), "send"),
        key("posts", &item("another"), "send")
    );
}

/// Decision 49: a dry run reads and asks for real, and only prepares acts,
/// keyed by each item's source, with the URL they would call.
#[test]
fn a_dry_run_reads_and_judges_and_only_prepares_acts() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/post.md", "Draft a post."),
        (
            "prompts/post.schema.json",
            r#"{"type": "object", "required": ["relevant", "score", "draft"],
            "properties": {"relevant": {"type": "boolean"}, "score": {"type": "number"}, "draft": {"type": "string"}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"], "dedupe": "link"},
        {"id": "draft", "type": "ai", "prompt": "prompts/post.md", "keep": "relevant"},
        {"id": "good", "type": "condition", "if": {"field": "answer.score", "op": ">=", "value": 3}, "else": "skip"},
        {"id": "ok", "type": "approval", "show": "answer.draft"},
        {"id": "send", "type": "act", "action": "typefully.create_draft",
            "fields": {"set": "hedgen", "content": "{{answer.draft}}"}, "next": null},
        {"id": "skip", "type": "act", "action": "airth.report", "fields": {"title": "Left out: {{title}}"}}
    ]}))
    .unwrap();
    let typefully = Integration {
        name: "typefully".into(),
        description: String::new(),
        base_url: "https://api.typefully.com".into(),
        headers: Default::default(),
        actions: vec![serde_json::from_value(json!({"name": "create_draft", "method": "POST",
            "path": "/v1/sets/{set}/drafts", "fields": [{"name": "set", "place": "path"}, {"name": "content", "place": "body"}]}))
        .unwrap()],
    };
    let mut fetched = 0;
    let mut fetch = |_: &str| {
        fetched += 1;
        Ok(crate::pipeline::run::Fetched {
            body: "<rss><channel><item><title>Flows</title><link>https://e.com/1</link></item>\
                   <item><title>Noise</title><link>https://e.com/2</link></item></channel></rss>"
                .into(),
            truncated: false,
        })
    };
    // A draft longer than a report shows.
    let long = json!({"relevant": true, "score": 5, "draft": format!("ETH left the queue. {}", "x".repeat(500))}).to_string();
    let mut answers = vec![r#"{"relevant": true, "score": 1, "draft": "Meh."}"#, &long];
    let mut ask = |_: &crate::models::ModelRequest| {
        Ok(ModelResponse {
            content: vec![ContentBlock::Text {
                text: answers.pop().unwrap().into(),
            }],
            stop_reason: StopReason::EndTurn,
            model: "m".into(),
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        })
    };
    let mut io = Io {
        fetch: &mut fetch,
        ask: &mut ask,
        prices: Prices::default(),
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let integrations = [typefully];
    let report = dry_run(
        "posts",
        &graph,
        Dry {
            root: root.path(),
            store: &mut store,
            io: &mut io,
            integrations: &integrations,
        },
        Utc::now(),
    )
    .unwrap()
    .to_string();
    assert!(
        report.contains("would ask the founder: ETH left the queue."),
        "{report}"
    );
    assert!(
        report.contains("would typefully.create_draft [posts/https://e.com/1/send]: POST https://api.typefully.com/v1/sets/hedgen/drafts"),
        "{report}"
    );
    assert!(
        report.contains("would airth.report [posts/https://e.com/2/skip]"),
        "{report}"
    );
    assert!(report.contains("good: 2 in, 2 out"), "{report}");
    assert!(
        report.contains("2 read, 0 too old, 0 already seen or repeated, 0 past the max: 2 kept"),
        "{report}"
    );
    assert!(report.contains("  kept: Flows ()"), "{report}");
    assert!(
        report.contains("set aside: 0 not relevant, 0 invalid"),
        "{report}"
    );
    assert!(
        report.contains(
            "cost: $0.0000 (+2 call(s) to a model without a known price), 20 tokens in, 10 out"
        ),
        "{report}"
    );
    assert!(report.contains("Total: cost:"), "{report}");
    assert!(
        report.contains(
            "[cut here for this report: 520 characters in all; the queue shows it whole]"
        ),
        "{report}"
    );
    drop(io);
    assert_eq!(fetched, 1, "only the feed was read");
    // Its model calls are journaled as a dry run's, out of the statistics.
    let calls = store.model_calls().unwrap();
    assert!(
        calls.len() == 2 && calls.iter().all(|c| c.dry_run),
        "{calls:?}"
    );
    let week = store.week(Utc::now() - chrono::Duration::days(7)).unwrap();
    assert_eq!((week.model_calls, week.unpriced_calls), (0, 0));
}

/// Decision 49: in a real run an item waits in the queue at an approval,
/// the links read are remembered, and an item without a source id never
/// reaches an act; an approved item's acts are done as approved.
#[test]
fn a_real_run_queues_at_the_approval_and_acts_only_on_known_sources() {
    let root = tempfile::tempdir().unwrap();
    let graph: Graph = serde_json::from_value(json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"], "dedupe": "link"},
        {"id": "ok", "type": "approval", "show": "title", "editable": "title"},
        {"id": "send", "type": "act", "action": "blog.publish", "fields": {"text": "{{title}}"}}
    ]}))
    .unwrap();
    let mut fetch = |_: &str| {
        Ok(crate::pipeline::run::Fetched {
            body: "<rss><channel><item><title>Flows</title><link>https://e.com/1?utm_source=x</link></item></channel></rss>".into(),
            truncated: false,
        })
    };
    let mut ask = |_: &crate::models::ModelRequest| -> anyhow::Result<ModelResponse> {
        unreachable!("no AI block")
    };
    let mut io = Io {
        fetch: &mut fetch,
        ask: &mut ask,
        prices: Prices::default(),
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let mut acted = Vec::new();
    let mut act = |action: &str, fields: &BTreeMap<String, String>, key: &str, approved: bool| {
        acted.push(format!("{action} {} {key} {approved}", fields["text"]));
        Ok("done".to_string())
    };
    let dry = Dry {
        root: root.path(),
        store: &mut store,
        io: &mut io,
        integrations: &[],
    };
    let report = run(
        "posts",
        &graph,
        dry,
        Some(Real {
            run: "r1",
            act: &mut act,
        }),
        Start::Trigger,
        Utc::now(),
    )
    .unwrap()
    .to_string();
    assert!(
        report.contains("asked the founder (item 1): Flows"),
        "{report}"
    );
    let queued = store.queue(&["pending"]).unwrap();
    assert_eq!(queued[0].source["block"], "ok");
    assert_eq!(
        store
            .seen("posts", &["https://e.com/1".into()])
            .unwrap()
            .len(),
        1
    );
    let serde_json::Value::Object(item) = queued[0].args.clone() else {
        panic!("an item")
    };
    let mut nameless = item.clone();
    nameless.insert("source_id".into(), "".into());
    nameless.insert("title".into(), "Nameless".into());
    for it in [item, nameless] {
        let dry = Dry {
            root: root.path(),
            store: &mut store,
            io: &mut io,
            integrations: &[],
        };
        let start = Start::After {
            block: "ok".into(),
            item: it,
        };
        let report = run(
            "posts",
            &graph,
            dry,
            Some(Real {
                run: "r1",
                act: &mut act,
            }),
            start,
            Utc::now(),
        )
        .unwrap()
        .to_string();
        assert!(
            !report.contains("Nameless")
                || report.contains("set aside: 'Nameless' has no source id"),
            "{report}"
        );
    }
    drop(io);
    assert_eq!(
        acted,
        vec!["blog.publish Flows posts/https://e.com/1/send true"]
    );
}

/// Milestone 4: the weekly report reads the business's own records and
/// puts them into words by code; its act is keyed by the week, so it
/// reports once a week however often it is retried.
#[test]
fn a_weekly_report_reads_the_week_and_reports_it_once() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("business")).unwrap();
    std::fs::write(
        root.path().join("business/goal.md"),
        "---\nbaseline_minutes: 300\n---\n",
    )
    .unwrap();
    let graph: Graph = serde_json::from_value(
        json!({"agent": "chief", "trigger": {"schedule": "mon 08:00"}, "blocks": [
            {"id": "week", "type": "read", "source": "business", "records": "week"},
            {"id": "report", "type": "act", "action": "airth.report",
             "fields": {"title": "Week {{week}}", "text": "{{summary}}"}}
        ]}),
    )
    .unwrap();
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    store
        .record_time(&crate::business::store::time::Entry {
            at: Utc::now(),
            activity: "reported".into(),
            seconds: 45 * 60,
            note: String::new(),
        })
        .unwrap();
    let (mut fetch, mut ask) = (
        |_: &str| -> Result<crate::pipeline::run::Fetched> {
            panic!("a business read fetches nothing")
        },
        |_: &crate::models::ModelRequest| -> Result<ModelResponse> {
            panic!("a weekly report asks no model")
        },
    );
    let mut io = Io {
        fetch: &mut fetch,
        ask: &mut ask,
        prices: Prices::default(),
    };
    let now: DateTime<Utc> = "2026-11-02T08:00:00Z".parse().unwrap();
    let mut report = |store: &mut Store| {
        dry_run(
            "weekly-report",
            &graph,
            Dry {
                root: root.path(),
                store,
                io: &mut io,
                integrations: &[],
            },
            now,
        )
        .unwrap()
        .to_string()
    };
    let first = report(&mut store);
    for line in [
        "the week 2026-W45 from the business's records",
        "would airth.report [weekly-report/week/2026-W45/report]:",
        "title: Week 2026-W45",
        "Week 2026-W45 (26 Oct to 2 Nov)",
        "Your time: 45 min (baseline: 5 h 00 a week, so this week took 15% of it)",
        "Your answers: 0 approved, 0 edited, 0 rejected",
        "Model cost: $0.00 for 0 model call(s)",
        "By routine:\n      none ran",
    ] {
        assert!(first.contains(line), "{line}\n{first}");
    }
    // Retried the same week, it has the same key: it would act once.
    assert!(report(&mut store).contains("[weekly-report/week/2026-W45/report]"));
}

/// Decision 58: a check is an AI block whose answer goes under its own name,
/// so the draft stays; its flag goes with the item to the queue. A check
/// that could not answer leaves the item going on, flagged as unchecked.
#[test]
fn a_check_flags_the_item_and_keeps_its_draft() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/post.md", "Draft a post."),
        (
            "prompts/post.schema.json",
            r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string"}}}"#,
        ),
        ("prompts/check.md", "Check the draft."),
        (
            "prompts/check.schema.json",
            r#"{"type": "object", "required": ["flag", "reason"], "properties": {"flag": {"enum": ["none", "note", "blocking"]}, "reason": {"type": "string"}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"]},
        {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
        {"id": "check", "type": "ai", "prompt": "prompts/check.md", "as": "check"},
        {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft", "flag": "check"}
    ]}))
    .unwrap();
    let mut fetch = |_: &str| {
        Ok(crate::pipeline::run::Fetched {
            body: format!(
                "<rss><channel>{}</channel></rss>",
                ["Flows", "Buy", "Odd"]
                    .iter()
                    .map(|t| format!(
                        "<item><title>{t}</title><link>https://e.com/{t}</link></item>"
                    ))
                    .collect::<String>()
            ),
            truncated: false,
        })
    };
    let mut ask = |req: &crate::models::ModelRequest| -> Result<ModelResponse> {
        let seen = format!("{req:?}");
        let title = ["Flows", "Buy", "Odd"]
            .into_iter()
            .find(|t| seen.contains(&format!("https://e.com/{t}")))
            .unwrap();
        let text = if seen.contains("Check the draft.") {
            match title {
                "Flows" => r#"{"flag": "none", "reason": ""}"#.to_string(),
                "Buy" => r#"{"flag": "blocking", "reason": "a call to buy"}"#.to_string(),
                _ => "I cannot say.".to_string(),
            }
        } else {
            json!({"draft": format!("{title}, drafted")}).to_string()
        };
        Ok(ModelResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            model: "m".into(),
            input_tokens: 1,
            output_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        })
    };
    let mut io = Io {
        fetch: &mut fetch,
        ask: &mut ask,
        prices: Prices::default(),
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let mut act = |_: &str, _: &BTreeMap<String, String>, _: &str, _: bool| -> Result<String> {
        unreachable!("no act")
    };
    let report = run(
        "posts",
        &graph,
        Dry {
            root: root.path(),
            store: &mut store,
            io: &mut io,
            integrations: &[],
        },
        Some(Real {
            run: "r1",
            act: &mut act,
        }),
        Start::Trigger,
        Utc::now(),
    )
    .unwrap()
    .to_string();
    assert!(report.contains("left unchecked"), "{report}");
    assert!(
        report.contains("[flagged blocking: a call to buy]: Buy, drafted"),
        "{report}"
    );
    let items = store.queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 3, "{report}");
    let by = |t: &str| {
        items
            .iter()
            .find(|i| i.args["answer"]["draft"] == format!("{t}, drafted"))
            .unwrap()
    };
    assert_eq!(by("Flows").flag, None);
    assert_eq!(
        (by("Buy").flag.as_deref(), by("Buy").flag_reason.as_deref()),
        (Some("blocking"), Some("a call to buy"))
    );
    assert_eq!(by("Odd").flag.as_deref(), Some("note"));
    assert!(
        by("Odd")
            .flag_reason
            .as_deref()
            .unwrap()
            .contains("not checked")
    );
    // The founder's answer is the draft's outcome, not the check's.
    let calls = store.model_calls().unwrap();
    let draft_calls: Vec<i64> = calls
        .iter()
        .filter(|c| c.answer.as_deref().is_some_and(|a| a.contains("drafted")))
        .map(|c| c.id)
        .collect();
    assert!(
        items
            .iter()
            .all(|i| draft_calls.contains(&i.model_call.unwrap()))
    );
}
