use super::*;
use crate::business::store::FILE;
use crate::models::{ContentBlock, ModelResponse, StopReason};
use crate::pipeline::run::LIST_MAX;
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
            "fields": {"set": "acme", "content": "{{answer.draft}}"}, "next": null},
        {"id": "skip", "type": "act", "action": "airth.report", "fields": {"title": "Left out: {{title}}"}}
    ]}))
    .unwrap();
    let typefully = Integration {
        free_reads: false,
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
        fast: None,
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let integrations = [typefully];
    let report = dry_run(
        "posts",
        &graph,
        Dry {
            root: root.path(),
            context: "",
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
        report.contains("would typefully.create_draft [posts/https://e.com/1/send]: POST https://api.typefully.com/v1/sets/acme/drafts"),
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
    // A block without sort fields or filters says nothing of them.
    assert!(report.contains("  kept: Flows\n"), "{report}");
    assert!(
        report.contains("set aside: 0 not relevant, 0 invalid\n"),
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
        fast: None,
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let mut acted = Vec::new();
    let mut act = |action: &str,
                   fields: &BTreeMap<String, String>,
                   key: &str,
                   approved: bool,
                   _: &str,
                   _: &Item| {
        acted.push(format!("{action} {} {key} {approved}", fields["text"]));
        Ok("done".to_string())
    };
    let dry = Dry {
        root: root.path(),
        context: "",
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
            context: "",
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
    // Due Monday 07:30, started at 10:30: the Mac was asleep.
    let due: DateTime<Utc> = "2026-11-02T07:30:00Z".parse().unwrap();
    store
        .record_run(
            &crate::business::store::pipelines::Heartbeat {
                pipeline: "routine:daily-posts",
                due: Some(due),
                started: due + chrono::Duration::hours(3),
                finished: due + chrono::Duration::hours(3),
                items_read: 0,
                items_kept: 0,
                report: None,
                notes: "",
                error: None,
            },
            &[],
        )
        .unwrap();
    // That run's draft block dropped one item (decision 61).
    store
        .record_block(&BlockRun {
            routine: "daily-posts",
            run: "r1",
            block: "draft",
            started_at: due + chrono::Duration::hours(3),
            outcome: "ok",
            items_in: 3,
            items_out: 2,
            cost_usd: None,
            error: None,
            dropped: Some(
                json!([{"item": "Hack", "reason": "answer.draft has 300 characters, more than 280"}])
                    .to_string(),
            ),
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
        fast: None,
    };
    let now: DateTime<Utc> = "2026-11-02T08:00:00Z".parse().unwrap();
    let mut report = |store: &mut Store| {
        dry_run(
            "weekly-report",
            &graph,
            Dry {
                root: root.path(),
                context: "",
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
        "- daily-posts: 1 run(s), 0 failed, 1 late, 1 dropped, $0.00, 0 s of yours",
        "Started late: 1\n      - daily-posts: due Mon 2 Nov 07:30 UTC, started 3 h 00 later",
        "Dropped: 1\n      - daily-posts: answer.draft has 300 characters, more than 280 (1×)",
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
        fast: None,
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let mut act = |_: &str,
                   _: &BTreeMap<String, String>,
                   _: &str,
                   _: bool,
                   _: &str,
                   _: &Item|
     -> Result<String> { unreachable!("no act") };
    let report = run(
        "posts",
        &graph,
        Dry {
            root: root.path(),
            context: "",
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

/// A routine whose sort judges its whole list, with the model's answers
/// given in turn; its report, or why it failed, and the model's requests.
fn sort_list(titles: &[&str], answers: &[&str]) -> (Result<String>, Vec<String>) {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/pick.md", "Pick the best."),
        (
            "prompts/pick.schema.json",
            r#"{"type": "object", "required": ["relevant", "story"], "properties": {"relevant": {"type": "boolean"}, "story": {"type": "string"}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"]},
        {"id": "pick", "type": "ai", "prompt": "prompts/pick.md", "keep": "relevant", "distinct": "story", "over": "list"},
        {"id": "ok", "type": "approval", "show": "title"}
    ]}))
    .unwrap();
    let body = format!(
        "<rss><channel>{}</channel></rss>",
        titles
            .iter()
            .map(|t| format!("<item><title>{t}</title><link>https://e.com/{t}</link></item>"))
            .collect::<String>()
    );
    let mut fetch = |_: &str| {
        Ok(crate::pipeline::run::Fetched {
            body: body.clone(),
            truncated: false,
        })
    };
    let mut asked = Vec::new();
    let mut answers = answers.iter();
    let mut ask = |req: &crate::models::ModelRequest| -> Result<ModelResponse> {
        asked.push(format!("{req:?}"));
        Ok(ModelResponse {
            content: vec![ContentBlock::Text {
                text: answers.next().expect("no more answers").to_string(),
            }],
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
        fast: None,
    };
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    let report = dry_run(
        "posts",
        &graph,
        Dry {
            root: root.path(),
            context: "",
            store: &mut store,
            io: &mut io,
            integrations: &[],
        },
        Utc::now(),
    )
    .map(|r| r.to_string());
    (report, asked)
}

/// Decision 60: a list block asks once for every item, keeps those the
/// answer names, in its order, and `distinct` still checks behind it.
#[test]
fn a_list_block_judges_every_item_in_one_call() {
    let answer = json!({"items": [
        {"n": 3, "relevant": true, "story": "Hack"},
        {"n": 1, "relevant": true, "story": "hack"},
        {"n": 2, "relevant": false, "story": "ETF"}
    ]})
    .to_string();
    let (report, asked) = sort_list(&["A", "B", "C", "D"], &[&answer]);
    let report = report.unwrap();
    assert_eq!(asked.len(), 1, "one call for the list");
    for n in 1..=4 {
        assert!(asked[0].contains(&format!("\\\"n\\\":{n}")), "{}", asked[0]);
    }
    assert!(report.contains("would ask the founder: C\n"), "{report}");
    assert!(!report.contains("would ask the founder: A"), "{report}");
    // B is not relevant, D was left out, A is C's story.
    assert!(
        report.contains("set aside: 2 not relevant, 0 invalid, 1 by distinct story"),
        "{report}"
    );
}

/// Decision 60: an answer that names an item it was not given, names one
/// twice, or does not fit the schema is refused whole and asked again;
/// refused twice, the block fails, naming itself.
#[test]
fn a_refused_list_answer_is_asked_once_more_then_fails_the_block() {
    let good = r#"{"items": [{"n": 1, "relevant": true, "story": "Hack"}]}"#;
    for bad in [
        r#"{"items": [{"n": 3, "relevant": true, "story": "Hack"}]}"#,
        r#"{"items": [{"n": 1, "relevant": true, "story": "Hack"}, {"n": 1, "relevant": true, "story": "Hack"}]}"#,
        r#"{"items": [{"n": 1, "relevant": "yes", "story": "Hack"}]}"#,
        r#"{"items": [{"relevant": true, "story": "Hack"}]}"#,
        r#"{"picked": [1]}"#,
    ] {
        let (report, asked) = sort_list(&["A", "B"], &[bad, good]);
        let report = report.unwrap();
        assert_eq!(asked.len(), 2, "{bad}");
        assert!(report.contains("asked again"), "{report}");
        assert!(report.contains("would ask the founder: A"), "{report}");

        let (failed, asked) = sort_list(&["A", "B"], &[bad, bad]);
        let e = format!("{:#}", failed.unwrap_err());
        assert_eq!(asked.len(), 2, "{bad}");
        assert!(e.contains("pick failed"), "{e}");
        assert!(e.contains("did not match prompts/pick.md twice"), "{e}");
    }
}

/// A list call takes at most LIST_MAX items; the rest are set aside, noted.
#[test]
fn a_list_call_takes_at_most_its_bound() {
    let titles: Vec<String> = (0..LIST_MAX + 2).map(|i| format!("T{i}")).collect();
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    let (report, asked) = sort_list(&titles, &[r#"{"items": []}"#]);
    let report = report.unwrap();
    assert!(
        asked[0].contains(&format!("\\\"n\\\":{LIST_MAX}")),
        "the last one in"
    );
    assert!(!asked[0].contains(&format!("\\\"n\\\":{}", LIST_MAX + 1)));
    assert!(
        report.contains(&format!("judged the first {LIST_MAX} of {}", LIST_MAX + 2)),
        "{report}"
    );
}

/// Decision 60: a block with `"model": "fast"` asks the realm's fast model,
/// the others its main one; with no fast connection, the main one answers
/// and the report says so.
#[test]
fn a_block_asks_the_fast_model_or_says_the_main_one_answered() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/pick.md", "Pick."),
        ("prompts/post.md", "Draft."),
        ("prompts/pick.schema.json", r#"{"type": "object"}"#),
        ("prompts/post.schema.json", r#"{"type": "object"}"#),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(
        json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"]},
            {"id": "pick", "type": "ai", "prompt": "prompts/pick.md", "model": "fast"},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md", "model": "main"},
            {"id": "ok", "type": "approval", "show": "title"}
        ]}),
    )
    .unwrap();
    let response = |model: &str| ModelResponse {
        content: vec![ContentBlock::Text { text: "{}".into() }],
        stop_reason: StopReason::EndTurn,
        model: model.into(),
        input_tokens: 1,
        output_tokens: 1,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
    };
    for has_fast in [true, false] {
        let mut fetch = |_: &str| {
            Ok(crate::pipeline::run::Fetched {
                body: "<rss><channel><item><title>A</title><link>https://e.com/a</link></item></channel></rss>".into(),
                truncated: false,
            })
        };
        let mut asked = Vec::new();
        let mut ask = |req: &crate::models::ModelRequest| -> Result<ModelResponse> {
            asked.push(("main", format!("{req:?}")));
            Ok(response("main-m"))
        };
        let mut fast_asked = Vec::new();
        let mut fast = |req: &crate::models::ModelRequest| -> Result<Option<ModelResponse>> {
            fast_asked.push(format!("{req:?}"));
            Ok(has_fast.then(|| response("fast-m")))
        };
        let mut io = Io {
            fetch: &mut fetch,
            ask: &mut ask,
            prices: Prices::default(),
            fast: Some(&mut fast),
        };
        let mut store = Store::open(&root.path().join(FILE)).unwrap();
        let report = dry_run(
            "posts",
            &graph,
            Dry {
                root: root.path(),
                context: "",
                store: &mut store,
                io: &mut io,
                integrations: &[],
            },
            Utc::now(),
        )
        .unwrap()
        .to_string();
        // Only the pick goes to the fast model; the draft never does.
        assert_eq!(fast_asked.len(), 1, "{has_fast}");
        assert!(fast_asked[0].contains("Pick."));
        let main: Vec<&String> = asked.iter().map(|(_, r)| r).collect();
        assert!(main.iter().any(|r| r.contains("Draft.")));
        let note = "step 2: this business has no fast model: its main model answered";
        if has_fast {
            assert_eq!(main.len(), 1);
            assert!(!report.contains(note), "{report}");
        } else {
            assert_eq!(main.len(), 2);
            assert!(main[0].contains("Pick."));
            assert!(report.contains(note), "{report}");
        }
    }
}

/// A draft whose answer does not fit (here, over 280 characters) is asked
/// once more; refused twice, it is dropped and the report names it, why
/// included, so a missing post is never silent.
#[test]
fn a_draft_that_does_not_fit_is_asked_once_more_then_dropped_by_name() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/post.md", "Draft."),
        (
            "prompts/post.schema.json",
            r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string", "maxLength": 280}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(
        json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"]},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft"}
        ]}),
    )
    .unwrap();
    let long = json!({"draft": "x".repeat(300)}).to_string();
    let short = json!({"draft": "Short enough."}).to_string();
    // Flows is too long, then fits; Hack is too long twice.
    for (answers, kept) in [
        (vec![&long, &short, &long, &long], "Short enough."),
        (vec![&short, &long, &long], "Short enough."),
    ] {
        let mut fetch = |_: &str| {
            Ok(crate::pipeline::run::Fetched {
                body: "<rss><channel><item><title>Flows</title><link>https://e.com/1</link></item>\
                       <item><title>Hack</title><link>https://e.com/2</link></item></channel></rss>"
                    .into(),
                truncated: false,
            })
        };
        let mut answers = answers.into_iter();
        let mut asked = 0;
        let mut ask = |_: &crate::models::ModelRequest| -> Result<ModelResponse> {
            asked += 1;
            Ok(ModelResponse {
                content: vec![ContentBlock::Text {
                    text: answers.next().expect("no more answers").clone(),
                }],
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
            fast: None,
        };
        let mut store = Store::open(&root.path().join(FILE)).unwrap();
        let report = dry_run(
            "posts",
            &graph,
            Dry {
                root: root.path(),
                context: "",
                store: &mut store,
                io: &mut io,
                integrations: &[],
            },
            Utc::now(),
        )
        .unwrap()
        .to_string();
        assert!(
            report.contains(&format!("would ask the founder: {kept}")),
            "{report}"
        );
        assert!(report.contains("draft: 2 in, 1 out"), "{report}");
        assert!(
            report.contains("note: step 2: dropped \"Hack\": its answer did not match prompts/post.md twice (answer.draft has 300 characters, more than 280)"),
            "{report}"
        );
        assert!(!report.contains("dropped \"Flows\""), "{report}");
    }
}

/// Decision 61: what a run drops reaches the founder. The items it still
/// asks about carry one line on it; when it drops them all, a report says
/// so instead, never an incident.
#[test]
fn what_a_run_drops_reaches_the_founder() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/post.md", "Draft."),
        (
            "prompts/post.schema.json",
            r#"{"type": "object", "required": ["draft"], "properties": {"draft": {"type": "string", "maxLength": 280}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(
        json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"]},
            {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
            {"id": "ok", "type": "approval", "show": "answer.draft"}
        ]}),
    )
    .unwrap();
    let long = json!({"draft": "x".repeat(300)}).to_string();
    let why = "answer.draft has 300 characters, more than 280";
    for too_long in [vec!["Hack"], vec!["Flows", "Hack", "Rates"]] {
        let mut fetch = |_: &str| {
            Ok(crate::pipeline::run::Fetched {
                body: format!(
                    "<rss><channel>{}</channel></rss>",
                    ["Flows", "Hack", "Rates"]
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
            let title = ["Flows", "Hack", "Rates"]
                .into_iter()
                .find(|t| seen.contains(&format!("https://e.com/{t}")))
                .unwrap();
            let text = match too_long.contains(&title) {
                true => long.clone(),
                false => json!({"draft": format!("{title}, drafted")}).to_string(),
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
            fast: None,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let mut act = |_: &str,
                       _: &BTreeMap<String, String>,
                       _: &str,
                       _: bool,
                       _: &str,
                       _: &Item|
         -> Result<String> { unreachable!("no act") };
        run(
            "posts",
            &graph,
            Dry {
                root: root.path(),
                context: "",
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
        .unwrap();
        let items = store.queue(&["pending"]).unwrap();
        assert!(items.iter().all(|i| i.kind != "incident"), "{items:?}");
        if too_long.len() == 1 {
            let line = format!(r#"draft: 1 of 3 dropped: "Hack" ({why})"#);
            assert_eq!(items.len(), 2);
            assert!(
                items
                    .iter()
                    .all(|i| i.kind == "approval" && i.note.as_deref() == Some(line.as_str())),
                "{items:?}"
            );
        } else {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].kind, "report");
            assert_eq!(items[0].title, "posts: items dropped");
            let text = items[0].args["text"].as_str().unwrap();
            assert!(
                text.starts_with(r#"draft: 3 of 3 dropped: "Flows" ("#),
                "{text}"
            );
        }
    }
}

/// Decision 63: the post the founder sent is read through oEmbed into a
/// feed-like item, the angle goes to the model as the founder's intent, and
/// a post too thin to answer stops the run before any draft.
#[test]
fn a_reply_run_reads_the_post_and_stops_on_a_thin_one() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/reply.md", "Reply."),
        (
            "prompts/reply.schema.json",
            r#"{"type": "object", "required": ["reply"], "properties": {"reply": {"type": "string"}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(
        json!({"agent": "writer", "trigger": {"telegram": "x_post"}, "blocks": [
            {"id": "post", "type": "read", "source": "x_post"},
            {"id": "reply", "type": "ai", "prompt": "prompts/reply.md"},
            {"id": "ok", "type": "approval", "show": "answer.reply"}
        ]}),
    )
    .unwrap();
    for (text, thin) in [
        (
            "Staking yields fell to 2.8% this week and withdrawals doubled.",
            false,
        ),
        ("pic.twitter.com/abc", true),
    ] {
        let body = json!({"url": "https://x.com/jack/status/20", "author_name": "jack",
            "html": format!("<blockquote><p>{text}</p>&mdash; jack <a href=\"x\">November 3, 2026</a></blockquote>")})
        .to_string();
        let mut asked_url = String::new();
        let mut fetch = |url: &str| {
            asked_url = url.to_string();
            Ok(crate::pipeline::run::Fetched {
                body: body.clone(),
                truncated: false,
            })
        };
        let mut seen = Vec::new();
        let mut ask = |req: &crate::models::ModelRequest| -> Result<ModelResponse> {
            seen.push(format!("{req:?}"));
            Ok(ModelResponse {
                content: vec![ContentBlock::Text {
                    text: r#"{"reply": "My guess: it is rates."}"#.into(),
                }],
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
            fast: None,
        };
        let mut store = Store::open(&root.path().join(FILE)).unwrap();
        let mut event = Item::new();
        event.insert(
            POST_LINK.into(),
            "https://twitter.com/jack/status/20?s=46".into(),
        );
        event.insert(ANGLE.into(), "push back on the yield claim".into());
        let ran = run(
            "replies",
            &graph,
            Dry {
                root: root.path(),
                context: "",
                store: &mut store,
                io: &mut io,
                integrations: &[],
            },
            None,
            Start::Event(event),
            Utc::now(),
        );
        assert!(
            asked_url.starts_with("https://publish.x.com/oembed?"),
            "{asked_url}"
        );
        if thin {
            let e = format!("{:#}", ran.unwrap_err());
            assert!(
                e.contains("not drafted: the post is mostly an image or a chart"),
                "{e}"
            );
            assert!(seen.is_empty(), "nothing is drafted");
        } else {
            let report = ran.unwrap().to_string();
            assert!(
                report.contains("would ask the founder: My guess: it is rates."),
                "{report}"
            );
            assert!(
                seen[0].contains("founder_angle")
                    && seen[0].contains("push back on the yield claim")
            );
            assert!(seen[0].contains("Staking yields fell to 2.8%"));
        }
    }
}

/// Decision 99: a block with `recent` shows the model what it kept in its
/// runs of the last days, and sets aside an item of the same story, as
/// `distinct` does within a run; past those days, the story may come back.
#[test]
fn a_story_covered_in_the_last_days_is_not_covered_again() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("prompts/pick.md", "Pick the best."),
        (
            "prompts/pick.schema.json",
            r#"{"type": "object", "required": ["story"], "properties": {"story": {"type": "string"}}}"#,
        ),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let graph: Graph = serde_json::from_value(json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"], "dedupe": "link"},
        {"id": "pick", "type": "ai", "prompt": "prompts/pick.md", "distinct": "story", "over": "list", "recent": 3},
        {"id": "ok", "type": "approval", "show": "title"}
    ]}))
    .unwrap();
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    // One day's run: its feed, the model's answer, and what the model was sent.
    let mut day = |titles: &[&str], answer: Value, now: DateTime<Utc>| -> (String, String) {
        let body = format!(
            "<rss><channel>{}</channel></rss>",
            titles
                .iter()
                .map(|t| format!("<item><title>{t}</title><link>https://e.com/{t}</link></item>"))
                .collect::<String>()
        );
        let mut fetch = |_: &str| {
            Ok(crate::pipeline::run::Fetched {
                body: body.clone(),
                truncated: false,
            })
        };
        let mut asked = String::new();
        let mut ask = |req: &crate::models::ModelRequest| -> Result<ModelResponse> {
            asked = format!("{req:?}");
            Ok(ModelResponse {
                content: vec![ContentBlock::Text {
                    text: answer.to_string(),
                }],
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
            fast: None,
        };
        let mut act =
            |_: &str, _: &BTreeMap<String, String>, _: &str, _: bool, _: &str, _: &Item| {
                Ok("done".to_string())
            };
        let report = run(
            "posts",
            &graph,
            Dry {
                root: root.path(),
                context: "",
                store: &mut store,
                io: &mut io,
                integrations: &[],
            },
            Some(Real {
                run: "r",
                act: &mut act,
            }),
            Start::Trigger,
            now,
        )
        .unwrap()
        .to_string();
        drop(io);
        (report, asked)
    };

    let today = Utc::now();
    let (report, asked) = day(
        &["A"],
        json!({"items": [{"n": 1, "story": "Binance hack"}]}),
        today,
    );
    assert!(report.contains("asked the founder (item 1): A"), "{report}");
    assert!(!asked.contains("<covered>"), "nothing covered yet: {asked}");

    // The next day: the model is told, and the same story is set aside.
    let answer = json!({"items": [{"n": 1, "story": "binance hack "}, {"n": 2, "story": "ETF"}]});
    let (report, asked) = day(&["B", "C"], answer, today + chrono::Duration::days(1));
    assert!(asked.contains("<covered>"), "{asked}");
    assert!(asked.contains("binance hack"), "{asked}");
    assert!(report.contains("asked the founder (item 2): C"), "{report}");
    assert!(!report.contains(": B"), "{report}");
    assert!(report.contains("1 by distinct story"), "{report}");

    // Past the three days, the story may come back.
    let answer = json!({"items": [{"n": 1, "story": "Binance hack"}]});
    let (report, asked) = day(&["D"], answer, today + chrono::Duration::days(5));
    assert!(!asked.contains("<covered>"), "{asked}");
    assert!(report.contains("asked the founder (item 3): D"), "{report}");
}

/// Decision 70: a routine reads its agent's mailbox, each mail once; what
/// comes of a mail that speaks of bank details waits for the founder,
/// flagged, and cannot be approved without overriding the flag.
#[test]
fn a_routine_reads_its_mailbox_once_and_holds_a_bank_mail() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("agents")).unwrap();
    std::fs::write(
        root.path().join("agents/support.md"),
        "---\ntitle: Support\nmailbox: support@n.example\n---\nAnswer.\n",
    )
    .unwrap();
    let graph: Graph = serde_json::from_value(
        json!({"agent": "support", "trigger": {"manual": true}, "blocks": [
            {"id": "inbox", "type": "read", "source": "mailbox", "max": 1},
            {"id": "ok", "type": "approval", "show": "subject"}
        ]}),
    )
    .unwrap();
    let mut store = Store::open(&root.path().join(FILE)).unwrap();
    for (uid, subject, text) in [
        (1, "Nouveau RIB", "Merci de virer sur le nouveau compte."),
        (2, "Où est mon lot ?", "Bonjour."),
    ] {
        let mail = crate::mail::Mail {
            from: "x@y.example".into(),
            subject: subject.into(),
            text: text.into(),
            ..Default::default()
        };
        store
            .keep_mail(&crate::business::store::mail::NewMail {
                mailbox: "support@n.example",
                agent: "support",
                uid_validity: 1,
                uid,
                mail: &mail,
                at: Utc::now(),
            })
            .unwrap();
    }
    let mut fetch =
        |_: &str| -> anyhow::Result<crate::pipeline::run::Fetched> { unreachable!("no web") };
    let mut ask = |_: &crate::models::ModelRequest| -> anyhow::Result<ModelResponse> {
        unreachable!("no AI block")
    };
    let mut io = Io {
        fetch: &mut fetch,
        ask: &mut ask,
        prices: Prices::default(),
        fast: None,
    };
    let mut act = |_: &str,
                   _: &BTreeMap<String, String>,
                   _: &str,
                   _: bool,
                   _: &str,
                   _: &Item|
     -> anyhow::Result<String> { unreachable!("no act") };
    let mut report = |store: &mut Store, io: &mut Io| {
        let dry = Dry {
            root: root.path(),
            context: "",
            store,
            io,
            integrations: &[],
        };
        run(
            "mail",
            &graph,
            dry,
            Some(Real {
                run: "r",
                act: &mut act,
            }),
            Start::Trigger,
            Utc::now(),
        )
        .unwrap()
        .to_string()
    };
    // The max reads one, oldest first; the next run reads the other.
    let first = report(&mut store, &mut io);
    assert!(
        first.contains("1 new mail in support@n.example, 1 left for the next run"),
        "{first}"
    );
    assert!(
        first.contains("[flagged blocking: the mail speaks of bank details"),
        "{first}"
    );
    let second = report(&mut store, &mut io);
    assert!(second.contains("Où est mon lot"), "{second}");
    let third = report(&mut store, &mut io);
    assert!(third.contains("0 new mail"), "{third}");
    let queued = store.queue(&["pending"]).unwrap();
    assert_eq!(queued.len(), 2);
    let flagged = queued.iter().find(|i| i.title == "Nouveau RIB").unwrap();
    assert_eq!(flagged.flag.as_deref(), Some("blocking"));
    let err = store
        .answer(
            flagged.id,
            Some(flagged.revision),
            crate::business::store::queue::Answer::Approve,
            "app",
            1,
        )
        .unwrap_err();
    assert!(format!("{err:#}").contains("flagged"), "{err:#}");
}
