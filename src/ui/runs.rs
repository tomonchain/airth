//! Chats and objectives: starting, following, answering and adopting runs.

use super::*;
use crate::supervise::Locked;

/// How long a page waits for a run to change before it asks again: under
/// the gap after which the founder counts as gone, as a closed page keeps
/// looking until its last wait ends.
const NEXT_CHANGE_WAIT: Duration = Duration::from_secs(8);

impl App {
    /// The chats and objectives routes; None when the request is not one of them.
    pub(super) fn runs_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        query: &str,
    ) -> Result<Option<HttpResponse>> {
        let ok = || json_response(200, json!({"ok": true}));
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "runs"]) => json_response(200, self.runs(&self.realm(r)?)?),
            (Method::Post, ["realms", r, "runs"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let choice = self.model_choice(&realm, &body)?;
                let mut state = self.hub.create_run_in(
                    &realm,
                    body["objective"].as_str().unwrap_or_default(),
                    body["repo"].as_str(),
                )?;
                if choice.is_some() {
                    state.model_choice = choice;
                    state.save()?;
                }
                let id = state.id.clone();
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": id}))
            }
            (Method::Get, ["realms", r, "runs", id]) => {
                json_response(200, self.run(&self.realm(r)?, id, query)?)
            }
            (Method::Get, ["realms", r, "runs", id, "live"]) => {
                json_response(200, self.next_change(&self.realm(r)?, id, query))
            }
            (Method::Patch, ["realms", r, "runs", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.hub.load_run(&realm, id)?;
                realm.set_run_title(id, body["title"].as_str().unwrap_or_default())?;
                json_response(200, json!({"title": realm.run_titles().get(*id)}))
            }
            (Method::Post, ["realms", r, "runs", id, "archive"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.ensure_idle(r, id)?;
                self.hub.load_run(&realm, id)?;
                let archived = body["archived"].as_bool().unwrap_or(true);
                realm.set_run_archived(id, archived)?;
                json_response(200, json!({"archived": archived}))
            }
            (Method::Delete, ["realms", r, "runs", id]) => {
                self.ensure_idle(r, id)?;
                self.hub.delete_run(&self.realm(r)?, id)?;
                ok()
            }
            (Method::Get, ["realms", r, "runs", id, "diff"]) => {
                let diff = self.hub.diff(&self.realm(r)?, id)?;
                json_response(200, json!({"diff": truncate_diff(diff)}))
            }
            (Method::Post, ["realms", r, "runs", id, "messages"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let choice = self.model_choice(&realm, &body)?;
                // At work: the agent reads it at its next turn.
                let message = body["message"].as_str().unwrap_or_default().trim();
                if let Some(live) = self.live(r, id)
                    && !message.is_empty()
                    && live.tell(message)
                {
                    return Ok(Some(json_response(200, json!({"queued": true}))));
                }
                self.ensure_idle(r, id)?;
                let mut state =
                    self.hub
                        .follow_up(&realm, id, body["message"].as_str().unwrap_or_default())?;
                // The picker always sends its value: "auto" goes back to the realm's routing.
                if body.get("model").is_some() {
                    state.model_choice = choice;
                    state.save()?;
                }
                self.spawn(realm, state, Job::Drive);
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "resume"]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                self.ensure_idle(r, id)?;
                let state = self.hub.resume(&realm, id)?;
                self.spawn(realm, state, Job::Drive);
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "approval"]) => {
                let body = read_json(request)?;
                self.answer(r, id, &body)?;
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "stop"]) => {
                let live = self.live(r, id).context("this run is not in progress")?;
                live.cancel.cancel();
                live.bump();
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "checks"]) => {
                let realm = self.realm(r)?;
                self.ensure_idle(r, id)?;
                let state = self.hub.load_run(&realm, id)?;
                self.spawn(realm, state, Job::Check);
                ok()
            }
            (Method::Post, ["realms", r, "runs", id, "adopt"]) => {
                self.ensure_idle(r, id)?;
                let realm = self.realm(r)?;
                match self.hub.prepare_adoption(&realm, id)? {
                    Adoption::Ready => {
                        self.hub.adopt(&realm, id)?;
                        json_response(200, json!({"adopted": true}))
                    }
                    Adoption::NeedsChecks => {
                        let state = self.hub.load_run(&realm, id)?;
                        self.spawn(realm, state, Job::CheckThenAdopt);
                        json_response(200, json!({"checking": true}))
                    }
                    Adoption::Conflict(files) => json_response(200, json!({"conflicts": files})),
                }
            }
            (Method::Post, ["realms", r, "runs", id, "resolve"]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                self.ensure_idle(r, id)?;
                match self.hub.resolve(&realm, id)? {
                    Some(state) => {
                        self.spawn(realm, state, Job::Drive);
                        json_response(200, json!({"resolving": true}))
                    }
                    None => {
                        let state = self.hub.load_run(&realm, id)?;
                        if !self.hub.repo_of(&realm, &state).checks.is_empty() {
                            self.spawn(realm, state, Job::Check);
                        }
                        json_response(200, json!({"resolving": false}))
                    }
                }
            }
            (Method::Get, ["realms", r, "runs", id, "pull-request"]) => {
                let status = self.hub.pull_request_status(&self.realm(r)?, id)?;
                json_response(200, json!({"status": status}))
            }
            (Method::Post, ["realms", r, "runs", id, "merge"]) => {
                self.ensure_idle(r, id)?;
                self.hub.merge_pull_request(&self.realm(r)?, id)?;
                json_response(200, json!({"merged": true}))
            }
            (Method::Post, ["realms", r, "runs", id, "pull-request"]) => {
                self.ensure_idle(r, id)?;
                let realm = self.realm(r)?;
                let mut state = self.hub.load_run(&realm, id)?;
                self.hub.deliver(&realm, &mut state, true)?;
                let pr = state.change.and_then(|c| c.pull_request);
                json_response(200, json!({"pull_request": pr}))
            }
            (Method::Post, ["realms", r, "runs", id, "discard"]) => {
                self.ensure_idle(r, id)?;
                self.hub.discard(&self.realm(r)?, id)?;
                ok()
            }
            _ => return Ok(None),
        }))
    }
}

impl App {
    fn runs(&self, realm: &Realm) -> Result<Value> {
        // Read the runs before taking the registry: while it is held, no
        // run can start or end, and no other page can list what is live.
        let saved = self.hub.runs(realm)?;
        let registry = self.registry.locked();
        let titles = realm.run_titles();
        let archived = realm.archived_runs();
        let runs: Vec<Value> = saved
            .into_iter()
            .rev()
            .map(|r| {
                let live = registry.get(&key(realm.name(), &r.id));
                let needs_approval = live.is_some_and(|l| l.pending.locked().is_some());
                json!({
                    "id": r.id,
                    "title": titles.get(&r.id),
                    "archived": archived.contains(&r.id),
                    "objective": r.objective,
                    "status": r.display_status(),
                    "live": r.live || live.is_some(),
                    "iterations": r.iterations,
                    "updated_at": r.updated_at,
                    "needs_approval": needs_approval,
                    "change": r.change.map(|c| c.as_str()),
                    "checks_passed": r.checks_passed,
                    "touches": r.touches,
                    "conflicts": r.conflicts,
                    "pull_request": r.pull_request,
                    "plan_done": r.plan_done,
                    "plan_total": r.plan_total,
                    "routine": r.routine,
                    "asked_by": r.asked_by,
                })
            })
            .collect();
        Ok(json!(runs))
    }

    /// Waits for a run driven here to change from what the page has: its
    /// state (`version`) or the words the model is writing (`draft`). The
    /// page asks again at once, so the founder reads the answer as it comes.
    pub(super) fn next_change(&self, realm: &Realm, id: &str, query: &str) -> Value {
        let number = |name: &str| {
            query
                .split('&')
                .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
                .and_then(|v| v.parse::<u64>().ok())
        };
        let Some(live) = self.live(realm.name(), id) else {
            return json!({"here": false});
        };
        let started = std::time::Instant::now();
        loop {
            // Waiting counts as looking: the founder is on the run's page.
            live.watch.seen();
            let version = live.version.load(Ordering::SeqCst);
            let seq = live.draft.seq.load(Ordering::SeqCst);
            if number("version") != Some(version)
                || number("draft") != Some(seq)
                || started.elapsed() >= NEXT_CHANGE_WAIT
            {
                let draft = live.draft.text.locked().clone();
                return json!({"here": true, "version": version, "draft_seq": seq, "draft": draft});
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    fn run(&self, realm: &Realm, id: &str, query: &str) -> Result<Value> {
        let since: Option<u64> = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("since="))
            .and_then(|v| v.parse().ok());
        // How the change reaches its repository decides the review's main
        // button: open a pull request, or adopt into the checkout.
        let repo = |state: &State| {
            let r = self.hub.repo_of(realm, state);
            (
                !r.checks.is_empty(),
                state.change.as_ref().map(|_| r.name),
                r.delivery.as_str(),
            )
        };
        if let Some(live) = self.live(realm.name(), id) {
            live.watch.seen();
            let version = live.version.load(Ordering::SeqCst);
            if since == Some(version) {
                return Ok(json!({"unchanged": true, "version": version}));
            }
            let state = live.snapshot.locked().clone();
            let (checks_required, repo_name, delivery) = repo(&state);
            let pending = live.pending.locked().as_ref().map(|p| {
                json!({
                    "id": p.id,
                    "tool": p.request.tool,
                    "capability": p.request.capability.as_str(),
                    "preview": p.request.preview,
                    "warnings": p.request.warnings,
                    "grantable": p.request.grantable(),
                    "grant_label": p.request.grant_label(),
                })
            });
            return Ok(json!({
                "version": version,
                "live": true,
                "stopping": live.cancel.is_cancelled(),
                "state": state,
                "pending": pending,
                "checks_required": checks_required,
                "config": config_of(&state),
                "repo": repo_name,
                "delivery": delivery,
                "title": realm.run_titles().get(id),
            }));
        }
        let state = self.hub.load_run(realm, id)?;
        let (checks_required, repo_name, delivery) = repo(&state);
        let live_elsewhere = self.hub.is_live(realm, id);
        Ok(json!({
            "version": 0,
            "live": live_elsewhere,
            "elsewhere": live_elsewhere,
            "interrupted": state.status == Status::Running && !live_elsewhere,
            "state": state,
            "pending": null,
            "checks_required": checks_required,
                "config": config_of(&state),
            "repo": repo_name,
            "delivery": delivery,
            "title": realm.run_titles().get(id),
        }))
    }

    pub(super) fn answer(&self, realm: &str, id: &str, body: &Value) -> Result<()> {
        let live = self
            .live(realm, id)
            .context("this run is not in progress")?;
        let mut pending = live.pending.locked();
        let p = pending
            .take_if(|p| Some(p.id) == body["approval_id"].as_u64())
            .context("this approval is no longer pending")?;
        let answer = if body["approve"].as_bool() == Some(true) {
            if body["for_run"].as_bool() == Some(true) && p.request.grantable() {
                Answer::YesForRun
            } else {
                Answer::Yes
            }
        } else {
            match body["reason"].as_str().map(str::trim) {
                Some(r) if !r.is_empty() => Answer::No(format!("denied by user: {r}")),
                _ => Answer::No("denied by user".into()),
            }
        };
        p.reply
            .send(answer)
            .map_err(|_| anyhow!("the run is no longer waiting"))?;
        live.bump();
        let entry = time::Entry {
            at: chrono::Utc::now(),
            activity: time::ANSWER.into(),
            seconds: time::answer_seconds(p.asked.elapsed()),
            note: format!("{} in run {id}", p.request.tool),
        };
        // The answer is given either way; a ledger that cannot be written is
        // reported, not a reason to keep the run waiting.
        if let Err(e) = self
            .realm(realm)
            .and_then(|r| Store::of(&r))
            .and_then(|mut s| s.record_time(&entry))
        {
            eprintln!("could not record the answer's time in {realm}: {e:#}");
        }
        Ok(())
    }

    pub(super) fn spawn(&self, realm: Realm, state: State, job: Job) {
        self.runner.spawn(realm, state, job);
    }

    /// `POST /hooks/<realm>/<objective>/<token>`: start the objective's run
    /// with the request body as its (untrusted) data. Anything that does not
    /// match an enabled webhook objective gets the same 404.
    pub(super) fn webhook(&self, request: &mut Request, hook: &str) -> HttpResponse {
        let not_found = || json_response(404, json!({"error": "not found"}));
        let parts: Vec<String> = hook.split('/').map(percent_decode).collect();
        let [realm, id, token] = parts.as_slice() else {
            return not_found();
        };
        let Ok(realm) = self.realm(realm) else {
            return not_found();
        };
        let Ok(objective) = realm.objectives().get(id) else {
            return not_found();
        };
        let valid = matches!(&objective.trigger, Trigger::Webhook { token: t } if constant_time_eq(t, token));
        if !valid || !objective.enabled || realm.config.archived {
            return not_found();
        }
        let mut body = Vec::new();
        let _ = request.as_reader().take(64_000).read_to_end(&mut body);
        let payload = String::from_utf8_lossy(&body).into_owned();
        match self
            .hub
            .start_objective(&realm, id, "webhook", Some(&payload), chrono::Utc::now())
        {
            Ok((state, o)) => {
                let run = state.id.clone();
                self.spawn(
                    realm,
                    state,
                    Job::Unattended {
                        objective: o.id,
                        wait_minutes: o.approval_wait_minutes,
                    },
                );
                json_response(202, json!({"run": run}))
            }
            Err(e) => json_response(409, json!({"error": format!("{e:#}")})),
        }
    }
}

/// Keep huge diffs (lockfiles, generated code) from freezing the page.
fn truncate_diff(diff: String) -> String {
    const MAX: usize = 400_000;
    if diff.len() <= MAX {
        return diff;
    }
    let mut cut = MAX;
    while !diff.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n… diff truncated ({} bytes total); see `airth review`",
        &diff[..cut],
        diff.len()
    )
}

/// The business config a run's change touches, for the review (decision 44).
fn config_of(state: &State) -> crate::business::ConfigChanges {
    crate::business::config_changes(
        state
            .change
            .as_ref()
            .map_or(&[][..], |c| c.files.as_slice()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task's view says how its change would reach the repository, so
    /// the review offers the right button: adopt, or open a pull request.
    #[test]
    fn a_run_says_how_its_change_is_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "r".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let mut state = State::new("fix the header", Some(&realm.runs_dir()));
        state.status = Status::Completed;
        state.save().unwrap();
        let app = super::super::tests::test_app(home, 0);
        let view = app.run(&realm, &state.id, "").unwrap();
        assert_eq!(view["delivery"], "adopt");
        assert_eq!(view["repo"], Value::Null, "no change, no repository named");
    }
}
