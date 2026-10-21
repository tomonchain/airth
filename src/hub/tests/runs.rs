//! Continuing, resuming, deleting and isolating runs.

use super::*;

#[test]
fn send_continues_a_stopped_run_with_a_fresh_budget() {
    let f = fixture();
    let (hub, seen, _) = make_hub(
        &f.home,
        vec![call("1", "list_directory", json!({})), done("Done.")],
    );

    let first = hub
        .dispatch(
            &f.realm,
            "look around",
            config(1),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(first.status, Status::Stopped);
    assert_eq!(first.realm, "work");
    assert_eq!(hub.runs(&f.realm).unwrap().len(), 1);
    assert!(
        !hub.is_live(&f.realm, &first.id),
        "lock released after the run"
    );

    let second = hub
        .send(
            &f.realm,
            "last",
            "keep going",
            config(1),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.status, Status::Completed);
    assert_eq!(second.report.as_deref(), Some("Done."));
    assert_eq!(second.iterations, 2);

    // The follow-up was merged into the pending user turn (after the tool result).
    let sent = &seen.lock().unwrap()[1];
    assert_eq!(sent.len(), 3);
    assert_eq!(
        sent[2].content.last(),
        Some(&ContentBlock::Text {
            text: "keep going".into()
        })
    );
    assert_eq!(hub.runs(&f.realm).unwrap()[0].status, Status::Completed);
}

#[test]
fn interrupted_runs_can_be_continued() {
    let f = fixture();
    let hub = Hub::new(f.home.clone());
    let mut state = hub.create_run(&f.realm, "obj").unwrap();
    state.messages.push(Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: "t".into(),
            name: "list_directory".into(),
            input: json!({}),
        }],
    });
    state.save().unwrap();
    // A lock left by a process that no longer exists.
    std::fs::write(
        f.realm.runs_dir().join(format!("{}.lock", state.id)),
        "999999",
    )
    .unwrap();

    assert_eq!(
        hub.runs(&f.realm).unwrap()[0].display_status(),
        "interrupted"
    );
    let resumed = hub.follow_up(&f.realm, &state.id, "carry on").unwrap();
    assert_eq!(resumed.status, Status::Running);
    let last = resumed.messages.last().unwrap();
    assert!(matches!(
        &last.content[0],
        ContentBlock::ToolResult { is_error: true, .. }
    ));
}

#[test]
fn live_runs_cannot_be_continued() {
    let f = fixture();
    let hub = Hub::new(f.home.clone());
    let state = hub.create_run(&f.realm, "obj").unwrap();
    let _lock = RunLock::acquire(&f.realm.runs_dir(), &state.id).unwrap();
    assert!(hub.is_live(&f.realm, &state.id));
    assert!(hub.follow_up(&f.realm, &state.id, "hi").is_err());
}

#[test]
fn deleting_a_run_removes_it_and_its_marks() {
    let f = fixture();
    let hub = Hub::new(f.home.clone());
    let state = hub.create_run(&f.realm, "obj").unwrap();
    f.realm.set_run_title(&state.id, "Name").unwrap();
    f.realm.set_run_archived(&state.id, true).unwrap();
    hub.delete_run(&f.realm, &state.id).unwrap();
    assert!(hub.runs(&f.realm).unwrap().is_empty());
    assert!(f.realm.run_titles().is_empty());
    assert!(f.realm.archived_runs().is_empty());
    assert!(hub.delete_run(&f.realm, &state.id).is_err());
}

#[test]
fn live_runs_cannot_be_deleted() {
    let f = fixture();
    let hub = Hub::new(f.home.clone());
    let state = hub.create_run(&f.realm, "obj").unwrap();
    let _lock = RunLock::acquire(&f.realm.runs_dir(), &state.id).unwrap();
    assert!(hub.delete_run(&f.realm, &state.id).is_err());
    assert_eq!(hub.runs(&f.realm).unwrap().len(), 1);
}

#[test]
fn send_rejects_unknown_or_malformed_runs() {
    let f = fixture();
    let hub = Hub::new(f.home.clone());
    assert!(hub.follow_up(&f.realm, "last", "hi").is_err());
    assert!(hub.follow_up(&f.realm, "nope", "hi").is_err());
    assert!(hub.follow_up(&f.realm, "../x", "hi").is_err());
}

/// v0.2 acceptance: two realms, neither sees the other's private data.
#[test]
fn realms_do_not_see_each_other() {
    let f = fixture();
    std::fs::write(f.root.path().join("contract.txt"), "work-confidential").unwrap();
    f.realm
        .memory()
        .add("The client is Initech", "owner")
        .unwrap();
    f.realm
        .set_secret("WORK_TOKEN", "work-token-value")
        .unwrap();

    let personal = f
        .home
        .create_realm(NewRealm {
            name: "personal".into(),
            description: String::new(),
            root: None,
            isolation: None,
        })
        .unwrap();
    let mut policy = personal.config.policy.clone();
    policy.execute = Rule::Allow;
    let mut personal = personal;
    personal.config.policy = policy;
    personal.save().unwrap();
    personal.memory().add("Likes hiking", "owner").unwrap();

    let contract = f.root.path().canonicalize().unwrap().join("contract.txt");
    let secrets = f.home.dir().join("realms/work/secrets.env");
    let (hub, seen, systems) = make_hub(
        &f.home,
        vec![
            call("1", "read_file", json!({"path": contract})),
            call("2", "read_file", json!({"path": secrets})),
            call(
                "3",
                "execute_command",
                json!({"command": format!("echo \"[$WORK_TOKEN]\"; cat {} {}", contract.display(), secrets.display())}),
            ),
            done("ok"),
        ],
    );
    let state = hub
        .dispatch(
            &personal,
            "snoop",
            config(10),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.status, Status::Completed);

    let system = &systems.lock().unwrap()[0];
    assert!(system.contains("Likes hiking"));
    assert!(!system.contains("Initech"));

    assert!(
        state.events[0].result.contains("outside the workspace"),
        "{}",
        state.events[0].result
    );
    assert!(
        state.events[1].result.contains("outside the workspace"),
        "{}",
        state.events[1].result
    );
    let shell = &state.events[2].result;
    assert!(shell.contains("[]"), "{shell}");
    // Reading the other realm's files from a command is only stopped by the sandbox.
    if crate::sandbox::available() {
        assert!(!shell.contains("work-token-value"), "{shell}");
        assert!(!shell.contains("work-confidential"), "{shell}");
    }
    // What the model saw includes the command's output, so this too needs the sandbox.
    if crate::sandbox::available() {
        let everything = format!("{:?}", seen.lock().unwrap());
        assert!(!everything.contains("work-token-value"));
    }
    // The work realm itself can read its file, and never sees its token in clear.
    let (hub, _, systems) = make_hub(
        &f.home,
        vec![
            call("1", "read_file", json!({"path": "contract.txt"})),
            call(
                "2",
                "execute_command",
                json!({"command": "echo $WORK_TOKEN"}),
            ),
            done("ok"),
        ],
    );
    let state = hub
        .dispatch(&f.realm, "read", config(10), hooks(&mut NoObserver, true))
        .unwrap();
    assert_eq!(state.events[0].result, "work-confidential");
    assert!(
        state.events[1].result.contains("[secret:WORK_TOKEN]"),
        "{}",
        state.events[1].result
    );
    assert!(systems.lock().unwrap()[0].contains("Initech"));
}

/// Approves everything for the rest of the run, and records what it was asked.
struct GrantAll(Arc<Mutex<Vec<String>>>);

impl Approver for GrantAll {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        self.0.lock().unwrap().push(request.tool.clone());
        Ok(Answer::YesForRun)
    }
}

/// A tracker API on loopback: serves the issue list, records posted comments.
/// Each request the tracker received: "METHOD path", Authorization, body.
type Received = Vec<(String, String, String)>;

fn tracker() -> (String, std::thread::JoinHandle<Received>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}/api", server.server_addr().to_ip().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let mut req = server.recv().unwrap();
            let auth = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"))
                .map(|h| h.value.to_string())
                .unwrap_or_default();
            let mut body = String::new();
            req.as_reader().read_to_string(&mut body).unwrap();
            seen.push((format!("{} {}", req.method(), req.url()), auth, body));
            let reply = if req.url() == "/api/issues" {
                r#"[{"id":1,"title":"Login fails on Safari"},{"id":2,"title":"Typo in footer"}]"#
            } else {
                r#"{"ok":true}"#
            };
            req.respond(tiny_http::Response::from_string(reply))
                .unwrap();
        }
        seen
    });
    (base, handle)
}

fn plan(steps: &[(&str, &str)]) -> serde_json::Value {
    json!({"steps": steps.iter().map(|(t, s)| json!({"title": t, "status": s})).collect::<Vec<_>>()})
}

/// v0.3 acceptance: Airth completes a multi-step task across authorized
/// tools and resumes after interruption.
#[test]
fn multi_step_task_across_tools_resumes_after_interruption() {
    let mut f = fixture();
    let (base, server) = tracker();
    f.realm
        .set_secret("TRACKER_TOKEN", "trk-secret-42")
        .unwrap();
    f.realm
        .set_integration(crate::integration::Integration {
            name: "tracker".into(),
            description: "The team's issue tracker".into(),
            base_url: base,
            headers: [(
                "Authorization".to_string(),
                "Bearer [secret:TRACKER_TOKEN]".to_string(),
            )]
            .into(),
        })
        .unwrap();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let run_hooks = |observer| RunHooks {
        approver: Box::new(GrantAll(asked.clone())),
        observer,
        cancel: CancelToken::default(),
    };
    let steps = [
        "Fetch open issues",
        "Write the triage report",
        "Post a summary",
    ];

    // Part one: plan, fetch through the integration, then the model provider goes down.
    let (hub, _, systems) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "update_plan",
                plan(&[
                    (steps[0], "in_progress"),
                    (steps[1], "pending"),
                    (steps[2], "pending"),
                ]),
            ),
            call(
                "2",
                "http_request",
                json!({"integration": "tracker", "url": "issues"}),
            ),
            call(
                "3",
                "update_plan",
                plan(&[
                    (steps[0], "done"),
                    (steps[1], "in_progress"),
                    (steps[2], "pending"),
                ]),
            ),
        ],
    );
    let mut observer = NoObserver;
    let err = hub
        .dispatch(
            &f.realm,
            "Triage the open issues",
            config(20),
            run_hooks(&mut observer),
        )
        .unwrap_err();
    assert!(format!("{err:#}").contains("connection reset"));
    let failed = hub.load_run(&f.realm, "last").unwrap();
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(failed.plan.len(), 3);
    assert!(failed.plan[0].status.is_closed());
    assert!(failed.events[1].result.contains("Login fails on Safari"));
    assert_eq!(
        failed.grants.len(),
        1,
        "the owner allowed the tracker for this run"
    );
    assert!(systems.lock().unwrap()[0].contains("http_request"));

    // Part two: the owner resumes it. The agent picks up from its plan.
    let (hub, seen, _) = make_hub(
        &f.home,
        vec![
            call(
                "4",
                "write_file",
                json!({"path": "triage.md", "content": "# Triage\n\n1. Login fails on Safari: high\n2. Typo in footer: low\n"}),
            ),
            call(
                "5",
                "http_request",
                json!({"integration": "tracker", "method": "POST", "url": "issues/1/comments", "body": "Marked high priority."}),
            ),
            call(
                "6",
                "update_plan",
                plan(&[(steps[0], "done"), (steps[1], "done"), (steps[2], "done")]),
            ),
            done("Triaged 2 issues: report in triage.md, summary posted on #1."),
        ],
    );
    let mut observer = NoObserver;
    let state = hub
        .resume_and_drive(&f.realm, &failed.id, config(20), run_hooks(&mut observer))
        .unwrap();
    assert_eq!(state.status, Status::Completed);
    assert!(state.plan.iter().all(|s| s.status.is_closed()));
    assert!(
        std::fs::read_to_string(f.root.path().join("triage.md"))
            .unwrap()
            .contains("Safari")
    );

    // The agent was told how it was interrupted, with its plan.
    let seen = seen.lock().unwrap();
    let resumed = serde_json::to_string(seen[0].last().unwrap()).unwrap();
    assert!(resumed.contains("connection reset"), "{resumed}");
    assert!(resumed.contains("[x] 1. Fetch open issues"), "{resumed}");
    assert!(
        resumed.contains("[~] 2. Write the triage report"),
        "{resumed}"
    );

    // The tracker was approved once for the whole run, the write asked for.
    assert_eq!(
        *asked.lock().unwrap(),
        vec!["http_request".to_string(), "write_file".to_string()]
    );

    // The credential reached the tracker, and never the model.
    let requests = server.join().unwrap();
    assert_eq!(requests[0].0, "GET /api/issues");
    assert_eq!(requests[1].0, "POST /api/issues/1/comments");
    assert_eq!(requests[1].2, "Marked high priority.");
    assert!(requests.iter().all(|r| r.1 == "Bearer trk-secret-42"));
    for messages in seen.iter() {
        assert!(
            !serde_json::to_string(messages)
                .unwrap()
                .contains("trk-secret-42")
        );
    }
}

#[test]
fn resume_answers_tool_calls_cut_off_by_a_crash() {
    let f = fixture();
    let (hub, seen, _) = make_hub(&f.home, vec![done("Checked: nothing was written.")]);
    let mut state = hub.create_run(&f.realm, "write a file").unwrap();
    // What a process killed while running a tool leaves behind.
    state.messages.push(Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: "t1".into(),
            name: "write_file".into(),
            input: json!({"path": "a.txt", "content": "x"}),
        }],
    });
    state.save().unwrap();
    assert_eq!(
        hub.runs(&f.realm).unwrap()[0].display_status(),
        "interrupted"
    );

    let state = hub
        .resume_and_drive(
            &f.realm,
            &state.id,
            config(5),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.status, Status::Completed);
    let sent = serde_json::to_string(seen.lock().unwrap()[0].last().unwrap()).unwrap();
    assert!(sent.contains("Not run: the run was interrupted."), "{sent}");
    assert!(sent.contains("was interrupted: the process"), "{sent}");

    // A completed run is continued with a message, not resumed.
    assert!(hub.resume(&f.realm, &state.id).is_err());
}
