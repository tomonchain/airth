//! App-wide routes: setup, the owner, updates, the inbox and usage.

use super::*;

impl App {
    /// The app-wide routes; None when the request is not one of them.
    pub(super) fn general_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        query: &str,
    ) -> Result<Option<HttpResponse>> {
        let ok = || json_response(200, json!({"ok": true}));
        Ok(Some(match (method, segments) {
            (Method::Get, ["info"]) => json_response(200, self.info()),
            (Method::Get, ["setup"]) => json_response(200, self.setup()?),
            (Method::Post, ["setup", "key"]) => {
                let body = read_json(request)?;
                self.hub
                    .set_shared_anthropic_key(body["key"].as_str().unwrap_or_default())?;
                json_response(200, self.setup()?)
            }
            (Method::Get, ["owner"]) => json_response(200, self.owner()?),
            (Method::Put, ["owner"]) => {
                let owner: Owner =
                    serde_json::from_value(read_json(request)?).context("invalid profile")?;
                self.hub.home().save_owner(&owner)?;
                json_response(200, self.owner()?)
            }
            // Pictures of the owner and of each realm, for the app.
            (Method::Get, ["owner", "avatar"]) => image_response(self.hub.home().dir(), "avatar"),
            (Method::Put, ["owner", "avatar"]) => {
                images::save(self.hub.home().dir(), "avatar", &read_bytes(request)?)?;
                json_response(200, self.owner()?)
            }
            (Method::Delete, ["owner", "avatar"]) => {
                images::remove(self.hub.home().dir(), "avatar")?;
                json_response(200, self.owner()?)
            }
            (Method::Post, ["update", "check"]) => {
                let mut st = self.update.lock().unwrap();
                if st.phase == "idle" || st.phase == "failed" {
                    st.phase = "checking";
                    st.prepared_commit = None;
                }
                ok()
            }
            (Method::Post, ["update", "restart"]) => {
                let staged = {
                    let mut st = self.update.lock().unwrap();
                    if st.phase != "ready" {
                        return Err(anyhow!("no update is ready to install"));
                    }
                    let staged = st.staged.clone().context("no update is ready")?;
                    st.phase = "restarting";
                    staged
                };
                let exe = match crate::update::install(&staged) {
                    Ok(exe) => exe,
                    Err(e) => {
                        let mut st = self.update.lock().unwrap();
                        st.phase = "failed";
                        st.error = Some(format!("installing failed: {e:#}"));
                        return Err(e);
                    }
                };
                eprintln!("installed the update; restarting");
                let (registry, path, port) =
                    (self.registry.clone(), server_file(&self.hub), self.port);
                std::thread::spawn(move || restart(registry, path, exe, port));
                ok()
            }
            (Method::Post, ["inbox", "read"]) => {
                let body = read_json(request)?;
                let ids: Option<Vec<u64>> = body["ids"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_u64).collect());
                self.hub.home().inbox().mark_read(ids.as_deref())?;
                ok()
            }
            (Method::Get, ["usage"]) => {
                let days: u32 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("days="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(30);
                let report = crate::usage::report(&self.hub, days, chrono::Local::now())?;
                json_response(200, json!(report))
            }
            _ => return Ok(None),
        }))
    }
}

impl App {
    /// What is set up already, for the welcome guide.
    fn setup(&self) -> Result<Value> {
        let realms: Vec<_> = self
            .hub
            .home()
            .realms()?
            .into_iter()
            .filter(|r| !r.config.archived)
            .collect();
        let model_ready =
            self.hub.shared_model_ready() || realms.iter().any(|r| self.hub.model_for(r).is_ok());
        Ok(json!({ "realms": realms.len(), "model_ready": model_ready }))
    }

    /// The owner profile, and the version of their picture (None: none).
    fn owner(&self) -> Result<Value> {
        let mut owner = json!(self.hub.home().owner()?);
        owner["avatar"] = json!(images::version(self.hub.home().dir(), "avatar"));
        Ok(owner)
    }
}
