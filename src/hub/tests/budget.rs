//! Decision 139: the month's caps on model cost, the business's and an
//! agent's, bound each task and stop new work once reached.

use super::*;
use crate::business::store::Store;
use crate::state::Usage;

/// A call this month that cost $1.50 at list prices ($50 per million
/// output tokens).
fn spend(state: &mut State) {
    state.usage.push(Usage {
        at: Utc::now(),
        model: "claude-fable-5".into(),
        input: 0,
        output: 30_000,
        cache_read: 0,
        cache_write: 0,
        purpose: Default::default(),
    });
    state.save().unwrap();
}

fn incidents(realm: &Realm) -> Vec<String> {
    Store::of(realm)
        .unwrap()
        .queue(&["pending", "done"])
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "incident")
        .map(|i| i.title)
        .collect()
}

#[test]
fn a_task_spends_no_more_than_the_month_leaves_and_none_past_it() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(root.join("agents/writer.md"), "---\n---\nWrite.\n").unwrap();
    std::fs::write(root.join("agents/editor.md"), "---\n---\nEdit.\n").unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let now = Utc::now();

    // No cap: the task budget, and no limit to report.
    let mut first = hub.start_task(&f.realm, "writer", "Draft").unwrap();
    assert_eq!(first.budget.max_cost_usd, Some(1.0));
    assert_eq!(
        hub.month_allows(&f.realm, Some("writer"), now).unwrap(),
        None
    );
    spend(&mut first);
    assert!((hub.month_spent(&f.realm, None, now).unwrap() - 1.5).abs() < 1e-9);
    assert_eq!(hub.month_spent(&f.realm, Some("editor"), now).unwrap(), 0.0);

    // A business cap of $2: $0.50 left, so the next task gets $0.50.
    crate::business::set_monthly_cap(&root, None, Some(2.0), &[]).unwrap();
    let state = hub.start_task(&f.realm, "editor", "Edit").unwrap();
    assert!((state.budget.max_cost_usd.unwrap() - 0.5).abs() < 1e-9);

    // An agent's own cap binds it alone: writer has spent $1.50 of $1.
    crate::business::set_monthly_cap(&root, Some("writer"), Some(1.0), &[]).unwrap();
    let err = hub.start_task(&f.realm, "writer", "More").unwrap_err();
    assert!(
        format!("{err:#}").contains("Writer's monthly budget is reached: $1.50 spent of $1.00"),
        "{err:#}"
    );
    assert!(hub.start_task(&f.realm, "editor", "Edit again").is_ok());
    // Told once this month, however many times it is refused.
    assert!(hub.start_task(&f.realm, "writer", "Again").is_err());
    assert_eq!(incidents(&f.realm), ["writer's monthly budget reached"]);

    // Past the business's cap, no task starts, for anyone.
    let mut second = hub.start_task(&f.realm, "editor", "Spend").unwrap();
    spend(&mut second);
    let err = hub.start_task(&f.realm, "editor", "Edit").unwrap_err();
    assert!(
        format!("{err:#}").contains("The business's monthly budget is reached"),
        "{err:#}"
    );
    assert_eq!(
        incidents(&f.realm),
        ["writer's monthly budget reached", "Monthly budget reached"]
    );

    // Removing the caps lets work start again.
    crate::business::set_monthly_cap(&root, None, None, &[]).unwrap();
    crate::business::set_monthly_cap(&root, Some("writer"), None, &[]).unwrap();
    assert!(hub.start_task(&f.realm, "writer", "Go").is_ok());
    assert!(crate::business::set_monthly_cap(&root, None, Some(-1.0), &[]).is_err());

    // The Finance page's figures.
    let view = hub.month_budgets(&f.realm, now).unwrap();
    assert!(view["cap"].is_null());
    assert!((view["spent"].as_f64().unwrap() - 3.0).abs() < 1e-9);
    assert_eq!(view["agents"].as_array().unwrap().len(), 2);
}

/// A scheduled routine past the business's cap waits: a heartbeat that
/// says so, no run, and no failure of its own.
#[test]
fn a_routine_waits_past_the_business_cap() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    for (path, text) in [
        ("agents/watcher.md".to_string(), "Watch the news.".to_string()),
        ("routines/watch.json".into(), json!({"agent": "watcher", "trigger": {"schedule": "every 1h"}, "blocks": [
            {"id": "news", "type": "read", "source": "rss", "feeds": ["https://news.example/unreachable.xml"]},
            {"id": "ok", "type": "approval", "show": "title"}
        ]}).to_string()),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let mut state = hub.start_task(&f.realm, "watcher", "Look").unwrap();
    spend(&mut state);
    crate::business::set_monthly_cap(&root, None, Some(1.0), &[]).unwrap();

    hub.run_scheduled_routine(&f.realm, "watch", Utc::now());
    hub.run_scheduled_routine(&f.realm, "watch", Utc::now());
    let beats = Store::of(&f.realm).unwrap().heartbeats().unwrap();
    assert!(
        beats
            .iter()
            .all(|b| b.1.notes == "waited: the monthly budget is reached"),
        "{beats:?}"
    );
    assert_eq!(
        incidents(&f.realm),
        ["Monthly budget reached"],
        "not one a run"
    );
    assert!(hub.run_routine_by_hand(&f.realm, "watch").is_err());
}
