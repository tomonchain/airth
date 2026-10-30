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
