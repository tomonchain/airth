use super::*;
use crate::business::store::FILE;
use crate::models::{ContentBlock, ModelResponse, StopReason};
use crate::prices::Prices;
use serde_json::json;

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
    let mut answers = vec![
        r#"{"relevant": true, "score": 1, "draft": "Meh."}"#,
        r#"{"relevant": true, "score": 5, "draft": "ETH left the queue."}"#,
    ];
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
    .unwrap();
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
    drop(io);
    assert_eq!(fetched, 1, "only the feed was read");
}
