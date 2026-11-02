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
