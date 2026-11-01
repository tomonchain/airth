use super::*;
use serde_json::json;

fn business() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("agents/writer.md", "Write."),
        ("prompts/watch.md", "Judge it."),
        ("prompts/watch.schema.json", "{\"type\": \"object\"}"),
        ("prompts/bare.md", "No schema."),
    ] {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    root
}

/// Checks `graph` written as routines/test.json.
fn try_graph(root: &Path, graph: Value) -> Result<Graph> {
    std::fs::create_dir_all(root.join(DIR)).unwrap();
    std::fs::write(root.join("routines/test.json"), graph.to_string()).unwrap();
    load(&Checks { root }, "test")
}

fn posts(blocks: Value) -> Value {
    json!({"agent": "writer", "trigger": {"schedule": "daily 07:30"}, "blocks": blocks})
}

fn rss() -> Value {
    json!({"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"], "max_age_days": 2, "dedupe": "link"})
}

#[test]
fn a_routine_of_six_blocks_loads() {
    let root = business();
    let g = try_graph(root.path(), posts(json!([
        rss(),
        {"id": "pick", "type": "ai", "prompt": "prompts/watch.md", "keep": "relevant", "limit": 3},
        {"id": "good", "type": "condition", "if": {"field": "answer.score", "op": ">=", "value": 3}, "else": "skip"},
        {"id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft"},
        {"id": "send", "type": "act", "action": "blog.publish", "fields": {"text": "{{answer.draft}}"}, "next": null},
        {"id": "skip", "type": "act", "action": "airth.report", "fields": {"title": "Left out"}}
    ])))
    .unwrap();
    assert_eq!(g.blocks.len(), 6);
    assert_eq!(g.blocks[4].next, Some(None));
    // Hedgen's Writer: a mission, then the founder, then publishing.
    try_graph(root.path(), json!({"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "business"},
        {"id": "write", "type": "act", "action": "airth.launch_mission", "fields": {"brief": "Write"}},
        {"id": "ok", "type": "approval", "show": "result.report"},
        {"id": "publish", "type": "act", "action": "blog.publish"}
    ]}))
    .unwrap();
}

#[test]
fn a_wrong_graph_is_refused_naming_what_is_wrong() {
    let root = business();
    let act = |id: &str, action: &str| json!({"id": id, "type": "act", "action": action});
    let cases = [
        (
            json!({"agent": "writer", "trigger": {"manual": true, "schedule": "daily 07:00"}, "blocks": [act("a", "airth.report")]}),
            "one of schedule",
        ),
        (
            json!({"agent": "writer", "trigger": {"event": "mail.received"}, "blocks": [act("a", "airth.report")]}),
            "unknown field",
        ),
        (
            json!({"agent": "ghost", "trigger": {"manual": true}, "blocks": [act("a", "airth.report")]}),
            "the agent 'ghost'",
        ),
        (posts(json!([])), "at least one block"),
        (
            posts(json!([act("a", "airth.report"), act("a", "airth.report")])),
            "used twice",
        ),
        (
            posts(json!([{"id": "a", "type": "act", "action": "airth.report", "next": "nowhere"}])),
            "does not exist",
        ),
        (
            posts(
                json!([{"id": "a", "type": "act", "action": "airth.report", "next": "b"}, {"id": "b", "type": "act", "action": "airth.report", "next": "a"}]),
            ),
            "no loop",
        ),
        (
            posts(
                json!([{"id": "a", "type": "act", "action": "airth.report", "next": null}, act("b", "airth.report")]),
            ),
            "never reached",
        ),
        (
            posts(json!([{"id": "a", "type": "read", "source": "rss"}])),
            "needs feeds",
        ),
        (
            posts(json!([{"id": "a", "type": "read", "source": "ftp"}])),
            "unknown source",
        ),
        (
            posts(json!([{"id": "a", "type": "read", "source": "connector"}])),
            "names its action",
        ),
        (
            posts(json!([{"id": "a", "type": "ai", "prompt": "../secrets.md"}])),
            "prompts/<name>.md",
        ),
        (
            posts(json!([{"id": "a", "type": "ai", "prompt": "prompts/bare.md"}])),
            "bare.schema.json",
        ),
        (
            posts(
                json!([{"id": "a", "type": "condition", "if": {"field": "x", "op": "~", "value": 1}, "else": "a"}]),
            ),
            "unknown operator",
        ),
        (posts(json!([act("a", "bankpay")])), "not an action like"),
        (
            posts(json!([{"id": "a", "type": "shell", "command": "rm"}])),
            "not a valid routine",
        ),
        (
            posts(json!([{"id": "a", "type": "ai", "prompt": "prompts/watch.md", "limit": 0}])),
            "limit is at least 1",
        ),
        (
            posts(json!([{"id": "a", "type": "approval", "show": "answer..draft"}])),
            "not a field",
        ),
        (
            posts(
                json!([{"id": "a", "type": "act", "action": "airth.report", "fields": {"title": "{{answer.title"}}]),
            ),
            "without closing",
        ),
    ];
    for (graph, why) in cases {
        let e = format!("{:#}", try_graph(root.path(), graph.clone()).unwrap_err());
        assert!(e.contains(why), "{graph}: {e}");
    }
}
