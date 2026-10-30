//! Chats and objectives: starting, following, answering and adopting runs.

use super::*;

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
            (Method::Get, ["realms", r, "objectives"]) => {
                json_response(200, self.objectives(&self.realm(r)?)?)
            }
            (Method::Post, ["realms", r, "objectives"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                self.save_objective(&realm, &body)?;
                json_response(200, self.objectives(&realm)?)
            }
            (Method::Patch, ["realms", r, "objectives", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                if let Some(enabled) = body["enabled"].as_bool() {
                    let found = realm.objectives().update(id, |o| {
                        o.enabled = enabled;
                        // Re-arm: a schedule missed while disabled does not fire.
                        o.next_at = None;
                        o.fingerprint = None;
                    })?;
                    if !found {
                        return Err(anyhow!("no such objective"));
                    }
                }
                json_response(200, self.objectives(&realm)?)
            }
            (Method::Delete, ["realms", r, "objectives", id]) => {
                let realm = self.realm(r)?;
                if !realm.objectives().remove(id)? {
                    return Err(anyhow!("no such objective"));
                }
                json_response(200, self.objectives(&realm)?)
            }
            (Method::Post, ["realms", r, "objectives", id, "run"]) => {
                let realm = self.realm(r)?;
                self.hub.model_for(&realm).context("no model available")?;
                let (state, _) = self.hub.start_objective(
                    &realm,
                    id,
                    "started by the owner",
                    None,
                    chrono::Utc::now(),
                )?;
                let run = state.id.clone();
                // The owner is here: they answer approvals as for any run.
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": run}))
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
        let registry = self.registry.lock().unwrap();
        let titles = realm.run_titles();
        let archived = realm.archived_runs();
        let runs: Vec<Value> = self
            .hub
            .runs(realm)?
            .into_iter()
            .rev()
            .map(|r| {
                let live = registry.get(&key(realm.name(), &r.id));
                let needs_approval = live.is_some_and(|l| l.pending.lock().unwrap().is_some());
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
                })
            })
            .collect();
        Ok(json!(runs))
    }

    fn run(&self, realm: &Realm, id: &str, query: &str) -> Result<Value> {
        let since: Option<u64> = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("since="))
            .and_then(|v| v.parse().ok());
        let repo = |state: &State| {
            let r = self.hub.repo_of(realm, state);
            (!r.checks.is_empty(), state.change.as_ref().map(|_| r.name))
        };
        if let Some(live) = self.live(realm.name(), id) {
            let version = live.version.load(Ordering::SeqCst);
            if since == Some(version) {
                return Ok(json!({"unchanged": true, "version": version}));
            }
            let state = live.snapshot.lock().unwrap().clone();
            let (checks_required, repo_name) = repo(&state);
            let pending = live.pending.lock().unwrap().as_ref().map(|p| {
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
                "repo": repo_name,
                "title": realm.run_titles().get(id),
            }));
        }
        let state = self.hub.load_run(realm, id)?;
        let (checks_required, repo_name) = repo(&state);
        let live_elsewhere = self.hub.is_live(realm, id);
        Ok(json!({
            "version": 0,
            "live": live_elsewhere,
            "elsewhere": live_elsewhere,
            "interrupted": state.status == Status::Running && !live_elsewhere,
            "state": state,
            "pending": null,
            "checks_required": checks_required,
            "repo": repo_name,
            "title": realm.run_titles().get(id),
        }))
    }

    pub(super) fn answer(&self, realm: &str, id: &str, body: &Value) -> Result<()> {
        let live = self
            .live(realm, id)
            .context("this run is not in progress")?;
        let mut pending = live.pending.lock().unwrap();
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

    fn objectives(&self, realm: &Realm) -> Result<Value> {
        let list: Vec<Value> = realm
            .objectives()
            .list()?
            .into_iter()
            .map(|o| {
                let mut v = json!(o);
                if let Trigger::Webhook { token } = &o.trigger {
                    v["webhook_url"] = json!(format!(
                        "http://127.0.0.1:{}/hooks/{}/{}/{token}",
                        self.port,
                        realm.name(),
                        o.id
                    ));
                }
                v["trigger_label"] = json!(o.trigger.describe());
                v["running"] =
                    json!(o.last_run.as_deref().is_some_and(
                        |r| self.live(realm.name(), r).is_some() || self.hub.is_live(realm, r)
                    ));
                v
            })
            .collect();
        Ok(json!(list))
    }

    /// Create or replace an objective from the app's form. Its history (last
    /// run, schedule state, webhook token) is kept when it is replaced.
    fn save_objective(&self, realm: &Realm, body: &Value) -> Result<()> {
        let id = body["id"].as_str().unwrap_or_default().trim().to_string();
        let existing = realm.objectives().get(&id).ok();
        let trigger = match body["trigger"]["kind"].as_str().unwrap_or("manual") {
            "manual" => Trigger::Manual,
            "schedule" => Trigger::Schedule {
                every: body["trigger"]["every"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .into(),
            },
            "watch" => Trigger::Watch {
                path: body["trigger"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .into(),
            },
            "webhook" => Trigger::Webhook {
                token: match existing.as_ref().map(|o| &o.trigger) {
                    Some(Trigger::Webhook { token }) => token.clone(),
                    _ => random_token()?,
                },
            },
            other => return Err(anyhow!("unknown trigger '{other}'")),
        };
        let approvals = serde_json::from_value(body["approvals"].clone()).unwrap_or_default();
        let budget = serde_json::from_value(body["budget"].clone()).unwrap_or_default();
        let base = existing.filter(|o| o.trigger == trigger);
        realm.objectives().save(Objective {
            id,
            objective: body["objective"].as_str().unwrap_or_default().trim().into(),
            trigger,
            approvals,
            budget,
            approval_wait_minutes: body["approval_wait_minutes"]
                .as_u64()
                .unwrap_or(10)
                .min(24 * 60) as u32,
            enabled: body["enabled"].as_bool().unwrap_or(true),
            created_at: base
                .as_ref()
                .map_or_else(chrono::Utc::now, |o| o.created_at),
            repo: body["repo"]
                .as_str()
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(String::from),
            next_at: None,
            fingerprint: None,
            last_run: base.as_ref().and_then(|o| o.last_run.clone()),
            last_at: base.as_ref().and_then(|o| o.last_at),
            last_status: base.and_then(|o| o.last_status),
        })
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
