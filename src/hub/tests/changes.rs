//! Checks, adoption, repositories and pull requests.

use super::*;

/// v0.1 acceptance, minus the model: the change happens in a worktree, is
/// committed and checked, and reaches the owner's checkout only on adoption.
#[test]
fn isolated_runs_are_checked_then_adopted() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let repo = crate::isolation::tests::repo();
    let mut realm = home
        .create_realm(NewRealm {
            name: "code".into(),
            description: String::new(),
            root: Some(repo.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    assert_eq!(realm.config.isolation, Isolation::Worktree);
    realm.config.checks = vec!["grep -q improved a.txt".into()];
    realm.save().unwrap();

    let (hub, _, systems) = make_hub(
        &home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "a.txt", "content": "improved\n"}),
            ),
            // Writing the owner's checkout from a command is stopped by the sandbox.
            call(
                "2",
                "execute_command",
                json!({"command": if crate::sandbox::available() {
                    format!("echo x > {}/a.txt", repo.path().display())
                } else {
                    "true".into()
                }}),
            ),
            done("Improved a.txt."),
        ],
    );
    let state = hub
        .dispatch(&realm, "improve", config(10), hooks(&mut NoObserver, true))
        .unwrap();
    assert!(systems.lock().unwrap()[0].contains("isolated git worktree"));
    assert_eq!(state.status, Status::Completed);
    if crate::sandbox::available() {
        assert!(
            state.events[1].result.contains("Operation not permitted"),
            "{}",
            state.events[1].result
        );
    }
    let change = state.change.clone().unwrap();
    assert_eq!(change.files, vec!["M\ta.txt"]);
    assert!(change.checks_passed(), "{:?}", change.checks);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(hub.diff(&realm, &state.id).unwrap().contains("+improved"));
    let summary = &hub.runs(&realm).unwrap()[0];
    assert_eq!(summary.checks_passed, Some(true));

    std::process::Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["config", "user.email", "t@t"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["config", "user.name", "t"])
        .status()
        .unwrap();
    let adopted = hub.adopt(&realm, &state.id).unwrap();
    assert_eq!(adopted.change.unwrap().status, ChangeStatus::Adopted);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "improved\n"
    );
    assert!(hub.follow_up(&realm, &state.id, "more").is_err());
}

fn git_in(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A GitHub API that knows one pull request (#7), as `pr` describes it:
/// merged, head (its latest commit), checks (a check run's conclusion,
/// or "in_progress") and merge_state. Merging it sets `merged`.
fn fake_github(pr: Arc<Mutex<serde_json::Value>>) -> (String, Arc<Mutex<Vec<String>>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);
            let auth = req.headers().iter().any(|h| {
                h.field.equiv("Authorization") && h.value.as_str() == "Bearer gh-test-token"
            });
            seen.lock()
                .unwrap()
                .push(format!("{} {} {body}", req.method(), req.url()));
            let mut pr = pr.lock().unwrap();
            let merged = pr["merged"].as_bool().unwrap_or(false);
            let head = pr["head"].as_str().unwrap_or_default().to_string();
            let url = req.url().to_string();
            let (code, answer) = match (req.method().as_str(), url.as_str()) {
                _ if !auth => (401, json!({"message": "Bad credentials"})),
                ("POST", "/repos/o/r/pulls") => (
                    201,
                    json!({"number": 7, "html_url": "https://github.com/o/r/pull/7"}),
                ),
                ("GET", "/repos/o/r/pulls/7") => (
                    200,
                    json!({
                        "state": if merged { "closed" } else { "open" },
                        "merged": merged,
                        "title": "Add c.txt",
                        "html_url": "https://github.com/o/r/pull/7",
                        "head": {"sha": head},
                        "mergeable_state": pr["merge_state"].as_str().unwrap_or("clean"),
                    }),
                ),
                ("GET", u) if u.starts_with("/repos/o/r/commits/") => {
                    let checks = pr["checks"].as_str().unwrap_or("success");
                    let run = if checks == "in_progress" {
                        json!({"name": "check", "status": "in_progress", "conclusion": null})
                    } else {
                        json!({"name": "check", "status": "completed", "conclusion": checks})
                    };
                    (200, json!({"total_count": 1, "check_runs": [run]}))
                }
                ("PUT", "/repos/o/r/pulls/7/merge") => {
                    pr["merged"] = json!(true);
                    (200, json!({"merged": true}))
                }
                _ => (200, json!({})),
            };
            let _ = req.respond(
                tiny_http::Response::from_string(answer.to_string()).with_status_code(code),
            );
        }
    });
    (format!("http://127.0.0.1:{port}"), calls)
}

/// A realm that delivers pull requests: changes are pushed and opened as
/// pull requests once checked, kept current with the base, their
/// conflicts resolved by the chat, and recorded merged.
#[test]
fn pull_requests_are_opened_followed_and_merged() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let repo = crate::isolation::tests::repo();
    // GitHub's copy of the repository: a bare repo in a folder of remotes.
    let remotes = tempfile::tempdir().unwrap();
    let bare = remotes.path().join("o/r.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    git_in(
        remotes.path(),
        &[
            "clone",
            "-q",
            "--bare",
            repo.path().to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    git_in(
        repo.path(),
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    let fake = Arc::new(Mutex::new(json!({"merged": false})));
    let (api, calls) = fake_github(fake.clone());
    *TEST_GITHUB.lock().unwrap() = Some((api, format!("file://{}", remotes.path().display())));

    let mut realm = home
        .create_realm(NewRealm {
            name: "code".into(),
            description: String::new(),
            root: Some(repo.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    realm.config.checks = vec!["test -f b.txt".into()];
    realm.config.delivery = Delivery::PullRequest;
    realm.save().unwrap();
    realm
        .set_secret(crate::github::TOKEN, "gh-test-token")
        .unwrap();

    let (hub, _, _) = make_hub(
        &home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "b.txt", "content": "mine\n"}),
            ),
            call(
                "2",
                "execute_command",
                json!({"command": "echo token=${GITHUB_TOKEN:-none}"}),
            ),
            done("Added b.txt."),
            // The pull request's title.
            done("\"Add b.txt to the shop.\""),
        ],
    );
    let state = hub
        .dispatch(&realm, "add b", config(10), hooks(&mut NoObserver, true))
        .unwrap();
    // Chats never get the token the runtime pushes with.
    assert!(
        state.events[1].result.contains("token=none"),
        "{}",
        state.events[1].result
    );
    let change = state.change.clone().unwrap();
    let pr = change.pull_request.clone().expect("a pull request");
    assert_eq!(pr.number, 7);
    assert_eq!(pr.pushed, change.head);
    let branch_on_github = |b: &str| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&bare)
            .args(["rev-parse", &format!("refs/heads/{b}")])
            .output()
            .unwrap()
    };
    assert!(branch_on_github(&change.branch).status.success());
    let opened = calls
        .lock()
        .unwrap()
        .iter()
        .find(|c| c.starts_with("POST"))
        .cloned()
        .unwrap();
    assert!(
        opened.contains("\"title\":\"Add b.txt to the shop\"") && opened.contains("Added b.txt."),
        "{opened}"
    );
    assert!(
        hub.adopt(&realm, &state.id).is_err(),
        "merged on GitHub, not here"
    );

    // Nothing new on GitHub: nothing to do.
    assert_eq!(
        hub.follow_pull_request(&realm, &state.id).unwrap(),
        PrEvent::Nothing
    );

    // Someone else's work lands on main: the branch takes it in and is pushed.
    let other = tempfile::tempdir().unwrap();
    git_in(other.path(), &["clone", "-q", bare.to_str().unwrap(), "."]);
    std::fs::write(other.path().join("c.txt"), "theirs\n").unwrap();
    git_in(other.path(), &["add", "."]);
    git_in(other.path(), &["commit", "-q", "-m", "c"]);
    git_in(other.path(), &["push", "-q", "origin", "HEAD:main"]);
    assert_eq!(
        hub.follow_pull_request(&realm, &state.id).unwrap(),
        PrEvent::Updated
    );
    let state = hub.load_run(&realm, &state.id).unwrap();
    let change = state.change.clone().unwrap();
    assert!(change.path.join("c.txt").exists());
    assert_eq!(change.files, vec!["A\tb.txt"]);
    let pushed = String::from_utf8(branch_on_github(&change.branch).stdout).unwrap();
    assert_eq!(pushed.trim(), change.head);

    // Then main gets a b.txt of its own: a conflict, which the chat resolves.
    std::fs::write(other.path().join("b.txt"), "theirs\n").unwrap();
    git_in(other.path(), &["add", "."]);
    git_in(other.path(), &["commit", "-q", "-m", "b"]);
    git_in(other.path(), &["push", "-q", "origin", "HEAD:main"]);
    assert_eq!(
        hub.follow_pull_request(&realm, &state.id).unwrap(),
        PrEvent::Conflict
    );
    let inbox = home.inbox().list().unwrap();
    assert!(
        inbox.iter().any(|n| n.title.contains("#7 conflicts")),
        "{inbox:?}"
    );
    let mut state = hub.resolve(&realm, &state.id).unwrap().expect("a conflict");
    assert!(format!("{:?}", state.messages.last()).contains("b.txt"));
    let (hub, _, _) = make_hub(
        &home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "b.txt", "content": "mine and theirs\n"}),
            ),
            done("Kept both."),
        ],
    );
    hub.drive(&realm, &mut state, config(10), hooks(&mut NoObserver, true))
        .unwrap();
    let change = state.change.clone().unwrap();
    assert!(
        change.conflicts.is_empty() && change.merging.is_none(),
        "{change:?}"
    );
    assert!(change.checks_passed());
    assert_eq!(change.pull_request.as_ref().unwrap().pushed, change.head);
    let pushed = String::from_utf8(branch_on_github(&change.branch).stdout).unwrap();
    assert_eq!(pushed.trim(), change.head);

    // Merged on GitHub: the chat's change is adopted and its copy removed.
    fake.lock().unwrap()["merged"] = json!(true);
    assert_eq!(
        hub.follow_pull_request(&realm, &state.id).unwrap(),
        PrEvent::Merged
    );
    let state = hub.load_run(&realm, &state.id).unwrap();
    let change = state.change.unwrap();
    assert_eq!(change.status, ChangeStatus::Adopted);
    assert!(!change.path.exists());
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("DELETE /repos/o/r/git/refs/heads/airth/"))
    );

    // Another chat, merged from Airth once GitHub's checks pass on what it pushed.
    *fake.lock().unwrap() = json!({"merged": false, "checks": "in_progress"});
    let (hub, _, _) = make_hub(
        &home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "d.txt", "content": "d\n"}),
            ),
            done("Added d.txt."),
            done("Add d.txt"),
        ],
    );
    let state = hub
        .dispatch(&realm, "add d", config(10), hooks(&mut NoObserver, true))
        .unwrap();
    let head = state.change.as_ref().unwrap().head.clone();
    let refused = |hub: &Hub| {
        hub.merge_pull_request(&realm, &state.id)
            .unwrap_err()
            .to_string()
    };
    assert!(refused(&hub).contains("not received"));
    fake.lock().unwrap()["head"] = json!(head);
    assert!(refused(&hub).contains("still running"));
    fake.lock().unwrap()["checks"] = json!("failure");
    assert!(refused(&hub).contains("failed"));
    let status = hub.pull_request_status(&realm, &state.id).unwrap().unwrap();
    assert_eq!(
        (status.checks, status.failed),
        (Checks::Failing, vec!["check".to_string()])
    );
    fake.lock().unwrap()["checks"] = json!("success");
    fake.lock().unwrap()["merge_state"] = json!("behind");
    assert!(refused(&hub).contains("moved on"));
    fake.lock().unwrap()["merge_state"] = json!("clean");
    let merged = hub.merge_pull_request(&realm, &state.id).unwrap();
    let change = merged.change.unwrap();
    assert_eq!(change.status, ChangeStatus::Adopted);
    assert!(!change.path.exists());
    let put = calls
        .lock()
        .unwrap()
        .iter()
        .find(|c| c.starts_with("PUT"))
        .cloned()
        .unwrap();
    assert!(
        put.contains("\"squash\"") && put.contains("Add c.txt (#7)") && put.contains(&head),
        "{put}"
    );
    *TEST_GITHUB.lock().unwrap() = None;
}

/// Adopting a change that missed new commits on its base updates it,
/// checks it again, and the realm's other open changes follow.
#[test]
fn adoption_updates_the_change_and_the_others() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let repo = crate::isolation::tests::repo();
    git_in(repo.path(), &["config", "user.name", "t"]);
    git_in(repo.path(), &["config", "user.email", "t@t"]);
    let mut realm = home
        .create_realm(NewRealm {
            name: "code".into(),
            description: String::new(),
            root: Some(repo.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    realm.config.checks = vec!["true".into()];
    realm.save().unwrap();
    let chat = |file: &str, text: &str| {
        let (hub, _, _) = make_hub(
            &home,
            vec![
                call("1", "write_file", json!({"path": file, "content": text})),
                done("Done."),
            ],
        );
        hub.dispatch(
            &realm,
            &format!("write {file}"),
            config(10),
            hooks(&mut NoObserver, true),
        )
        .unwrap()
    };
    let first = chat("b.txt", "first\n");
    let second = chat("c.txt", "second\n");
    let third = chat("b.txt", "third\n");
    let (hub, _, _) = make_hub(&home, vec![]);

    assert_eq!(
        hub.prepare_adoption(&realm, &first.id).unwrap(),
        Adoption::Ready
    );
    hub.adopt(&realm, &first.id).unwrap();
    // The others were updated with it: the second merged cleanly, the
    // third conflicts on b.txt.
    let second = hub.load_run(&realm, &second.id).unwrap();
    assert!(second.change.as_ref().unwrap().path.join("b.txt").exists());
    let third = hub.load_run(&realm, &third.id).unwrap();
    assert_eq!(third.change.as_ref().unwrap().conflicts, vec!["b.txt"]);
    assert!(hub.runs(&realm).unwrap().iter().any(|r| r.conflicts));

    // The second's checks ran on its old head: they run again first.
    assert_eq!(
        hub.prepare_adoption(&realm, &second.id).unwrap(),
        Adoption::NeedsChecks
    );
    hub.check(&realm, &second.id, &mut NoObserver, &CancelToken::default())
        .unwrap();
    assert_eq!(
        hub.prepare_adoption(&realm, &second.id).unwrap(),
        Adoption::Ready
    );
    hub.adopt(&realm, &second.id).unwrap();
    assert!(repo.path().join("c.txt").exists());
    assert_eq!(
        hub.prepare_adoption(&realm, &third.id).unwrap(),
        Adoption::Conflict(vec!["b.txt".into()])
    );
}

/// A realm whose folder is not a repository works in repositories added
/// to it: cloned or existing, each with its own checks, in isolation.
#[test]
fn chats_work_in_the_realms_repositories() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let mut realm = home
        .create_realm(NewRealm {
            name: "company".into(),
            description: String::new(),
            root: None,
            isolation: None,
        })
        .unwrap();
    let (hub, _, _) = make_hub(&home, vec![]);
    // One cloned from a URL, one already on this machine.
    let upstream = crate::isolation::tests::repo();
    let place = tempfile::tempdir().unwrap();
    let url = format!("file://{}", upstream.path().display());
    let lib = hub
        .connect_repo(&mut realm, &url, None, Some(&place.path().join("lib")))
        .unwrap();
    assert_eq!(lib.url.as_deref(), Some(url.as_str()));
    assert!(lib.path.join("a.txt").exists());
    let site = crate::isolation::tests::repo();
    hub.connect_repo(
        &mut realm,
        site.path().to_str().unwrap(),
        Some("site"),
        None,
    )
    .unwrap();
    assert!(
        hub.connect_repo(
            &mut realm,
            &url,
            Some("again"),
            Some(&place.path().join("lib"))
        )
        .is_err(),
        "a clone never lands in a folder that has files"
    );
    assert!(
        hub.connect_repo(
            &mut realm,
            home_dir.path().to_str().unwrap(),
            Some("h"),
            None
        )
        .is_err(),
        "nor in Airth's home"
    );
    let name = lib.name.clone();
    realm
        .update_repo(&name, Some(vec!["test -f new.txt".into()]), None)
        .unwrap();

    let (hub, _, _) = make_hub(
        &home,
        vec![
            call(
                "1",
                "write_file",
                json!({"path": "new.txt", "content": "x\n"}),
            ),
            done("Added new.txt."),
        ],
    );
    let mut state = hub
        .create_run_in(&realm, "add new.txt", Some(&name))
        .unwrap();
    hub.drive(&realm, &mut state, config(10), hooks(&mut NoObserver, true))
        .unwrap();
    let change = state.change.clone().unwrap();
    assert_eq!(change.files, vec!["A\tnew.txt"]);
    assert!(change.checks_passed(), "{:?}", change.checks);
    assert!(!lib.path.join("new.txt").exists(), "isolated until adopted");
    assert_eq!(hub.repo_of(&realm, &state).name, name);
    git_in(&lib.path, &["config", "user.name", "t"]);
    git_in(&lib.path, &["config", "user.email", "t@t"]);
    assert_eq!(
        hub.prepare_adoption(&realm, &state.id).unwrap(),
        Adoption::Ready
    );
    hub.adopt(&realm, &state.id).unwrap();
    assert!(lib.path.join("new.txt").exists());
    assert!(!site.path().join("new.txt").exists());

    // Other realms' chats cannot reach these repositories.
    let other = home
        .create_realm(NewRealm {
            name: "personal".into(),
            description: String::new(),
            root: None,
            isolation: None,
        })
        .unwrap();
    let denied = hub.denied_to(&other).unwrap();
    assert!(
        denied.contains(&lib.path)
            && denied
                .iter()
                .any(|p| p.ends_with(site.path().file_name().unwrap()))
    );
    assert!(hub.create_run_in(&realm, "x", Some("nope")).is_err());
}

#[test]
fn failed_checks_block_adoption() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let repo = crate::isolation::tests::repo();
    let mut realm = home
        .create_realm(NewRealm {
            name: "code".into(),
            description: String::new(),
            root: Some(repo.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    realm.config.checks = vec!["false".into(), "echo never".into()];
    realm.save().unwrap();
    let (hub, _, _) = make_hub(
        &home,
        vec![
            call("1", "write_file", json!({"path": "b.txt", "content": "x"})),
            done("Done."),
        ],
    );
    let state = hub
        .dispatch(&realm, "add b", config(10), hooks(&mut NoObserver, true))
        .unwrap();
    let checks = state.change.as_ref().unwrap().checks.clone().unwrap();
    assert!(checks.finished);
    assert_eq!(checks.results.len(), 1, "stops at the first failure");
    assert!(!checks.results[0].ok);
    assert!(hub.adopt(&realm, &state.id).is_err());
    hub.discard(&realm, &state.id).unwrap();
    assert!(!repo.path().join("b.txt").exists());
}
