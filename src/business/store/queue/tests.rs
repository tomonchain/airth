use super::*;
use crate::business::store::FILE;
use crate::business::store::autonomy::NewItem;
use serde_json::json;

fn propose(store: &mut Store, model_call: Option<i64>) -> i64 {
    let args = json!({"title": "Flows", "answer": {"draft": "ETH left the queue."}}).to_string();
    store
        .propose(&NewItem {
            kind: "approval",
            title: "Post: ETH left the queue.",
            agent: Some("post-writer"),
            routine: Some("daily-posts"),
            run: Some("r1"),
            action: None,
            args: &args,
            execute_at: None,
            source: r#"{"block": "ok", "editable": "answer.draft"}"#,
            model_call,
        })
        .unwrap()
}

fn time(store: &Store) -> Vec<(i64, Option<i64>, Option<String>)> {
    store
        .conn
        .prepare("SELECT seconds, item, channel FROM owner_time ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn the_founder_edits_then_approves_and_the_time_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join(FILE)).unwrap();
    s.conn
        .execute(
            "INSERT INTO model_calls (at, pipeline, step, prompt, prompt_version, model, input_tokens,
             output_tokens, cache_read_tokens, cache_write_tokens, latency_ms, valid)
             VALUES ('t', 'daily-posts', 3, 'prompts/post.md', 'v1', 'm', 1, 1, 0, 0, 1, 1)",
            [],
        )
        .unwrap();
    let id = propose(&mut s, Some(1));
    let item = s
        .answer(
            id,
            Some(1),
            Answer::Edit("ETH left. Calmly.".into()),
            "telegram",
            30,
        )
        .unwrap();
    assert_eq!((item.status.as_str(), item.revision), ("pending", 2));
    assert_eq!(item.args["answer"]["draft"], "ETH left. Calmly.");
    assert_eq!(item.original_args["answer"]["draft"], "ETH left the queue.");
    // An answer on the older read is refused.
    assert!(s.answer(id, Some(1), Answer::Approve, "app", 5).is_err());
    let item = s.answer(id, Some(2), Answer::Approve, "app", 4000).unwrap();
    assert_eq!(item.status, "approved");
    // Approving again changes nothing, and is not counted again.
    s.answer(id, None, Answer::Approve, "app", 5).unwrap();
    assert_eq!(
        time(&s),
        vec![
            (30, Some(id), Some("telegram".into())),
            (600, Some(id), Some("app".into()))
        ]
    );
    let outcome: String = s
        .conn
        .query_row("SELECT outcome FROM model_calls WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(outcome, "edited");
    assert!(
        s.answer(id, None, Answer::Reject("late".into()), "app", 1)
            .is_err()
    );
    assert_eq!(s.queue(&["approved"]).unwrap().len(), 1);
}

#[test]
fn a_rejection_is_final_and_an_item_without_an_editable_field_is_not_edited() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join(FILE)).unwrap();
    let id = propose(&mut s, None);
    s.answer(id, None, Answer::Reject("off topic".into()), "cli", 12)
        .unwrap();
    s.answer(id, None, Answer::Reject("again".into()), "cli", 12)
        .unwrap();
    assert!(s.answer(id, None, Answer::Approve, "cli", 1).is_err());
    assert_eq!(time(&s).len(), 1);
    let other = s
        .propose(&NewItem {
            kind: "approval",
            title: "Pay",
            agent: None,
            routine: None,
            run: None,
            action: Some("bank.pay"),
            args: "{}",
            execute_at: None,
            source: "{}",
            model_call: None,
        })
        .unwrap();
    let err = s
        .answer(other, None, Answer::Edit("x".into()), "cli", 1)
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("nothing the founder may edit"),
        "{err:#}"
    );
}

/// Team's figure: the founder's time on each agent's items, from the ledger.
#[test]
fn the_founders_time_adds_up_by_agent() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join(FILE)).unwrap();
    let one = propose(&mut s, None);
    let two = propose(&mut s, None);
    s.answer(one, None, Answer::Edit("ETH left.".into()), "app", 30)
        .unwrap();
    s.answer(one, None, Answer::Approve, "app", 12).unwrap();
    s.answer(two, None, Answer::Reject("no".into()), "telegram", 8)
        .unwrap();
    let since = Utc::now() - chrono::Duration::days(30);
    assert_eq!(
        s.owner_seconds_by_agent(since).unwrap(),
        vec![(Some("post-writer".to_string()), 50)]
    );
    let later = Utc::now() + chrono::Duration::minutes(1);
    assert!(s.owner_seconds_by_agent(later).unwrap().is_empty());
}

fn flagged(store: &mut Store, flag: &str) -> i64 {
    let args = json!({"answer": {"draft": "BUY ETH NOW"}}).to_string();
    store
        .propose_flagged(
            &NewItem {
                kind: "approval",
                title: "Post",
                agent: Some("post-writer"),
                routine: Some("daily-posts"),
                run: Some("r1"),
                action: None,
                args: &args,
                execute_at: None,
                source: r#"{"block": "ok", "editable": "answer.draft"}"#,
                model_call: None,
            },
            Some((flag, "a call to buy")),
            None,
        )
        .unwrap()
}

/// Decision 58: a blocking flag stops Approve; only the founder's override,
/// from the app or Telegram, approves it as it is, logged with who, when,
/// the channel and the reason. Overriding what is not blocked is refused.
#[test]
fn a_blocking_flag_stops_approve_until_the_founder_overrides_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join(FILE)).unwrap();
    let id = flagged(&mut s, "blocking");
    let e = s.answer(id, None, Answer::Approve, "app", 3).unwrap_err();
    assert!(format!("{e:#}").contains("a call to buy"), "{e:#}");
    let e = s.answer(id, None, Answer::Override, "cli", 3).unwrap_err();
    assert!(
        format!("{e:#}").contains("only from the app or Telegram"),
        "{e:#}"
    );
    assert_eq!(s.queue(&["pending"]).unwrap().len(), 1, "still waiting");
    let item = s.answer(id, None, Answer::Override, "telegram", 3).unwrap();
    assert_eq!(item.status, "approved");
    assert_eq!(item.flag.as_deref(), Some("overridden"));
    let log = s.queue_log(id).unwrap();
    assert!(
        log.iter()
            .any(|(action, by, channel)| action == "overridden"
                && by == "founder"
                && channel.as_deref() == Some("telegram")),
        "{log:?}"
    );
    // Tapped twice: nothing more.
    s.answer(id, None, Answer::Override, "telegram", 3).unwrap();
    let note = flagged(&mut s, "note");
    let e = s
        .answer(note, None, Answer::Override, "app", 3)
        .unwrap_err();
    assert!(format!("{e:#}").contains("no blocking flag"), "{e:#}");
    assert_eq!(
        s.answer(note, None, Answer::Approve, "app", 3)
            .unwrap()
            .status,
        "approved"
    );
}

/// Decision 58, amended: an edit lifts the block, since the text is the
/// founder's, and is logged like an override; then Approve works.
#[test]
fn an_edit_lifts_the_block_and_the_journal_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join(FILE)).unwrap();
    let id = flagged(&mut s, "blocking");
    let item = s
        .answer(
            id,
            None,
            Answer::Edit("ETH left the queue.".into()),
            "app",
            20,
        )
        .unwrap();
    assert_eq!(
        (item.status.as_str(), item.flag.as_deref()),
        ("pending", Some("lifted"))
    );
    let log = s.queue_log(id).unwrap();
    assert!(
        log.iter()
            .any(|(action, by, channel)| action == "flag lifted by edit"
                && by == "founder"
                && channel.as_deref() == Some("app")),
        "{log:?}"
    );
    assert_eq!(
        s.answer(id, None, Answer::Approve, "app", 2)
            .unwrap()
            .status,
        "approved"
    );
    // A check flags note or blocking, nothing else.
    let e = s
        .propose_flagged(
            &NewItem {
                kind: "approval",
                title: "x",
                agent: None,
                routine: None,
                run: None,
                action: None,
                args: "{}",
                execute_at: None,
                source: "{}",
                model_call: None,
            },
            Some(("overridden", "")),
            None,
        )
        .unwrap_err();
    assert!(format!("{e:#}").contains("note or blocking"), "{e:#}");
}

/// Decision 116: what the founder said about a routine's drafts, newest
/// first: an edit before approval, a reason to reject, a redo. A bare
/// rejection, an untouched approval, another routine and what the founder
/// forgot say nothing.
#[test]
fn the_founders_words_on_drafts_are_kept_for_the_next_ones() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join(FILE)).unwrap();
    let draft = |store: &mut Store, routine: &str, text: &str| {
        store
            .propose(&NewItem {
                kind: "approval",
                title: text,
                agent: Some("post-writer"),
                routine: Some(routine),
                run: Some("r1"),
                action: None,
                args: &json!({"answer": {"draft": text}}).to_string(),
                execute_at: None,
                source: r#"{"block": "ok", "show": "answer.draft", "editable": "answer.draft"}"#,
                model_call: None,
            })
            .unwrap()
    };
    let edited = draft(&mut store, "posts", "A long dry post.");
    store
        .answer(
            edited,
            None,
            Answer::Edit("Short and punchy.".into()),
            "app",
            1,
        )
        .unwrap();
    store
        .answer(edited, None, Answer::Approve, "app", 1)
        .unwrap();
    let kept = draft(&mut store, "posts", "Fine as it is.");
    store.answer(kept, None, Answer::Approve, "app", 1).unwrap();
    let bare = draft(&mut store, "posts", "Meh.");
    store
        .answer(
            bare,
            None,
            Answer::Reject("from Telegram".into()),
            "telegram",
            1,
        )
        .unwrap();
    let why = draft(&mut store, "posts", "Too technical.");
    store
        .answer(
            why,
            None,
            Answer::Reject("too much jargon".into()),
            "app",
            1,
        )
        .unwrap();
    let redo = draft(&mut store, "posts", "Flat.");
    store
        .answer(
            redo,
            None,
            Answer::Reject("redone: funnier".into()),
            "app",
            1,
        )
        .unwrap();
    let other = draft(&mut store, "replies", "Elsewhere.");
    store
        .answer(other, None, Answer::Reject("wrong tone".into()), "app", 1)
        .unwrap();
    draft(&mut store, "posts", "Still waiting.");

    let since = Utc::now() - chrono::Duration::days(30);
    let said = store.founder_feedback("posts", since, 12).unwrap();
    let kinds: Vec<(&str, &str, &str)> = said
        .iter()
        .map(|f| (f.kind, f.said.as_str(), f.edited.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("redo", "funnier", ""),
            ("reject", "too much jargon", ""),
            ("edit", "", "Short and punchy."),
        ]
    );
    assert_eq!(
        said[2].draft, "A long dry post.",
        "the draft as the agent wrote it"
    );
    assert_eq!(store.founder_feedback("posts", since, 1).unwrap().len(), 1);
    // Taken back, it is no longer said.
    store.forget_feedback(why).unwrap();
    let said = store.founder_feedback("posts", since, 12).unwrap();
    assert!(said.iter().all(|f| f.item != why));
    assert_eq!(said.len(), 2);
    // Older than the window, nothing.
    assert!(
        store
            .founder_feedback("posts", Utc::now() + chrono::Duration::hours(1), 12)
            .unwrap()
            .is_empty()
    );
}
