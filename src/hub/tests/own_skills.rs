//! Decision 149: the business's own skills, asked of the founder once per
//! version, offered to agents only as approved.

use super::*;
use crate::business::store::Store;
use crate::business::store::queue::{Answer as Said, Item};
use crate::business::store::skills::SKILL;

const REFUNDS: &str =
    "---\nname: refunds\ndescription: How we refund a customer.\n---\nRefund within 30 days.\n";

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn asked(realm: &Realm) -> Vec<Item> {
    Store::of(realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .filter(|i| i.action.as_deref() == Some(SKILL))
        .collect()
}

fn offered(hub: &Hub, realm: &Realm) -> Vec<(String, String)> {
    hub.offered_skills(realm)
        .unwrap()
        .into_iter()
        .map(|s| (s.name.clone(), s.skill_md().to_string()))
        .collect()
}

fn answer(hub: &Hub, realm: &Realm, id: i64, said: Said) -> (Item, Option<String>) {
    hub.answer_item(realm, id, None, said, "app", 5).unwrap()
}

#[test]
fn a_skill_is_offered_once_approved_and_again_only_once_its_change_is() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    write(&root, "skills/refunds/reference/policy.md", "30 days.");

    // New: asked once, not offered meanwhile.
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 1);
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 0, "asked once");
    let items = asked(&f.realm);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "New skill: refunds");
    let text = items[0].args["text"].as_str().unwrap();
    assert!(text.starts_with("How we refund a customer."), "{text}");
    assert!(
        text.contains("Files: SKILL.md, reference/policy.md"),
        "{text}"
    );
    assert!(text.contains("Refund within 30 days."), "{text}");
    assert!(offered(&hub, &f.realm).is_empty());

    // Approved: offered, the item done.
    let (item, said) = answer(&hub, &f.realm, items[0].id, Said::Approve);
    assert_eq!(item.status, "done", "{said:?}");
    let done = Store::of(&f.realm).unwrap().queue(&["done"]).unwrap();
    assert!(done.iter().any(|i| i.id == items[0].id), "{said:?}");
    assert_eq!(
        offered(&hub, &f.realm),
        [("refunds".to_string(), REFUNDS.to_string())]
    );
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 0);

    // Changed: no longer offered, asked again with what changed.
    write(
        &root,
        "skills/refunds/SKILL.md",
        &REFUNDS.replace("30", "60"),
    );
    write(&root, "skills/refunds/run.sh", "echo");
    assert!(offered(&hub, &f.realm).is_empty(), "a changed skill waits");
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 1);
    let change = asked(&f.realm).remove(0);
    assert_eq!(change.title, "Skill changed: refunds");
    let text = change.args["text"].as_str().unwrap();
    assert!(text.contains("Changed: SKILL.md\nNew: run.sh"), "{text}");
    assert!(
        text.contains("-Refund within 30 days.\n+Refund within 60 days."),
        "{text}"
    );

    // Changed again before the founder answers: the earlier ask goes, and
    // approving it does nothing but say so.
    write(&root, "skills/refunds/run.sh", "echo again");
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 1);
    let pending = asked(&f.realm);
    assert_eq!(pending.len(), 1, "the earlier ask is replaced");
    assert_ne!(pending[0].id, change.id);
    // An answer to an ask whose files changed since offers nothing.
    write(&root, "skills/refunds/run.sh", "echo a third time");
    let (_, said) = answer(&hub, &f.realm, pending[0].id, Said::Approve);
    assert!(said.unwrap().contains("changed since you saw it"));
    assert!(offered(&hub, &f.realm).is_empty());
    assert_eq!(asked(&f.realm).len(), 1, "the new version is asked");
}

#[test]
fn a_rejected_skill_is_neither_offered_nor_asked_again_until_it_changes() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    hub.ask_about_skills(&f.realm).unwrap();
    let id = asked(&f.realm)[0].id;
    answer(&hub, &f.realm, id, Said::Reject("not ours".into()));
    assert!(offered(&hub, &f.realm).is_empty());
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 0);
    write(
        &root,
        "skills/refunds/SKILL.md",
        &REFUNDS.replace("30", "14"),
    );
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 1);
}

/// The boundary: only an answer of the founder's approves a skill, never a
/// write to the business folder.
#[test]
fn nothing_but_the_founders_answer_approves_a_skill() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    hub.ask_about_skills(&f.realm).unwrap();
    let item = asked(&f.realm).remove(0);
    let skill = crate::business::skills::found(&root).remove(0);
    let approved = crate::business::store::skills::Approved {
        name: skill.name.clone(),
        hash: skill.hash.clone(),
        files: Default::default(),
        skill_md: String::new(),
    };
    let mut store = Store::of(&f.realm).unwrap();
    // Pending, not yet the founder's.
    assert!(store.approve_skill(item.id, &approved).is_err());
    // Approved by the founder, but for other files, or another skill.
    store
        .answer(item.id, None, Said::Approve, "app", 5)
        .unwrap();
    let other_files = crate::business::store::skills::Approved {
        hash: "0".repeat(64),
        ..approved.clone()
    };
    let other_skill = crate::business::store::skills::Approved {
        name: "returns".into(),
        ..approved.clone()
    };
    assert!(store.approve_skill(item.id, &other_files).is_err());
    assert!(store.approve_skill(item.id, &other_skill).is_err());
    assert!(store.approved_skills().unwrap().is_empty());
    assert!(offered(&hub, &f.realm).is_empty());
    // As the founder approved it: offered.
    store.approve_skill(item.id, &approved).unwrap();
    assert_eq!(offered(&hub, &f.realm).len(), 1);

    // Rejected: never.
    write(&root, "skills/returns/SKILL.md", REFUNDS);
    hub.ask_about_skills(&f.realm).unwrap();
    let item = asked(&f.realm).remove(0);
    answer(&hub, &f.realm, item.id, Said::Reject("no".into()));
    let skill = crate::business::skills::found(&root).remove(1);
    let rejected = crate::business::store::skills::Approved {
        name: skill.name.clone(),
        hash: skill.hash.clone(),
        ..approved
    };
    assert!(store.approve_skill(item.id, &rejected).is_err());
    assert_eq!(offered(&hub, &f.realm).len(), 1);
}

/// An ask whose files changed since goes, even when the new version is not
/// asked (rejected before); those files back, they are asked again.
#[test]
fn an_ask_goes_with_its_files_and_comes_back_with_them() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let v2 = REFUNDS.replace("30", "60");
    // Version 2 asked and rejected.
    write(&root, "skills/refunds/SKILL.md", &v2);
    hub.ask_about_skills(&f.realm).unwrap();
    let id = asked(&f.realm)[0].id;
    answer(&hub, &f.realm, id, Said::Reject("no".into()));
    // Version 1 asked, then the files go back to version 2: its ask goes,
    // and version 2 is not asked again.
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 1);
    write(&root, "skills/refunds/SKILL.md", &v2);
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 0);
    assert!(asked(&f.realm).is_empty(), "the stale ask is gone");
    // Version 1 back: asked again, its earlier ask having gone.
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 1);
    // The skill removed: its ask goes.
    std::fs::remove_dir_all(root.join("skills")).unwrap();
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 0);
    assert!(asked(&f.realm).is_empty());
}

/// A catalog skill keeps its name: a business skill under the same one is
/// not asked about, and not offered.
#[test]
fn a_skill_named_as_a_catalog_skill_is_left_out() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let catalog = f.realm.skills_dir().join("catalog/pdf");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(catalog.join("SKILL.md"), "---\nname: pdf\n---\n").unwrap();
    std::fs::write(
        catalog.join(".airth-skill.json"),
        json!({"name": "pdf", "title": "pdf", "description": "", "source": "", "installed_at": Utc::now(), "files": ["SKILL.md"], "skipped": []}).to_string(),
    )
    .unwrap();
    assert_eq!(
        crate::tools::catalog::installed(&f.realm.skills_dir()).len(),
        1
    );
    write(&root, "skills/pdf/SKILL.md", "---\nname: pdf\n---\nOurs.\n");
    assert_eq!(hub.ask_about_skills(&f.realm).unwrap(), 0);
    assert!(offered(&hub, &f.realm).is_empty());
}

/// In a run, `use_skill` gives the skill as approved: what was written in
/// the folder since is not what the agent reads.
#[test]
fn a_run_reads_the_skill_as_approved() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    let (hub, _, _) = make_hub(&f.home, vec![]);
    hub.ask_about_skills(&f.realm).unwrap();
    let id = asked(&f.realm)[0].id;
    answer(&hub, &f.realm, id, Said::Approve);

    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call("1", "use_skill", json!({"name": "refunds"})),
            done("Read it."),
        ],
    );
    let mut state = hub.create_run_in(&f.realm, "Refund Ann", None).unwrap();
    hub.drive(
        &f.realm,
        &mut state,
        config(5),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    let read = format!("{:?}", state.messages);
    assert!(read.contains("Refund within 30 days."), "{read}");

    // Changed on disk, not yet approved: a new run has no such skill.
    write(
        &root,
        "skills/refunds/SKILL.md",
        &REFUNDS.replace("30", "300"),
    );
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call("1", "use_skill", json!({"name": "refunds"})),
            done("No."),
        ],
    );
    let mut state = hub.create_run_in(&f.realm, "Refund Bob", None).unwrap();
    hub.drive(
        &f.realm,
        &mut state,
        config(5),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    let read = format!("{:?}", state.messages);
    assert!(!read.contains("300 days"), "{read}");
    // No skill to read at all: the call is refused, not answered.
    assert!(!read.contains("30 days"), "{read}");
    assert!(read.contains("is_error: true"), "{read}");
}

/// What the founder reads in the Marketplace: each skill's state, the
/// agents that name it, and the folders that are no skill.
#[test]
fn the_founder_sees_each_skills_state() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    write(
        &root,
        "agents/writer.md",
        "---\ntitle: Writer\nskills: refunds\n---\nWrite.\n",
    );
    write(&root, "skills/refunds/SKILL.md", REFUNDS);
    write(
        &root,
        "skills/tone/SKILL.md",
        "---\nname: tone\ndescription: Our tone.\n---\nCalm.\n",
    );
    write(&root, "skills/Bad_Name/SKILL.md", "x");
    hub.ask_about_skills(&f.realm).unwrap();
    let state = |name: &str| {
        let v = hub.skills_view(&f.realm).unwrap();
        v["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .cloned()
            .unwrap()
    };
    let refunds = state("refunds");
    assert_eq!(refunds["state"], "new");
    assert!(refunds["item"].is_i64(), "its ask, to open");
    assert_eq!(refunds["agents"], json!(["Writer"]));
    let id = refunds["item"].as_i64().unwrap();
    answer(&hub, &f.realm, id, Said::Approve);
    assert_eq!(state("refunds")["state"], "offered");
    assert!(state("refunds")["item"].is_null());

    write(
        &root,
        "skills/refunds/SKILL.md",
        &REFUNDS.replace("30", "60"),
    );
    assert_eq!(state("refunds")["state"], "changed");

    let tone = state("tone")["item"].as_i64().unwrap();
    answer(&hub, &f.realm, tone, Said::Reject("no".into()));
    assert_eq!(state("tone")["state"], "rejected");

    let view = hub.skills_view(&f.realm).unwrap();
    assert_eq!(view["not_skills"][0]["name"], "Bad_Name");
}

/// A catalog skill installed under the name of one waiting for the founder:
/// the ask goes, and the Marketplace offers nothing to review.
#[test]
fn a_catalog_skill_installed_since_takes_the_ask_away() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    write(&root, "skills/pdf/SKILL.md", "---\nname: pdf\n---\nOurs.\n");
    hub.ask_about_skills(&f.realm).unwrap();
    assert_eq!(asked(&f.realm).len(), 1);
    let catalog = f.realm.skills_dir().join("catalog/pdf");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(catalog.join("SKILL.md"), "---\nname: pdf\n---\n").unwrap();
    std::fs::write(
        catalog.join(".airth-skill.json"),
        json!({"name": "pdf", "title": "pdf", "description": "", "source": "", "installed_at": Utc::now(), "files": ["SKILL.md"], "skipped": []}).to_string(),
    )
    .unwrap();
    let view = hub.skills_view(&f.realm).unwrap();
    assert_eq!(view["skills"][0]["state"], "left out");
    assert!(view["skills"][0]["item"].is_null());
    hub.ask_about_skills(&f.realm).unwrap();
    assert!(asked(&f.realm).is_empty());
}
