//! Decision 55: a failed action is tried again only when that cannot do it
//! twice, and a failure of the service never demotes the agent.

use super::*;
use crate::business::store::Store;
use crate::business::store::autonomy::NewItem;
use crate::business::store::queue::{Answer, Item};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// What the service does with each request in turn: answer with a status
/// and extra header lines, or close the connection once it has read it.
enum Reply {
    Status(u16, &'static str),
    Cut,
}

/// A service on loopback that replies as `script` says, and keeps each
/// request it read.
fn service(script: Vec<Reply>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = got.clone();
    std::thread::spawn(move || {
        for reply in script {
            let (mut stream, _) = listener.accept().unwrap();
            kept.lock().unwrap().push(read_request(&mut stream));
            if let Reply::Status(code, headers) = reply {
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code} X\r\n{headers}content-length: 2\r\nconnection: close\r\n\r\n{{}}"
                );
            }
        }
    });
    (base, got)
}

fn read_request(stream: &mut TcpStream) -> String {
    let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
    loop {
        let n = stream.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf).to_lowercase();
        if let Some(end) = text.find("\r\n\r\n") {
            let len = text[..end]
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map_or(0, |v| v.trim().parse().unwrap());
            if buf.len() >= end + 4 + len {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buf).to_lowercase()
}

/// An address where nothing listens: a request there never leaves.
fn nowhere() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

/// A realm whose agent proposed a write to `base` (with an idempotency key
/// when `keyed`), waiting in the queue as item 1; the agent is at level 2.
fn proposed(base: &str, keyed: bool) -> Fixture {
    let mut f = fixture();
    let mut action = json!({"name": "create", "method": "POST", "path": "/v1/items",
        "fields": [{"name": "text", "place": "body"}], "class": "reversible"});
    if keyed {
        action["idempotency_header"] = "Idempotency-Key".into();
    }
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
            name: "svc".into(),
            description: String::new(),
            base_url: base.into(),
            headers: Default::default(),
            actions: vec![serde_json::from_value(action).unwrap()],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    let mut store = Store::of(&f.realm).unwrap();
    store
        .set_level("writer", "svc.create", 2, 3, "founder", "")
        .unwrap();
    let args = json!({"integration": "svc", "method": "POST", "url": format!("{base}/v1/items"),
        "body": "{\"text\": \"hi\"}", "headers": {"Content-Type": "application/json"}});
    store
        .propose(&NewItem {
            kind: "approval",
            title: "writer: svc.create",
            agent: Some("writer"),
            routine: Some("missions"),
            run: Some("r1"),
            action: Some("svc.create"),
            args: &args.to_string(),
            execute_at: None,
            source: r#"{"tool": "http_request"}"#,
            model_call: None,
        })
        .unwrap();
    f
}

fn approve(hub: &Hub, f: &Fixture) -> (Item, String) {
    let (item, report) = hub
        .answer_item(&f.realm, 1, None, Answer::Approve, "app", 5)
        .unwrap();
    (item, report.unwrap_or_default())
}

fn item(f: &Fixture) -> Item {
    let all = Store::of(&f.realm)
        .unwrap()
        .queue(&["approved", "done", "failed"]);
    all.unwrap().into_iter().find(|i| i.id == 1).unwrap()
}

fn incidents(f: &Fixture) -> Vec<Item> {
    let pending = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    pending
        .into_iter()
        .filter(|i| i.kind == "incident")
        .collect()
}

fn level(f: &Fixture) -> u8 {
    let store = Store::of(&f.realm).unwrap();
    store.level("writer", "svc.create", 3, 1).unwrap()
}

fn in_secs(s: i64) -> DateTime<Utc> {
    Utc::now() + chrono::Duration::seconds(s)
}

#[test]
fn a_request_that_never_left_is_tried_three_times_then_is_an_incident_that_does_not_demote() {
    let f = proposed(&nowhere(), false);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (waiting, report) = approve(&hub, &f);
    assert!(report.contains("tried again at"), "{report}");
    assert_eq!(waiting.status, "approved", "open until its last attempt");
    assert!(incidents(&f).is_empty());
    assert!(hub.retry_due(in_secs(30)).unwrap().is_empty(), "a minute");
    let second = hub.retry_due(in_secs(61)).unwrap();
    assert!(second[0].contains("failed, tried again at"), "{second:?}");
    assert!(hub.retry_due(in_secs(240)).unwrap().is_empty(), "five more");
    let third = hub.retry_due(in_secs(370)).unwrap();
    assert!(third[0].contains("an incident"), "{third:?}");
    assert!(
        hub.retry_due(in_secs(3600)).unwrap().is_empty(),
        "no fourth"
    );
    let found = incidents(&f);
    assert_eq!(found.len(), 1);
    let detail = found[0].args["detail"].as_str().unwrap();
    assert!(detail.contains("failed three times"), "{detail}");
    assert_eq!(level(&f), 2, "the service failed, not the agent");
    assert_eq!(item(&f).status, "failed");
}

#[test]
fn a_write_cut_after_sending_is_an_incident_never_tried_again() {
    let (base, got) = service(vec![Reply::Cut]);
    let f = proposed(&base, false);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (closed, _) = approve(&hub, &f);
    assert_eq!(closed.status, "failed");
    let found = incidents(&f);
    assert_eq!(found.len(), 1);
    let detail = found[0].args["detail"].as_str().unwrap();
    assert!(detail.contains("may have been done"), "{detail}");
    assert!(hub.retry_due(in_secs(3600)).unwrap().is_empty());
    assert_eq!(got.lock().unwrap().len(), 1);
    assert_eq!(level(&f), 2);
}

#[test]
fn a_write_with_an_idempotency_key_is_tried_again_after_a_cut_with_the_same_key() {
    let (base, got) = service(vec![Reply::Cut, Reply::Status(201, "")]);
    let f = proposed(&base, true);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (waiting, _) = approve(&hub, &f);
    assert_eq!(waiting.status, "approved");
    let done = hub.retry_due(in_secs(61)).unwrap();
    assert!(done[0].ends_with("done"), "{done:?}");
    assert_eq!(item(&f).status, "done");
    let requests = got.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| r.contains("idempotency-key: item/1\r\n")),
        "{requests:?}"
    );
    assert!(incidents(&f).is_empty());
}

#[test]
fn a_429_is_tried_again_after_its_retry_after() {
    let (base, got) = service(vec![
        Reply::Status(429, "retry-after: 30\r\n"),
        Reply::Status(201, ""),
    ]);
    let f = proposed(&base, false);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    approve(&hub, &f);
    assert!(hub.retry_due(in_secs(20)).unwrap().is_empty(), "not before");
    let done = hub.retry_due(in_secs(31)).unwrap();
    assert!(done[0].ends_with("done"), "{done:?}");
    assert_eq!(item(&f).status, "done");
    assert_eq!(got.lock().unwrap().len(), 2);
    assert!(incidents(&f).is_empty());
    assert_eq!(level(&f), 2);
}

#[test]
fn a_504_on_a_write_is_an_incident_that_does_not_demote() {
    let (base, got) = service(vec![Reply::Status(504, "")]);
    let f = proposed(&base, false);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (closed, _) = approve(&hub, &f);
    assert_eq!(closed.status, "failed");
    let found = incidents(&f);
    assert_eq!(found.len(), 1);
    let detail = found[0].args["detail"].as_str().unwrap();
    assert!(detail.contains("may have been done"), "{detail}");
    assert!(hub.retry_due(in_secs(3600)).unwrap().is_empty());
    assert_eq!(got.lock().unwrap().len(), 1);
    assert_eq!(level(&f), 2);
}

#[test]
fn a_retry_waiting_when_airth_restarts_is_still_done_once() {
    let (base, got) = service(vec![Reply::Status(503, ""), Reply::Status(201, "")]);
    let f = proposed(&base, false);
    {
        let (hub, _, _) = make_hub(&f.home, vec![]);
        approve(&hub, &f);
    }
    // Airth restarts: a new hub, nothing in memory.
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let done = hub.retry_due(in_secs(61)).unwrap();
    assert!(done[0].ends_with("done"), "{done:?}");
    assert!(hub.retry_due(in_secs(3600)).unwrap().is_empty());
    assert_eq!(item(&f).status, "done");
    assert_eq!(got.lock().unwrap().len(), 2);
    assert_eq!(
        Store::of(&f.realm).unwrap().journal().unwrap(),
        vec![("item/1".to_string(), "confirmed".to_string(), true)]
    );
}
