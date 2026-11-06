use super::*;
use crate::business::store::autonomy::NewItem;
use crate::home::{Home, NewRealm};
use std::cell::RefCell;

/// Stands in for Telegram: keeps every call, numbers the messages.
#[derive(Default)]
struct Fake {
    calls: RefCell<Vec<(String, Value)>>,
}

impl Api for Fake {
    fn call(&self, method: &str, body: &Value) -> Result<Value> {
        let mut calls = self.calls.borrow_mut();
        calls.push((method.to_string(), body.clone()));
        Ok(match method {
            "getMe" => json!({"username": "acme_bot"}),
            _ => json!({"message_id": 100 + calls.len()}),
        })
    }
}

impl Fake {
    fn sent(&self) -> Vec<Value> {
        self.calls
            .borrow()
            .iter()
            .filter(|(m, _)| m == "sendMessage")
            .map(|(_, b)| b.clone())
            .collect()
    }
}

/// A home with the business `acme` and the hub over it.
fn business() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Hub,
    crate::realm::Realm,
) {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::at(dir.path()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let realm = home
        .create_realm(NewRealm {
            name: "acme".into(),
            description: String::new(),
            root: Some(root.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    (dir, root, Hub::new(home), realm)
}

/// A post waiting for the founder, its draft editable.
fn post(realm: &crate::realm::Realm, draft: &str) {
    let args = json!({"answer": {"draft": draft}}).to_string();
    Store::of(realm)
        .unwrap()
        .propose(&NewItem {
            kind: "approval",
            title: draft,
            agent: Some("post-writer"),
            routine: None,
            run: None,
            action: None,
            args: &args,
            execute_at: None,
            source: r#"{"show": "answer.draft", "editable": "answer.draft"}"#,
            model_call: None,
        })
        .unwrap();
}

fn message(chat: i64, user: i64, text: &str) -> Value {
    json!({"update_id": 1, "message": {"chat": {"id": chat}, "from": {"id": user}, "text": text}})
}

#[test]
fn only_the_founder_pairs_with_the_link() {
    let (_home, _root, hub, _) = business();
    let api = Fake::default();
    let mut config = Config {
        code: "c1".into(),
        ..Config::default()
    };
    handle(&hub, &api, &mut config, &message(5, 7, "/start wrong")).unwrap();
    let mut forwarded = message(5, 7, "/start c1");
    forwarded["message"]["forward_date"] = json!(1);
    handle(&hub, &api, &mut config, &forwarded).unwrap();
    assert_eq!(config.chat, None);
    handle(&hub, &api, &mut config, &message(5, 7, "/start c1")).unwrap();
    assert_eq!(
        (config.chat, config.user, config.code.as_str()),
        (Some(5), Some(7), "")
    );
    // Once paired: the same chat and user only, never a forwarded message.
    assert!(from_founder(&config, &message(5, 7, "x")["message"]));
    assert!(
        !from_founder(&config, &message(5, 8, "x")["message"]),
        "another user in the chat"
    );
    assert!(
        !from_founder(&config, &message(6, 7, "x")["message"]),
        "another chat"
    );
    assert!(!from_founder(&config, &forwarded["message"]));
}

#[test]
fn each_item_goes_once_whole_and_failures_follow() {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::at(dir.path()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let realm = home
        .create_realm(NewRealm {
            name: "acme".into(),
            description: String::new(),
            root: Some(root.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    let hub = Hub::new(home.clone());
    let mut store = Store::of(&realm).unwrap();
    let long = format!("Line one.\n{}", "x".repeat(9000));
    for draft in ["ETH left the queue.", long.as_str()] {
        let args = json!({"answer": {"draft": draft}}).to_string();
        store
            .propose(&NewItem {
                kind: "approval",
                title: "A post",
                agent: Some("post-writer"),
                routine: Some("daily-posts"),
                run: Some("r1"),
                action: None,
                args: &args,
                execute_at: None,
                source: r#"{"block": "ok", "show": "answer.draft", "editable": "answer.draft"}"#,
                model_call: None,
            })
            .unwrap();
    }
    home.inbox()
        .push(
            crate::notify::Kind::Failed,
            "acme",
            None,
            "daily-watch failed",
            "no feed",
        )
        .unwrap();
    let api = Fake::default();
    let mut config = Config {
        chat: Some(5),
        user: Some(7),
        ..Config::default()
    };
    push(&hub, &api, &mut config).unwrap();
    let sent = api.sent();
    assert!(
        sent[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("acme · Needs you\nA post\n\nETH left the queue.")
    );
    // The long one: every character, in pieces, the buttons on the last only.
    let pieces: Vec<&Value> = sent[1..sent.len() - 1].iter().collect();
    assert!(pieces.len() >= 3, "{}", pieces.len());
    let joined: String = pieces.iter().map(|p| p["text"].as_str().unwrap()).collect();
    assert_eq!(joined.matches('x').count(), 9000);
    assert!(
        pieces
            .iter()
            .all(|p| p["text"].as_str().unwrap().chars().count() <= 4096)
    );
    assert!(
        pieces[..pieces.len() - 1]
            .iter()
            .all(|p| p["reply_markup"].is_null())
    );
    let keys = &pieces.last().unwrap()["reply_markup"]["inline_keyboard"][0];
    assert_eq!(keys[1]["callback_data"], "e/acme/2/1");
    assert!(
        sent.last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("daily-watch failed")
    );
    // Nothing goes twice.
    push(&hub, &api, &mut config).unwrap();
    assert_eq!(api.sent().len(), sent.len());
    assert_eq!(config.sent.len(), 2);
}

fn tap(user: i64, data: &str) -> Value {
    json!({"update_id": 2, "callback_query": {"id": "q", "from": {"id": user}, "message": {"chat": {"id": 5}}, "data": data}})
}

fn reply(to: i64, text: &str) -> Value {
    let mut m = message(5, 7, text);
    m["message"]["reply_to_message"] = json!({"message_id": to});
    m
}

/// Decision 52: an edit counts only as a reply to its own item's message;
/// a stale button and another user's tap are refused.
#[test]
fn an_edit_is_a_reply_to_its_item_and_stale_buttons_are_refused() {
    let (_home, _root, hub, realm) = business();
    post(&realm, "ETH left the queue.");
    post(&realm, "Staking is up.");
    let api = Fake::default();
    let mut config = Config {
        chat: Some(5),
        user: Some(7),
        ..Config::default()
    };
    push(&hub, &api, &mut config).unwrap();
    let first = config.sent["acme/1"].messages[0];
    let second = config.sent["acme/2"].messages[0];
    let draft = |id: i64| {
        Store::of(&realm)
            .unwrap()
            .queue(&["pending", "approved"])
            .unwrap()
            .into_iter()
            .find(|i| i.id == id)
            .unwrap()
            .args["answer"]["draft"]
            .clone()
    };

    // Someone else's tap changes nothing.
    handle(&hub, &api, &mut config, &tap(8, "e/acme/1/1")).unwrap();
    assert_eq!(config.editing, None);
    handle(&hub, &api, &mut config, &tap(7, "e/acme/1/1")).unwrap();
    assert_eq!(config.editing.as_deref(), Some("acme/1"));
    // A free message, or a reply to the other item: asked again.
    let asked = api.sent().len();
    handle(&hub, &api, &mut config, &message(5, 7, "ETH left. Calmly.")).unwrap();
    handle(&hub, &api, &mut config, &reply(second, "ETH left. Calmly.")).unwrap();
    assert_eq!(api.sent().len(), asked + 2);
    assert!(
        api.sent().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("reply to its message")
    );
    assert_eq!(draft(1), "ETH left the queue.");
    assert_eq!(draft(2), "Staking is up.");
    // The reply to the item's own message is the edit; it comes back.
    handle(&hub, &api, &mut config, &reply(first, "ETH left. Calmly.")).unwrap();
    assert_eq!(draft(1), "ETH left. Calmly.");
    assert_eq!(config.editing, None);
    assert_eq!(config.sent["acme/1"].revision, 2);
    // The old buttons are out of date; the new ones work.
    handle(&hub, &api, &mut config, &tap(7, "a/acme/1/1")).unwrap();
    assert!(
        api.sent().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .starts_with("Not done")
    );
    handle(&hub, &api, &mut config, &tap(7, "a/acme/1/2")).unwrap();
    assert!(
        api.sent().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .starts_with("acme · approved")
    );
}

/// Decision 66: a legal deadline is answered Filed or Seen on the phone; an
/// overdue one, Filed only.
#[test]
fn a_deadline_is_filed_or_seen_and_overdue_only_filed() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = crate::business::store::Store::open(&dir.path().join("business.db")).unwrap();
    for (key, overdue) in [("cs/2026-11-06/30", false), ("cs/2026-11-06/overdue", true)] {
        store
            .propose(&NewItem {
                kind: "deadline",
                title: "Confirmation statement due Fri 20 Nov 2026",
                agent: None,
                routine: None,
                run: None,
                action: None,
                args: &json!({"detail": "Companies House.", "overdue": overdue}).to_string(),
                execute_at: None,
                source: &json!({"key": key, "show": "detail"}).to_string(),
                model_call: None,
            })
            .unwrap();
    }
    let items = store.queue(&["pending"]).unwrap();
    let labels = |item| -> Vec<String> {
        buttons("acme", item)[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["text"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(labels(&items[0]), ["Filed", "Seen"]);
    assert_eq!(labels(&items[1]), ["Filed"]);
    assert!(item_text("acme", &items[0]).starts_with("acme · Legal deadline"));
}

/// Decision 58: on the phone, a blocked item says why and offers Override,
/// Edit and Reject, never a plain Approve; a note is shown and blocks
/// nothing.
#[test]
fn a_blocked_item_offers_override_not_approve() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = crate::business::store::Store::open(&dir.path().join("business.db")).unwrap();
    let new = NewItem {
        kind: "approval",
        title: "Post",
        agent: None,
        routine: Some("daily-posts"),
        run: Some("r1"),
        action: None,
        args: r#"{"answer": {"draft": "BUY ETH NOW"}}"#,
        execute_at: None,
        source: r#"{"block": "ok", "show": "answer.draft", "editable": "answer.draft"}"#,
        model_call: None,
    };
    store
        .propose_flagged(&new, Some(("blocking", "a call to buy")), None)
        .unwrap();
    store
        .propose_flagged(&new, Some(("note", "long")), None)
        .unwrap();
    let items = store.queue(&["pending"]).unwrap();
    let labels = |item| -> Vec<String> {
        buttons("acme", item)[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["text"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(labels(&items[0]), ["Override", "Edit", "Reject"]);
    assert!(item_text("acme", &items[0]).contains("Flagged by the check: a call to buy"));
    assert_eq!(labels(&items[1]), ["Approve", "Edit", "Reject"]);
    assert!(item_text("acme", &items[1]).contains("Note from the check: long"));
    // What the same run dropped reaches the phone too (decision 61).
    store
        .propose_flagged(
            &new,
            None,
            Some(r#"draft: 1 of 3 dropped: "Hack" (too long)"#),
        )
        .unwrap();
    let items = store.queue(&["pending"]).unwrap();
    assert!(
        item_text("acme", &items[2])
            .contains(r#"Also in this run: draft: 1 of 3 dropped: "Hack" (too long)"#)
    );
}

/// An approved item whose send failed says so, with its reason in one
/// line, never "done".
#[test]
fn an_approved_item_that_failed_to_send_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join(crate::business::store::FILE)).unwrap();
    let new = NewItem {
        kind: "approval",
        title: "HKMA",
        agent: None,
        routine: Some("daily-posts"),
        run: Some("r1"),
        action: None,
        args: "{}",
        execute_at: None,
        source: r#"{"block": "ok"}"#,
        model_call: None,
    };
    store.propose(&new).unwrap();
    // As the hub leaves it once the approved send failed.
    let mut item = store.queue(&["pending"]).unwrap().remove(0);
    item.status = "failed".into();
    assert_eq!(
        answered(
            "acme",
            &item,
            Some("typefully.create_draft: HTTP 403 Forbidden: v1 is closed")
        ),
        "acme · approved, sending failed: HKMA\ntypefully.create_draft: HTTP 403 Forbidden: v1 is closed"
    );
    let mut done = item.clone();
    done.status = "done".into();
    assert!(answered("acme", &done, None).starts_with("acme · done: HKMA"));
}

/// Decision 62 on the phone: Retry when nothing was done; "Retry…" first
/// asks the founder to check when it may have been; an unknown button does
/// nothing, never a rejection.
#[test]
fn an_incident_offers_retry_and_an_ambiguous_one_asks_first() {
    let (_home, _root, hub, realm) = business();
    let incident = |title: &str, retry: Option<&str>| {
        Store::of(&realm)
            .unwrap()
            .propose(&NewItem {
                kind: "incident",
                title,
                agent: None,
                routine: None,
                run: None,
                action: Some("typefully.create_draft"),
                args: &json!({"detail": "HTTP 403", "key": "posts/a/send", "retry": retry})
                    .to_string(),
                execute_at: None,
                source: r#"{"show": "detail"}"#,
                model_call: None,
            })
            .unwrap()
    };
    incident("refused", Some("clear"));
    incident("cut", Some("ambiguous"));
    incident("commits", None);
    let api = Fake::default();
    let mut config = Config {
        chat: Some(5),
        user: Some(7),
        ..Config::default()
    };
    push(&hub, &api, &mut config).unwrap();
    let labels = |n: usize| -> Vec<String> {
        let markup = &api.sent()[n]["reply_markup"]["inline_keyboard"][0];
        markup
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["text"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(labels(0), ["Retry", "Seen"]);
    assert_eq!(labels(1), ["Retry…", "Seen"]);
    assert_eq!(labels(2), ["Seen"]);
    // "Retry…" asks first; its button is the checked retry.
    handle(&hub, &api, &mut config, &tap(7, "u/acme/2/1")).unwrap();
    let asked = api.sent().last().unwrap().clone();
    assert!(
        asked["text"]
            .as_str()
            .unwrap()
            .contains("Check that nothing was created")
    );
    assert_eq!(
        asked["reply_markup"]["inline_keyboard"][0][0]["callback_data"],
        "c/acme/2/1"
    );
    // An unknown button answers nothing and leaves the item waiting.
    let before = api.sent().len();
    handle(&hub, &api, &mut config, &tap(7, "z/acme/3/1")).unwrap();
    assert_eq!(api.sent().len(), before);
    assert!(
        Store::of(&realm)
            .unwrap()
            .queue(&["pending"])
            .unwrap()
            .iter()
            .any(|i| i.id == 3)
    );
    // Retry is taken; this one belongs to no routine, so it says it failed.
    handle(&hub, &api, &mut config, &tap(7, "t/acme/1/1")).unwrap();
    let said = api.sent().last().unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(said.starts_with("acme · retry failed: refused"), "{said}");
}

/// Decision 63 on the phone: the founder's message with a post link drafts
/// a reply, which comes next for approval; a post too thin to answer is
/// said; a forwarded message, or a reply to an item, starts nothing.
#[test]
fn a_post_link_from_the_founder_drafts_a_reply() {
    let (_home, root, hub, realm) = business();
    let api = Fake::default();
    let mut config = Config {
        chat: Some(5),
        user: Some(7),
        ..Config::default()
    };
    let link = "https://x.com/jack/status/777";
    handle(&hub, &api, &mut config, &message(5, 7, link)).unwrap();
    assert!(
        api.sent().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("No business has a reply routine")
    );
    std::fs::create_dir_all(root.path().join("agents")).unwrap();
    std::fs::create_dir_all(root.path().join("routines")).unwrap();
    std::fs::write(root.path().join("agents/writer.md"), "Reply.").unwrap();
    std::fs::write(
        root.path().join("routines/replies.json"),
        json!({"agent": "writer", "trigger": {"telegram": "x_post"}, "blocks": [
            {"id": "post", "type": "read", "source": "x_post"},
            {"id": "ok", "type": "approval", "show": "summary"}]})
        .to_string(),
    )
    .unwrap();
    let oembed = |text: &str| {
        json!({"url": link, "author_name": "jack",
            "html": format!("<blockquote><p>{text}</p>&mdash; jack <a href=\"x\">November 3, 2026</a></blockquote>")})
        .to_string()
    };
    let url = crate::routine::x_post::oembed_url(link);
    crate::hub::TEST_FEEDS.lock().unwrap().insert(
        url.clone(),
        oembed("Staking yields fell to 2.8% this week and withdrawals doubled."),
    );
    handle(
        &hub,
        &api,
        &mut config,
        &message(5, 7, &format!("acme {link} push back on the yield claim")),
    )
    .unwrap();
    let said = api.sent().last().unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        said,
        "acme · reply drafted: it comes next, for your approval."
    );
    let items = Store::of(&realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].args["founder_angle"],
        "push back on the yield claim"
    );
    // A forwarded message, or one replying to an item, starts nothing.
    let before = api.sent().len();
    let mut forwarded = message(5, 7, link);
    forwarded["message"]["forward_date"] = json!(1);
    handle(&hub, &api, &mut config, &forwarded).unwrap();
    let mut reply = message(5, 7, link);
    reply["message"]["reply_to_message"] = json!({"message_id": 1});
    handle(&hub, &api, &mut config, &reply).unwrap();
    assert_eq!(api.sent().len(), before);
    assert_eq!(
        Store::of(&realm)
            .unwrap()
            .queue(&["pending"])
            .unwrap()
            .len(),
        1
    );
    // Too thin: said, nothing drafted.
    crate::hub::TEST_FEEDS
        .lock()
        .unwrap()
        .insert(url, oembed("pic.twitter.com/abc"));
    handle(&hub, &api, &mut config, &message(5, 7, link)).unwrap();
    let said = api.sent().last().unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        said.starts_with("acme · not drafted: the post is mostly an image or a chart"),
        "{said}"
    );
    assert_eq!(
        Store::of(&realm)
            .unwrap()
            .queue(&["pending"])
            .unwrap()
            .len(),
        1
    );
}
