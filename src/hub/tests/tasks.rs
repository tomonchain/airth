//! Decision 64: a task is the founder's order, worked as one agent, with
//! that agent's scope only and the business's task budget.

use super::*;
use crate::business::store::Store;

fn agent(root: &std::path::Path, name: &str, front: &str) {
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(
        root.join(format!("agents/{name}.md")),
        format!("---\n{front}---\nWrite the blog.\n"),
    )
    .unwrap();
}

fn integration(name: &str) -> crate::integration::Integration {
    crate::integration::Integration {
        name: name.into(),
        description: String::new(),
        base_url: format!("https://{name}.example.com"),
        headers: Default::default(),
        actions: Vec::new(),
    }
}

/// The task works as its agent: its job and the order in its first
/// message, only the integrations its file declares, the business's budget.
#[test]
fn a_task_works_as_its_agent_within_its_scope_and_budget() {
    let mut f = fixture();
    let root = f.root.path().to_path_buf();
    agent(&root, "writer", "integrations: blog\n");
    std::fs::create_dir_all(root.join("business")).unwrap();
    std::fs::write(
        root.join("business/profile.md"),
        "---\ntask_max_minutes: 10\ntask_max_cost_usd: 0.5\n---\nA media.\n",
    )
    .unwrap();
    for name in ["blog", "bank"] {
        f.realm.set_integration(integration(name)).unwrap();
    }
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let state = hub
        .start_task(&f.realm, "writer", "Outline the week's article")
        .unwrap();
    assert_eq!(state.agent.as_deref(), Some("writer"));
    assert_eq!(
        state.tools,
        Some(vec!["blog".to_string()]),
        "never the whole realm's"
    );
    assert_eq!(
        (state.budget.max_minutes, state.budget.max_cost_usd),
        (Some(10), Some(0.5))
    );
    let first = format!("{:?}", state.messages[0]);
    assert!(
        first.contains("Write the blog.") && first.contains("Outline the week's article"),
        "{first}"
    );
    // Defaults without the fields in the profile; an unknown agent refused.
    std::fs::write(root.join("business/profile.md"), "---\n---\nA media.\n").unwrap();
    let state = hub.start_task(&f.realm, "writer", "Again").unwrap();
    assert_eq!(
        (state.budget.max_minutes, state.budget.max_cost_usd),
        (Some(30), Some(1.0))
    );
    assert!(hub.start_task(&f.realm, "ghost", "Do it").is_err());
}

/// An agent that declares nothing has nothing beyond read-only web: no
/// file or command tool is even offered, so a call to one is unknown.
#[test]
fn a_task_without_a_repository_has_no_files_or_commands() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    agent(&root, "writer", "");
    std::fs::write(root.join("secret-plan.md"), "do not read").unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call("1", "read_file", json!({"path": "secret-plan.md"})),
            call(
                "2",
                "execute_command",
                json!({"command": "cat secret-plan.md"}),
            ),
            done("ok"),
        ],
    );
    let mut state = hub.start_task(&f.realm, "writer", "Read the plan").unwrap();
    assert_eq!(state.tools, Some(Vec::new()));
    hub.drive(
        &f.realm,
        &mut state,
        config(10),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    assert_eq!(state.events[0].result, "unknown tool 'read_file'");
    assert_eq!(state.events[1].result, "unknown tool 'execute_command'");
}

/// A task stopped by its budget tells the founder why, in Needs you.
#[test]
fn a_task_over_its_budget_tells_the_founder() {
    let f = fixture();
    agent(f.root.path(), "writer", "");
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let mut state = hub
        .start_task(&f.realm, "writer", "Outline the article")
        .unwrap();
    state.status = Status::Stopped;
    state.report = Some("Stopped: reached its cost budget ($1.20 of $1.00).".into());
    hub.task_ended(&f.realm, &state).unwrap();
    let items = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "incident");
    assert_eq!(items[0].title, "Task stopped: Outline the article");
    assert!(
        items[0].args["detail"]
            .as_str()
            .unwrap()
            .contains("reached its cost budget ($1.20 of $1.00)")
    );
    // Stopped by the founder, or done: nothing to tell.
    state.report = Some("Stopped by the user.".into());
    hub.task_ended(&f.realm, &state).unwrap();
    assert_eq!(
        Store::of(&f.realm)
            .unwrap()
            .queue(&["pending"])
            .unwrap()
            .len(),
        1
    );
}

/// Decision 64: in a task nobody watches, a permission the realm's rules
/// would ask becomes the founder's question in Needs you, never a silent
/// wait; allowed, the task holds that permission and is ready to go on.
#[test]
fn an_unwatched_tasks_ask_waits_in_needs_you_then_goes_on() {
    let mut f = fixture();
    agent(f.root.path(), "writer", "");
    f.realm.config.policy.network = Rule::Ask;
    f.realm.save().unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "http_request",
                json!({"method": "GET", "url": "https://example.com/rates"}),
            ),
            done("never reached"),
        ],
    );
    let mut state = hub
        .start_task(&f.realm, "writer", "Check today's rates")
        .unwrap();
    hub.drive(
        &f.realm,
        &mut state,
        config(10),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    assert_eq!(state.status, Status::Stopped, "{:?}", state.report);
    assert!(
        state.events[0]
            .result
            .contains("the founder is asked in Needs you (item 1)"),
        "{}",
        state.events[0].result
    );
    let items = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "writer asks to use network");
    assert_eq!(items[0].source["ask"]["capability"], "network");
    // The founder allows it, from the phone.
    let (_, said) = hub
        .answer_item(
            &f.realm,
            items[0].id,
            None,
            crate::business::store::queue::Answer::Approve,
            "telegram",
            3,
        )
        .unwrap();
    assert_eq!(said.as_deref(), Some("allowed: the task goes on"));
    let ready = hub.tasks_to_resume().unwrap();
    assert_eq!(ready.len(), 1);
    let (_, resumed) = &ready[0];
    assert!(
        resumed
            .grants
            .iter()
            .any(|g| g.capability == crate::permissions::Capability::Network)
    );
    assert!(resumed.resume.is_none());
    assert!(format!("{:?}", resumed.messages.last()).contains("The founder allowed this"));
    // Taken once.
    assert!(hub.tasks_to_resume().unwrap().is_empty());
    // That task only: the realm's rules are unchanged, and a later task
    // starts without the permission.
    let realm = f.home.realm(f.realm.name()).unwrap();
    assert_eq!(realm.config.policy.network, Rule::Ask);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let later = hub
        .start_task(&realm, "writer", "Check tomorrow's rates")
        .unwrap();
    assert!(later.grants.is_empty());
}
