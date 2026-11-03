//! Decisions 53 and 54: the bank's figures for the HQ, read at most once an
//! hour from a connected bank only, and unknown, never stale, when the
//! consent is gone; a refused consent is one incident, not one an hour.

use super::*;
use crate::business::store::Store;
use crate::hub::finance::{REAUTHORIZE, shown};
use aws_lc_rs::encoding::AsDer;
use chrono::{Duration, Utc};
use serde_json::Value;

/// A Revolut that answers each request with the next answer, and counts.
fn revolut(answers: Vec<(u16, Value)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let api = format!("http://{}", server.server_addr().to_ip().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    std::thread::spawn(move || {
        let mut answers = answers.into_iter();
        for req in server.incoming_requests() {
            kept.lock().unwrap().push(req.url().to_string());
            let (code, answer) = answers.next().unwrap_or((500, json!({})));
            req.respond(
                tiny_http::Response::from_string(answer.to_string()).with_status_code(code),
            )
            .unwrap();
        }
    });
    (api, seen)
}

#[test]
fn the_banks_figures_are_read_hourly_and_unknown_once_the_consent_is_gone() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let now = Utc::now();
    // No bank connected: nothing is called, and the HQ shows the empty state.
    assert!(hub.refresh_finance(now).unwrap().is_empty());
    assert!(shown(&f.realm, now).is_none());
    let (api, seen) = revolut(vec![
        (
            200,
            json!({"access_token": "a1", "expires_in": 2400, "refresh_token": "r1"}),
        ),
        (
            200,
            json!([{"balance": 12300.0, "currency": "GBP", "state": "active"}]),
        ),
        (
            200,
            json!([{"created_at": now.format("%Y-%m-%dT09:00:00Z").to_string(), "type": "card_payment", "state": "completed",
            "legs": [{"amount": -420.0, "currency": "GBP", "counterparty": {"account_type": "external"}}]}]),
        ),
        (400, json!({"error": "invalid_grant"})),
        (400, json!({"error": "invalid_grant"})),
    ]);
    let pair = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
    let key: aws_lc_rs::encoding::Pkcs8V1Der = pair.as_der().unwrap();
    let redirect = "https://hedgen.xyz/airth/revolut";
    crate::bank::connect(
        &f.realm.skills_dir(),
        &api,
        "cid",
        redirect,
        key.as_ref(),
        &format!("{redirect}?code=oa_1"),
        now,
    )
    .unwrap();
    hub.refresh_finance(now).unwrap();
    let figures = shown(&f.realm, now).unwrap();
    assert_eq!(
        (figures["cash"].as_str(), figures["out"].as_str()),
        (Some("£12,300"), Some("£420"))
    );
    assert!(
        figures["source"]
            .as_str()
            .unwrap()
            .starts_with("Revolut Business · read-only · ")
    );
    // Within the hour: no call.
    let calls = seen.lock().unwrap().len();
    hub.refresh_finance(now + Duration::minutes(30)).unwrap();
    assert_eq!(seen.lock().unwrap().len(), calls);
    // A day without a read: unknown, never yesterday's figures.
    assert_eq!(
        shown(&f.realm, now + Duration::days(2)).unwrap()["unknown"],
        "Last read more than a day ago"
    );
    // The consent is gone: unknown, and one incident however often it is tried.
    hub.refresh_finance(now + Duration::hours(2)).unwrap();
    hub.refresh_finance(now + Duration::hours(4)).unwrap();
    assert_eq!(
        shown(&f.realm, now + Duration::hours(4)).unwrap()["unknown"],
        REAUTHORIZE
    );
    let incidents: Vec<_> = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident" && i.title == REAUTHORIZE)
        .collect();
    assert_eq!(incidents.len(), 1);
    assert!(
        incidents[0].args["detail"]
            .as_str()
            .unwrap()
            .contains("airth bank connect")
    );
}

/// Another refusal than a lost consent is an ordinary incident, with
/// Revolut's message; it is not a request to re-authorize.
#[test]
fn another_refusal_is_an_ordinary_incident_with_revolut_s_message() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let now = Utc::now();
    let (api, _) = revolut(vec![
        (
            200,
            json!({"access_token": "a1", "expires_in": 2400, "refresh_token": "r1"}),
        ),
        (400, json!({"message": "from must be a date"})),
        (400, json!({"message": "from must be a date"})),
    ]);
    let pair = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
    let key: aws_lc_rs::encoding::Pkcs8V1Der = pair.as_der().unwrap();
    let redirect = "https://hedgen.xyz/airth/revolut";
    crate::bank::connect(
        &f.realm.skills_dir(),
        &api,
        "cid",
        redirect,
        key.as_ref(),
        &format!("{redirect}?code=oa_1"),
        now,
    )
    .unwrap();
    hub.refresh_finance(now).unwrap();
    hub.refresh_finance(now + Duration::hours(2)).unwrap();
    let incidents: Vec<_> = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident")
        .collect();
    assert_eq!(incidents.len(), 1, "once while open");
    assert_eq!(incidents[0].title, crate::hub::finance::REFUSED);
    assert!(
        incidents[0].args["detail"]
            .as_str()
            .unwrap()
            .contains("HTTP 400: from must be a date")
    );
    assert_ne!(shown(&f.realm, now).unwrap()["unknown"], REAUTHORIZE);
}
