//! Decision 55: a failure of the service never demotes the agent.

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

/// A realm whose agent proposed a write to `base`, waiting in the queue as
/// item 1; the agent is at level 2.
fn proposed(base: &str) -> Fixture {
    let mut f = fixture();
    let action = json!({"name": "create", "method": "POST", "path": "/v1/items",
        "fields": [{"name": "text", "place": "body"}], "class": "reversible"});
    f.realm
        .set_integration(crate::integration::Integration {
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

#[test]
fn a_write_cut_after_sending_is_an_incident_that_does_not_demote() {
    let (base, got) = service(vec![Reply::Cut]);
    let f = proposed(&base);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (closed, _) = approve(&hub, &f);
    assert_eq!(closed.status, "failed");
    assert_eq!(incidents(&f).len(), 1);
    assert_eq!(got.lock().unwrap().len(), 1);
    assert_eq!(level(&f), 2, "the service failed, not the agent");
}

#[test]
fn a_504_on_a_write_is_an_incident_that_does_not_demote() {
    let (base, got) = service(vec![Reply::Status(504, "")]);
    let f = proposed(&base);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let (closed, _) = approve(&hub, &f);
    assert_eq!(closed.status, "failed");
    assert_eq!(incidents(&f).len(), 1);
    assert_eq!(got.lock().unwrap().len(), 1);
    assert_eq!(level(&f), 2);
}
