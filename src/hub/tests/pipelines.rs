//! Pipelines run from the hub: their reads go through the realm's runtime.

use super::*;
use crate::business::store::Store;

/// Records what it was asked, and says no.
struct Ask(Arc<Mutex<Vec<(String, Capability)>>>);

impl Approver for Ask {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        self.0
            .lock()
            .unwrap()
            .push((request.tool.clone(), request.capability));
        Ok(Answer::No("not now".into()))
    }
}

fn with_watch(f: &Fixture) {
    let dir = f.root.path().join("business/pipelines");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("watch.json"),
        r#"{"steps": [{"type": "rss", "feeds": ["https://example.com/feed.xml"]}]}"#,
    )
    .unwrap();
}

fn heartbeats(f: &Fixture) -> Vec<(bool, String)> {
    Store::of(&f.realm)
        .unwrap()
        .heartbeats()
        .unwrap()
        .into_iter()
        .map(|(_, r)| (r.ok, r.error.unwrap_or_default()))
        .collect()
}

#[test]
fn a_pipeline_reads_only_what_the_realm_allows() {
    let f = fixture();
    with_watch(&f);
    let hub = Hub::new(f.home.clone());
    // The realm asks before network requests: the owner is asked, says no,
    // and the run fails with a heartbeat that says why.
    let asked = Arc::new(Mutex::new(Vec::new()));
    let err = hub
        .run_pipeline(&f.realm, "watch", Box::new(Ask(asked.clone())), Vec::new())
        .unwrap_err();
    assert!(format!("{err:#}").contains("permission denied"));
    assert_eq!(
        *asked.lock().unwrap(),
        vec![("http_request".to_string(), Capability::Network)]
    );
    // Network denied by the realm's rules: nobody is asked.
    let mut realm = f.realm.clone();
    realm.config.policy.network = Rule::Deny;
    realm.save().unwrap();
    let asked = Arc::new(Mutex::new(Vec::new()));
    assert!(
        hub.run_pipeline(&realm, "watch", Box::new(Ask(asked.clone())), Vec::new())
            .is_err()
    );
    assert!(asked.lock().unwrap().is_empty());
    let beats = heartbeats(&f);
    assert_eq!(beats.len(), 2);
    assert!(
        beats
            .iter()
            .all(|(ok, error)| !ok && error.contains("denied"))
    );
    // A pipeline the business does not declare is refused before anything runs.
    assert!(
        hub.run_pipeline(&realm, "missing", Box::new(Fixed(false)), Vec::new())
            .is_err()
    );
    assert_eq!(heartbeats(&f).len(), 2);
}

#[test]
fn a_scheduled_pipeline_runs_unattended_and_tells_the_owner() {
    let f = fixture();
    let dir = f.root.path().join("business/pipelines");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("watch.json"),
        r#"{"schedule": "every 1h", "steps": [{"type": "rss", "feeds": ["https://example.com/feed.xml"]}]}"#,
    )
    .unwrap();
    let hub = Hub::new(f.home.clone());
    let t0 = Utc::now();
    // Armed when first seen, due an hour later, once.
    assert!(hub.due_pipelines(t0).unwrap().is_empty());
    let due = hub.due_pipelines(t0 + chrono::Duration::hours(1)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].1, "watch");
    assert!(
        hub.due_pipelines(t0 + chrono::Duration::hours(1))
            .unwrap()
            .is_empty()
    );
    // The realm asks before network requests, and nobody is there: the read
    // is denied at once, and the owner hears why.
    hub.run_pipeline_unattended(&due[0].0, "watch");
    let beats = heartbeats(&f);
    assert_eq!(beats.len(), 1);
    assert!(!beats[0].0 && beats[0].1.contains("runs unattended"));
    let inbox = f.home.inbox().list().unwrap();
    assert_eq!(inbox.last().unwrap().title, "watch failed");
    // Airth off for hours: the next run is late, once, and says so.
    let late = hub.due_pipelines(t0 + chrono::Duration::hours(5)).unwrap();
    assert_eq!(late.len(), 1);
    let titles: Vec<String> = f
        .home
        .inbox()
        .list()
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert!(titles.iter().any(|t| t.starts_with("watch missed its run")));
}
