//! Objectives, budgets and the North Star.

use super::*;

#[test]
fn north_star_milestones_are_brainstormed_then_advanced() {
    let f = fixture();
    let hub = Hub::new(f.home.clone());
    assert!(hub.brainstorm_north_star(&f.realm).is_err(), "no goal yet");
    f.realm
        .north_star()
        .set_goal("100 kit sales by December", "Fund the next product")
        .unwrap();
    assert!(hub.push_north_star(&f.realm).is_err(), "no milestones yet");

    // Brainstorm: a proposal first, recorded only after the owner agrees.
    let milestones = json!({"action": "set_milestones", "milestones": [
        {"title": "Landing page live", "note": "deployed, checkout works"},
        {"title": "First 10 sales"}]});
    let (hub, _, systems) = make_hub(
        &f.home,
        vec![
            done("I propose: 1. Landing page live 2. First 10 sales. What should change?"),
            call("1", "north_star", milestones),
            call(
                "2",
                "north_star",
                json!({"action": "set_goal", "goal": "100 kit sales by 15 December", "why": "Fund the next product"}),
            ),
            done("Recorded."),
        ],
    );
    let mut brainstorm = hub.brainstorm_north_star(&f.realm).unwrap();
    assert_eq!(brainstorm.tier, Some(Tier::Strong));
    hub.drive(
        &f.realm,
        &mut brainstorm,
        config(5),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    assert!(
        f.realm
            .north_star()
            .get()
            .unwrap()
            .unwrap()
            .milestones
            .is_empty()
    );
    let system = systems.lock().unwrap()[0].clone();
    assert!(system.contains("100 kit sales by December"), "{system}");
    assert!(system.contains("No milestones yet"), "{system}");
    hub.send(
        &f.realm,
        &brainstorm.id,
        "Looks good",
        config(5),
        hooks(&mut NoObserver, true),
    )
    .unwrap();
    let star = f.realm.north_star().get().unwrap().unwrap();
    assert_eq!(star.progress(), (0, 2));
    assert_eq!(star.milestones[0].done_when, "deployed, checkout works");
    assert_eq!(
        star.goal, "100 kit sales by 15 December",
        "the owner approved it"
    );

    // A push run advances the next milestone; an unrelated run does not count.
    let (hub, _, systems) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "north_star",
                json!({"action": "update", "milestone": "m1", "status": "done", "note": "Live on Vercel"}),
            ),
            done("Landing page is live."),
            done("Here is the weather."),
        ],
    );
    let mut push = hub.push_north_star(&f.realm).unwrap();
    hub.drive(&f.realm, &mut push, config(5), hooks(&mut NoObserver, true))
        .unwrap();
    assert!(systems.lock().unwrap()[0].contains("[ ] m1. Landing page live"));
    let other = hub
        .dispatch(
            &f.realm,
            "Weather?",
            config(5),
            hooks(&mut NoObserver, true),
        )
        .unwrap();
    let star = f.realm.north_star().get().unwrap().unwrap();
    assert_eq!(star.progress(), (1, 2));
    assert!(star.involves(&push.id));
    let focus: Vec<(String, bool)> = hub
        .runs(&f.realm)
        .unwrap()
        .into_iter()
        .map(|r| (r.id, r.focus))
        .collect();
    assert!(focus.contains(&(brainstorm.id.clone(), true)));
    assert!(focus.contains(&(push.id.clone(), true)));
    assert!(focus.contains(&(other.id, false)));

    // Denied by the owner, nothing is recorded.
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "north_star",
                json!({"action": "update", "milestone": "m2", "status": "done"}),
            ),
            done("ok"),
        ],
    );
    hub.dispatch(&f.realm, "x", config(5), hooks(&mut NoObserver, false))
        .unwrap();
    assert_eq!(
        f.realm.north_star().get().unwrap().unwrap().progress(),
        (1, 2)
    );

    // The daily push: an objective whose runs may record progress unasked.
    hub.schedule_north_star(&f.realm, Some("weekdays 09:00"))
        .unwrap();
    let o = f.realm.objectives().get(northstar::OBJECTIVE_ID).unwrap();
    assert_eq!(o.objective, PUSH_OBJECTIVE);
    assert_eq!(o.approvals[0].scope.as_deref(), Some(northstar::SCOPE));
    assert!(hub.schedule_north_star(&f.realm, Some("whenever")).is_err());
    let (state, _) = hub
        .start_objective(
            &f.realm,
            northstar::OBJECTIVE_ID,
            "manual",
            None,
            Utc::now(),
        )
        .unwrap();
    assert!(state.north_star);
    hub.schedule_north_star(&f.realm, None).unwrap();
    assert!(f.realm.objectives().get(northstar::OBJECTIVE_ID).is_err());
}

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
            project: None,
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
            project: None,
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
