use super::*;
use serde_json::json;

/// A Drive with the shared folder R, its subfolder D1 holding a Doc, a
/// Sheet in R, and a Doc in another folder X the account may also read.
fn fake_drive() -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    std::thread::spawn(move || {
        let meta = |id: &str| -> Option<Value> {
            Some(match id {
                "R" => {
                    json!({"id": "R", "name": "Hedgen", "mimeType": "application/vnd.google-apps.folder", "parents": ["root"]})
                }
                "D1" => {
                    json!({"id": "D1", "name": "Legal", "mimeType": "application/vnd.google-apps.folder", "parents": ["R"]})
                }
                "doc" => {
                    json!({"id": "doc", "name": "Articles", "mimeType": "application/vnd.google-apps.document", "parents": ["D1"]})
                }
                "sheet" => {
                    json!({"id": "sheet", "name": "Budget", "mimeType": "application/vnd.google-apps.spreadsheet", "parents": ["R"]})
                }
                "pdf" => {
                    json!({"id": "pdf", "name": "taxpayer_id.pdf", "mimeType": "application/pdf", "parents": ["R"]})
                }
                "X" => {
                    json!({"id": "X", "name": "Personal", "mimeType": "application/vnd.google-apps.folder", "parents": ["root"]})
                }
                "out" => {
                    json!({"id": "out", "name": "Tax return", "mimeType": "application/vnd.google-apps.document", "parents": ["X"]})
                }
                "root" => {
                    json!({"id": "root", "name": "My Drive", "mimeType": "application/vnd.google-apps.folder"})
                }
                _ => return None,
            })
        };
        for req in server.incoming_requests() {
            let authorized = req
                .headers()
                .iter()
                .any(|h| h.field.equiv("Authorization") && h.value.as_str() == "Bearer tok");
            let url = url::Url::parse(&format!("http://x{}", req.url())).unwrap();
            let q: std::collections::HashMap<String, String> =
                url.query_pairs().into_owned().collect();
            let path = url
                .path()
                .trim_start_matches("/files")
                .trim_start_matches('/')
                .to_string();
            let (status, body) = if !authorized {
                (401, json!({"error": {"message": "no token"}}).to_string())
            } else if path.is_empty()
                && q.get("q").is_some_and(|x| x.contains("fullText"))
                && q.contains_key("orderBy")
            {
                // As Drive answers a sorted full-text search.
                (403, json!({"error": {"message": "Sorting is not supported for queries with fullText terms."}}).to_string())
            } else if path.is_empty() {
                let query = q.get("q").cloned().unwrap_or_default();
                let files: Vec<Value> = if query.contains("'R' in parents") {
                    ["D1", "sheet"].iter().map(|i| meta(i).unwrap()).collect()
                } else if query.contains("'D1' in parents") {
                    vec![meta("doc").unwrap()]
                } else if query.contains("fullText contains 'tax'") {
                    vec![meta("out").unwrap(), meta("doc").unwrap()]
                } else {
                    vec![]
                };
                (200, json!({"files": files}).to_string())
            } else if let Some(id) = path.strip_suffix("/export") {
                match (id, q.get("mimeType").map(String::as_str)) {
                    ("doc", Some("text/markdown")) => (200, "# Articles\n\nOne a week.".into()),
                    ("sheet", Some("text/csv")) => (200, "month,spent\nNov,120".into()),
                    _ => (
                        400,
                        json!({"error": {"message": "cannot export"}}).to_string(),
                    ),
                }
            } else if path == "pdf" && q.get("alt").map(String::as_str) == Some("media") {
                (200, "%PDF-1.4\n1 0 obj\n<< /Length 44 >>\nstream\nBT (Taxpayer ID: 0123456789) Tj ET\nendstream\nendobj\n%%EOF".into())
            } else {
                match meta(&path) {
                    Some(m) => (200, m.to_string()),
                    None => (
                        404,
                        json!({"error": {"message": "File not found"}}).to_string(),
                    ),
                }
            };
            req.respond(tiny_http::Response::from_string(body).with_status_code(status))
                .unwrap();
        }
    });
    base
}

#[test]
fn a_folder_link_gives_its_id() {
    let id = "1AbCdEfGhIjKlMnOp_qrS-tu";
    for link in [
        format!("https://drive.google.com/drive/folders/{id}?usp=sharing"),
        format!("https://drive.google.com/drive/u/1/folders/{id}"),
        format!("https://drive.google.com/open?id={id}"),
        id.to_string(),
    ] {
        assert_eq!(folder_id(&link).unwrap(), id, "{link}");
    }
    assert!(folder_id("https://drive.google.com/drive/my-drive").is_err());
}

/// Only the shared folder and what is under it is read: a file the account
/// may read elsewhere is refused, and not found by a search.
#[test]
fn only_the_shared_folder_is_read() {
    let drive = Drive::at(&fake_drive(), "tok", "R").unwrap();
    let names = |files: Vec<Value>| {
        files
            .iter()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(drive.list(None).unwrap()), ["Legal", "Budget"]);
    assert_eq!(names(drive.list(Some("D1")).unwrap()), ["Articles"]);
    assert!(
        drive.list(Some("X")).is_err(),
        "another folder is not listed"
    );

    let (name, text) = drive.read("doc").unwrap();
    assert_eq!(
        (name.as_str(), text.as_str()),
        ("Articles", "# Articles\n\nOne a week.")
    );
    assert_eq!(drive.read("sheet").unwrap().1, "month,spent\nNov,120");
    // A PDF made by software reads as its text (decision 86).
    assert_eq!(drive.read("pdf").unwrap().1, "Taxpayer ID: 0123456789");
    let err = format!("{:#}", drive.read("out").unwrap_err());
    assert!(
        err.contains("Tax return is not in the folder shared"),
        "{err}"
    );
    assert!(drive.read("D1").is_err(), "a folder is listed, not read");

    assert_eq!(
        names(drive.search("tax").unwrap()),
        ["Articles"],
        "the file elsewhere is left out"
    );
    // A wrong token is said, not hidden.
    let other = Drive::at(&fake_drive(), "wrong", "R").unwrap();
    assert!(format!("{:#}", other.list(None).unwrap_err()).contains("401"));
}

#[test]
fn a_long_document_is_cut_and_says_so() {
    let text = cut("é".repeat(MAX_TEXT + 5));
    assert!(text.ends_with(&format!(
        "[cut: the document goes on past {MAX_TEXT} characters]"
    )));
    assert_eq!(cut("short".into()), "short");
}
