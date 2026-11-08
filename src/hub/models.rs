//! Which model a realm's runs use: its connections and tiers, shared
//! credentials, and routing each turn to the fast or main model.

use super::*;

impl Hub {
    /// The main model of a realm (to check one is configured, and name it).
    pub fn model_for(&self, realm: &Realm) -> Result<Arc<dyn Model>> {
        Ok(self.connections_for(realm)?.main)
    }

    /// The realm's model settings, looked up by name: its own credentials,
    /// then the shared ones, then the environment.
    pub(super) fn settings(&self, realm: &Realm) -> Result<impl Fn(&str) -> Option<String>> {
        let secrets = realm.secrets()?;
        let shared = self.home.provider_settings()?;
        Ok(move |key: &str| {
            secrets
                .iter()
                .chain(&shared)
                .find(|(k, v)| k == key && !v.is_empty())
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(key).ok())
        })
    }

    /// A provider's key, for Airth's own requests to that provider (its
    /// balance): the shared one, else the environment's, else the first
    /// business's that has one.
    pub fn provider_key(&self, var: &str) -> Option<String> {
        let shared = self.home.provider_settings().unwrap_or_default();
        let found = |list: &[(String, String)]| {
            list.iter()
                .find(|(k, v)| k == var && !v.trim().is_empty())
                .map(|(_, v)| v.clone())
        };
        found(&shared)
            .or_else(|| std::env::var(var).ok().filter(|v| !v.trim().is_empty()))
            .or_else(|| {
                self.home
                    .realms()
                    .ok()?
                    .iter()
                    .find_map(|r| found(&r.secrets().ok()?))
            })
    }

    /// Whether a model is set up for every realm (shared settings or the
    /// environment): what the welcome guide checks before any realm exists.
    pub fn shared_model_ready(&self) -> bool {
        let shared = self.home.provider_settings().unwrap_or_default();
        let get = |key: &str| {
            shared
                .iter()
                .find(|(k, v)| k == key && !v.is_empty())
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(key).ok())
        };
        crate::models::from_settings(&get).is_ok()
    }

    /// Check an Anthropic API key with the API (listing models is free),
    /// then keep it for every realm.
    pub fn set_shared_anthropic_key(&self, key: &str) -> Result<()> {
        let key = key.trim();
        if key.is_empty() {
            bail!("paste your API key");
        }
        if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            bail!("an API key has no spaces");
        }
        crate::models::list_models("anthropic", &|k| {
            (k == "ANTHROPIC_API_KEY").then(|| key.to_string())
        })
        .map_err(|e| anyhow::anyhow!("Anthropic did not accept this key ({e})"))?;
        self.home.set_provider_setting("ANTHROPIC_API_KEY", key)
    }

    /// The model for a choice like `deepseek/deepseek-chat`, built with the
    /// realm's credential for that provider only. Errors say what is missing.
    pub fn chosen_model(&self, realm: &Realm, choice: &str) -> Result<Arc<dyn Model>> {
        let get = self.settings(realm)?;
        let settings = crate::models::choice_settings(choice, &get)?;
        Ok(Arc::from(crate::models::from_settings(&settings)?))
    }

    /// The model connections set for a realm, for the app: per tier, its
    /// provider, model and effort, and where they come from ("realm", "all"
    /// for every realm's shared settings, "env", or None: not set).
    pub fn model_settings(&self, realm: &Realm) -> Result<serde_json::Value> {
        let secrets = realm.secrets()?;
        let shared = self.home.provider_settings()?;
        let find = |key: &str| -> Option<(String, &'static str)> {
            let in_list = |list: &[(String, String)]| {
                list.iter()
                    .find(|(k, v)| k == key && !v.trim().is_empty())
                    .map(|(_, v)| v.clone())
            };
            in_list(&secrets)
                .map(|v| (v, "realm"))
                .or_else(|| in_list(&shared).map(|v| (v, "all")))
                .or_else(|| {
                    std::env::var(key)
                        .ok()
                        .filter(|v| !v.trim().is_empty())
                        .map(|v| (v, "env"))
                })
        };
        let mut out = serde_json::Map::new();
        for tier in MODEL_TIERS {
            let [provider, model, effort] = tier_keys(tier).map(|k| find(&k));
            let source = model.as_ref().or(provider.as_ref()).map(|(_, s)| *s);
            out.insert(
                tier.to_string(),
                serde_json::json!({
                    "provider": provider.map(|(v, _)| v),
                    "model": model.map(|(v, _)| v),
                    "effort": effort.map(|(v, _)| v),
                    "source": source,
                }),
            );
        }
        Ok(serde_json::Value::Object(out))
    }

    /// Set one model connection of a realm ("main", "fast" or "fallback"),
    /// for this realm only or for every realm. An empty value removes the
    /// setting: an empty model turns the fast or fallback connection off,
    /// and an empty provider or effort inherits the main connection's.
    pub fn set_model_tier(
        &self,
        realm: &Realm,
        tier: &str,
        provider: &str,
        model: &str,
        effort: &str,
        all_realms: bool,
    ) -> Result<()> {
        if !MODEL_TIERS.contains(&tier) {
            bail!("unknown model tier '{tier}' (main, fast or fallback)");
        }
        let provider = provider.trim().to_lowercase();
        let model = model.trim();
        let effort = effort.trim().to_lowercase();
        if !provider.is_empty()
            && !crate::models::provider_catalog()
                .iter()
                .any(|(id, _, _)| *id == provider)
        {
            bail!("unknown provider '{provider}'");
        }
        if model.len() > 200 || model.chars().any(|c| c.is_whitespace() || c.is_control()) {
            bail!("a model name has no spaces");
        }
        if !effort.is_empty() && !EFFORTS.contains(&effort.as_str()) {
            bail!("effort is one of {}", EFFORTS.join(", "));
        }
        let off = tier != "main" && model.is_empty();
        let values = if off {
            [String::new(), String::new(), String::new()]
        } else {
            [provider, model.to_string(), effort]
        };
        for (key, value) in tier_keys(tier).iter().zip(values) {
            if all_realms {
                // The shared value applies here only without a realm value.
                realm.remove_secret(key)?;
                if value.is_empty() {
                    self.home.remove_provider_setting(key)?;
                } else {
                    self.home.set_provider_setting(key, &value)?;
                }
            } else if value.is_empty() {
                realm.remove_secret(key)?;
            } else {
                realm.set_secret(key, &value)?;
            }
        }
        Ok(())
    }

    /// Providers and their live model lists, for the app's model picker.
    pub fn model_options(&self, realm: &Realm) -> Result<Vec<serde_json::Value>> {
        let get = self.settings(realm)?;
        let catalog = crate::models::provider_catalog();
        let lists: Vec<Option<Result<Vec<String>>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = catalog
                .iter()
                .map(|(id, _, key)| {
                    let configured =
                        key.is_none_or(|k| get(k).is_some_and(|v| !v.trim().is_empty()));
                    let get = &get;
                    scope.spawn(move || {
                        (configured && *id != "ollama").then(|| crate::models::list_models(id, get))
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or(None))
                .collect()
        });
        let main = get("AIRTH_PROVIDER").unwrap_or_else(|| "anthropic".into());
        Ok(catalog
            .iter()
            .zip(lists)
            .map(|((id, name, key), list)| {
                let configured = key.is_none_or(|k| get(k).is_some_and(|v| !v.trim().is_empty()));
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "key": key,
                    "configured": configured,
                    "default_model": (main.trim().eq_ignore_ascii_case(id)).then(|| get("AIRTH_MODEL")).flatten(),
                    "models": list.as_ref().and_then(|l| l.as_ref().ok()).cloned().unwrap_or_default(),
                    "error": list.and_then(|l| l.err()).map(|e| format!("{e:#}")),
                })
            })
            .collect())
    }

    /// The model a drive uses: the one the owner picked for the run, else the
    /// realm's routing. Switching model marks the history as foreign, so the
    /// new model does not get the previous one's provider-specific blocks.
    pub(super) fn model_for_drive(
        &self,
        realm: &Realm,
        connections: &Connections,
        state: &mut State,
    ) -> Result<Routed> {
        let key = state.model_choice.clone().unwrap_or_else(|| "auto".into());
        let switched = state.answered_by.as_deref().is_some_and(|prev| prev != key);
        state.answered_by = Some(key);
        let Some(choice) = state.model_choice.clone() else {
            let mut routed = self.route(connections, state);
            if switched {
                routed = Routed::new_after(routed, state.model_len());
            }
            return Ok(routed);
        };
        let chosen = self.chosen_model(realm, &choice)?;
        // If the chosen service fails, the realm's own connections take over.
        let mut order = vec![chosen];
        order.extend(connections.order(Tier::Strong));
        Ok(Routed::new(
            order,
            if switched { state.model_len() } else { 0 },
        ))
    }

    /// Every model connection of a realm: its own provider settings if it
    /// has some (realm secrets), else the shared ones, else the environment's.
    pub fn connections_for(&self, realm: &Realm) -> Result<Connections> {
        if let Some(connections) = &self.connections {
            return Ok(connections.clone());
        }
        Connections::from_settings(&self.settings(realm)?)
    }

    /// The model a drive of `state` uses. With a fast connection, the latest
    /// ask is triaged: the run stays on the fast model only while asks are
    /// simple. Triage usage is counted in the run.
    fn route(&self, connections: &Connections, state: &mut State) -> Routed {
        if connections.fast.is_some() {
            // Runs from before routing, and skill development, use the main model.
            let current = state
                .tier
                .or((state.iterations > 0).then_some(Tier::Strong));
            let mut tier = current;
            if current != Some(Tier::Strong)
                && let Some(ask) = latest_ask(state)
            {
                let triage = connections.triage(&ask);
                if let Some(r) = triage.response {
                    state.input_tokens += r.input_tokens;
                    state.output_tokens += r.output_tokens;
                    state.usage.push(crate::state::Usage {
                        at: Utc::now(),
                        model: r.model,
                        input: r.input_tokens,
                        output: r.output_tokens,
                        cache_read: r.cache_read_tokens,
                        cache_write: r.cache_write_tokens,
                        purpose: crate::state::Purpose::Triage,
                    });
                }
                tier = Some(triage.tier);
            }
            state.tier = Some(tier.unwrap_or(Tier::Strong));
            // Moving to another model: what the previous one wrote is foreign.
            let foreign = if current.is_some() && current != state.tier {
                state.model_len()
            } else {
                0
            };
            return Routed::new(connections.order(state.tier.unwrap()), foreign);
        }
        Routed::new(connections.order(Tier::Strong), 0)
    }
}

/// The model connections a realm can set (see models/router.rs).
const MODEL_TIERS: [&str; 3] = ["main", "fast", "fallback"];

/// Reasoning effort levels (Anthropic's; OpenAI-style providers take the
/// lower ones).
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The provider, model and effort settings of a tier.
fn tier_keys(tier: &str) -> [String; 3] {
    let prefix = match tier {
        "main" => "AIRTH_".to_string(),
        other => format!("AIRTH_{}_", other.to_uppercase()),
    };
    ["PROVIDER", "MODEL", "EFFORT"].map(|k| format!("{prefix}{k}"))
}
