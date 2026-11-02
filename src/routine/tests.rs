use super::*;
use crate::actions::{Class, Declared};
use serde_json::json;

fn declared(id: &str, class: Class) -> Declared {
    Declared {
        id: id.into(),
        class,
        ceiling: class.ceiling(),
        confirmed: true,
        start: 1,
    }
}

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

/// Checks `graph` written as routines/test.json; the agent may create
/// drafts and reports alone (level 3), and nothing else.
fn try_graph(root: &Path, graph: Value) -> Result<Graph> {
    std::fs::create_dir_all(root.join(DIR)).unwrap();
    std::fs::write(root.join("routines/test.json"), graph.to_string()).unwrap();
    let actions = [
        declared("airth.launch_mission", Class::Reversible),
        declared("airth.report", Class::Read),
        declared("airth.create_task", Class::Reversible),
        declared("typefully.create_draft", Class::Reversible),
        declared("blog.publish", Class::Commits),
        declared("stats.read", Class::Read),
    ];
    let level = |_: &str, action: &str| {
        if action == "typefully.create_draft" || action == "airth.report" {
            3
        } else {
            1
        }
    };
    load(
        &Checks {
            root,
            actions: &actions,
            level: &level,
        },
        "test",
    )
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
        {"id": "pick", "type": "ai", "prompt": "prompts/watch.md", "keep": "relevant", "limit": 3, "distinct": ["pillar", "story"]},
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
        {"id": "news", "type": "read", "source": "business", "records": "week"},
        {"id": "write", "type": "act", "action": "airth.launch_mission", "fields": {"brief": "Write"}},
        {"id": "ok", "type": "approval", "show": "result.report"},
        {"id": "publish", "type": "act", "action": "blog.publish"}
    ]}))
    .unwrap();
}

#[test]
fn content_from_outside_reaches_no_act_without_an_approval() {
    let root = business();
    let err = |g: Value| format!("{:#}", try_graph(root.path(), g).unwrap_err());
    let e = err(posts(
        json!([rss(), {"id": "send", "type": "act", "action": "blog.publish"}]),
    ));
    assert!(
        e.contains("without an approval") && e.contains("'send'"),
        "{e}"
    );
    // A mission's result is from outside too.
    let e = err(posts(json!([
        {"id": "write", "type": "act", "action": "airth.launch_mission"},
        {"id": "publish", "type": "act", "action": "blog.publish"}
    ])));
    assert!(e.contains("'publish'"), "{e}");
    // A webhook's body as well.
    let e = err(
        json!({"agent": "writer", "trigger": {"webhook": "orders"}, "blocks": [
        {"id": "send", "type": "act", "action": "blog.publish"}]}),
    );
    assert!(e.contains("without an approval"), "{e}");
    // Through a condition's other branch, too.
    let e = err(posts(json!([rss(),
        {"id": "c", "type": "condition", "if": {"field": "x", "op": "exists"}, "else": "send"},
        {"id": "ok", "type": "approval", "show": "x", "next": null},
        {"id": "send", "type": "act", "action": "blog.publish"}])));
    assert!(e.contains("'send'"), "{e}");
    // A task born of outside content and given to an agent is its
    // instructions: checked. One for the founder is not.
    let task = |assignee: &str| {
        posts(
            json!([rss(), {"id": "t", "type": "act", "action": "airth.create_task",
            "fields": {"title": "{{title}}", "assignee": assignee}}]),
        )
    };
    assert!(err(task("writer")).contains("airth.create_task"));
    try_graph(root.path(), task("founder")).unwrap();
    // The business's own records are not from outside.
    try_graph(
        root.path(),
        posts(json!([{"id": "week", "type": "read", "source": "business", "records": "week"},
            {"id": "send", "type": "act", "action": "blog.publish", "fields": {"text": "{{summary}}"}}])),
    )
    .unwrap();
    // An action the agent may take alone (level 2 or more) is allowed.
    try_graph(
        root.path(),
        posts(json!([rss(), {"id": "d", "type": "act", "action": "typefully.create_draft"}])),
    )
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
            posts(
                json!([{"id": "a", "type": "read", "source": "connector", "action": "blog.publish"}]),
            ),
            "not a read action",
        ),
        (
            posts(json!([{"id": "a", "type": "read", "source": "business"}])),
            "names its records",
        ),
        (
            posts(json!([{"id": "a", "type": "read", "source": "business", "records": "mail"}])),
            "unknown records 'mail'",
        ),
        (
            posts(
                json!([{"id": "a", "type": "read", "source": "rss", "feeds": ["https://e.com/f"], "records": "week"}]),
            ),
            "only a business read names records",
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
        (
            posts(json!([act("a", "bank.pay")])),
            "not a declared action",
        ),
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
        // Decision 60: a check judges each draft alone.
        (
            posts(
                json!([{"id": "a", "type": "ai", "prompt": "prompts/watch.md", "as": "check", "over": "list"}]),
            ),
            "judges each item alone",
        ),
        (
            posts(json!([{"id": "a", "type": "ai", "prompt": "prompts/watch.md", "over": "all"}])),
            "unknown variant",
        ),
        (
            posts(
                json!([{"id": "a", "type": "ai", "prompt": "prompts/watch.md", "model": "deepseek-flash"}]),
            ),
            "unknown variant",
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

/// Decision 58: an approval's flag must be a check's answer on every way to
/// it, and a check's answer cannot take a field the engine keeps.
#[test]
fn a_flag_needs_its_check_on_every_way() {
    let root = business();
    let check = json!({"id": "check", "type": "ai", "prompt": "prompts/watch.md", "as": "check"});
    let ok = json!({"id": "ok", "type": "approval", "show": "answer.draft", "flag": "check"});
    try_graph(root.path(), posts(json!([rss(), check, ok]))).unwrap();
    let e = |g: Value| format!("{:#}", try_graph(root.path(), g).unwrap_err());
    // No check at all.
    assert!(e(posts(json!([rss(), ok]))).contains("not the answer of a check"));
    // A check under another name.
    let other = json!({"id": "check", "type": "ai", "prompt": "prompts/watch.md", "as": "review"});
    assert!(e(posts(json!([rss(), other, ok]))).contains("not the answer of a check"));
    // A check on one way only: the condition's other side skips it.
    let g = posts(json!([rss(),
        {"id": "c", "type": "condition", "if": {"field": "x", "op": "exists"}, "else": "ok"},
        check, ok]));
    assert!(e(g).contains("on every way"));
    // A check's answer cannot take the place of the engine's fields.
    let bad = json!({"id": "check", "type": "ai", "prompt": "prompts/watch.md", "as": "source_id"});
    assert!(e(posts(json!([rss(), bad]))).contains("keeps that field"));
}

/// A routine whose schema uses a keyword Airth does not check is refused,
/// naming it: the founder must not believe a rule holds when it does not.
#[test]
fn a_schema_with_a_rule_airth_does_not_check_is_refused() {
    let root = business();
    std::fs::write(root.path().join("prompts/post.md"), "Draft.").unwrap();
    let schema = root.path().join("prompts/post.schema.json");
    let graph = posts(json!([rss(), {"id": "draft", "type": "ai", "prompt": "prompts/post.md"}]));
    std::fs::write(
        &schema,
        r#"{"type": "object", "properties": {"draft": {"type": "string", "maxLength": 280}}}"#,
    )
    .unwrap();
    assert!(try_graph(root.path(), graph.clone()).is_ok());
    std::fs::write(
        &schema,
        r#"{"type": "object", "properties": {"draft": {"type": "string", "pattern": "^[^#]*$"}}}"#,
    )
    .unwrap();
    let e = format!("{:#}", try_graph(root.path(), graph).unwrap_err());
    assert!(
        e.contains("prompts/post.schema.json: schema.draft uses \"pattern\""),
        "{e}"
    );
}
