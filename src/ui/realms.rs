//! A realm's routes: its settings, files, memory, secrets, models and
//! repositories.

use super::*;

/// Let a bot go, unless a routine still names it: that routine would fail.
pub(super) fn let_go(realm: &Realm, name: &str) -> Result<()> {
    let routines: Vec<String> = realm
        .objectives()
        .list()?
        .into_iter()
        .filter(|o| o.bot.as_deref() == Some(name))
        .map(|o| o.id)
        .collect();
    if !routines.is_empty() {
        bail!(
            "{name} still does {}: remove those routines first",
            routines.join(", ")
        );
    }
    crate::business::remove_job(&realm.root()?, name)
}

/// Create or replace a routine from the app's form. Its history (last run,
/// schedule state) is kept when it is replaced with the same trigger.
pub(super) fn save_routine(realm: &Realm, body: &Value) -> Result<()> {
    use crate::objective::{Objective, Trigger};
    let id = body["id"].as_str().unwrap_or_default().trim().to_string();
    let existing = realm.objectives().get(&id).ok();
    let trigger = match body["every"]
        .as_str()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        Some(every) => Trigger::Schedule {
            every: every.into(),
        },
        None => Trigger::Manual,
    };
    let bot = body["bot"]
        .as_str()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(String::from);
    if let Some(bot) = &bot {
        crate::business::job(&realm.root()?, bot)?;
    }
    let tools = match body["tools"].as_array() {
        Some(list) => {
            let names: Vec<String> = list
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            let known = crate::tools::extension_names(realm);
            if let Some(unknown) = names.iter().find(|n| !known.contains(n)) {
                bail!("no integration, MCP server or skill '{unknown}' in this business");
            }
            Some(names)
        }
        None => None,
    };
    let approvals = body["allow"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|a| crate::objective::parse_grant(realm, a))
        .collect::<Result<Vec<_>>>()?;
    let delegates: Vec<String> = body["delegates"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let routines = realm.objectives().list()?;
    if let Some(unknown) = delegates
        .iter()
        .find(|d| !routines.iter().any(|o| o.id == **d))
    {
        bail!("no routine {unknown} in this business");
    }
    let minutes = body["max_minutes"].as_u64().filter(|m| *m > 0);
    let base = existing.filter(|o| o.trigger == trigger);
    realm.objectives().save(Objective {
        id,
        objective: body["objective"].as_str().unwrap_or_default().trim().into(),
        trigger,
        approvals,
        budget: crate::agent::Budget {
            max_minutes: minutes.map(|m| m.min(24 * 60) as u32),
            ..Default::default()
        },
        approval_wait_minutes: body["wait"].as_u64().unwrap_or(10).min(24 * 60) as u32,
        enabled: true,
        created_at: base
            .as_ref()
            .map_or_else(chrono::Utc::now, |o| o.created_at),
        repo: base.as_ref().and_then(|o| o.repo.clone()),
        bot,
        tools,
        delegates,
        next_at: base.as_ref().and_then(|o| o.next_at),
        fingerprint: None,
        last_run: base.as_ref().and_then(|o| o.last_run.clone()),
        last_at: base.as_ref().and_then(|o| o.last_at),
        last_status: base.and_then(|o| o.last_status),
    })
}

impl App {
    /// The a realm's own routes; None when the request is not one of them.
    pub(super) fn realms_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        query: &str,
    ) -> Result<Option<HttpResponse>> {
        let ok = || json_response(200, json!({"ok": true}));
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "image"]) => image_response(self.realm(r)?.dir(), "image"),
            (Method::Put, ["realms", r, "image"]) => {
                let realm = self.realm(r)?;
                images::save(realm.dir(), "image", &read_bytes(request)?)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "image"]) => {
                let realm = self.realm(r)?;
                images::remove(realm.dir(), "image")?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms"]) => json_response(200, self.realms()?),
            (Method::Post, ["realms"]) => {
                let body = read_json(request)?;
                let realm = self.create_realm(&body)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r]) => json_response(200, self.realm_detail(&self.realm(r)?)?),
            (Method::Patch, ["realms", r]) => {
                let body = read_json(request)?;
                let realm = self.update_realm(r, &body)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r, "files"]) => {
                json_response(200, self.files(&self.realm(r)?, &param(query, "path"))?)
            }
            (Method::Get, ["realms", r, "files", "raw"]) => {
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, &param(query, "path"))?;
                let size = std::fs::metadata(&path)?.len();
                if !path.is_file() || size > crate::documents::MAX_FILE as u64 {
                    return Ok(Some(json_response(
                        413,
                        json!({"error": "too large to preview"}),
                    )));
                }
                Response::from_data(std::fs::read(&path)?)
                    .with_header(content_type(crate::documents::mime(&path)))
                    .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
                    .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
                    .with_header(Header::from_bytes("Content-Security-Policy", "sandbox").unwrap())
            }
            (Method::Get, ["realms", r, "files", "text"]) => {
                let root = self.realm(r)?.root()?;
                json_response(
                    200,
                    json!(crate::documents::read_text(&root, &param(query, "path"))?),
                )
            }
            // Cheap enough to ask every few seconds: has the open file changed?
            (Method::Get, ["realms", r, "files", "version"]) => {
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, &param(query, "path"))?;
                json_response(200, json!({ "version": crate::documents::version(&path)? }))
            }
            (Method::Put, ["realms", r, "files", "text"]) => {
                // JSON escapes can make the text up to six times longer.
                let bytes = read_body(
                    request,
                    6 * crate::documents::MAX_TEXT as usize + 4096,
                    "the text",
                )?;
                let body: Value =
                    serde_json::from_slice(&bytes).context("request body must be JSON")?;
                let root = self.realm(r)?.root()?;
                let str_of = |k: &str| body[k].as_str().unwrap_or_default();
                match crate::documents::save_text(
                    &root,
                    str_of("path"),
                    str_of("text"),
                    str_of("version"),
                ) {
                    Ok(version) => json_response(200, json!({ "version": version })),
                    Err(crate::documents::SaveError::Changed(version)) => json_response(
                        409,
                        json!({ "error": "the file changed since you opened it", "changed": true, "version": version }),
                    ),
                    Err(crate::documents::SaveError::Other(e)) => return Err(e),
                }
            }
            (Method::Post, ["realms", r, "files", "new"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                let path = crate::documents::create(
                    &root,
                    body["dir"].as_str().unwrap_or(""),
                    body["name"].as_str().unwrap_or(""),
                    body["folder"].as_bool() == Some(true),
                )?;
                json_response(200, json!({ "path": path }))
            }
            (Method::Get, ["realms", r, "files", "all"]) => {
                const LIMIT: usize = 5000;
                let root = self.realm(r)?.root()?;
                let files = crate::documents::all_files(&root, LIMIT);
                json_response(
                    200,
                    json!({ "truncated": files.len() >= LIMIT, "files": files }),
                )
            }
            (Method::Post, ["realms", r, "files", "preview"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, body["path"].as_str().unwrap_or(""))?;
                if crate::documents::mime(&path) != "application/pdf" || !path.is_file() {
                    return Err(anyhow!("only PDF files open this way"));
                }
                let ticket = random_token()?;
                self.previews
                    .lock()
                    .unwrap()
                    .insert(ticket.clone(), (path, std::time::Instant::now()));
                json_response(200, json!({ "url": format!("/preview/{ticket}") }))
            }
            (Method::Put, ["realms", r, "files", "raw"]) => {
                let root = self.realm(r)?.root()?;
                let bytes = read_body(request, crate::documents::MAX_FILE, "the file")?;
                let path = crate::documents::add(
                    &root,
                    &param(query, "dir"),
                    &param(query, "name"),
                    &bytes,
                )?;
                json_response(200, json!({ "path": path }))
            }
            (Method::Post, ["realms", r, "files", "open"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                let path = crate::documents::resolve(&root, body["path"].as_str().unwrap_or(""))?;
                // With the Mac's default app, or shown in the Finder.
                let mut open = std::process::Command::new("/usr/bin/open");
                if body["reveal"].as_bool() == Some(true) {
                    open.arg("-R");
                }
                let status = open.arg(&path).status().context("cannot run open")?;
                if !status.success() {
                    return Err(anyhow!("macOS could not open it"));
                }
                ok()
            }
            (Method::Post, ["realms", r, "files", "trash"]) => {
                let body = read_json(request)?;
                let root = self.realm(r)?.root()?;
                crate::documents::trash(&root, body["path"].as_str().unwrap_or(""))?;
                ok()
            }
            (Method::Post, ["realms", r, "memory"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let text = body["text"].as_str().unwrap_or_default();
                match body["replaces"].as_str().filter(|id| !id.is_empty()) {
                    Some(id) => realm.memory().replace(id, text, "owner")?,
                    None => realm.memory().add(text, "owner")?,
                };
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "memory", id]) => {
                let realm = self.realm(r)?;
                if !realm.memory().remove(id)? {
                    return Err(anyhow!("no such memory"));
                }
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r, "business"]) => {
                json_response(200, self.business(&self.realm(r)?)?)
            }
            (Method::Get, ["realms", r, "routines"]) => {
                json_response(200, self.routines(&self.realm(r)?)?)
            }
            (Method::Post, ["realms", r, "routines"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                save_routine(&realm, &body)?;
                ok()
            }
            (Method::Patch, ["realms", r, "routines", id]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                if let Some(enabled) = body["enabled"].as_bool() {
                    let found = realm.objectives().update(id, |o| {
                        o.enabled = enabled;
                        // Re-armed: a time missed while paused does not fire.
                        o.next_at = None;
                        o.fingerprint = None;
                    })?;
                    if !found {
                        bail!("no routine {id}");
                    }
                }
                ok()
            }
            (Method::Delete, ["realms", r, "routines", id]) => {
                if !self.realm(r)?.objectives().remove(id)? {
                    bail!("no routine {id}");
                }
                ok()
            }
            (Method::Post, ["realms", r, "routines", id, "run"]) => {
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
                // The owner is here: they answer approvals as for any task.
                self.spawn(realm, state, Job::Drive);
                json_response(200, json!({"id": run}))
            }
            (Method::Get, ["realms", r, "planning"]) => {
                json_response(200, self.planning(&self.realm(r)?, chrono::Utc::now())?)
            }
            (Method::Get, ["realms", r, "agents"]) => {
                json_response(200, self.agents(&self.realm(r)?)?)
            }
            (Method::Put, ["realms", r, "agents", name]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let (root, own) = (realm.root()?, realm.config.departments.clone());
                // The job always; its title and department when sent, all
                // checked before any is written.
                let job = body["job"].as_str().unwrap_or_default();
                if job.trim().is_empty() {
                    bail!("an agent needs a job");
                }
                // Lists come as arrays of names, written comma-separated.
                let field = |key: &str| match &body[key] {
                    Value::Array(a) => Some(
                        a.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    v => v.as_str().map(str::to_string),
                };
                const FIELDS: [&str; 6] = [
                    "title",
                    "first_name",
                    "department",
                    "integrations",
                    "skills",
                    "repos",
                ];
                for key in FIELDS {
                    if let Some(value) = field(key) {
                        crate::business::check_agent_field(key, &value, &own)?;
                    }
                }
                crate::business::set_job(&root, name, job)?;
                for key in FIELDS {
                    if let Some(value) = field(key) {
                        crate::business::set_agent_field(&root, name, key, &value, &own)?;
                    }
                }
                ok()
            }
            (Method::Delete, ["realms", r, "agents", name]) => {
                let_go(&self.realm(r)?, name)?;
                ok()
            }
            (Method::Get, ["realms", r, "steering"]) => {
                json_response(200, self.steering(&self.realm(r)?)?)
            }
            (Method::Put, ["realms", r, "profile"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                crate::business::set_profile(
                    &realm.root()?,
                    body["text"].as_str().unwrap_or_default(),
                )?;
                ok()
            }
            (Method::Put, ["realms", r, "strategy"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                crate::business::set_strategy(
                    &realm.root()?,
                    body["text"].as_str().unwrap_or_default(),
                )?;
                ok()
            }
            (Method::Post, ["realms", r, "pipelines", name, "run"]) => {
                let realm = self.realm(r)?;
                // Refused at once when the file does not hold.
                crate::pipeline::load(&crate::business::dir(&realm.root()?), name)?;
                let (hub, name) = (self.hub.clone(), name.to_string());
                std::thread::spawn(move || hub.run_pipeline_unattended(&realm, &name));
                ok()
            }
            (Method::Get, ["realms", r, "drafts"]) => {
                // `status=pending,approved`; every draft without it.
                let status = param(query, "status");
                let statuses: Vec<&str> = status.split(',').filter(|s| !s.is_empty()).collect();
                let drafts = Store::of(&self.realm(r)?)?.drafts(&statuses)?;
                json_response(200, json!({"drafts": drafts}))
            }
            (Method::Post, ["realms", r, "drafts", id]) => {
                let body = read_json(request)?;
                let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
                let action = match body["action"].as_str() {
                    Some("approve") => drafts::Move::Approve,
                    Some("edit") => drafts::Move::Edit(text("text")),
                    Some("reject") => drafts::Move::Reject(text("reason")),
                    Some("published") => drafts::Move::Published,
                    _ => bail!("action is approve, edit, reject or published"),
                };
                let draft = Store::of(&self.realm(r)?)?.move_draft(
                    id.parse().context("no such draft")?,
                    body["revision"].as_i64(),
                    action,
                    // How long the draft was in front of the owner, as the app saw it.
                    body["seconds"].as_u64().unwrap_or(0),
                )?;
                json_response(200, json!({"draft": draft}))
            }
            (Method::Post, ["realms", r, "secrets"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                realm.set_secret(
                    body["key"].as_str().unwrap_or_default().trim(),
                    body["value"].as_str().unwrap_or_default(),
                )?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "secrets", k]) => {
                let realm = self.realm(r)?;
                if !realm.remove_secret(k)? {
                    return Err(anyhow!("no such credential"));
                }
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r, "repos"]) => {
                json_response(200, json!({"repos": self.repos(&self.realm(r)?)}))
            }
            // What the business's GitHub token reaches, to pick from.
            (Method::Get, ["realms", r, "github", "repos"]) => {
                match self.hub.github_repos(&self.realm(r)?) {
                    Ok(Some(repos)) => json_response(200, json!({"token": true, "repos": repos})),
                    Ok(None) => json_response(200, json!({"token": false, "repos": []})),
                    Err(e) => json_response(
                        200,
                        json!({"token": true, "repos": [], "error": format!("{e:#}")}),
                    ),
                }
            }
            (Method::Post, ["realms", r, "repos"]) => {
                let body = read_json(request)?;
                let mut realm = self.realm(r)?;
                let text = |k: &str| body[k].as_str().map(str::trim).filter(|v| !v.is_empty());
                let dest = text("path").map(expand_home);
                self.hub.connect_repo(
                    &mut realm,
                    text("source").unwrap_or_default(),
                    text("name"),
                    dest.as_deref(),
                )?;
                json_response(200, json!({"repos": self.repos(&realm)}))
            }
            (Method::Patch, ["realms", r, "repos", name]) => {
                let body = read_json(request)?;
                let mut realm = self.realm(r)?;
                let checks = body["checks"].as_array().map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .map(String::from)
                        .collect()
                });
                let delivery = body["delivery"].as_str().map(Delivery::parse).transpose()?;
                realm.update_repo(&percent_decode(name), checks, delivery)?;
                json_response(200, json!({"repos": self.repos(&realm)}))
            }
            (Method::Delete, ["realms", r, "repos", name]) => {
                let mut realm = self.realm(r)?;
                realm.remove_repo(&percent_decode(name))?;
                json_response(200, json!({"repos": self.repos(&realm)}))
            }
            (Method::Post, ["realms", r, "models"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
                self.hub.set_model_tier(
                    &realm,
                    &text("tier"),
                    &text("provider"),
                    &text("model"),
                    &text("effort"),
                    body["all_realms"].as_bool().unwrap_or(false),
                )?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Get, ["realms", r, "models"]) => {
                let realm = self.realm(r)?;
                let force = query.split('&').any(|kv| kv == "refresh=1");
                json_response(200, self.model_options(&realm, force)?)
            }
            _ => return Ok(None),
        }))
    }
}

impl App {
    fn realms(&self) -> Result<Value> {
        let registry = self.registry.lock().unwrap();
        let default = self.hub.home().default_realm();
        let realms: Vec<Value> = self
            .hub
            .home()
            .realms()?
            .into_iter()
            .map(|r| {
                let prefix = key(r.name(), "");
                let lives: Vec<&Arc<Live>> = registry
                    .iter()
                    .filter(|(k, _)| k.starts_with(&prefix))
                    .map(|(_, v)| v)
                    .collect();
                json!({
                    "name": r.name(),
                    "title": r.root().ok().and_then(|root| crate::business::title(&root)),
                    "description": r.config.description,
                    "archived": r.config.archived,
                    "default": default.as_deref() == Some(r.name()),
                    "image": images::version(r.dir(), "image"),
                    "live": lives.len(),
                    "needs_approval": lives.iter().filter(|l| l.pending.lock().unwrap().is_some()).count(),
                })
            })
            .collect();
        Ok(json!(realms))
    }

    /// A folder of the realm's documents; at the top, also what runs made.
    fn files(&self, realm: &Realm, rel: &str) -> Result<Value> {
        let root = realm.root()?;
        let dir = crate::documents::resolve(&root, rel)?;
        if !dir.is_dir() {
            return Err(anyhow!("{rel} is not a folder"));
        }
        let made = if dir == root {
            let titles = realm.run_titles();
            let runs = self.hub.all_runs(realm)?;
            crate::documents::made_by_runs(&root, &runs, 8)
                .into_iter()
                .map(|m| {
                    let run = runs.iter().find(|r| r.id == m.run);
                    json!({
                        "path": m.path,
                        "run": m.run,
                        "run_title": titles.get(&m.run).cloned().or_else(|| run.map(|r| r.objective.lines().next().unwrap_or("").to_string())),
                        "at": m.at,
                        "edited": m.edited,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(json!({
            "root": root.display().to_string(),
            "own_root": realm.config.root.is_some(),
            "path": crate::documents::relative(&root, &dir),
            "entries": crate::documents::list(&dir)?,
            "made": made,
        }))
    }

    /// The business's pipelines, each with its last runs and next time.
    fn business(&self, realm: &Realm) -> Result<Value> {
        let business = crate::business::dir(&realm.root()?);
        let store = Store::of(realm)?;
        let mut pipelines = Vec::new();
        for (name, loaded) in crate::pipeline::load_all(&business)? {
            let runs = store.pipeline_runs(&name, 10)?;
            pipelines.push(match loaded {
                Ok(p) => json!({
                    "name": name,
                    "description": p.description,
                    "schedule": p.schedule,
                    "steps": p.steps.iter().map(|s| s.describe(&p.prompts)).collect::<Vec<_>>(),
                    "next_at": store.next_run(&name)?,
                    "runs": runs,
                }),
                Err(e) => json!({"name": name, "error": format!("{e:#}"), "runs": runs}),
            });
        }
        Ok(json!({ "pipelines": pipelines }))
    }

    /// The business's routines (decision 37), with what the form offers:
    /// its agents and the extensions a routine can be limited to.
    pub(super) fn routines(&self, realm: &Realm) -> Result<Value> {
        let list: Vec<Value> = realm
            .objectives()
            .list()?
            .into_iter()
            .map(|o| {
                let running = o
                    .last_run
                    .as_deref()
                    .is_some_and(|r| self.live(realm.name(), r).is_some() || self.hub.is_live(realm, r));
                json!({
                    "id": o.id,
                    "objective": o.objective,
                    "every": match &o.trigger {
                        crate::objective::Trigger::Schedule { every } => Some(every.clone()),
                        _ => None,
                    },
                    "trigger": o.trigger.describe(),
                    "bot": o.bot,
                    "tools": o.tools,
                    "allow": o.approvals.iter().map(|g| match &g.scope {
                        Some(s) => format!("{}:{}", g.capability.as_str(), s.trim_start_matches("integration ")),
                        None => g.capability.as_str().to_string(),
                    }).collect::<Vec<_>>(),
                    "delegates": o.delegates,
                    "max_minutes": o.budget.max_minutes,
                    "wait": o.approval_wait_minutes,
                    "enabled": o.enabled,
                    "running": running,
                    "next_at": o.next_at,
                    "last_run": o.last_run,
                    "last_at": o.last_at,
                    "last_status": o.last_status,
                })
            })
            .collect();
        Ok(json!({
            "routines": list,
            "agents": crate::business::bots(&realm.root()?)?.into_iter().map(|b| b.0).collect::<Vec<_>>(),
            "extensions": crate::tools::extension_names(realm),
        }))
    }

    /// What runs when (decision 37): the coming week's times of every
    /// scheduled routine and pipeline, and what ran in the last one.
    pub(super) fn planning(
        &self,
        realm: &Realm,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Value> {
        use crate::objective::{Schedule, Trigger};
        let week = chrono::Duration::days(7);
        let mut coming = Vec::new();
        let mut times = |name: &str,
                         kind: &str,
                         bot: Option<&str>,
                         schedule: &Schedule,
                         first: chrono::DateTime<chrono::Utc>| {
            let mut at = first.max(now);
            if at > first {
                at = schedule.next_after(now);
            }
            // A schedule every few minutes would fill the week: at most 50 times each.
            for _ in 0..50 {
                if at > now + week {
                    break;
                }
                coming.push(json!({"at": at, "name": name, "kind": kind, "bot": bot}));
                at = schedule.next_after(at);
            }
        };
        let routines = realm.objectives().list()?;
        for o in routines.iter().filter(|o| o.enabled) {
            if let Trigger::Schedule { every } = &o.trigger
                && let Ok(schedule) = Schedule::parse(every)
            {
                let first = o.next_at.unwrap_or_else(|| schedule.next_after(now));
                times(&o.id, "routine", o.bot.as_deref(), &schedule, first);
            }
        }
        let root = realm.root()?;
        let store = Store::of(realm)?;
        let mut past = Vec::new();
        for (name, loaded) in crate::pipeline::load_all(&crate::business::dir(&root))? {
            if let Ok(p) = loaded
                && let Some(Ok(schedule)) = p.schedule.as_deref().map(Schedule::parse)
            {
                let first = store
                    .next_run(&name)?
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
                    .map_or_else(
                        || schedule.next_after(now),
                        |t| t.with_timezone(&chrono::Utc),
                    );
                times(&name, "pipeline", None, &schedule, first);
            }
            for r in store.pipeline_runs(&name, 50)? {
                if let Ok(at) = chrono::DateTime::parse_from_rfc3339(&r.started_at)
                    && at.with_timezone(&chrono::Utc) > now - week
                {
                    past.push(json!({
                        "at": at.with_timezone(&chrono::Utc),
                        "name": name,
                        "kind": "pipeline",
                        "status": if r.ok { "completed" } else { "failed" },
                    }));
                }
            }
        }
        for s in self.hub.runs(realm)? {
            if s.updated_at > now - week
                && let Some(routine) = &s.routine
            {
                past.push(json!({
                    "at": s.updated_at,
                    "name": routine,
                    "kind": "routine",
                    "status": s.display_status(),
                    "run": s.id,
                }));
            }
        }
        coming.sort_by(|a, b| a["at"].as_str().cmp(&b["at"].as_str()));
        past.sort_by(|a, b| b["at"].as_str().cmp(&a["at"].as_str()));
        Ok(json!({"coming": coming, "past": past}))
    }

    /// The business's bots (decisions 36 and 37): each one's job, the
    /// routines it does, its last tasks and what it cost this month.
    pub(super) fn agents(&self, realm: &Realm) -> Result<Value> {
        let root = realm.root()?;
        let routines = realm.objectives().list()?;
        let runs = self.hub.all_runs(realm)?;
        let prices = crate::prices::Prices::load(self.hub.home());
        let mut agents = Vec::new();
        for (name, job) in crate::business::bots(&root)? {
            let mine: Vec<_> = routines
                .iter()
                .filter(|o| o.bot.as_deref() == Some(name.as_str()))
                .collect();
            let ids: Vec<&str> = mine.iter().map(|o| o.id.as_str()).collect();
            let worked: Vec<(String, State)> = runs
                .iter()
                .filter(|s| {
                    s.origin
                        .as_ref()
                        .is_some_and(|o| ids.contains(&o.objective.as_str()))
                })
                .map(|s| (realm.name().to_string(), s.clone()))
                .collect();
            let month = crate::usage::aggregate(&worked, 1, chrono::Local::now(), &prices).month;
            let mut last: Vec<&State> = worked.iter().map(|(_, s)| s).collect();
            last.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
            agents.push(json!({
                "name": name,
                "job": job.text,
                "version": &job.version[..12],
                "routines": mine.iter().map(|o| json!({
                    "id": o.id,
                    "objective": o.objective,
                    "trigger": o.trigger.describe(),
                    "enabled": o.enabled,
                    "tools": o.tools,
                    "approvals": o.approvals.iter().map(|g| match &g.scope {
                        Some(s) => format!("{} ({s})", g.capability.as_str()),
                        None => g.capability.as_str().to_string(),
                    }).collect::<Vec<_>>(),
                    "budget": o.budget.describe(),
                    "last_status": o.last_status,
                    "next_at": o.next_at,
                })).collect::<Vec<_>>(),
                "month_cost": month.cost.total(),
                "month_calls": month.calls,
                "unpriced": month.unpriced,
                "tasks": last.iter().take(5).map(|s| json!({
                    "id": s.id,
                    "objective": s.objective,
                    "status": s.status.as_str(),
                    "updated_at": s.updated_at,
                })).collect::<Vec<_>>(),
            }));
        }
        Ok(json!({"agents": agents}))
    }

    /// The owner's page (decision 37): what waits for a decision, what needs
    /// attention, the strategy, and the week.
    pub(super) fn steering(&self, realm: &Realm) -> Result<Value> {
        let root = realm.root()?;
        let store = Store::of(realm)?;
        let mut approvals = Vec::new();
        for (k, live) in self.registry.lock().unwrap().iter() {
            let Some(id) = k.strip_prefix(&format!("{}/", realm.name())) else {
                continue;
            };
            if let Some(p) = live.pending.lock().unwrap().as_ref() {
                let state = live.snapshot.lock().unwrap();
                approvals.push(json!({
                    "run": id,
                    "objective": state.objective,
                    "routine": state.origin.as_ref().map(|o| o.objective.clone()),
                    "approval_id": p.id,
                    "tool": p.request.tool,
                    "capability": p.request.capability.as_str(),
                    "preview": p.request.preview.chars().take(1200).collect::<String>(),
                    "warnings": p.request.warnings,
                }));
            }
        }
        let week_ago = chrono::Utc::now() - chrono::Duration::days(7);
        let archived = realm.archived_runs();
        let mut attention: Vec<Value> = self
            .hub
            .runs(realm)?
            .into_iter()
            .filter(|r| r.display_status() == "failed" && r.updated_at > week_ago)
            .filter(|r| !archived.contains(&r.id))
            .map(|r| json!({"kind": "failed", "run": r.id, "title": r.objective, "at": r.updated_at}))
            .collect();
        for o in realm.objectives().list()? {
            if matches!(o.last_status.as_deref(), Some("failed" | "stopped")) {
                attention.push(json!({
                    "kind": "routine",
                    "routine": o.id,
                    "title": format!("{} {}", o.id, o.last_status.unwrap_or_default()),
                    "run": o.last_run,
                    "at": o.last_at,
                }));
            }
        }
        for (name, loaded) in crate::pipeline::load_all(&crate::business::dir(&root))? {
            if let Err(e) = loaded {
                attention.push(
                    json!({"kind": "pipeline", "title": format!("{name} is refused: {e:#}")}),
                );
            } else if let Some(last) = store.pipeline_runs(&name, 1)?.pop()
                && !last.ok
            {
                attention.push(json!({
                    "kind": "pipeline",
                    "title": format!("{name} failed: {}", last.error.unwrap_or_default()),
                    "at": last.started_at,
                }));
            }
        }
        // What the team finished this week, newest first, with the start of
        // each report.
        let routines = realm.objectives().list()?;
        let bot_of = |routine: &str| {
            routines
                .iter()
                .find(|o| o.id == routine)
                .and_then(|o| o.bot.clone())
        };
        let mut done: Vec<State> = self
            .hub
            .all_runs(realm)?
            .into_iter()
            .filter(|s| s.status == crate::state::Status::Completed && s.updated_at > week_ago)
            .filter(|s| !archived.contains(&s.id))
            .collect();
        done.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        let delivered: Vec<Value> = done
            .iter()
            .take(8)
            .map(|s| {
                let routine = s.origin.as_ref().map(|o| o.objective.clone());
                json!({
                    "run": s.id,
                    "title": s.objective.lines().next().unwrap_or_default(),
                    "routine": routine,
                    "agent": routine.as_deref().and_then(bot_of),
                    "at": s.updated_at,
                    "report": s.report.as_deref().unwrap_or_default().chars().take(280).collect::<String>(),
                })
            })
            .collect();
        // Each agent: what it is doing or did last, and when it works next.
        let team: Vec<Value> = crate::business::bots(&root)?
            .into_iter()
            .map(|(name, _)| {
                let mine: Vec<_> = routines
                    .iter()
                    .filter(|o| o.bot.as_deref() == Some(name.as_str()))
                    .collect();
                let running = mine.iter().any(|o| {
                    o.last_run.as_deref().is_some_and(|r| {
                        self.live(realm.name(), r).is_some() || self.hub.is_live(realm, r)
                    })
                });
                let last = mine
                    .iter()
                    .filter(|o| o.last_at.is_some())
                    .max_by_key(|o| o.last_at);
                json!({
                    "name": name,
                    "running": running,
                    "routines": mine.len(),
                    "last_status": last.and_then(|o| o.last_status.clone()),
                    "last_at": last.and_then(|o| o.last_at),
                    "next_at": mine.iter().filter(|o| o.enabled).filter_map(|o| o.next_at).min(),
                })
            })
            .collect();
        Ok(json!({
            "approvals": approvals,
            "drafts": store.drafts(&["pending"])?,
            "attention": attention,
            "strategy": crate::business::strategy(&root)?,
            "week": store.week(week_ago)?,
            "delivered": delivered,
            "team": team,
        }))
    }

    /// The realm's repositories for the app, with where each one lives on GitHub.
    fn repos(&self, realm: &Realm) -> Vec<Value> {
        realm
            .repos()
            .into_iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "path": r.path,
                    "url": r.url,
                    "main": r.main,
                    "checks": r.checks,
                    "delivery": r.delivery.as_str(),
                    "github": crate::github::Remote::of(&r.path).ok().map(|g| g.slug()),
                    "branch": crate::isolation::current_branch(&r.path),
                })
            })
            .collect()
    }

    pub(super) fn realm_detail(&self, realm: &Realm) -> Result<Value> {
        let c = &realm.config;
        let model = self.hub.connections_for(realm);
        let secrets: Vec<Value> = realm
            .secrets()?
            .into_iter()
            .map(|(k, _)| json!({"key": k, "provider": PROVIDER_KEYS.contains(&k.as_str())}))
            .collect();
        Ok(json!({
            "name": c.name,
            "description": c.description,
            "root": realm.root().map(|p| p.display().to_string()).unwrap_or_default(),
            "own_root": c.root.is_some(),
            "isolation": c.isolation.as_str(),
            "delivery": c.delivery.as_str(),
            "github": realm
                .repo(None)
                .ok()
                .flatten()
                .and_then(|r| crate::github::Remote::of(&r.path).ok())
                .map(|r| r.slug()),
            "repos": self.repos(realm),
            "github_token": realm.secret(crate::github::TOKEN).is_some(),
            "policy": c.policy,
            "checks": c.checks,
            "departments": super::company::departments(realm),
            "share_owner_profile": c.share_owner_profile,
            "archived": c.archived,
            "default": self.hub.home().default_realm().as_deref() == Some(realm.name()),
            "image": images::version(realm.dir(), "image"),
            "model": model.as_ref().ok().map(|m| m.describe()),
            "model_error": model.err().map(|e| format!("{e:#}")),
            "models": self.hub.model_settings(realm)?,
            "memory": realm.memory().list()?,
            "secrets": secrets,
            "integrations": c.integrations.iter().map(|i| {
                let missing: Vec<String> = i
                    .secret_names()
                    .into_iter()
                    .filter(|k| realm.secret(k).is_none())
                    .collect();
                json!({
                    "name": i.name,
                    "description": i.description,
                    "base_url": i.base_url,
                    "headers": i.headers,
                    "missing_secrets": missing,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    fn create_realm(&self, body: &Value) -> Result<Realm> {
        let root = body["root"]
            .as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(expand_home);
        let isolation = match body["isolation"].as_str() {
            Some("none") => Some(Isolation::None),
            Some("worktree") => Some(Isolation::Worktree),
            _ => None,
        };
        let realm = self.hub.home().create_realm(NewRealm {
            name: body["name"].as_str().unwrap_or_default().trim().to_string(),
            description: body["description"].as_str().unwrap_or_default().to_string(),
            root,
            isolation,
        })?;
        if let Some(root) = &realm.config.root {
            self.hub.import_runs(&realm, root)?;
        }
        Ok(realm)
    }

    pub(super) fn update_realm(&self, name: &str, body: &Value) -> Result<Realm> {
        let mut realm = self.realm(name)?;
        let root = realm.root().ok();
        let c = &mut realm.config;
        if let Some(d) = body["description"].as_str() {
            c.description = d.trim().to_string();
        }
        if let Some(policy) = body["policy"].as_object() {
            for (cap, rule) in policy {
                c.policy.set(
                    Capability::parse(cap)?,
                    Rule::parse(rule.as_str().unwrap_or_default())?,
                );
            }
        }
        if let Some(checks) = body["checks"].as_array() {
            c.checks = checks
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
        }
        // The founder's own departments (decision 79): one in use by an
        // agent stays, so no agent is set aside by it.
        if let Some(list) = body["departments"].as_array() {
            let mut own = Vec::new();
            for d in list.iter().filter_map(Value::as_str) {
                let d = crate::business::config::new_department(d)?;
                if !own.contains(&d) {
                    own.push(d);
                }
            }
            if let Some(root) = &root
                && let Ok(config) = crate::business::config::load(root, &c.departments)
            {
                for (name, doc) in &config.agents {
                    if let Some(d) = doc.text("department")
                        && c.departments.contains(&d)
                        && !own.contains(&d)
                    {
                        bail!("{name} is in {d}: move it to another department first");
                    }
                }
            }
            c.departments = own;
        }
        if let Some(share) = body["share_owner_profile"].as_bool() {
            c.share_owner_profile = share;
        }
        if let Some(archived) = body["archived"].as_bool() {
            c.archived = archived;
        }
        if let Some(delivery) = body["delivery"].as_str() {
            let delivery = Delivery::parse(delivery)?;
            if delivery == Delivery::PullRequest {
                if c.isolation != Isolation::Worktree {
                    bail!("pull requests need a realm whose chats work in their own worktree");
                }
                crate::github::Remote::of(&realm.root()?)?;
            }
            realm.config.delivery = delivery;
        }
        realm.save()?;
        if body["default"].as_bool() == Some(true) {
            self.hub.home().set_default_realm(name)?;
        }
        Ok(realm)
    }

    /// The model picked in the app for a request (`provider/model`), checked
    /// now so a missing key is reported before the run starts. "auto" or
    /// nothing: the realm's own model and routing.
    pub(super) fn model_choice(&self, realm: &Realm, body: &Value) -> Result<Option<String>> {
        match body["model"]
            .as_str()
            .map(str::trim)
            .filter(|m| !m.is_empty() && *m != "auto")
        {
            Some(choice) => {
                self.hub.chosen_model(realm, choice)?;
                Ok(Some(choice.to_string()))
            }
            None => {
                self.hub.model_for(realm).context("no model available")?;
                Ok(None)
            }
        }
    }

    /// Providers and models for the picker, cached ten minutes per realm (the
    /// lists come from each provider's API).
    fn model_options(&self, realm: &Realm, force: bool) -> Result<Value> {
        static CACHE: std::sync::OnceLock<Mutex<HashMap<String, (std::time::Instant, Value)>>> =
            std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if !force
            && let Some((at, v)) = cache.lock().unwrap().get(realm.name())
            && at.elapsed() < Duration::from_secs(600)
        {
            return Ok(v.clone());
        }
        let value = json!({
            "providers": self.hub.model_options(realm)?,
            "default": self.hub.model_for(realm).ok().map(|m| m.name()),
        });
        cache.lock().unwrap().insert(
            realm.name().to_string(),
            (std::time::Instant::now(), value.clone()),
        );
        Ok(value)
    }
}
