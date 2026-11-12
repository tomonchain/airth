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
        free_reads: false,
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
    // Its first name, when it has one, is how it is told who it is.
    agent(&root, "writer", "first_name: Ada\n");
    let state = hub.start_task(&f.realm, "writer", "Again").unwrap();
    let first = format!("{:?}", state.messages[0]);
    assert!(first.contains("You are Ada (the agent writer)"), "{first}");
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

/// Decision 64's routing: the only agent takes it with no model call; a
/// named agent is taken at its word; else the fast model chooses among the
/// business's agents, its call journaled, and "none" or an answer that
/// names no agent asks the founder to pick.
#[test]
fn an_order_finds_its_agent() {
    use crate::hub::orders::{Route, named};
    let f = fixture();
    let root = f.root.path().to_path_buf();
    agent(&root, "writer", "title: Writer\n");
    let (hub, seen, _) = make_hub(&f.home, vec![]);
    assert_eq!(
        hub.route_order(&f.realm, "Outline the week").unwrap().agent,
        "writer"
    );
    assert!(
        seen.lock().unwrap().is_empty(),
        "no model call for the only agent"
    );
    agent(&root, "watcher", "title: Watcher\n");
    let agents = vec!["watcher".to_string(), "writer".to_string()];
    assert_eq!(
        named("writer: outline the week", &agents),
        Some(("writer".to_string(), "outline the week"))
    );
    assert_eq!(named("note: buy milk", &agents), None);
    let (hub, seen, _) = make_hub(
        &f.home,
        vec![
            done(r#"{"agent": "watcher", "why": "It reads the news."}"#),
            done(r#"{"agent": "none", "why": "Nobody does accounting."}"#),
            done(r#"{"agent": "intern", "why": "?"}"#),
        ],
    );
    assert_eq!(
        hub.route_order(&f.realm, "What moved in staking today?")
            .unwrap(),
        Route {
            agent: "watcher".into(),
            why: "It reads the news.".into()
        }
    );
    let asked = format!("{:?}", seen.lock().unwrap()[0]);
    assert!(
        asked.contains("watcher (Watcher)") && asked.contains("What moved in staking today?"),
        "{asked}"
    );
    let none = hub
        .route_order(&f.realm, "File the VAT return")
        .unwrap_err();
    assert!(format!("{none:#}").contains("pick the agent"), "{none:#}");
    let invalid = hub.route_order(&f.realm, "Something").unwrap_err();
    assert!(
        format!("{invalid:#}").contains("did not fit"),
        "{invalid:#}"
    );
    let calls = Store::of(&f.realm).unwrap().model_calls().unwrap();
    assert_eq!(calls.len(), 3, "each routing call is journaled");
    assert!(calls[2].error.is_some());
}

/// An agent given another repository: what the founder writes to its task
/// in the old one is refused with \"start a new task instead\", which the
/// app takes to start one, in the new repository.
#[test]
fn a_task_left_behind_by_its_agents_repository_is_not_gone_on() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let business = crate::isolation::tests::repo();
    let site = crate::isolation::tests::repo();
    let mut realm = home
        .create_realm(NewRealm {
            name: "biz".into(),
            description: String::new(),
            root: Some(business.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    realm.add_repo("site.xyz", site.path(), None).unwrap();
    realm.save().unwrap();
    agent(business.path(), "developer", "");
    let (hub, _, _) = make_hub(&home, vec![]);
    let first = hub
        .start_task(&realm, "developer", "Read the plan")
        .unwrap();
    assert!(
        hub.follow_up(&realm, &first.id, "And now?").is_ok(),
        "same repository"
    );

    agent(business.path(), "developer", "repos: site.xyz\n");
    let err = hub
        .follow_up(&realm, &first.id, "Optimise the SEO")
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("now works in site.xyz: start a new task instead"),
        "{err:#}"
    );
    let next = hub
        .start_task(&realm, "developer", "Optimise the SEO")
        .unwrap();
    assert_eq!(
        next.change.unwrap().repo.canonicalize().unwrap(),
        site.path().canonicalize().unwrap()
    );
}

/// What the founder gives an agent counts in its open task from the next
/// message: a task keeps no old copy of what its agent may use.
#[test]
fn a_task_goes_on_with_what_its_agent_may_use_now() {
    let f = fixture();
    let root = f.root.path().to_path_buf();
    agent(&root, "chief", "");
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let task = hub
        .start_task(&f.realm, "chief", "Read the company documents")
        .unwrap();
    assert_eq!(task.tools, Some(Vec::new()));
    agent(
        &root,
        "chief",
        "integrations: google-drive\nskills: linear\n",
    );
    let going_on = hub
        .follow_up(&f.realm, &task.id, "Now you have the Drive")
        .unwrap();
    assert_eq!(
        going_on.tools,
        Some(vec!["google-drive".to_string(), "linear".into()])
    );
    // Taken away, it is gone too.
    agent(&root, "chief", "");
    let state = hub.follow_up(&f.realm, &task.id, "Without it").unwrap();
    assert_eq!(state.tools, Some(Vec::new()));
}

/// Decision 120: the founder's one interlocutor takes every order not
/// named and puts a plan to the founder; nothing starts before their
/// approval, a step that waits starts once the one before it has finished,
/// with what it delivered, and the interlocutor is told, once, at the end.
#[test]
fn the_interlocutor_passes_an_order_on_once_the_founder_approves_its_plan() {
    use crate::business::store::queue::Answer;
    use crate::tools::dispatch::{Proposal, Step};
    let f = fixture();
    let root = f.root.path().to_path_buf();
    agent(
        &root,
        "chief-of-staff",
        "title: Chief of staff\ndispatch: true\n",
    );
    agent(&root, "writer", "title: Writer\n");
    agent(&root, "developer", "title: Developer\n");
    let (hub, seen, _) = make_hub(&f.home, vec![]);
    let route = hub.route_order(&f.realm, "Prepare the year's end").unwrap();
    assert_eq!(route.agent, "chief-of-staff");
    assert!(seen.lock().unwrap().is_empty(), "no model call to route");

    // Only the interlocutor's tasks get the tool, naming the others.
    let writer = hub.start_task(&f.realm, "writer", "Outline").unwrap();
    assert!(hub.dispatch_for(&f.realm, &writer).unwrap().is_none());
    std::thread::sleep(std::time::Duration::from_millis(5));
    let chief = hub
        .start_task(&f.realm, "chief-of-staff", "Prepare the year's end")
        .unwrap();
    let dispatch = hub.dispatch_for(&f.realm, &chief).unwrap().unwrap();
    let team: Vec<&str> = dispatch.team.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(team, ["developer", "writer"]);

    // Proposing starts nothing: the plan waits for the founder.
    let plan = Proposal {
        name: "Year-end".into(),
        steps: vec![
            Step {
                agent: "writer".into(),
                task: "A recap article".into(),
                after: vec![],
                milestone: None,
            },
            Step {
                agent: "developer".into(),
                task: "A banner linking to it".into(),
                after: vec!["writer".into()],
                milestone: None,
            },
        ],
        ..Default::default()
    };
    (dispatch.propose)(&plan, None).unwrap();
    let before = hub.runs(&f.realm).unwrap().len();
    let item = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(item.action.as_deref(), Some("airth.dispatch"));
    assert!(
        item.args["text"]
            .as_str()
            .unwrap()
            .contains("- developer (after writer): A banner")
    );
    assert_eq!(hub.runs(&f.realm).unwrap().len(), before);

    // Approved: the writer starts; the developer waits for it.
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
    let mark = format!("{}/{}", chief.id, item.id);
    let mut first = handed[0].1.clone();
    assert_eq!(first.agent.as_deref(), Some("writer"));
    assert_eq!(first.dispatched_by.as_deref(), Some(mark.as_str()));
    // The approval made the project (decision 129), every step in it,
    // the writer's with its run, the developer's still to come.
    let project = Store::of(&f.realm)
        .unwrap()
        .projects()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (project.name.as_str(), project.status.as_str()),
        ("Year-end", "active")
    );
    assert_eq!(project.run.as_deref(), Some(chief.id.as_str()));
    let runs: Vec<Option<&str>> = project.tasks.iter().map(|t| t.run.as_deref()).collect();
    assert_eq!(runs, [Some(first.id.as_str()), None]);
    let states: Vec<&str> = project
        .tasks
        .iter()
        .map(|t| hub.project_task_state(&f.realm, t))
        .collect();
    assert_eq!(states, ["running", "to come"]);
    let mut chief_done = hub.load_run(&f.realm, &chief.id).unwrap();
    chief_done.status = Status::Completed;
    chief_done.save().unwrap();

    // The writer ends: the developer starts with what it delivered.
    first.status = Status::Completed;
    first.report = Some("Article drafted: hedgen.xyz/blog/2026".into());
    first.save().unwrap();
    hub.dispatched_ended(&f.realm, &first).unwrap();
    let next = hub.take_hand_offs().unwrap();
    assert_eq!(next.len(), 1, "the developer, not yet the interlocutor");
    let mut second = next[0].1.clone();
    assert_eq!(second.agent.as_deref(), Some("developer"));
    assert!(
        second.objective.contains("hedgen.xyz/blog/2026"),
        "{}",
        second.objective
    );

    // The last ends: the interlocutor is told how each went, once.
    second.status = Status::Completed;
    second.report = Some("Pull request #42 opened.".into());
    second.save().unwrap();
    hub.dispatched_ended(&f.realm, &second).unwrap();
    hub.dispatched_ended(&f.realm, &second).unwrap();
    let told = hub.take_hand_offs().unwrap();
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].1.id, chief.id);
    let last = format!("{:?}", told[0].1.messages.last().unwrap());
    assert!(
        last.contains("Article drafted") && last.contains("Pull request #42"),
        "{last}"
    );
    // Every task ended: the project is done.
    let project = Store::of(&f.realm)
        .unwrap()
        .projects()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(project.status, "done");
    assert_eq!(project.tasks[1].run.as_deref(), Some(second.id.as_str()));
    assert!(
        project
            .tasks
            .iter()
            .all(|t| hub.project_task_state(&f.realm, t) == "done")
    );
}

/// Decision 120: the founder edits the plan before approving it, and the
/// plan as edited is the one that runs; a step whose wait failed never
/// starts, and the interlocutor hears why.
#[test]
fn the_plan_runs_as_the_founder_edited_it_and_stops_where_a_step_failed() {
    use crate::business::store::queue::Answer;
    use crate::tools::dispatch::{Proposal, Step};
    let f = fixture();
    let root = f.root.path().to_path_buf();
    agent(
        &root,
        "chief-of-staff",
        "title: Chief of staff\ndispatch: true\n",
    );
    agent(&root, "writer", "title: Writer\n");
    agent(&root, "developer", "title: Developer\n");
    agent(&root, "seo", "title: SEO\n");
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let chief = hub
        .start_task(&f.realm, "chief-of-staff", "Year-end")
        .unwrap();
    let dispatch = hub.dispatch_for(&f.realm, &chief).unwrap().unwrap();
    let plan = Proposal {
        name: "Year-end".into(),
        steps: vec![
            Step {
                agent: "writer".into(),
                task: "A recap".into(),
                after: vec![],
                milestone: None,
            },
            Step {
                agent: "seo".into(),
                task: "Keywords".into(),
                after: vec![],
                milestone: None,
            },
        ],
        ..Default::default()
    };
    (dispatch.propose)(&plan, None).unwrap();
    let item = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .pop()
        .unwrap();
    // The founder drops SEO and adds the developer after the writer.
    let edited = "Year-end\n\n- writer: A recap of 800 words\n- developer (after writer): A banner";
    hub.answer_item(
        &f.realm,
        item.id,
        Some(item.revision),
        Answer::Edit(edited.into()),
        "app",
        5,
    )
    .unwrap();
    let item = Store::of(&f.realm)
        .unwrap()
        .queue(&["pending"])
        .unwrap()
        .pop()
        .unwrap();
    hub.answer_item(
        &f.realm,
        item.id,
        Some(item.revision),
        Answer::Approve,
        "app",
        1,
    )
    .unwrap();
    let handed = hub.take_hand_offs().unwrap();
    assert_eq!(handed.len(), 1);
    assert!(handed[0].1.objective.starts_with("A recap of 800 words"));
    // The writer fails: the developer never starts, and the interlocutor hears why.
    let mut chief_done = hub.load_run(&f.realm, &chief.id).unwrap();
    chief_done.status = Status::Completed;
    chief_done.save().unwrap();
    let mut first = handed[0].1.clone();
    first.status = Status::Failed;
    first.report = Some("The model stopped answering.".into());
    first.save().unwrap();
    hub.dispatched_ended(&f.realm, &first).unwrap();
    let told = hub.take_hand_offs().unwrap();
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].1.id, chief.id);
    let last = format!("{:?}", told[0].1.messages.last().unwrap());
    assert!(
        last.contains("developer (not started): writer ended failed"),
        "{last}"
    );
    // The project holds the plan as edited, and why the developer's task
    // never started.
    let project = Store::of(&f.realm)
        .unwrap()
        .projects()
        .unwrap()
        .pop()
        .unwrap();
    let tasks: Vec<(&str, Option<&str>)> = project
        .tasks
        .iter()
        .map(|t| (t.agent.as_str(), t.skipped.as_deref()))
        .collect();
    assert_eq!(
        tasks,
        [("writer", None), ("developer", Some("writer ended failed"))]
    );
    assert_eq!(project.status, "done");
    let states: Vec<&str> = project
        .tasks
        .iter()
        .map(|t| hub.project_task_state(&f.realm, t))
        .collect();
    assert_eq!(states, ["failed", "not started"]);
}

/// Decision 125: a task stopped on its budget gets remedies from the fast
/// model, journaled; the one the founder applies raises its budget within
/// the bound, tells its agent, closes the incident, and the task goes on.
#[test]
fn a_stopped_task_goes_on_with_the_remedy_the_founder_chose() {
    let f = fixture();
    agent(f.root.path(), "writer", "");
    let answer = r#"{"remedies": [
        {"title": "Write from what you have", "message": "Stop researching and write the article from your notes."},
        {"title": "Raise to $3", "message": "Finish the article.", "budget_usd": 3}
    ]}"#;
    let (hub, seen, _) = make_hub(&f.home, vec![done(answer)]);
    let mut state = hub
        .start_task(&f.realm, "writer", "Write the year-end article")
        .unwrap();
    state.status = Status::Stopped;
    state.report = Some("Stopped: reached its cost budget ($1.05 of $1.00).".into());
    state.save().unwrap();
    hub.task_ended(&f.realm, &state).unwrap();
    let item = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap()[0].clone();
    assert_eq!(item.run.as_deref(), Some(state.id.as_str()));

    let remedies = hub.remedies(&f.realm, item.id).unwrap();
    assert_eq!(remedies.len(), 2);
    assert_eq!(remedies[1].budget_usd, Some(3.0));
    let asked = format!("{:?}", seen.lock().unwrap().last().unwrap());
    assert!(
        asked.contains("the most it may be raised to: $5.00"),
        "{asked}"
    );
    let calls = Store::of(&f.realm).unwrap().model_calls().unwrap();
    assert!(
        calls
            .iter()
            .any(|c| c.pipeline == "remedy" && c.error.is_none())
    );

    // Beyond the bound, refused; within it, applied.
    let err = hub
        .apply_remedy(&f.realm, item.id, "Finish it.", Some(50.0), "app", 0)
        .unwrap_err();
    assert!(format!("{err:#}").contains("$5.00 at most"), "{err:#}");
    assert!(
        hub.apply_remedy(&f.realm, item.id, "  ", None, "app", 0)
            .is_err()
    );
    let resumed = hub
        .apply_remedy(
            &f.realm,
            item.id,
            "Finish the article.",
            Some(3.0),
            "app",
            0,
        )
        .unwrap();
    assert_eq!(resumed.status, Status::Running);
    assert_eq!(resumed.budget.max_cost_usd, Some(3.0));
    let last = format!("{:?}", resumed.messages.last().unwrap());
    assert!(
        last.contains("The founder raised its cost budget to $3.00 and says: Finish the article."),
        "{last}"
    );
    let store = Store::of(&f.realm).unwrap();
    assert!(store.queue(&["pending"]).unwrap().is_empty());
    assert_eq!(store.queue(&["done"]).unwrap()[0].id, item.id);
    // Answered once: a second remedy is refused.
    assert!(
        hub.apply_remedy(&f.realm, item.id, "Again.", None, "app", 0)
            .is_err()
    );
}

/// A remedy that does not fit is recorded and refused, never repaired.
#[test]
fn remedies_that_do_not_fit_are_refused() {
    let f = fixture();
    agent(f.root.path(), "writer", "");
    let (hub, _, _) = make_hub(
        &f.home,
        vec![done(
            r#"{"remedies": [{"title": "Raise", "message": "Go on.", "budget_usd": 40}]}"#,
        )],
    );
    let mut state = hub.start_task(&f.realm, "writer", "Write").unwrap();
    state.status = Status::Stopped;
    state.report = Some("Stopped: reached its cost budget ($1.05 of $1.00).".into());
    state.save().unwrap();
    hub.task_ended(&f.realm, &state).unwrap();
    let item = Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap()[0].clone();
    let err = hub.remedies(&f.realm, item.id).unwrap_err();
    assert!(format!("{err:#}").contains("did not fit"), "{err:#}");
    let calls = Store::of(&f.realm).unwrap().model_calls().unwrap();
    assert!(
        calls
            .iter()
            .any(|c| c.pipeline == "remedy" && c.error.is_some())
    );
}

/// Decision 129: a later plan adds to an open project rather than making
/// another, with its milestones, and the project is active again.
#[test]
fn a_later_plan_adds_to_the_open_project_it_names() {
    use crate::business::store::queue::Answer;
    use crate::tools::dispatch::{Milestone, Proposal, Step};
    let f = fixture();
    let root = f.root.path().to_path_buf();
    agent(
        &root,
        "chief-of-staff",
        "title: Chief of staff\ndispatch: true\n",
    );
    agent(&root, "writer", "title: Writer\n");
    let (hub, _, _) = make_hub(&f.home, vec![]);
    let chief = hub
        .start_task(&f.realm, "chief-of-staff", "Year-end")
        .unwrap();
    let step = |task: &str, milestone| Step {
        agent: "writer".into(),
        task: task.into(),
        after: vec![],
        milestone,
    };
    let approve = |hub: &Hub| {
        let item = Store::of(&f.realm)
            .unwrap()
            .queue(&["pending"])
            .unwrap()
            .pop()
            .unwrap();
        hub.answer_item(
            &f.realm,
            item.id,
            Some(item.revision),
            Answer::Approve,
            "app",
            1,
        )
        .unwrap();
        item
    };
    let dispatch = hub.dispatch_for(&f.realm, &chief).unwrap().unwrap();
    assert!(dispatch.projects.is_empty());
    let first = Proposal {
        name: "Year-end recap".into(),
        description: "Hedgen's 2026, told everywhere.".into(),
        milestones: vec![Milestone {
            title: "The article".into(),
            due: Some("2026-12-15".into()),
        }],
        steps: vec![step("A recap article", Some(0))],
    };
    (dispatch.propose)(&first, None).unwrap();
    let item = approve(&hub);
    assert!(
        item.args["text"]
            .as_str()
            .unwrap()
            .contains("## The article (by 2026-12-15)")
    );
    let id = Store::of(&f.realm).unwrap().projects().unwrap()[0].id;

    // The interlocutor's next task sees the open project and adds to it.
    let dispatch = hub.dispatch_for(&f.realm, &chief).unwrap().unwrap();
    assert_eq!(dispatch.projects, [(id, "Year-end recap".to_string())]);
    let more = Proposal {
        name: "Year-end recap".into(),
        milestones: vec![Milestone {
            title: "Everywhere".into(),
            due: None,
        }],
        steps: vec![step("A LinkedIn post", Some(0))],
        ..Default::default()
    };
    (dispatch.propose)(&more, Some(id)).unwrap();
    approve(&hub);
    let projects = Store::of(&f.realm).unwrap().projects().unwrap();
    assert_eq!(projects.len(), 1, "the same project, not another");
    let p = &projects[0];
    assert_eq!(p.description, "Hedgen's 2026, told everywhere.");
    assert_eq!(p.milestones.len(), 2);
    let tasks: Vec<(&str, Option<i64>)> = p
        .tasks
        .iter()
        .map(|t| (t.task.as_str(), t.milestone))
        .collect();
    assert_eq!(
        tasks,
        [
            ("A recap article", Some(p.milestones[0].id)),
            ("A LinkedIn post", Some(p.milestones[1].id))
        ]
    );
    assert!(p.tasks.iter().all(|t| t.run.is_some()));
}
