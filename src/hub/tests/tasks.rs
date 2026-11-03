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
