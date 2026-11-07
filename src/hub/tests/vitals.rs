//! Decisions 53 and 90: a business's vitals for the HQ, read at most once
//! an hour from its own server's `get_vitals`, only once the founder
//! classed that tool as a read; the last figures stay a day, never more.

use super::*;
use crate::hub::vitals::{TOOL, shown};
use crate::tools::mcp::{McpStore, McpToolSpec};
use chrono::{Duration, Utc};
use serde_json::Value;

/// An MCP server that takes one token and answers `get_vitals` with the
/// next answer; it counts the calls.
fn server(answers: Vec<String>) -> (String, Arc<Mutex<usize>>) {
    let http = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", http.server_addr().to_ip().unwrap());
    let calls = Arc::new(Mutex::new(0));
    let counted = calls.clone();
    std::thread::spawn(move || {
        let mut answers = answers.into_iter();
        for mut req in http.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let token = req
                .headers()
                .iter()
                .any(|h| h.field.equiv("Authorization") && h.value.as_str() == "Bearer hg_test");
            let msg: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let id = msg["id"].clone();
            let answer = match msg["method"].as_str() {
                _ if !token => tiny_http::Response::from_string("").with_status_code(401),
                Some("initialize") => tiny_http::Response::from_string(
                    json!({"jsonrpc": "2.0", "id": id, "result": {"serverInfo": {"name": "hedgen"}}}).to_string(),
                ),
                Some("tools/call") => {
                    *counted.lock().unwrap() += 1;
                    let text = answers.next().unwrap_or_default();
                    tiny_http::Response::from_string(
                        json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": text}]}}).to_string(),
                    )
                }
                _ => tiny_http::Response::from_string("").with_status_code(202),
            };
            let _ = req.respond(answer.with_header(
                tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
            ));
        }
    });
    (url, calls)
}

#[test]
fn the_businesss_vitals_are_read_hourly_from_a_tool_classed_as_a_read() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let skills = f.realm.skills_dir();
    let good = r#"{"vitals": [
        {"key": "mrr", "label": "MRR", "value": 4501, "unit": "USD", "note": "3 paying clients"},
        {"key": "pipeline", "label": "Open deals", "value": 2, "range": [5, 50]}]}"#;
    let (url, calls) = server(vec![good.into(), "not json".into()]);
    let store = McpStore::new(&skills);
    store.save_remote("hedgen", &url, "").unwrap();
    crate::tools::remote::save_token(&skills, "hedgen", "hg_test").unwrap();
    store
        .update("hedgen", |s| {
            s.tools = vec![McpToolSpec {
                name: TOOL.into(),
                description: String::new(),
                input_schema: json!({"type": "object", "properties": {}}),
            }]
        })
        .unwrap();
    let now = Utc::now();

    // Not classed by the founder: never called.
    assert!(hub.refresh_vitals(now).unwrap().is_empty());
    assert_eq!(*calls.lock().unwrap(), 0);
    // Classed as something else than a read: still never called.
    store
        .classify("hedgen", TOOL, crate::actions::Class::Reversible, None)
        .unwrap();
    assert!(hub.refresh_vitals(now).unwrap().is_empty());
    assert_eq!(*calls.lock().unwrap(), 0);

    store
        .classify("hedgen", TOOL, crate::actions::Class::Read, None)
        .unwrap();
    let done = hub.refresh_vitals(now).unwrap();
    assert!(done[0].contains("vitals read from hedgen"), "{done:?}");
    let vitals = shown(&f.realm, now).unwrap();
    assert_eq!(vitals[0]["value"], "$4,501");
    assert!(
        vitals[0]["note"]
            .as_str()
            .unwrap()
            .starts_with("3 paying clients · From hedgen")
    );
    assert_eq!(vitals[0]["out"], false);
    assert_eq!(vitals[1]["out"], true, "2 is under its range");

    // Within the hour: not called again.
    hub.refresh_vitals(now + Duration::minutes(30)).unwrap();
    assert_eq!(*calls.lock().unwrap(), 1);
    // A wrong answer later: refused whole, the last figures stay a day.
    let done = hub.refresh_vitals(now + Duration::hours(2)).unwrap();
    assert!(done[0].contains("vitals not read"), "{done:?}");
    assert_eq!(
        shown(&f.realm, now + Duration::hours(3)).unwrap()[0]["value"],
        "$4,501"
    );
    assert!(shown(&f.realm, now + Duration::days(1)).is_none());
    // The kept file is private to the owner.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(f.realm.dir().join("vitals.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
