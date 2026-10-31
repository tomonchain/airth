//! What a realm can be extended with: the catalog, connectors, MCP
//! servers, skills and integrations.

use super::*;

impl App {
    /// The catalog, connector, MCP, skill and integration routes; None when the request is not one of them.
    pub(super) fn extensions_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["catalog"]) => {
                let force = query.split('&').any(|kv| kv == "refresh=1");
                let (index, refreshing) =
                    catalog::index(&self.hub.home().dir().join("catalog.json"), force);
                let skills: Vec<Value> = index
                    .skills
                    .iter()
                    .map(|s| {
                        let (take, skip) = s.installable();
                        json!({
                            "name": s.name,
                            "title": s.title,
                            "description": s.description,
                            "files": take.len(),
                            "skipped": skip.len(),
                        })
                    })
                    .collect();
                json_response(
                    200,
                    json!({
                        "source": format!("https://github.com/{}", catalog::REPO),
                        "fetched_at": index.fetched_at,
                        "error": index.error,
                        "refreshing": refreshing,
                        "skills": skills,
                    }),
                )
            }
            (Method::Get, ["realms", r, "catalog"]) => {
                json_response(200, self.catalog_installed(&self.realm(r)?))
            }
            (Method::Post, ["realms", r, "catalog", name, "install"]) => {
                let realm = self.realm(r)?;
                let (index, _) = catalog::index(&self.hub.home().dir().join("catalog.json"), false);
                let skill = index
                    .skills
                    .into_iter()
                    .find(|s| s.name == *name)
                    .context("this skill is not in the catalog (refresh it?)")?;
                catalog::install_in_background(skill, realm.skills_dir());
                json_response(200, self.catalog_installed(&realm))
            }
            (Method::Delete, ["realms", r, "catalog", name]) => {
                let realm = self.realm(r)?;
                if !catalog::uninstall(&realm.skills_dir(), name)? {
                    return Err(anyhow!("this skill is not installed"));
                }
                json_response(200, self.catalog_installed(&realm))
            }
            (Method::Get, ["realms", r, "connectors"]) => {
                json_response(200, self.connectors(&self.realm(r)?, None))
            }
            (Method::Post, ["realms", r, "connectors", id]) => {
                let body = read_json(request)?;
                let c = connectors::find(id)?;
                let realm = {
                    let _w = self.write_lock();
                    let mut realm = self.realm(r)?;
                    connectors::connect(
                        &mut realm,
                        &c,
                        body["token"].as_str().unwrap_or_default(),
                    )?;
                    realm
                };
                // Prove it works right away (no lock held while waiting on the
                // service); a refused token stays saved, to fix or remove.
                let check = connectors::check(&realm, &c);
                json_response(200, self.connectors(&realm, Some((id, check))))
            }
            (Method::Post, ["realms", r, "connectors", id, "check"]) => {
                let realm = self.realm(r)?;
                let check = connectors::check(&realm, &connectors::find(id)?);
                json_response(200, self.connectors(&realm, Some((id, check))))
            }
            (Method::Delete, ["realms", r, "connectors", id]) => {
                let _w = self.write_lock();
                let mut realm = self.realm(r)?;
                if !connectors::disconnect(&mut realm, &connectors::find(id)?)? {
                    return Err(anyhow!("this connector is not connected"));
                }
                json_response(200, self.connectors(&realm, None))
            }
            (Method::Post, ["realms", r, "connect"]) => {
                // A remote MCP connector: save it, then open its sign-in page.
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let dir = realm.skills_dir();
                let name = body["name"].as_str().unwrap_or_default().trim().to_string();
                {
                    let _w = self.write_lock();
                    McpStore::new(&dir).save_remote(
                        &name,
                        body["url"].as_str().unwrap_or_default(),
                        body["description"].as_str().unwrap_or_default(),
                    )?;
                }
                let redirect = format!("http://127.0.0.1:{}/oauth/callback", self.port);
                match crate::tools::remote::start(&dir, &name, &redirect) {
                    Ok(crate::tools::remote::Started::SignIn(url)) => {
                        let open = url.clone();
                        std::thread::spawn(move || open_url(&open));
                        json_response(200, json!({"sign_in": url}))
                    }
                    Ok(crate::tools::remote::Started::Open) => {
                        self.mcp_connect(&realm, &name)?;
                        json_response(200, json!({"open": true}))
                    }
                    Err(e) => {
                        let message = format!("{e:#}");
                        let _w = self.write_lock();
                        McpStore::new(&dir).update(&name, |s| s.error = Some(message.clone()))?;
                        return Err(e);
                    }
                }
            }
            (Method::Get, ["registry"]) => {
                let search = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("search="))
                    .map(percent_decode)
                    .unwrap_or_default();
                json_response(200, json!(registry_search(&search)?))
            }
            (Method::Get, ["realms", r, "mcp"]) => json_response(200, self.mcp(&self.realm(r)?)?),
            (Method::Post, ["realms", r, "mcp"]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                let name = body["name"].as_str().unwrap_or_default().trim();
                McpStore::new(&realm.skills_dir()).save(
                    name,
                    body["command"].as_str().unwrap_or_default(),
                    body["description"].as_str().unwrap_or_default(),
                )?;
                self.mcp_connect(&realm, name)?;
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Post, ["realms", r, "mcp", name, "connect"]) => {
                let realm = self.realm(r)?;
                self.mcp_connect(&realm, name)?;
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Patch, ["realms", r, "mcp", name]) => {
                let body = read_json(request)?;
                let realm = self.realm(r)?;
                if let Some(enabled) = body["enabled"].as_bool() {
                    McpStore::new(&realm.skills_dir()).update(name, |s| s.enabled = enabled)?;
                }
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Delete, ["realms", r, "mcp", name]) => {
                let realm = self.realm(r)?;
                if !McpStore::new(&realm.skills_dir()).remove(name)? {
                    return Err(anyhow!("no such MCP server"));
                }
                crate::tools::remote::forget_auth(&realm.skills_dir(), name);
                json_response(200, self.mcp(&realm)?)
            }
            (Method::Post, ["realms", r, "integrations"]) => {
                let body = read_json(request)?;
                let mut realm = self.realm(r)?;
                let integration: Integration =
                    serde_json::from_value(body).context("invalid integration")?;
                realm.set_integration(integration)?;
                json_response(200, self.realm_detail(&realm)?)
            }
            (Method::Delete, ["realms", r, "integrations", name]) => {
                let mut realm = self.realm(r)?;
                if !realm.remove_integration(name)? {
                    return Err(anyhow!("no such integration"));
                }
                json_response(200, self.realm_detail(&realm)?)
            }
            _ => return Ok(None),
        }))
    }
}

impl App {
    /// The connectors catalog with each one's state in the realm, and the
    /// result of a connection check when one just ran.
    fn connectors(&self, realm: &Realm, check: Option<(&str, Result<Option<String>>)>) -> Value {
        let list: Vec<Value> = connectors::catalog()
            .iter()
            .map(|c| connectors::status(realm, c))
            .collect();
        let check = check.map(|(id, r)| match r {
            Ok(message) => json!({"id": id, "ok": true, "message": message}),
            Err(e) => json!({"id": id, "ok": false, "message": format!("{e:#}")}),
        });
        json!({
            "connectors": list,
            "check": check,
        })
    }

    /// Catalog skills installed in the realm, those installing, and failures.
    fn catalog_installed(&self, realm: &Realm) -> Value {
        let dir = realm.skills_dir();
        let (installing, errors) = catalog::install_status(&dir);
        json!({
            "installed": catalog::installed(&dir),
            "installing": installing,
            "errors": errors.into_iter().collect::<HashMap<_, _>>(),
        })
    }

    /// The realm's MCP servers, with the names the agent sees for their tools.
    fn mcp(&self, realm: &Realm) -> Result<Value> {
        let servers: Vec<Value> = McpStore::new(&realm.skills_dir())
            .list()?
            .into_iter()
            .map(|s| {
                let tools: Vec<Value> = s
                    .tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "agent_name": crate::tools::mcp::tool_name(&s.name, &t.name),
                            "description": t.description,
                        })
                    })
                    .collect();
                json!({
                    "name": s.name,
                    "command": s.command,
                    "url": s.url,
                    "signed_in": crate::tools::remote::load_auth(&realm.skills_dir(), &s.name)
                        .is_some_and(|a| a.access_token.is_some()),
                    "description": s.description,
                    "enabled": s.enabled,
                    "connecting": s.connecting(),
                    "server_info": s.server_info,
                    "connected_at": s.connected_at,
                    "error": s.error,
                    "tools": tools,
                })
            })
            .collect();
        Ok(json!(servers))
    }

    /// Start a server in the realm's sandbox, with its credentials, exactly as
    /// a run would, and list its tools in the background.
    fn mcp_connect(&self, realm: &Realm, name: &str) -> Result<()> {
        let ws = self
            .hub
            .workspace_for(realm, &State::new("connect an MCP server", None))?;
        crate::tools::mcp::connect_in_background(&realm.skills_dir(), name, ws)
    }
}

/// Search the official MCP Registry for servers that can be connected as
/// connectors: a Streamable HTTP endpoint that needs no API key header (the
/// ones that need sign-in use OAuth).
fn registry_search(search: &str) -> Result<Vec<Value>> {
    let mut url = reqwest::Url::parse("https://registry.modelcontextprotocol.io/v0/servers")?;
    url.query_pairs_mut()
        .append_pair("limit", "60")
        .append_pair("version", "latest");
    if !search.trim().is_empty() {
        url.query_pairs_mut().append_pair("search", search.trim());
    }
    let body: Value = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get(url)
        .send()
        .context("cannot reach the MCP Registry")?
        .json()
        .context("unexpected answer from the MCP Registry")?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in body["servers"].as_array().into_iter().flatten() {
        let s = &entry["server"];
        let Some(name) = s["name"].as_str() else {
            continue;
        };
        let remote = s["remotes"].as_array().into_iter().flatten().find(|r| {
            r["type"] == "streamable-http"
                && !r["headers"]
                    .as_array()
                    .is_some_and(|h| h.iter().any(|h| h["isRequired"] == true))
                && r["url"].as_str().is_some_and(|u| !u.contains('{'))
        });
        let Some(remote) = remote else { continue };
        if !seen.insert(name.to_string()) {
            continue;
        }
        out.push(json!({
            "name": name,
            "title": s["title"].as_str().unwrap_or_else(|| name.rsplit('/').next().unwrap_or(name)),
            "description": s["description"],
            "url": remote["url"],
            "website": s["websiteUrl"].as_str().or(s["repository"]["url"].as_str()),
        }));
    }
    Ok(out)
}
