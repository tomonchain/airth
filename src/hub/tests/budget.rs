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
    let view = hub.month_budgets(&f.realm, now).unwrap();
    assert!((view["spent"].as_f64().unwrap() - 1.5).abs() < 1e-9);
    // The founder's own chat (no agent) is not the agents' work.
    let mut chat = hub.create_run_in(&f.realm, "Hello", None).unwrap();
    spend(&mut chat);
    let view = hub.month_budgets(&f.realm, now).unwrap();
    assert!(
        (view["spent"].as_f64().unwrap() - 1.5).abs() < 1e-9,
        "{view}"
    );

    // A business cap of $2: $0.50 left, so the next task gets $0.50, and
    // holds it while it runs: nothing is left to give another.
    crate::business::set_monthly_cap(&root, None, Some(2.0), &[]).unwrap();
    std::fs::remove_file(chat.path().unwrap()).unwrap();
    let mut state = hub.start_task(&f.realm, "editor", "Edit").unwrap();
    assert!((state.budget.max_cost_usd.unwrap() - 0.5).abs() < 1e-9);
    assert!(hub.start_task(&f.realm, "editor", "Edit twice").is_err());
    // It ended without spending: the $0.50 is free again.
    state.status = Status::Completed;
    state.save().unwrap();
    first.status = Status::Completed;
    first.save().unwrap();

    // An agent's own cap binds it alone: writer has spent $1.50 of $1.
    crate::business::set_monthly_cap(&root, Some("writer"), Some(1.0), &[]).unwrap();
    let err = hub.start_task(&f.realm, "writer", "More").unwrap_err();
    assert!(
        format!("{err:#}").contains("Writer's monthly budget is reached: $1.50 spent of $1.00"),
        "{err:#}"
    );
    let mut again = hub.start_task(&f.realm, "editor", "Edit again").unwrap();
    again.status = Status::Completed;
    again.save().unwrap();
    // Told once this month, however many times it is refused.
    assert!(hub.start_task(&f.realm, "writer", "Again").is_err());
    assert_eq!(
        incidents(&f.realm),
        ["Monthly budget reached", "writer's monthly budget reached"]
    );

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
        ["Monthly budget reached", "writer's monthly budget reached"],
        "each told once this month"
    );

    // Removing the caps lets work start again.
    crate::business::set_monthly_cap(&root, None, None, &[]).unwrap();
    crate::business::set_monthly_cap(&root, Some("writer"), None, &[]).unwrap();
    assert!(hub.start_task(&f.realm, "writer", "Go").is_ok());
    assert!(crate::business::set_monthly_cap(&root, None, Some(-1.0), &[]).is_err());
    // A typo in the file is refused when the configuration loads.
    std::fs::write(
        root.join("business/profile.md"),
        "---\nmonthly_max_cost_usd: 4O\n---\nA media.\n",
    )
    .unwrap();
    assert!(crate::business::config::load(&root, &[]).is_err());

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
    assert_eq!(beats.len(), 2, "{beats:?}");
    assert!(
        beats
            .iter()
            .all(|b| b.1.ok && b.1.notes == "waited: the monthly budget is reached"),
        "{beats:?}"
    );
    assert_eq!(
        incidents(&f.realm),
        ["Monthly budget reached"],
        "not one a run"
    );
    let err = hub.run_routine_by_hand(&f.realm, "watch").unwrap_err();
    assert!(
        format!("{err:#}").contains("monthly budget is reached"),
        "{err:#}"
    );
    // One under way goes on: after an approval, past the cap, it is not
    // refused for the budget.
    let mut item = serde_json::Map::new();
    item.insert("title".into(), json!("Supply at a high"));
    let after = hub.run_routine(
        &f.realm,
        "watch",
        "watch-after",
        crate::routine::run::Start::After {
            block: "ok".into(),
            item,
        },
        None,
    );
    if let Err(e) = &after {
        assert!(!crate::hub::budget::cap_reached(e), "{e:#}");
    }
}

/// A day's plan keeps the tasks it started when a later one meets a cap:
/// the approval goes through, and the refused step waits.
#[test]
fn a_plan_keeps_what_it_started_when_a_step_meets_a_cap() {
    use crate::business::store::queue::Answer;
    use crate::tools::dispatch::{Proposal, Step};
    let f = fixture();
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    for (name, front) in [
        ("chief-of-staff", "title: Chief of staff\ndispatch: true\n"),
        ("writer", "title: Writer\n"),
        ("developer", "title: Developer\nmonthly_max_cost_usd: 1\n"),
    ] {
        std::fs::write(
            root.join(format!("agents/{name}.md")),
            format!("---\n{front}---\nWork.\n"),
        )
        .unwrap();
    }
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let mut dev = hub
        .start_task(&f.realm, "developer", "Earlier work")
        .unwrap();
    spend(&mut dev);
    dev.status = Status::Completed;
    dev.save().unwrap();
    let chief = hub
        .start_task(&f.realm, "chief-of-staff", "Year end")
        .unwrap();
    let dispatch = hub.dispatch_for(&f.realm, &chief).unwrap().unwrap();
    let step = |agent: &str| Step {
        agent: agent.into(),
        task: format!("{agent}'s part"),
        after: vec![],
        milestone: None,
    };
    (dispatch.propose)(
        &Proposal {
            name: "Year-end".into(),
            steps: vec![step("writer"), step("developer")],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let item = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .into_iter()
        .find(|i| i.action.as_deref() == Some("airth.dispatch"))
        .unwrap();
    hub.answer_item(
        &f.realm,
        item.id,
        Some(item.revision),
        Answer::Approve,
        "app",
        3,
    )
    .unwrap();
    let handed = hub.take_hand_offs().unwrap();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].1.agent.as_deref(), Some("writer"));
    let project = Store::of(&f.realm)
        .unwrap()
        .projects()
        .unwrap()
        .pop()
        .unwrap();
    // Waiting, not skipped: it starts on a later pass.
    assert!(
        project.tasks.iter().all(|t| t.skipped.is_none()),
        "{:?}",
        project.tasks
    );
    let runs: Vec<bool> = project.tasks.iter().map(|t| t.run.is_some()).collect();
    assert_eq!(
        runs,
        [true, false],
        "the writer's kept, the developer's waits"
    );
}

/// An objective is work like a task: the month's caps bound it, its bot's
/// and the business's, and none starts once one is reached.
#[test]
fn an_objective_waits_past_the_month_cap_too() {
    use crate::objective::{Objective, Trigger};
    let f = fixture();
    let root = f.root.path().to_path_buf();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(root.join("agents/writer.md"), "---\n---\nWrite.\n").unwrap();
    f.realm
        .objectives()
        .save(Objective {
            id: "weekly-article".into(),
            objective: "Write this week's article".into(),
            trigger: Trigger::Manual,
            approvals: vec![],
            budget: Default::default(),
            approval_wait_minutes: 0,
            enabled: true,
            created_at: Utc::now(),
            repo: None,
            bot: Some("writer".into()),
            tools: None,
            delegates: Vec::new(),
            next_at: None,
            fingerprint: None,
            last_run: None,
            last_at: None,
            last_status: None,
        })
        .unwrap();
    let (hub, _, _) = make_hub(&f.home, vec![]);

    // Under the cap: it starts, bound by what the month leaves.
    crate::business::set_monthly_cap(&root, Some("writer"), Some(5.0), &[]).unwrap();
    let (mut first, _) = hub
        .start_objective(&f.realm, "weekly-article", "manual", None, Utc::now())
        .unwrap();
    assert_eq!(first.budget.max_cost_usd, Some(5.0));
    // What it spent is its bot's, and the business's.
    spend(&mut first);
    let view = hub.month_budgets(&f.realm, Utc::now()).unwrap();
    assert!(
        (view["spent"].as_f64().unwrap() - 1.5).abs() < 1e-9,
        "{view}"
    );
    first.status = Status::Completed;
    first.save().unwrap();

    // Past it: refused, and the founder told once.
    crate::business::set_monthly_cap(&root, Some("writer"), Some(1.0), &[]).unwrap();
    let err = hub
        .start_objective(&f.realm, "weekly-article", "manual", None, Utc::now())
        .unwrap_err();
    assert!(crate::hub::budget::cap_reached(&err), "{err:#}");
    assert_eq!(incidents(&f.realm), ["writer's monthly budget reached"]);
}
