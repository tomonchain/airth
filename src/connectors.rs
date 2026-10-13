//! The connectors catalog: well-known services, each connected in one step.
//!
//! An HTTP connector is an integration preset (decision 10): the service's
//! official API as base URL, and its authentication header referring to a
//! credential the owner pastes once. Connecting saves that credential in the
//! realm and adds the integration, so runs call the service by name through
//! `http_request`, the token can only ever go to that API, and the model never
//! sees it. A Mac connector (Google, iCloud) needs no token: the account is
//! added to macOS, and runs reach it through the Mac app tools.

use crate::integration::Integration;
use crate::realm::Realm;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Kind {
    Http {
        base_url: &'static str,
        /// The realm credential holding the token.
        secret: &'static str,
        /// The `Authorization`-style header: name, and value with `{}` for the token.
        auth: (&'static str, &'static str),
        /// Other headers every request needs.
        extra: &'static [(&'static str, &'static str)],
        /// A cheap authenticated request that proves the token works.
        #[serde(skip)]
        check: Option<Check>,
        /// Where to create the token.
        token_url: &'static str,
        token_help: &'static str,
    },
    /// Reached through the macOS account and Airth's Mac app tools.
    Mac { setup_url: &'static str },
}

#[derive(Debug, Clone, Copy)]
pub struct Check {
    pub method: &'static str,
    pub path: &'static str,
    pub body: Option<&'static str>,
    /// The JSON answer must have `"ok": true` (Slack answers 200 to bad tokens).
    pub json_ok: bool,
}

const fn get(path: &'static str) -> Option<Check> {
    Some(Check {
        method: "GET",
        path,
        body: None,
        json_ok: false,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Connector {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    /// What runs can do with it, for the owner.
    pub description: &'static str,
    /// How to use the API, for the agent (the integration's description).
    #[serde(skip)]
    pub agent_hint: &'static str,
    pub docs: &'static str,
    #[serde(flatten)]
    pub kind: Kind,
}

/// The catalog. A data table, kept one field group per line for reading.
#[rustfmt::skip]
pub fn catalog() -> Vec<Connector> {
    use Kind::*;
    vec![
        Connector {
            id: "google", name: "Google", category: "Productivity",
            description: "Google Calendar, Gmail and Contacts, through your Mac: add the Google account in System Settings → Internet Accounts, and runs use it with the Calendar, Reminders, Notes and Mail tools (Mail only prepares drafts).",
            agent_hint: "", docs: "https://support.apple.com/guide/mac-help/add-an-internet-account-mchl1b6b2a9b/mac",
            kind: Mac { setup_url: "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension" },
        },
        Connector {
            id: "icloud", name: "iCloud", category: "Productivity",
            description: "iCloud Calendar, Reminders, Notes and Mail, through your Mac and Airth's Mac app tools, under this realm's apps rule.",
            agent_hint: "", docs: "https://support.apple.com/guide/mac-help/add-an-internet-account-mchl1b6b2a9b/mac",
            kind: Mac { setup_url: "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension" },
        },
        Connector {
            id: "notion", name: "Notion", category: "Productivity",
            description: "Search, read and write pages and databases the integration is shared with.",
            agent_hint: "Notion API (Notion-Version header already set): POST v1/search, GET v1/pages/{id}, GET v1/blocks/{id}/children, POST v1/pages, PATCH v1/blocks/{id}/children, POST v1/databases/{id}/query.",
            docs: "https://developers.notion.com/reference/intro",
            kind: Http {
                base_url: "https://api.notion.com", secret: "NOTION_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[("Notion-Version", "2022-06-28")], check: get("/v1/users/me"),
                token_url: "https://www.notion.so/profile/integrations",
                token_help: "Create an internal integration, copy its secret, then share the pages it may use with it (••• → Connections).",
            },
        },
        Connector {
            id: "linear", name: "Linear", category: "Product",
            description: "Read and create issues, projects and comments.",
            agent_hint: "Linear GraphQL API: POST graphql with a JSON body {\"query\": \"...\", \"variables\": {...}}, e.g. { viewer { id name } }, issues, issueCreate.",
            docs: "https://linear.app/developers/graphql",
            kind: Http {
                base_url: "https://api.linear.app", secret: "LINEAR_API_KEY", auth: ("Authorization", "{}"),
                extra: &[("Content-Type", "application/json")],
                check: Some(Check { method: "POST", path: "/graphql", body: Some(r#"{"query":"{ viewer { id } }"}"#), json_ok: false }),
                token_url: "https://linear.app/settings/account/security",
                token_help: "Settings → Security & access → Personal API keys → New key.",
            },
        },
        Connector {
            id: "github", name: "GitHub", category: "Development",
            description: "Repositories, issues, pull requests and releases.",
            agent_hint: "GitHub REST API: GET user, GET repos/{owner}/{repo}/issues, POST repos/{owner}/{repo}/issues, GET search/issues?q=...",
            docs: "https://docs.github.com/rest",
            kind: Http {
                base_url: "https://api.github.com", secret: "GITHUB_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[("Accept", "application/vnd.github+json"), ("X-GitHub-Api-Version", "2022-11-28"), ("User-Agent", "airth")],
                check: get("/user"),
                token_url: "https://github.com/settings/personal-access-tokens",
                token_help: "Fine-grained token, limited to the repositories and permissions this realm needs.",
            },
        },
        Connector {
            id: "slack", name: "Slack", category: "Communication",
            description: "Read channels and post messages as your Slack app.",
            agent_hint: "Slack Web API: GET conversations.list, GET conversations.history?channel=..., POST chat.postMessage with JSON {\"channel\", \"text\"}. Answers carry \"ok\".",
            docs: "https://api.slack.com/methods",
            kind: Http {
                base_url: "https://slack.com/api", secret: "SLACK_BOT_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[],
                check: Some(Check { method: "GET", path: "auth.test", body: None, json_ok: true }),
                token_url: "https://api.slack.com/apps",
                token_help: "Create an app, add bot scopes (e.g. chat:write, channels:read), install it, copy the Bot User OAuth Token (xoxb-…).",
            },
        },
        Connector {
            id: "discord", name: "Discord", category: "Communication",
            description: "Post and read messages in your server through a bot.",
            agent_hint: "Discord API v10: GET v10/users/@me, GET v10/channels/{id}/messages, POST v10/channels/{id}/messages with JSON {\"content\"}.",
            docs: "https://discord.com/developers/docs/reference",
            kind: Http {
                base_url: "https://discord.com/api", secret: "DISCORD_BOT_TOKEN", auth: ("Authorization", "Bot {}"),
                extra: &[], check: get("/v10/users/@me"),
                token_url: "https://discord.com/developers/applications",
                token_help: "Application → Bot → Reset Token, then invite the bot to your server.",
            },
        },
        Connector {
            id: "stripe", name: "Stripe", category: "Business",
            description: "Payments, customers, invoices and balance.",
            agent_hint: "Stripe REST API: GET v1/balance, GET v1/charges?limit=10, GET v1/customers. POST bodies are form-encoded (application/x-www-form-urlencoded).",
            docs: "https://docs.stripe.com/api",
            kind: Http {
                base_url: "https://api.stripe.com", secret: "STRIPE_SECRET_KEY", auth: ("Authorization", "Bearer {}"),
                extra: &[], check: get("/v1/balance"),
                token_url: "https://dashboard.stripe.com/apikeys",
                token_help: "Prefer a restricted key with only the permissions this realm needs (read-only to start).",
            },
        },
        Connector {
            id: "lemonsqueezy", name: "Lemon Squeezy", category: "Business",
            description: "Store, products, orders and subscriptions.",
            agent_hint: "Lemon Squeezy JSON:API: GET v1/users/me, GET v1/stores, GET v1/orders?filter[store_id]=..., GET v1/products.",
            docs: "https://docs.lemonsqueezy.com/api",
            kind: Http {
                base_url: "https://api.lemonsqueezy.com", secret: "LEMONSQUEEZY_API_KEY", auth: ("Authorization", "Bearer {}"),
                extra: &[("Accept", "application/vnd.api+json"), ("Content-Type", "application/vnd.api+json")],
                check: get("/v1/users/me"),
                token_url: "https://app.lemonsqueezy.com/settings/api",
                token_help: "Settings → API → create an API key.",
            },
        },
        Connector {
            id: "qonto", name: "Qonto", category: "Business",
            description: "Bank account balance and transactions (read).",
            agent_hint: "Qonto API v2: GET v2/organization (accounts and balances), GET v2/transactions?bank_account_id=...",
            docs: "https://api-doc.qonto.com",
            kind: Http {
                base_url: "https://thirdparty.qonto.com", secret: "QONTO_API_KEY", auth: ("Authorization", "{}"),
                extra: &[], check: get("/v2/organization"),
                token_url: "https://app.qonto.com",
                token_help: "Settings → Integrations & partnerships → API key. Paste it as login:secret-key.",
            },
        },
        Connector {
            id: "brevo", name: "Brevo", category: "Marketing",
            description: "Contacts, lists and email campaigns.",
            agent_hint: "Brevo API v3: GET v3/account, GET v3/contacts, POST v3/contacts, GET v3/emailCampaigns.",
            docs: "https://developers.brevo.com/reference",
            kind: Http {
                base_url: "https://api.brevo.com", secret: "BREVO_API_KEY", auth: ("api-key", "{}"),
                extra: &[], check: get("/v3/account"),
                token_url: "https://app.brevo.com/settings/keys/api",
                token_help: "SMTP & API → API keys → Generate a new API key.",
            },
        },
        Connector {
            id: "hubspot", name: "HubSpot", category: "Marketing",
            description: "CRM contacts, companies and deals.",
            agent_hint: "HubSpot CRM API v3: GET crm/v3/objects/contacts?limit=10, POST crm/v3/objects/contacts/search, GET crm/v3/objects/deals.",
            docs: "https://developers.hubspot.com/docs/api/overview",
            kind: Http {
                base_url: "https://api.hubapi.com", secret: "HUBSPOT_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[], check: get("/crm/v3/objects/contacts?limit=1"),
                token_url: "https://app.hubspot.com",
                token_help: "Settings → Integrations → Private apps → create one with the CRM scopes you need, copy its access token.",
            },
        },
        Connector {
            id: "airtable", name: "Airtable", category: "Productivity",
            description: "Read and update records in your bases.",
            agent_hint: "Airtable Web API: GET v0/meta/bases, GET v0/{baseId}/{table}, POST v0/{baseId}/{table} with JSON {\"records\": [...]}.",
            docs: "https://airtable.com/developers/web/api/introduction",
            kind: Http {
                base_url: "https://api.airtable.com", secret: "AIRTABLE_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[], check: get("/v0/meta/whoami"),
                token_url: "https://airtable.com/create/tokens",
                token_help: "Personal access token with the scopes and bases this realm needs.",
            },
        },
        Connector {
            id: "calendly", name: "Calendly", category: "Productivity",
            description: "Scheduled events and invitees.",
            agent_hint: "Calendly API v2: GET users/me (your user URI), GET scheduled_events?user={uri}.",
            docs: "https://developer.calendly.com/api-docs",
            kind: Http {
                base_url: "https://api.calendly.com", secret: "CALENDLY_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[], check: get("/users/me"),
                token_url: "https://calendly.com/integrations/api_webhooks",
                token_help: "Integrations → API & Webhooks → generate a personal access token.",
            },
        },
        Connector {
            id: "figma", name: "Figma", category: "Design",
            description: "Read files, frames and comments.",
            agent_hint: "Figma REST API: GET v1/me, GET v1/files/{key}, GET v1/files/{key}/comments.",
            docs: "https://www.figma.com/developers/api",
            kind: Http {
                base_url: "https://api.figma.com", secret: "FIGMA_TOKEN", auth: ("X-Figma-Token", "{}"),
                extra: &[], check: get("/v1/me"),
                token_url: "https://www.figma.com/settings",
                token_help: "Settings → Security → Personal access tokens.",
            },
        },
        Connector {
            id: "vercel", name: "Vercel", category: "Development",
            description: "Projects, deployments and domains.",
            agent_hint: "Vercel REST API: GET v2/user, GET v9/projects, GET v6/deployments?projectId=...",
            docs: "https://vercel.com/docs/rest-api",
            kind: Http {
                base_url: "https://api.vercel.com", secret: "VERCEL_TOKEN", auth: ("Authorization", "Bearer {}"),
                extra: &[], check: get("/v2/user"),
                token_url: "https://vercel.com/account/tokens",
                token_help: "Account settings → Tokens → create one scoped to the team you use.",
            },
        },
        Connector {
            id: "resend", name: "Resend", category: "Communication",
            description: "Send transactional emails from your domain.",
            agent_hint: "Resend API: POST emails with JSON {\"from\", \"to\", \"subject\", \"html\" or \"text\"}.",
            docs: "https://resend.com/docs/api-reference/introduction",
            kind: Http {
                base_url: "https://api.resend.com", secret: "RESEND_API_KEY", auth: ("Authorization", "Bearer {}"),
                extra: &[], check: None, // sending-only keys cannot call anything harmless
                token_url: "https://resend.com/api-keys",
                token_help: "API Keys → Create (Sending access is enough to send).",
            },
        },
    ]
}

pub fn find(id: &str) -> Result<Connector> {
    catalog()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| anyhow!("no connector named {id}"))
}

/// The integration a connector adds (named after it).
pub fn integration_for(c: &Connector) -> Result<Integration> {
    let Kind::Http {
        base_url,
        secret,
        auth,
        extra,
        ..
    } = &c.kind
    else {
        bail!("{} connects through macOS, not with a token", c.name);
    };
    let mut headers = BTreeMap::new();
    headers.insert(
        auth.0.to_string(),
        auth.1.replace("{}", &format!("[secret:{secret}]")),
    );
    for (k, v) in extra.iter() {
        headers.insert(k.to_string(), v.to_string());
    }
    let integration = Integration {
        name: c.id.to_string(),
        description: format!(
            "{} ({}). {} Docs: {}",
            c.name, c.description, c.agent_hint, c.docs
        ),
        base_url: base_url.to_string(),
        headers,
    };
    integration.validate()?;
    Ok(integration)
}

/// What a connector looks like in a realm: connected or not.
pub fn status(realm: &Realm, c: &Connector) -> Value {
    let connected = match &c.kind {
        Kind::Http { secret, .. } => {
            realm.config.integrations.iter().any(|i| i.name == c.id)
                && realm.secret(secret).is_some()
        }
        Kind::Mac { .. } => false,
    };
    json!({"connector": c, "connected": connected})
}

/// Save the token and the integration.
pub fn connect(realm: &mut Realm, c: &Connector, token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        bail!("paste the token first");
    }
    if token.contains(['\n', '\r']) {
        bail!("a token fits on one line");
    }
    let integration = integration_for(c)?;
    let Kind::Http { secret, .. } = &c.kind else {
        bail!("{} connects through macOS, not with a token", c.name);
    };
    realm.set_secret(secret, token)?;
    realm.set_integration(integration)
}

/// Remove the integration, and its credential unless another integration uses it.
pub fn disconnect(realm: &mut Realm, c: &Connector) -> Result<bool> {
    let removed = realm.remove_integration(c.id)?;
    if let Kind::Http { secret, .. } = &c.kind {
        let still_used = realm
            .config
            .integrations
            .iter()
            .any(|i| i.secret_names().iter().any(|s| s == secret));
        if !still_used {
            realm.remove_secret(secret)?;
        }
    }
    Ok(removed)
}

/// Prove the token works with one harmless authenticated request. Ok(None)
/// when the connector has no such request.
pub fn check(realm: &Realm, c: &Connector) -> Result<Option<String>> {
    let Kind::Http {
        check: Some(check), ..
    } = &c.kind
    else {
        return Ok(None);
    };
    let integration = integration_for(c)?;
    let url = integration.url_for(check.path)?;
    let headers = integration.resolve_headers(&|k| realm.secret(k))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut req = match check.method {
        "POST" => client.post(url),
        _ => client.get(url),
    };
    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(body) = check.body {
        req = req.body(body);
    }
    let resp = req
        .send()
        .with_context(|| format!("cannot reach {}", c.name))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        bail!("{} answered {status}", c.name);
    }
    if check.json_ok {
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if v["ok"] != json!(true) {
            bail!(
                "{} refused the token: {}",
                c.name,
                v["error"].as_str().unwrap_or("unknown error")
            );
        }
    }
    Ok(Some(format!("{} accepted the token", c.name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_connector_makes_a_valid_integration() {
        let all = catalog();
        let mut ids: Vec<_> = all.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "ids are unique");
        for c in &all {
            match &c.kind {
                Kind::Http { secret, check, .. } => {
                    let i = integration_for(c).unwrap();
                    assert_eq!(i.name, c.id);
                    assert_eq!(i.secret_names(), vec![secret.to_string()], "{}", c.id);
                    assert!(
                        secret
                            .chars()
                            .all(|ch| ch.is_ascii_uppercase() || ch == '_')
                    );
                    if let Some(check) = check {
                        i.url_for(check.path).unwrap();
                    }
                }
                Kind::Mac { .. } => assert!(integration_for(c).is_err()),
            }
        }
    }

    #[test]
    fn the_token_goes_in_the_service_header_only() {
        let linear = integration_for(&find("linear").unwrap()).unwrap();
        let headers = linear
            .resolve_headers(&|k| (k == "LINEAR_API_KEY").then(|| "lin_api_x".to_string()))
            .unwrap();
        assert!(headers.contains(&("Authorization".into(), "lin_api_x".into())));
        let brevo = integration_for(&find("brevo").unwrap()).unwrap();
        assert_eq!(brevo.headers["api-key"], "[secret:BREVO_API_KEY]");
        assert!(linear.url_for("https://evil.example/graphql").is_err());
    }
}
