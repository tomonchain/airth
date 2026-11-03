//! Decision 66: a legal deadline reaches the founder once per reminder, then
//! overdue once passed unfiled; Filed records it and closes its other
//! reminders, Seen closes one. Deadlines passed long before Airth looked
//! raise nothing.

use super::*;
use crate::business::store::Store;
use crate::business::store::queue::{Answer, Item};
use chrono::{Local, NaiveDate, TimeZone, Utc};

/// Noon, local time, on that day.
fn on(y: i32, m: u32, d: u32) -> chrono::DateTime<Utc> {
    let day = NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let noon = Local.from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap());
    noon.earliest().unwrap().with_timezone(&Utc)
}

fn deadlines(realm: &Realm) -> Vec<Item> {
    let pending = Store::of(realm).unwrap().queue(&["pending"]).unwrap();
    pending
        .into_iter()
        .filter(|i| i.kind == "deadline")
        .collect()
}

fn profile(realm: &Realm, fields: &str) {
    let root = realm.root().unwrap();
    std::fs::create_dir_all(root.join("business")).unwrap();
    let text = format!("---\ncountry: GB\nlegal_form: ltd\n{fields}---\n");
    std::fs::write(root.join("business/profile.md"), text).unwrap();
}

const HEDGEN: &str = "incorporated_on: 2025-11-07\nfiscal_year_end: 11-30\n";

#[test]
fn a_deadline_is_reminded_once_per_reminder_then_filed() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    profile(&f.realm, "incorporated_on: TODO\n");
    hub.remind_deadlines(on(2026, 11, 3)).unwrap();
    assert!(
        deadlines(&f.realm).is_empty(),
        "no dates, nothing to remind"
    );
    profile(&f.realm, HEDGEN);
    // 17 days ahead: the 30-day reminder's day has passed, so it comes now.
    hub.remind_deadlines(on(2026, 11, 3)).unwrap();
    hub.remind_deadlines(on(2026, 11, 4)).unwrap();
    let first = deadlines(&f.realm);
    assert_eq!(first.len(), 1, "once, whatever the ticks");
    let queue = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert!(queue.iter().all(|i| i.kind != "incident"), "{queue:?}");
    assert_eq!(first[0].title, "Confirmation statement due Fri 20 Nov 2026");
    assert_eq!(
        first[0].args["link"],
        "https://www.gov.uk/guidance/confirmation-statement-guidance"
    );
    hub.remind_deadlines(on(2026, 11, 13)).unwrap();
    assert_eq!(deadlines(&f.realm).len(), 2, "the 7-day reminder");
    // Seen closes that reminder only.
    hub.answer_item(
        &f.realm,
        first[0].id,
        None,
        Answer::Reject("seen".into()),
        "app",
        1,
    )
    .unwrap();
    let left = deadlines(&f.realm);
    assert_eq!(left.len(), 1);
    // Filed: recorded, and nothing more comes for it.
    profile(&f.realm, &format!("{HEDGEN}reminders: [30, 7, 1]\n"));
    hub.remind_deadlines(on(2026, 11, 19)).unwrap();
    assert_eq!(deadlines(&f.realm).len(), 2, "the profile's own reminders");
    let (_, said) = hub
        .answer_item(&f.realm, left[0].id, None, Answer::Approve, "telegram", 1)
        .unwrap();
    assert_eq!(said.as_deref(), Some("filed"));
    assert!(deadlines(&f.realm).is_empty(), "its other reminder closed");
    let store = Store::of(&f.realm).unwrap();
    assert_eq!(
        store.filings().unwrap(),
        vec![(
            "confirmation-statement".to_string(),
            "2026-11-06".to_string()
        )]
    );
    drop(store);
    hub.remind_deadlines(on(2026, 11, 21)).unwrap();
    assert!(deadlines(&f.realm).is_empty(), "filed, never overdue");
}

#[test]
fn an_unfiled_deadline_is_overdue_until_filed() {
    let f = fixture();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    profile(&f.realm, HEDGEN);
    hub.remind_deadlines(on(2026, 11, 21)).unwrap();
    let overdue: Vec<Item> = deadlines(&f.realm)
        .into_iter()
        .filter(|i| i.args["overdue"] == true)
        .collect();
    assert_eq!(overdue.len(), 1);
    assert_eq!(
        overdue[0].title,
        "Overdue: Confirmation statement was due Fri 20 Nov 2026"
    );
    // A company older than Airth: what passed long ago raises nothing.
    let old = fixture();
    let (hub, _, _) = make_hub(&old.home, vec![]);
    profile(
        &old.realm,
        "incorporated_on: 2020-01-15\nfiscal_year_end: 12-31\n",
    );
    hub.remind_deadlines(on(2026, 11, 21)).unwrap();
    assert!(deadlines(&old.realm).is_empty());
    // A profile the rules cannot read is one incident, not one a tick.
    profile(
        &old.realm,
        "incorporated_on: soon\nfiscal_year_end: 12-31\n",
    );
    hub.remind_deadlines(on(2026, 11, 21)).unwrap();
    hub.remind_deadlines(on(2026, 11, 22)).unwrap();
    let incidents: Vec<Item> = Store::of(&old.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident")
        .collect();
    assert_eq!(incidents.len(), 1);
    assert_eq!(
        incidents[0].title,
        "The legal calendar could not be checked"
    );
}
