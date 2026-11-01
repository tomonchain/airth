//! Objectives and budgets.

use super::*;

/// Nobody is there: every request that needs approval is denied, and counted.
struct Away(Arc<Mutex<Vec<String>>>);

impl Approver for Away {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        self.0.lock().unwrap().push(request.tool.clone());
        Ok(Answer::No(away_reason(0)))
    }
}

/// v0.4 acceptance: Airth completes an approved scheduled objective while
/// the owner is absent.
#[test]
fn scheduled_objective_completes_while_the_owner_is_away() {
    use crate::objective::{Objective, Trigger};
    use crate::permissions::{Capability, Grant};
    use crate::scheduler;
    let f = fixture();
    f.realm
        .objectives()
        .save(Objective {
            id: "daily-notes".into(),
            objective: "Write today's notes to notes.md".into(),
            trigger: Trigger::Schedule {
                every: "every 1h".into(),
            },
            approvals: vec![Grant {
                capability: Capability::Write,
                scope: None,
            }],
            budget: crate::agent::Budget {
                max_iterations: Some(10),
                ..Default::default()
            },
            approval_wait_minutes: 0,
            enabled: true,
            created_at: Utc::now(),
            repo: None,
            bot: None,
            tools: None,
            delegates: Vec::new(),
            next_at: None,
            fingerprint: None,
            last_run: None,
            last_at: None,
            last_status: None,
        })
        .unwrap();
    let (hub, seen, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "notes.md", "content": "# Notes\n"}),
            ),
            call("2", "execute_command", json!({"command": "git push"})),
            done("Wrote notes.md. Pushing needs the owner: it was not approved."),
        ],
    );

    // The first look arms the schedule; it fires once its time has come.
    let t0 = Utc::now();
    assert!(scheduler::due(&hub, t0).unwrap().is_empty());
    let armed = f
        .realm
        .objectives()
        .get("daily-notes")
        .unwrap()
        .next_at
        .unwrap();
    assert!(
        scheduler::due(&hub, armed - chrono::Duration::seconds(1))
            .unwrap()
            .is_empty()
    );
    let due = scheduler::due(&hub, armed).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].trigger, "schedule: every 1h");

    let (mut state, _) = hub
        .start_objective(
            &due[0].realm,
            &due[0].objective,
            &due[0].trigger,
            None,
            armed,
        )
        .unwrap();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut observer = NoObserver;
    hub.drive(
        &f.realm,
        &mut state,
        config(50),
        RunHooks {
            approver: Box::new(Away(asked.clone())),
            observer: &mut observer,
            cancel: CancelToken::default(),
        },
    )
    .unwrap();

    assert_eq!(state.status, Status::Completed);
    assert_eq!(
        std::fs::read_to_string(f.root.path().join("notes.md")).unwrap(),
        "# Notes\n"
    );
    // The write was approved in advance; only the push needed the owner.
    assert_eq!(*asked.lock().unwrap(), vec!["execute_command".to_string()]);
    assert_eq!(state.events[1].status, crate::state::EventStatus::Denied);
    let brief = serde_json::to_string(&seen.lock().unwrap()[0][0]).unwrap();
    assert!(brief.contains("started by the objective"), "{brief}");
    assert!(brief.contains("all write"), "{brief}");

    // The owner finds a notification, and the objective is rescheduled.
    let inbox = f.home.inbox().list().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].title, "daily-notes: completed");
    assert!(inbox[0].body.contains("Pushing needs the owner"));
    let o = f.realm.objectives().get("daily-notes").unwrap();
    assert_eq!(o.last_status.as_deref(), Some("completed"));
    assert_eq!(o.next_at, Some(armed + chrono::Duration::hours(1)));
    assert!(
        scheduler::due(&hub, armed + chrono::Duration::minutes(5))
            .unwrap()
            .is_empty()
    );

    let summary = scheduler::summary(&hub, t0).unwrap();
    assert_eq!(summary[0].completed, 1);
    assert_eq!(summary[0].recent[0].origin.as_deref(), Some("daily-notes"));
}

#[test]
fn budget_of_an_objective_stops_its_run() {
    use crate::objective::{Objective, Trigger};
    let f = fixture();
    f.realm
        .objectives()
        .save(Objective {
            id: "loop".into(),
            objective: "Look around".into(),
            trigger: Trigger::Manual,
            approvals: vec![],
            budget: crate::agent::Budget {
                max_iterations: Some(1),
                ..Default::default()
            },
            approval_wait_minutes: 0,
            enabled: true,
            created_at: Utc::now(),
            repo: None,
            bot: None,
            tools: None,
            delegates: Vec::new(),
            next_at: None,
            fingerprint: None,
            last_run: None,
            last_at: None,
            last_status: None,
        })
        .unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![call("1", "list_directory", json!({})), done("never")],
    );
    let (mut state, _) = hub
        .start_objective(&f.realm, "loop", "manual", Some("x ```` y"), Utc::now())
        .unwrap();
    let brief = serde_json::to_string(&state.messages[0]).unwrap();
    assert!(brief.contains("`````\\nx ```` y\\n`````"), "{brief}");
    hub.drive(
        &f.realm,
        &mut state,
        config(50),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert_eq!(state.status, Status::Stopped);
    assert!(
        state
            .report
            .as_deref()
            .unwrap()
            .contains("iteration limit (1)")
    );
    let n = &f.home.inbox().list().unwrap()[0];
    assert_eq!(n.title, "loop: stopped");
    assert_eq!(
        f.realm
            .objectives()
            .get("loop")
            .unwrap()
            .last_status
            .as_deref(),
        Some("stopped")
    );
}

/// Decision 36: a bot's run is told its job and the owner's strategy, and
/// records which version of the job it followed. A bot whose job file is
/// missing does not start.
#[test]
fn a_bot_runs_with_its_job_and_the_strategy() {
    use crate::objective::{Objective, Trigger};
    let f = fixture();
    let business = f.root.path().join("business");
    std::fs::create_dir_all(f.root.path().join("agents")).unwrap();
    std::fs::create_dir_all(&business).unwrap();
    std::fs::write(business.join("goal.md"), "Explain, never hype.").unwrap();
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
    let (hub, _, systems) = make_hub(&f.home, vec![done("Drafted.")]);
    let err = hub
        .start_objective(&f.realm, "weekly-article", "manual", None, Utc::now())
        .unwrap_err();
    assert!(format!("{err:#}").contains("agents/writer.md"), "{err:#}");

    std::fs::write(
        f.root.path().join("agents/writer.md"),
        "Write one article a week for the blog.",
    )
    .unwrap();
    let (mut state, _) = hub
        .start_objective(&f.realm, "weekly-article", "manual", None, Utc::now())
        .unwrap();
    let brief = serde_json::to_string(&state.messages[0]).unwrap();
    assert!(
        brief.contains("You are writer, one of the business's bots. Your job, as the owner wrote it:\\nWrite one article a week for the blog."),
        "{brief}"
    );
    assert!(brief.contains("started by the objective"), "{brief}");
    let version = crate::business::job(f.root.path(), "writer")
        .unwrap()
        .version;
    assert_eq!(
        state.origin.as_ref().unwrap().job_version.as_deref(),
        Some(version.as_str())
    );
    hub.drive(
        &f.realm,
        &mut state,
        config(10),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert_eq!(state.status, Status::Completed);
    let system = &systems.lock().unwrap()[0];
    assert!(system.contains("Explain, never hype."), "{system}");
}

/// Decision 36: a bot given some of the realm's integrations cannot reach
/// the others, even with the network allowed: they are not in its run.
#[test]
fn a_bot_reaches_only_the_integrations_it_was_given() {
    use crate::integration::Integration;
    use crate::objective::{Objective, Trigger};
    let mut f = fixture();
    let integration = |name: &str| Integration {
        name: name.into(),
        description: String::new(),
        base_url: format!("https://{name}.invalid"),
        headers: Default::default(),
    };
    f.realm.config.integrations = vec![integration("blog"), integration("bank")];
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
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
            bot: None,
            tools: Some(vec!["blog".into()]),
            delegates: Vec::new(),
            next_at: None,
            fingerprint: None,
            last_run: None,
            last_at: None,
            last_status: None,
        })
        .unwrap();
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "http_request",
                json!({"integration": "bank", "url": "transfers"}),
            ),
            done("Could not reach the bank."),
        ],
    );
    let (mut state, _) = hub
        .start_objective(&f.realm, "weekly-article", "manual", None, Utc::now())
        .unwrap();
    assert_eq!(state.tools, Some(vec!["blog".to_string()]));
    hub.drive(
        &f.realm,
        &mut state,
        config(10),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert_ne!(state.events[0].status, crate::state::EventStatus::Ok);
    assert!(
        state.events[0].result.contains("no integration 'bank'"),
        "{}",
        state.events[0].result
    );
    assert!(
        state.events[0].result.contains("blog"),
        "{}",
        state.events[0].result
    );
}

/// Decision 41: a routine's run asks another routine for work and gets its
/// report; it can ask only the routines the owner listed, never one already
/// in the chain, and a hand-off waits for the app to start it.
#[test]
fn routines_ask_each_other_within_the_owners_list() {
    use crate::objective::{Objective, Trigger};
    let f = fixture();
    let routine = |id: &str, delegates: &[&str]| Objective {
        id: id.into(),
        objective: format!("Do the {id} work"),
        trigger: Trigger::Manual,
        approvals: vec![],
        budget: Default::default(),
        approval_wait_minutes: 0,
        enabled: true,
        created_at: Utc::now(),
        repo: None,
        bot: None,
        tools: None,
        delegates: delegates.iter().map(|d| d.to_string()).collect(),
        next_at: None,
        fingerprint: None,
        last_run: None,
        last_at: None,
        last_status: None,
    };
    let store = f.realm.objectives();
    store
        .save(routine("article", &["figures", "publish"]))
        .unwrap();
    store.save(routine("figures", &["article"])).unwrap();
    store.save(routine("publish", &[])).unwrap();
    store.save(routine("bank", &[])).unwrap();

    let (hub, seen, _) = make_hub(
        &f.home,
        vec![
            // article asks a routine it may not ask, then figures.
            call("1", "ask_agent", json!({"routine": "bank", "brief": "Pay"})),
            call(
                "2",
                "ask_agent",
                json!({"routine": "figures", "brief": "Staked ETH this week"}),
            ),
            // figures, asked: tries to ask article back, then answers.
            call(
                "3",
                "ask_agent",
                json!({"routine": "article", "brief": "Loop"}),
            ),
            done("Staked ETH: 34.2M, up 1.1%."),
            // article again: hands off, then ends.
            call(
                "4",
                "hand_off",
                json!({"routine": "publish", "brief": "Publish the article"}),
            ),
            done("Article written with the figures."),
        ],
    );
    let (mut state, _) = hub
        .start_objective(&f.realm, "article", "manual", None, Utc::now())
        .unwrap();
    hub.drive(
        &f.realm,
        &mut state,
        config(20),
        hooks(&mut NoObserver, false),
    )
    .unwrap();
    assert_eq!(state.status, Status::Completed, "{:?}", state.report);

    let refused = &state.events[0];
    assert_ne!(refused.status, crate::state::EventStatus::Ok);
    assert!(
        refused.result.contains("you may not ask 'bank'"),
        "{}",
        refused.result
    );
    let answered = &state.events[1];
    assert_eq!(
        answered.status,
        crate::state::EventStatus::Ok,
        "{}",
        answered.result
    );
    assert!(
        answered.result.contains("Staked ETH: 34.2M"),
        "{}",
        answered.result
    );

    // The run asked for work is figures' own, with the brief as data and
    // the chain that led to it; asking article back was refused.
    let runs = hub.all_runs(&f.realm).unwrap();
    let figures = runs
        .iter()
        .find(|s| s.origin.as_ref().is_some_and(|o| o.objective == "figures"))
        .unwrap();
    assert_eq!(figures.origin.as_ref().unwrap().chain, vec!["article"]);
    let brief = serde_json::to_string(&figures.messages[0]).unwrap();
    assert!(brief.contains("Staked ETH this week"), "{brief}");
    assert!(brief.contains("untrusted"), "{brief}");
    assert!(
        figures.events[0].result.contains("go round in circles"),
        "{}",
        figures.events[0].result
    );

    // The hand-off waits for the app, once.
    let handed = hub.take_hand_offs().unwrap();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].1.origin.as_ref().unwrap().objective, "publish");
    assert!(hub.take_hand_offs().unwrap().is_empty());
    assert_eq!(seen.lock().unwrap().len(), 6, "every model call was used");
}
