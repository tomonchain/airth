//! Skills: exposing tools, developing and testing new ones.

use super::*;

#[test]
fn skills_expose_tools_with_capabilities() {
    let f = fixture();
    let skills = Hub::new(f.home).skills();
    let cap = |name: &str| skills.iter().find(|s| s.name == name).unwrap().capability;
    assert_eq!(cap("read_file"), Capability::Read);
    assert_eq!(cap("write_file"), Capability::Write);
    assert_eq!(cap("execute_command"), Capability::Execute);
    assert_eq!(cap("remember"), Capability::Memory);
}

/// v0.5 acceptance: Airth identifies a missing capability, implements a
/// solution, tests it, and requests approval before enabling it.
#[test]
fn missing_capability_becomes_a_tested_skill_enabled_only_by_the_owner() {
    let f = fixture();
    std::fs::write(f.root.path().join("secret-plan.txt"), "realm data").unwrap();
    std::fs::write(f.root.path().join("notes.txt"), "alpha beta gamma delta").unwrap();

    // 1. A run hits the limitation and proposes a skill.
    let (hub, _, systems) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "propose_skill",
                json!({"name": "word_count", "description": "Count the words in a text file of the workspace.",
                       "reason": "Counting words with shell one-liners is error-prone.",
                       "spec": "Input {path}; prints '<n> words'; fails on a missing file."}),
            ),
            done("Proposed word_count."),
        ],
    );
    hub.dispatch(
        &f.realm,
        "How many words are in notes.txt?",
        config(5),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert!(systems.lock().unwrap()[0].contains("propose_skill"));
    let skills = f.realm.skills();
    let record = skills.get("word_count").unwrap();
    assert_eq!(record.status, SkillStatus::Proposed);
    assert!(record.proposed_by.starts_with("run:"));
    assert!(
        f.home.inbox().list().unwrap()[0]
            .title
            .contains("proposes a new skill")
    );

    // 2. A development run builds it in isolation and tests it.
    let sample = tempfile::tempdir().unwrap();
    crate::skill::tests::write_word_count(sample.path());
    let file = |n: &str| std::fs::read_to_string(sample.path().join(n)).unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "read_file",
                json!({"path": f.root.path().join("secret-plan.txt")}),
            ),
            call(
                "2",
                "write_file",
                json!({"path": "skill.json", "content": file("skill.json")}),
            ),
            call(
                "3",
                "write_file",
                json!({"path": "main.py", "content": file("main.py")}),
            ),
            call(
                "4",
                "write_file",
                json!({"path": "tests.json", "content": "[{\"name\": \"counts\", \"input\": {\"path\": \"a.txt\"}, \"files\": {\"a.txt\": \"one two\"}, \"expect\": {\"contains\": [\"3 words\"]}}]"}),
            ),
            call("5", "test_skill", json!({})),
            call(
                "6",
                "write_file",
                json!({"path": "tests.json", "content": file("tests.json")}),
            ),
            call("7", "test_skill", json!({})),
            done("word_count is built and all tests pass."),
        ],
    );
    let mut dev = hub.develop_skill(&f.realm, "word_count", None).unwrap();
    assert!(
        dev.messages[0]
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::Text { text } if text.contains("tests.json")))
    );
    hub.drive(&f.realm, &mut dev, config(20), hooks(&mut NoObserver, true))
        .unwrap();
    assert_eq!(dev.status, Status::Completed);
    assert!(
        dev.events[0].status != crate::state::EventStatus::Ok,
        "the development run cannot read the realm's files: {}",
        dev.events[0].result
    );
    assert!(
        dev.events[4].result.contains("0/1 tests passed"),
        "{}",
        dev.events[4].result
    );
    assert!(
        dev.events[6].result.contains("2/2 tests passed"),
        "{}",
        dev.events[6].result
    );
    let record = skills.get("word_count").unwrap();
    assert_eq!(record.status, SkillStatus::Ready);
    assert!(record.evaluation.as_ref().unwrap().passed());
    assert!(
        f.home
            .inbox()
            .list()
            .unwrap()
            .last()
            .unwrap()
            .title
            .contains("ready for your review")
    );

    // 3. Not enabled until the owner installs it.
    assert!(skills.enabled().is_empty());
    assert!(!hub.skills().iter().any(|s| s.name == "word_count"));
    skills.install("word_count").unwrap();

    // 4. Runs now have it, under the realm's skill rule.
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call("1", "word_count", json!({"path": "notes.txt"})),
            done("notes.txt has 4 words."),
        ],
    );
    let state = hub
        .dispatch(
            &f.realm,
            "How many words are in notes.txt?",
            config(5),
            hooks(&mut NoObserver, true),
        )
        .unwrap();
    assert_eq!(state.status, Status::Completed);
    assert_eq!(
        state.events[0].result.trim(),
        "4 words",
        "{:?}",
        state.events[0]
    );

    // Denied by policy, it does not run.
    let mut realm = f.home.realm("work").unwrap();
    realm.config.policy.set(Capability::Skill, Rule::Deny);
    realm.save().unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call("1", "word_count", json!({"path": "notes.txt"})),
            done("ok"),
        ],
    );
    let state = hub
        .dispatch(&realm, "count", config(5), hooks(&mut NoObserver, true))
        .unwrap();
    assert_eq!(state.events[0].status, crate::state::EventStatus::Denied);
}

#[test]
fn skill_tests_cannot_reach_the_realm() {
    let f = fixture();
    std::fs::write(f.root.path().join("private.txt"), "realm data").unwrap();
    let store = f.realm.skills();
    store
        .propose(SkillRecord {
            name: "word_count".into(),
            status: SkillStatus::Proposed,
            description: "Count words".into(),
            reason: String::new(),
            spec: String::new(),
            proposed_by: "owner".into(),
            created_at: Utc::now(),
            dev_run: None,
            evaluation: None,
            installed: None,
        })
        .unwrap();
    let draft = store.draft_dir("word_count");
    crate::skill::tests::write_word_count(&draft);
    let private = f.root.path().canonicalize().unwrap().join("private.txt");
    std::fs::write(
        draft.join("tests.json"),
        json!([{"name": "escape", "input": {"path": private}, "expect": {"exit": 1}}]).to_string(),
    )
    .unwrap();
    let hub = Hub::new(f.home.clone());
    let r = hub.evaluate_skill(&f.realm, "word_count").unwrap();
    let eval = r.evaluation.unwrap();
    if crate::sandbox::available() {
        assert!(
            eval.passed(),
            "the realm's file was readable: {}",
            eval.report()
        );
    }
}
