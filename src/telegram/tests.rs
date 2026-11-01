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
            "getMe" => json!({"username": "hedgen_bot"}),
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

/// A home with the business `hedgen` and the hub over it.
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
            name: "hedgen".into(),
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
            name: "hedgen".into(),
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
            "hedgen",
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
            .starts_with("hedgen · Needs you\nA post\n\nETH left the queue.")
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
    assert_eq!(keys[1]["callback_data"], "e/hedgen/2/1");
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
    let first = config.sent["hedgen/1"].messages[0];
    let second = config.sent["hedgen/2"].messages[0];
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
    handle(&hub, &api, &mut config, &tap(8, "e/hedgen/1/1")).unwrap();
    assert_eq!(config.editing, None);
    handle(&hub, &api, &mut config, &tap(7, "e/hedgen/1/1")).unwrap();
    assert_eq!(config.editing.as_deref(), Some("hedgen/1"));
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
    assert_eq!(config.sent["hedgen/1"].revision, 2);
    // The old buttons are out of date; the new ones work.
    handle(&hub, &api, &mut config, &tap(7, "a/hedgen/1/1")).unwrap();
    assert!(
        api.sent().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .starts_with("Not done")
    );
    handle(&hub, &api, &mut config, &tap(7, "a/hedgen/1/2")).unwrap();
    assert!(
        api.sent().last().unwrap()["text"]
            .as_str()
            .unwrap()
            .starts_with("hedgen · approved")
    );
}
