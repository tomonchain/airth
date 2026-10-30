//! A realm's routes: its settings, files, memory, secrets, models and
//! repositories.

use super::*;

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
                realm
                    .memory()
                    .add(body["text"].as_str().unwrap_or_default(), "owner")?;
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

    /// The business page's data: drafts waiting, each pipeline with its
    /// last runs and next time, the last seven days, and the realm's latest
    /// notifications.
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
        let notifications: Vec<_> = self
            .hub
            .home()
            .inbox()
            .list()?
            .into_iter()
            .rev()
            .filter(|n| n.realm == realm.name())
            .take(20)
            .collect();
        Ok(json!({
            "has_business": business.is_dir(),
            "drafts": store.drafts(&["pending"])?,
            "pipelines": pipelines,
            "week": store.week(chrono::Utc::now() - chrono::Duration::days(7))?,
            "notifications": notifications,
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

    fn update_realm(&self, name: &str, body: &Value) -> Result<Realm> {
        let mut realm = self.realm(name)?;
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
