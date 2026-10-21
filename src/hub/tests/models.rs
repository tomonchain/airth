//! Model tiers, shared keys, the frozen context and fast-model routing.

use super::*;

#[test]
fn a_shared_key_is_checked_before_it_is_kept() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    assert!(hub.set_shared_anthropic_key("  ").is_err());
    assert!(hub.set_shared_anthropic_key("sk-ant bad").is_err());
    assert!(f.home.provider_settings().unwrap().is_empty());
}

#[test]
fn model_tiers_are_set_per_realm_or_for_all() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let realm = f.realm;
    hub.set_model_tier(
        &realm,
        "main",
        "anthropic",
        "claude-opus-5-5",
        "high",
        false,
    )
    .unwrap();
    hub.set_model_tier(&realm, "fast", "", "claude-haiku-4-5", "", true)
        .unwrap();
    let m = hub.model_settings(&realm).unwrap();
    assert_eq!(m["main"]["model"], "claude-opus-5-5");
    assert_eq!(m["main"]["source"], "realm");
    assert_eq!(m["fast"]["model"], "claude-haiku-4-5");
    assert_eq!(m["fast"]["source"], "all");
    assert!(
        m["fast"]["provider"].is_null(),
        "inherits the main provider"
    );
    assert!(m["fallback"]["source"].is_null());
    assert_eq!(
        realm.secret("AIRTH_FAST_MODEL"),
        None,
        "shared, not in the realm"
    );
    // An empty model turns the fast connection off.
    hub.set_model_tier(&realm, "fast", "", "", "", true)
        .unwrap();
    assert!(hub.model_settings(&realm).unwrap()["fast"]["model"].is_null());
    assert!(
        hub.set_model_tier(&realm, "turbo", "", "x", "", false)
            .is_err()
    );
    assert!(
        hub.set_model_tier(&realm, "main", "nope", "x", "", false)
            .is_err()
    );
    assert!(
        hub.set_model_tier(&realm, "main", "", "a b", "", false)
            .is_err()
    );
    assert!(
        hub.set_model_tier(&realm, "main", "", "x", "huge", false)
            .is_err()
    );
}

#[test]
fn a_run_keeps_the_context_it_started_with() {
    let mut state = State::new("x", None);
    assert_eq!(frozen_context(&mut state, "memory: a".into()), "memory: a");
    state.messages.push(Message::user_text("x"));
    assert_eq!(frozen_context(&mut state, "memory: a".into()), "memory: a");
    assert_eq!(state.messages[0].content.len(), 1, "unchanged: no note");
    // The realm's context changed: the run still sends the old one, and
    // gets the new one once, as a note after the conversation.
    assert_eq!(
        frozen_context(&mut state, "memory: a, b".into()),
        "memory: a"
    );
    assert_eq!(state.messages.len(), 1);
    assert!(matches!(&state.messages[0].content[1],
        ContentBlock::Text { text } if text.ends_with("memory: a, b")));
    state.messages.push(Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text { text: "ok".into() }],
    });
    frozen_context(&mut state, "memory: a, b".into());
    assert_eq!(state.messages.len(), 2, "already sent");
    frozen_context(&mut state, "memory: b".into());
    assert_eq!(state.messages.len(), 3);
    assert_eq!(state.messages[2].role, Role::User);
}

#[test]
fn simple_asks_run_on_the_fast_model_until_one_needs_more() {
    let f = fixture();
    let named = |model: &str, r: ModelResponse| ModelResponse {
        model: model.into(),
        input_tokens: 5,
        ..r
    };
    let mut answer = done("It is 9 am in Tokyo.");
    answer.content.insert(
        0,
        ContentBlock::Reasoning {
            summary: "time zones".into(),
            raw: json!({"type": "thinking", "signature": "fast-sig"}),
        },
    );
    let fast = Scripted {
        responses: Mutex::new(vec![
            named("fast", done("SIMPLE")),
            named("fast", answer),
            named("fast", done("COMPLEX")),
        ]),
        ..Scripted::default()
    };
    let main = Scripted {
        responses: Mutex::new(vec![
            named("main", done("Planned the migration.")),
            named("main", done("Done again.")),
        ]),
        ..Scripted::default()
    };
    let (fast_seen, main_seen) = (fast.seen.clone(), main.seen.clone());
    let hub = Hub::new(f.home.clone()).with_connections(Connections {
        main: Arc::new(main),
        fast: Some(Arc::new(fast)),
        fallback: None,
    });

    let state = hub
        .dispatch(
            &f.realm,
            "What time is it in Tokyo?",
            config(5),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.tier, Some(Tier::Fast));
    assert_eq!(state.report.as_deref(), Some("It is 9 am in Tokyo."));
    // The triage call is counted with the run.
    let models: Vec<&str> = state.usage.iter().map(|u| u.model.as_str()).collect();
    assert_eq!(models, ["fast", "fast"]);
    assert_eq!(state.input_tokens, 10);

    // A follow-up that needs more moves the run to the main model.
    let state = hub
        .send(
            &f.realm,
            &state.id,
            "Now plan the database migration",
            config(5),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.tier, Some(Tier::Strong));
    assert_eq!(state.report.as_deref(), Some("Planned the migration."));
    let sent = format!("{:?}", main_seen.lock().unwrap()[0]);
    assert!(!sent.contains("fast-sig"), "{sent}");
    assert!(sent.contains("It is 9 am"), "{sent}");

    // It stays there: no more triage.
    hub.send(
        &f.realm,
        &state.id,
        "thanks",
        config(5),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert_eq!(fast_seen.lock().unwrap().len(), 3);
    assert_eq!(main_seen.lock().unwrap().len(), 2);
}
