//! App-wide routes: setup, the owner, updates, the inbox and usage.

use super::*;
use crate::supervise::Locked;

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
            // The name the founder gives this Airth (decision 113).
            (Method::Put, ["name"]) => {
                let body = read_json(request)?;
                self.hub
                    .home()
                    .set_name(body["name"].as_str().unwrap_or_default())?;
                json_response(200, json!({"name": self.hub.home().name()}))
            }
            // Notifications on the founder's phone (decision 117).
            (Method::Get, ["push"]) => {
                let home = self.hub.home();
                json_response(
                    200,
                    json!({"key": crate::push::public_key(home)?, "phones": crate::push::subscriptions(home).len()}),
                )
            }
            (Method::Post, ["push", "subscribe"]) => {
                let body = read_json(request)?;
                crate::push::subscribe(
                    self.hub.home(),
                    body["endpoint"].as_str().unwrap_or_default(),
                    body["keys"]["p256dh"].as_str().unwrap_or_default(),
                    body["keys"]["auth"].as_str().unwrap_or_default(),
                )?;
                ok()
            }
            (Method::Post, ["push", "unsubscribe"]) => {
                let body = read_json(request)?;
                crate::push::unsubscribe(
                    self.hub.home(),
                    body["endpoint"].as_str().unwrap_or_default(),
                )?;
                ok()
            }
            (Method::Post, ["push", "test"]) => {
                let n = crate::push::test(self.hub.home())?;
                json_response(200, json!({"phones": n}))
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
                let mut st = self.update.locked();
                if st.phase == "idle" || st.phase == "failed" {
                    st.phase = "checking";
                    st.prepared_commit = None;
                }
                ok()
            }
            (Method::Post, ["update", "restart"]) => {
                let staged = {
                    let mut st = self.update.locked();
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
                        let mut st = self.update.locked();
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
            (Method::Get, ["phone"]) => {
                let links = crate::phone::hosts(self.hub.home().dir())?
                    .into_iter()
                    .map(|host| {
                        let link = crate::phone::pairing_link(&host, &self.token());
                        let (width, cells) = crate::phone::qr(&link)?;
                        let cells: String = cells.iter().map(|c| if *c { '1' } else { '0' }).collect();
                        Ok(json!({"host": host, "link": link, "qr": {"width": width, "cells": cells}}))
                    })
                    .collect::<Result<Vec<_>>>()?;
                json_response(200, json!({"hosts": links}))
            }
            // A new key, from the Mac only: a lost phone could otherwise
            // lock the owner out with a key of its own (decision 98).
            (Method::Post, ["phone", "new-key"]) => {
                let host = header(request, "Host").unwrap_or_default();
                if !crate::phone::host_allowed(&host, self.port, &[]) {
                    return Ok(Some(json_response(
                        403,
                        json!({"error": "a new key is made on the Mac, not from a phone"}),
                    )));
                }
                json_response(200, json!({"token": self.new_key()?}))
            }
            (Method::Post, ["phone", "hosts"]) => {
                let body = read_json(request)?;
                let host = crate::phone::allow(
                    self.hub.home().dir(),
                    body["host"].as_str().unwrap_or_default(),
                )?;
                json_response(200, json!({"host": host}))
            }
            (Method::Delete, ["phone", "hosts", host]) => {
                if !crate::phone::forget(self.hub.home().dir(), host)? {
                    bail!("{host} was not allowed");
                }
                ok()
            }
            // What each model provider costs and how long its money lasts
            // (decision 107).
            (Method::Get, ["spend"]) => {
                let providers = crate::spend::status(&self.hub, chrono::Utc::now())?;
                let month: f64 = providers.iter().map(|p| p.month_usd).sum();
                json_response(200, json!({ "providers": providers, "month_usd": month }))
            }
            (Method::Put, ["spend", "budgets", provider]) => {
                let body = read_json(request)?;
                let budget = crate::spend::Budget {
                    monthly_usd: body["monthly_usd"]
                        .as_f64()
                        .context("monthly_usd is a number")?,
                    reset_day: body["reset_day"].as_u64().unwrap_or(1) as u32,
                };
                crate::spend::set_budget(self.hub.home(), provider, Some(budget))?;
                ok()
            }
            (Method::Delete, ["spend", "budgets", provider]) => {
                crate::spend::set_budget(self.hub.home(), provider, None)?;
                ok()
            }
            (Method::Get, ["usage"]) => {
                let days: u32 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("days="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(30);
                // `realm=<name>`: one business, with its pipelines' model
                // calls and the agent of each routine.
                let only = Some(param(query, "realm")).filter(|r| !r.is_empty());
                let report =
                    crate::usage::report(&self.hub, days, chrono::Local::now(), only.as_deref())?;
                let mut out = json!(report);
                if let Some(name) = &only {
                    let realm = self.realm(name)?;
                    let since = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
                    out["pipelines"] = json!(Store::of(&realm)?.pipeline_costs(since)?);
                    out["agents"] = json!(
                        realm
                            .objectives()
                            .list()?
                            .into_iter()
                            .filter_map(|o| o.bot.map(|b| (o.id, b)))
                            .collect::<std::collections::BTreeMap<_, _>>()
                    );
                }
                json_response(200, out)
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
