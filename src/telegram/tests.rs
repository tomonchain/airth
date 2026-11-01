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

fn message(chat: i64, user: i64, text: &str) -> Value {
    json!({"update_id": 1, "message": {"chat": {"id": chat}, "from": {"id": user}, "text": text}})
}

#[test]
fn only_the_founder_pairs_with_the_link() {
    let api = Fake::default();
    let mut config = Config {
        code: "c1".into(),
        ..Config::default()
    };
    handle(&api, &mut config, &message(5, 7, "/start wrong")).unwrap();
    let mut forwarded = message(5, 7, "/start c1");
    forwarded["message"]["forward_date"] = json!(1);
    handle(&api, &mut config, &forwarded).unwrap();
    assert_eq!(config.chat, None);
    handle(&api, &mut config, &message(5, 7, "/start c1")).unwrap();
    assert_eq!(
        (config.chat, config.user, config.code.as_str()),
        (Some(5), Some(7), "")
    );
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
