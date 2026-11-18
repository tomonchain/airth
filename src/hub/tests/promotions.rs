//! Decision 153: an agent's record on an action earns a proposal, checked
//! by a replay of its routine; only the founder's Allow sets the level.

use super::evals::{drafted, routine};
use super::*;
use crate::business::store::Store;
use crate::business::store::queue::{Answer, Item};

const ACTION: &str = "typefully.create_draft";

/// The posts routine of the evaluations' tests, its agent titled.
fn business() -> Fixture {
    let mut f = fixture();
    routine(&mut f);
    std::fs::write(
        f.root.path().join("agents/post-writer.md"),
        "---\ntitle: Post writer\n---\nDraft posts.",
    )
    .unwrap();
    f
}

/// `n` drafts approved as they were, spread from `from` to `to` days ago.
fn approved(f: &Fixture, n: usize, from: f64, to: f64) {
    let mut store = Store::of(&f.realm).unwrap();
    for i in 0..n {
        let id = drafted(&mut store, &format!("{from}-{i}"), &format!("Draft {i}"));
        store.answer(id, None, Answer::Approve, "app", 1).unwrap();
        let days = from - (from - to) * i as f64 / (n - 1).max(1) as f64;
        let at = Utc::now() - chrono::Duration::minutes((days * 24.0 * 60.0) as i64);
        store.backdate(id, at).unwrap();
    }
}

/// The model's answers to a replay of `n` cases: the pick keeps each one
/// but those in `dropped`, newest first, then a new draft each.
fn replay(n: usize, dropped: &[usize]) -> Vec<ModelResponse> {
    let pick = (0..n).map(|i| done(&format!(r#"{{"relevant": {}}}"#, !dropped.contains(&i))));
    let draft = (0..n).map(|i| done(&format!(r#"{{"draft": "New {i}"}}"#)));
    pick.chain(draft).collect()
}

fn promotions(f: &Fixture, statuses: &[&str]) -> Vec<Item> {
    let store = Store::of(&f.realm).unwrap();
    let items = store.queue(statuses).unwrap();
    items
        .into_iter()
        .filter(|i| i.kind == "promotion")
        .collect()
}

fn level(f: &Fixture) -> u8 {
    Store::of(&f.realm)
        .unwrap()
        .level("post-writer", ACTION, 3, 1)
        .unwrap()
}

#[test]
fn twelve_approvals_over_a_week_are_proposed_once_and_allow_sets_the_level() {
    let f = business();
    approved(&f, 12, 8.0, 0.1);
    let (hub, seen, _) = make_hub(&f.home, replay(12, &[]));

    let done = hub.propose_promotions(&f.realm).unwrap();

    assert_eq!(done.len(), 1, "{done:?}");
    let [item] = promotions(&f, &["pending"]).try_into().unwrap();
    assert_eq!(
        item.title,
        "Post writer: create draft in Typefully without asking?"
    );
    assert_eq!(
        (item.args["from"].as_u64(), item.args["to"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(item.args["count"], 12);
    assert_eq!(item.args["replay"]["cases"], 12);
    assert_eq!(item.args["replay"]["agree"], 12);
    let text = item.args["text"].as_str().unwrap();
    for said in [
        "12 approved as they were since",
        "12 of 12 match",
        "wait 30 minutes, with Cancel",
    ] {
        assert!(text.contains(said), "{said}: {text}");
    }
    assert_eq!(
        seen.lock().unwrap().len(),
        24,
        "one replay, one call a case and block"
    );
    // Nothing is proposed twice, nor replayed while it waits.
    assert!(hub.propose_promotions(&f.realm).unwrap().is_empty());
    assert_eq!(seen.lock().unwrap().len(), 24);
    assert_eq!(level(&f), 1, "only the founder promotes");

    let (answered, report) = hub
        .answer_item(&f.realm, item.id, None, Answer::Approve, "app", 1)
        .unwrap();
    assert!(report.unwrap().starts_with("Allowed."), "{answered:?}");
    assert_eq!(level(&f), 2);
    assert_eq!(promotions(&f, &["done"]).len(), 1);
    // The run that earned it does not earn another.
    assert!(hub.propose_promotions(&f.realm).unwrap().is_empty());
}

#[test]
fn not_now_changes_nothing_and_asks_again_only_after_another_twelve() {
    let f = business();
    approved(&f, 12, 20.0, 12.0);
    let (hub, _, _) = make_hub(&f.home, replay(12, &[]));
    hub.propose_promotions(&f.realm).unwrap();
    let [item] = promotions(&f, &["pending"]).try_into().unwrap();
    hub.answer_item(
        &f.realm,
        item.id,
        None,
        Answer::Reject("not now".into()),
        "app",
        1,
    )
    .unwrap();
    assert_eq!(level(&f), 1);
    assert!(hub.propose_promotions(&f.realm).unwrap().is_empty());

    // Asked eleven days ago: only what came after counts.
    let at = Utc::now() - chrono::Duration::days(11);
    Store::of(&f.realm).unwrap().backdate(item.id, at).unwrap();
    approved(&f, 11, 10.0, 1.0);
    assert!(hub.propose_promotions(&f.realm).unwrap().is_empty());
    approved(&f, 1, 0.5, 0.5);
    let (hub, _, _) = make_hub(&f.home, replay(12, &[]));
    assert_eq!(hub.propose_promotions(&f.realm).unwrap().len(), 1);
    let pending = promotions(&f, &["pending"]);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].args["count"], 12);
}

#[test]
fn nothing_is_proposed_under_twelve_within_a_week_or_at_the_ceiling() {
    for (n, days, ceiling) in [(11, 8.0, false), (12, 6.0, false), (12, 8.0, true)] {
        let f = business();
        approved(&f, n, days, 0.1);
        if ceiling {
            Store::of(&f.realm)
                .unwrap()
                .set_level("post-writer", ACTION, 3, 3, "founder", "trusted")
                .unwrap();
        }
        let (hub, seen, _) = make_hub(&f.home, replay(12, &[]));
        assert!(
            hub.propose_promotions(&f.realm).unwrap().is_empty(),
            "{n} over {days} days"
        );
        assert!(seen.lock().unwrap().is_empty(), "nothing was replayed");
        assert!(promotions(&f, &["pending", "expired"]).is_empty());
    }
}

#[test]
fn nothing_is_proposed_when_the_replay_disagrees_and_the_run_starts_again() {
    let f = business();
    approved(&f, 12, 8.0, 0.1);
    // Today's pick would drop the newest draft the founder approved.
    let (hub, seen, _) = make_hub(&f.home, replay(12, &[0]));

    let done = hub.propose_promotions(&f.realm).unwrap();

    assert!(done[0].contains("disagree with you on 1 of 12"), "{done:?}");
    assert!(promotions(&f, &["pending"]).is_empty());
    assert_eq!(promotions(&f, &["expired"]).len(), 1, "kept, never shown");
    let calls = seen.lock().unwrap().len();
    assert!(hub.propose_promotions(&f.realm).unwrap().is_empty());
    assert_eq!(seen.lock().unwrap().len(), calls, "not replayed every hour");
}
