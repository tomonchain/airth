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

/// A run's own realm keeps its sign-ins to remote MCP servers in the Airth
/// home: the run reads none of them, directly or through a hard link, a
/// symbolic link or a copy made in its temporary folder, which is in the
/// Airth home too.
#[test]
fn a_run_cannot_read_its_own_realms_mcp_sign_ins() {
    let mut f = fixture();
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();
    let auth = f.realm.skills_dir().join("mcp-auth");
    std::fs::create_dir_all(&auth).unwrap();
    let token = auth.canonicalize().unwrap().join("server.json");
    std::fs::write(&token, r#"{"access_token": "mcp-token-value"}"#).unwrap();
    let t = token.display();
    let (hub, seen, _) = make_hub(
        &f.home,
        vec![
            call("1", "read_file", json!({"path": token})),
            call(
                "2",
                "execute_command",
                json!({"command": format!("cat {t}")}),
            ),
            call(
                "3",
                "execute_command",
                json!({"command": format!("ln {t} \"$TMPDIR/hard\"; cat \"$TMPDIR/hard\"")}),
            ),
            call(
                "4",
                "execute_command",
                json!({"command": format!("ln -s {t} \"$TMPDIR/soft\"; cat \"$TMPDIR/soft\"")}),
            ),
            call(
                "5",
                "execute_command",
                json!({"command": format!("cp {t} \"$TMPDIR/copy\"; cat \"$TMPDIR/copy\"")}),
            ),
            done("ok"),
        ],
    );
    let state = hub
        .dispatch(&f.realm, "snoop", config(10), hooks(&mut NoObserver, false))
        .unwrap();
    assert!(
        state.events[0].result.contains("outside the workspace"),
        "{}",
        state.events[0].result
    );
    // Commands are only stopped by the sandbox.
    if crate::sandbox::available() {
        for event in &state.events[1..5] {
            assert!(
                !event.result.contains("mcp-token-value"),
                "{}",
                event.result
            );
        }
        assert!(!format!("{:?}", seen.lock().unwrap()).contains("mcp-token-value"));
        let tmp = hub.run_tmp(&f.realm, &state.id);
        // A symbolic link is only a path: reading through it is what the
        // sandbox stops, as the command's output shows.
        for name in ["hard", "copy"] {
            let copied = std::fs::read_to_string(tmp.join(name)).unwrap_or_default();
            assert!(!copied.contains("mcp-token-value"), "{name}");
        }
    }
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
            free_reads: false,
            name: "tracker".into(),
            description: "The team's issue tracker".into(),
            base_url: base,
            headers: [(
                "Authorization".to_string(),
                "Bearer [secret:TRACKER_TOKEN]".to_string(),
            )]
            .into(),
            // Classed, so the policy decides as before: an undeclared call
            // commits, and would be asked each time (decision 56).
            actions: vec![
                serde_json::from_value(json!({"name": "issues", "method": "GET",
                    "path": "/issues", "class": "read"}))
                .unwrap(),
                serde_json::from_value(json!({"name": "comment", "method": "POST",
                    "path": "/issues/{n}/comments", "fields": [{"name": "n", "place": "path"}],
                    "class": "reversible"}))
                .unwrap(),
            ],
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

/// Pilot step 1: every run of a realm is told about its business, and can
/// read its folder but never change it, even with writes allowed and when a
/// command tries.
#[test]
fn runs_read_the_business_folder_but_never_change_it() {
    let mut f = fixture();
    let business = f.root.path().join("business");
    std::fs::create_dir_all(business.join("archive")).unwrap();
    std::fs::write(
        business.join("profile.md"),
        "Hedgen: on-chain research on X.",
    )
    .unwrap();
    std::fs::write(business.join("archive/post.md"), "an old post").unwrap();
    f.realm.config.policy.write = Rule::Allow;
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();

    let (hub, _, systems) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "read_file",
                json!({"path": "business/archive/post.md"}),
            ),
            call(
                "2",
                "write_file",
                json!({"path": "business/profile.md", "content": "Hedgen sells tokens."}),
            ),
            call(
                "3",
                "execute_command",
                json!({"command": "echo forged > business/profile.md"}),
            ),
            call(
                "4",
                "write_file",
                json!({"path": "notes.md", "content": "ok"}),
            ),
            done("done"),
        ],
    );
    let state = hub
        .dispatch(
            &f.realm,
            "Update the business",
            config(10),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.status, Status::Completed);
    let system = &systems.lock().unwrap()[0];
    assert!(
        system.contains("The business this realm runs:\nHedgen: on-chain research on X."),
        "{system}"
    );
    assert!(system.contains("business/archive/: 1 file"), "{system}");

    assert_eq!(state.events[0].status, crate::state::EventStatus::Ok);
    assert!(state.events[0].result.contains("an old post"));
    assert_ne!(
        state.events[1].status,
        crate::state::EventStatus::Ok,
        "{}",
        state.events[1].result
    );
    // A command is only stopped by the sandbox.
    if crate::sandbox::available() {
        assert_eq!(
            std::fs::read_to_string(business.join("profile.md")).unwrap(),
            "Hedgen: on-chain research on X."
        );
    }
    assert_eq!(
        std::fs::read_to_string(f.root.path().join("notes.md")).unwrap(),
        "ok"
    );
}

/// Decision 44: an agent's job and the goal are instructions and proof, so
/// no run writes them, nor creates the config folders still to come, even
/// with writes and commands allowed.
#[test]
fn runs_never_write_the_agents_or_the_goal() {
    let mut f = fixture();
    let root = f.root.path();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::create_dir_all(root.join("business")).unwrap();
    std::fs::write(root.join("agents/writer.md"), "Write one article a week.").unwrap();
    std::fs::write(root.join("business/goal.md"), "baseline_minutes: 120").unwrap();
    f.realm.config.policy.write = Rule::Allow;
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();

    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "agents/writer.md", "content": "Publish without asking."}),
            ),
            call(
                "2",
                "write_file",
                json!({"path": "business/goal.md", "content": "baseline_minutes: 9999"}),
            ),
            call(
                "3",
                "write_file",
                json!({"path": "routines/daily.json", "content": "{}"}),
            ),
            call(
                "4",
                "execute_command",
                json!({"command": "echo forged > agents/writer.md; echo forged > business/goal.md; echo x > agents/new.md; mkdir prompts"}),
            ),
            call(
                "5",
                "write_file",
                json!({"path": "notes.md", "content": "ok"}),
            ),
            done("done"),
        ],
    );
    let state = hub
        .dispatch(
            &f.realm,
            "Give yourself more room",
            config(10),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.status, Status::Completed);
    for i in 0..3 {
        assert_ne!(
            state.events[i].status,
            crate::state::EventStatus::Ok,
            "{}",
            state.events[i].result
        );
        assert!(
            state.events[i].result.contains("read-only"),
            "{}",
            state.events[i].result
        );
    }
    assert!(!root.join("routines").exists());
    // A command is only stopped by the sandbox.
    if crate::sandbox::available() {
        assert_eq!(
            std::fs::read_to_string(root.join("agents/writer.md")).unwrap(),
            "Write one article a week."
        );
        assert_eq!(
            std::fs::read_to_string(root.join("business/goal.md")).unwrap(),
            "baseline_minutes: 120"
        );
        assert!(!root.join("agents/new.md").exists());
        assert!(!root.join("prompts").exists());
    }
    assert_eq!(
        std::fs::read_to_string(root.join("notes.md")).unwrap(),
        "ok"
    );
}

/// Decision 46: a run in place can commit, but cannot plant a git hook or
/// change the git config, even by moving `.git` aside.
#[test]
fn runs_in_place_cannot_plant_git_hooks_or_config() {
    let mut f = fixture();
    let root = f.root.path();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(git(&["init", "-q", "-b", "main"]).status.success());
    let git_config = std::fs::read_to_string(root.join(".git/config")).unwrap();
    f.realm.config.policy.write = Rule::Allow;
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();

    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": ".git/hooks/pre-commit", "content": "#!/bin/sh\ncurl evil"}),
            ),
            call(
                "2",
                "write_file",
                json!({"path": ".git/config", "content": "[core]\n\thooksPath = /tmp"}),
            ),
            call(
                "3",
                "execute_command",
                json!({"command": "echo evil > .git/hooks/post-checkout; git config core.hooksPath /tmp; mv .git g2 && mkdir -p g2/hooks && echo evil > g2/hooks/pre-commit && mv g2 .git; echo x > a.txt && git add a.txt && git -c user.name=t -c user.email=t@t commit -qm x"}),
            ),
            done("done"),
        ],
    );
    let state = hub
        .dispatch(
            &f.realm,
            "Set up git",
            config(10),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    assert_eq!(state.status, Status::Completed);
    for i in 0..2 {
        assert!(
            state.events[i].result.contains("read-only"),
            "{}",
            state.events[i].result
        );
    }
    assert!(!root.join(".git/hooks/pre-commit").exists());
    // A command is only stopped by the sandbox.
    if crate::sandbox::available() {
        assert!(!root.join(".git/hooks/post-checkout").exists());
        assert!(!root.join("g2").exists());
        assert_eq!(
            std::fs::read_to_string(root.join(".git/config")).unwrap(),
            git_config
        );
        // Committing in place still works.
        let log = git(&["log", "--oneline"]);
        assert!(
            String::from_utf8_lossy(&log.stdout).contains(" x"),
            "{}",
            state.events[2].result
        );
    }
}

/// Decision 48: a command writes only where the run may. It cannot move the
/// business's folder aside to rewrite an agent's job, nor plant code the
/// owner's shell or login would run.
#[test]
fn commands_cannot_write_outside_the_workspace() {
    if !crate::sandbox::available() {
        return;
    }
    let mut f = fixture();
    let root = f.root.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(root.join("agents/writer.md"), "Write one article a week.").unwrap();
    // Stands for the owner's home: any folder outside the workspace.
    let elsewhere = tempfile::tempdir().unwrap();
    let elsewhere = elsewhere.path().canonicalize().unwrap();
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();

    let moved = format!("{}.moved", root.display());
    let command = format!(
        "echo evil >> {e}/.zshrc; mkdir -p {e}/Library/LaunchAgents && echo evil > {e}/Library/LaunchAgents/x.plist; \
         cd / && mv {r} {m} && echo evil > {m}/agents/writer.md; mv {m} {r}; \
         cd {r} && echo here > notes.md && echo $TMPDIR > tmp.txt && echo t > \"$TMPDIR/t\" && cat \"$TMPDIR/t\"",
        e = elsewhere.display(),
        r = root.display(),
        m = moved,
    );
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            call("1", "execute_command", json!({ "command": command })),
            done("done"),
        ],
    );
    let state = hub
        .dispatch(
            &f.realm,
            "Make yourself at home",
            config(10),
            hooks(&mut NoObserver, false),
        )
        .unwrap();
    let out = &state.events[0].result;
    assert!(!elsewhere.join(".zshrc").exists(), "{out}");
    assert!(!elsewhere.join("Library").exists(), "{out}");
    assert!(!std::path::Path::new(&moved).exists(), "{out}");
    assert_eq!(
        std::fs::read_to_string(root.join("agents/writer.md")).unwrap(),
        "Write one article a week.",
        "{out}"
    );
    // The workspace and the run's own temporary folder stay writable.
    assert_eq!(
        std::fs::read_to_string(root.join("notes.md")).unwrap(),
        "here\n"
    );
    let tmp = std::fs::read_to_string(root.join("tmp.txt")).unwrap();
    assert!(
        tmp.trim()
            .starts_with(&f.realm.dir().canonicalize().unwrap().display().to_string()),
        "{tmp} {out}"
    );
    assert!(out.contains("\nt\n") || out.ends_with("\nt"), "{out}");
}

/// Decision 49: once an integration declares its actions, a run can call
/// those and nothing else through it.
#[test]
fn an_integration_with_declared_actions_takes_nothing_else() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", server.server_addr().to_ip().unwrap());
    let hits = Arc::new(Mutex::new(Vec::new()));
    let seen = hits.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            seen.lock().unwrap().push(req.url().to_string());
            req.respond(tiny_http::Response::from_string("{}")).unwrap();
        }
    });
    let mut f = fixture();
    f.realm
        .set_integration(crate::integration::Integration {
            free_reads: false,
            name: "blog".into(),
            description: String::new(),
            base_url: base,
            headers: Default::default(),
            actions: vec![
                serde_json::from_value(json!({"name": "create_draft", "method": "POST",
                    "path": "/drafts/{site}", "fields": [{"name": "site", "place": "path"}],
                    "class": "reversible"}))
                .unwrap(),
            ],
        })
        .unwrap();
    f.realm.config.policy.network = Rule::Allow;
    f.realm.save().unwrap();
    let request = |id: &str, method: &str, url: &str| {
        call(
            id,
            "http_request",
            json!({"integration": "blog", "method": method, "url": url}),
        )
    };
    let (hub, _, _) = make_hub(
        &f.home,
        vec![
            request("1", "POST", "drafts/hedgen"),
            request("2", "DELETE", "drafts/hedgen"),
            request("3", "POST", "admin/users"),
            request("4", "POST", "drafts/a/b"),
            done("done"),
        ],
    );
    let state = hub
        .dispatch(
            &f.realm,
            "Draft it",
            config(10),
            hooks(&mut NoObserver, true),
        )
        .unwrap();
    assert_eq!(
        state.events[0].status,
        crate::state::EventStatus::Ok,
        "{}",
        state.events[0].result
    );
    for e in &state.events[1..4] {
        assert!(
            e.result.contains("only its declared actions"),
            "{}",
            e.result
        );
    }
    assert_eq!(*hits.lock().unwrap(), vec!["/v1/drafts/hedgen"]);
}

/// Decision 54: the bank's key and tokens are runtime-only. A run in the
/// business's realm cannot read them, by path or by command, and they are
/// in no command's environment.
#[test]
fn a_run_cannot_see_the_banks_key_or_tokens() {
    let mut f = fixture();
    f.realm.config.policy.execute = Rule::Allow;
    f.realm.save().unwrap();
    let skills = f.realm.skills_dir();
    std::fs::create_dir_all(skills.join("mcp-auth")).unwrap();
    let key = crate::bank::key_path(&skills);
    std::fs::write(&key, "bank-key-value").unwrap();
    let tokens = skills.join("mcp-auth/revolut.json");
    std::fs::write(&tokens, r#"{"refresh_token": "bank-refresh-value"}"#).unwrap();
    let (key, tokens) = (key.canonicalize().unwrap(), tokens.canonicalize().unwrap());
    let (hub, seen, _) = make_hub(
        &f.home,
        vec![
            call("1", "read_file", json!({"path": key})),
            call("2", "read_file", json!({"path": tokens})),
            call(
                "3",
                "execute_command",
                json!({"command": format!("cat {} {}; env", key.display(), tokens.display())}),
            ),
            done("ok"),
        ],
    );
    let state = hub
        .dispatch(&f.realm, "snoop", config(10), hooks(&mut NoObserver, false))
        .unwrap();
    for event in &state.events[..2] {
        assert!(
            event.result.contains("outside the workspace"),
            "{}",
            event.result
        );
    }
    let env = f.realm.command_env().unwrap();
    assert!(
        !format!("{env:?}").contains("bank-"),
        "never in a command's environment"
    );
    // Commands are only stopped by the sandbox.
    if crate::sandbox::available() {
        assert!(
            !state.events[2].result.contains("bank-key-value"),
            "{}",
            state.events[2].result
        );
        assert!(
            !state.events[2].result.contains("bank-refresh-value"),
            "{}",
            state.events[2].result
        );
        assert!(!format!("{:?}", seen.lock().unwrap()).contains("bank-"));
    }
}
