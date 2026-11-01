//! What connectors may be asked to do, as the founder declared it (decisions
//! 45 and 49). Declarations live in the Airth home, with each connector's
//! record, out of every run's reach:
//!
//! - an integration declares its actions: method, path under its base URL,
//!   fields, risk class;
//! - each tool of an MCP server is an action; the founder confirms its class,
//!   and the class holds only while the tool's input schema is the one the
//!   founder saw (its hash is kept with the class);
//! - Airth's own actions are declared here, in code.
//!
//! An action with no confirmed class is `commits`: what nobody classed may
//! commit the business, so it never acts alone.

use crate::integration::Integration;
use crate::realm::validate_name;
use crate::tools::mcp::McpServer;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How serious an action is; it sets the highest autonomy level it can get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    Read,
    /// A write that can be undone: a draft, a label, a scheduled post.
    Reversible,
    /// Money, a contract, a deadline, a legal obligation, anything published
    /// in the business's name.
    Commits,
}

impl Class {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "read" => Class::Read,
            "reversible" => Class::Reversible,
            "commits" => Class::Commits,
            _ => bail!("unknown risk class '{s}' (read, reversible, commits)"),
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Read => "read",
            Class::Reversible => "reversible",
            Class::Commits => "commits",
        }
    }

    /// The highest level the class allows (decision 45).
    pub fn ceiling(self) -> u8 {
        match self {
            Class::Read | Class::Reversible => 3,
            Class::Commits => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    Path,
    Query,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    pub place: Place,
}

/// One call an integration may make, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpAction {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub method: String,
    /// Under the integration's base URL, with `{field}` placeholders.
    pub path: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// None until the founder classes it: then it is `commits`.
    #[serde(default)]
    pub class: Option<Class>,
    /// A ceiling below the class's, for a fixed rule (an X reply: 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_minutes: Option<u32>,
}

impl HttpAction {
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name).context("invalid action name")?;
        if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&self.method.as_str()) {
            bail!(
                "{}: method '{}' is not GET, POST, PUT, PATCH or DELETE",
                self.name,
                self.method
            );
        }
        if !self.path.starts_with('/') || self.path.contains("..") || self.path.contains("://") {
            bail!(
                "{}: the path must start with / and stay under the base URL",
                self.name
            );
        }
        let placeholders: Vec<&str> = self
            .path
            .split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(name, _)| name))
            .collect();
        for (i, f) in self.fields.iter().enumerate() {
            validate_name(&f.name).with_context(|| format!("{}: invalid field name", self.name))?;
            if self.fields[..i].iter().any(|g| g.name == f.name) {
                bail!("{}: field '{}' is declared twice", self.name, f.name);
            }
            if (f.place == Place::Path) != placeholders.contains(&f.name.as_str()) {
                bail!(
                    "{}: field '{}' must be a {{placeholder}} of the path exactly when it goes in the path",
                    self.name,
                    f.name
                );
            }
        }
        if let Some(p) = placeholders
            .iter()
            .find(|p| !self.fields.iter().any(|f| f.name == **p))
        {
            bail!("{}: the path's {{{p}}} has no field", self.name);
        }
        if self.max_level.is_some_and(|l| l > 3) {
            bail!("{}: a level is 0 to 3", self.name);
        }
        Ok(())
    }
}

impl HttpAction {
    /// Whether a request is this action: same method, and a path whose
    /// segments are the template's, a `{placeholder}` standing for one.
    pub fn matches(&self, method: &str, path: &str) -> bool {
        let want: Vec<&str> = self.path.trim_matches('/').split('/').collect();
        let got: Vec<&str> = path.trim_matches('/').split('/').collect();
        self.method.eq_ignore_ascii_case(method)
            && want.len() == got.len()
            && want
                .iter()
                .zip(&got)
                .all(|(w, g)| (w.starts_with('{') && w.ends_with('}') && !g.is_empty()) || w == g)
    }
}

/// The class the founder gave an MCP tool, for the schema they saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolClass {
    pub class: Class,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_level: Option<u8>,
    /// SHA-256 of the tool's input schema when it was classed.
    pub schema: String,
    pub confirmed_at: DateTime<Utc>,
}

/// The hash of a tool's input schema: keys sorted, so the same schema
/// always hashes the same.
pub fn schema_hash(schema: &Value) -> String {
    crate::update::sha256_hex(serde_json::to_string(schema).unwrap_or_default().as_bytes())
}

/// Airth's own actions (decision 49), with their class.
const AIRTH: &[(&str, Class)] = &[
    ("launch_mission", Class::Reversible),
    ("report", Class::Reversible),
    ("create_task", Class::Reversible),
    // Merging code never goes above 1 (`CLAUDE.md`).
    ("adopt_change", Class::Commits),
];

/// An action as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// `<connector>.<action>`.
    pub id: String,
    pub class: Class,
    pub ceiling: u8,
    /// Whether the founder classed it (always, for Airth's own).
    pub confirmed: bool,
}

fn declared(id: String, class: Option<Class>, max_level: Option<u8>) -> Declared {
    let confirmed = class.is_some();
    let class = class.unwrap_or(Class::Commits);
    let ceiling = max_level.map_or(class.ceiling(), |m| m.min(class.ceiling()));
    Declared {
        id,
        class,
        ceiling,
        confirmed,
    }
}

/// Every action of a realm: Airth's, each integration's, each MCP tool's.
pub fn all(integrations: &[Integration], servers: &[McpServer]) -> Vec<Declared> {
    let mut out: Vec<Declared> = AIRTH
        .iter()
        .map(|(name, class)| declared(format!("airth.{name}"), Some(*class), None))
        .collect();
    for i in integrations {
        for a in &i.actions {
            out.push(declared(
                format!("{}.{}", i.name, a.name),
                a.class,
                a.max_level,
            ));
        }
    }
    for s in servers {
        for tool in &s.tools {
            // A class confirmed for another schema no longer holds: the tool
            // may have been widened since.
            let c = s
                .classes
                .get(&tool.name)
                .filter(|c| c.schema == schema_hash(&tool.input_schema));
            out.push(declared(
                format!("{}.{}", s.name, tool.name),
                c.map(|c| c.class),
                c.and_then(|c| c.max_level),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::mcp::McpToolSpec;
    use serde_json::json;

    fn action(path: &str, fields: Value) -> HttpAction {
        serde_json::from_value(json!({
            "name": "create_draft", "method": "POST", "path": path, "fields": fields
        }))
        .unwrap()
    }

    #[test]
    fn an_http_action_is_checked_when_declared() {
        let ok = action(
            "/v1/drafts/{social_set}",
            json!([
                {"name": "social_set", "place": "path", "required": true},
                {"name": "content", "place": "body", "required": true}
            ]),
        );
        assert!(ok.validate().is_ok());
        let bad = [
            (action("v1/drafts", json!([])), "start with /"),
            (action("/v1/../admin", json!([])), "start with /"),
            (action("https://evil.example/x", json!([])), "start with /"),
            (action("/v1/{id}", json!([])), "has no field"),
            (
                action("/v1/x", json!([{"name": "id", "place": "path"}])),
                "placeholder",
            ),
            (
                action("/v1/{id}", json!([{"name": "id", "place": "body"}])),
                "placeholder",
            ),
            (
                action(
                    "/v1/x",
                    json!([{"name": "a", "place": "body"}, {"name": "a", "place": "query"}]),
                ),
                "twice",
            ),
        ];
        for (a, why) in bad {
            let err = format!("{:#}", a.validate().unwrap_err());
            assert!(err.contains(why), "{}: {err}", a.path);
        }
        let mut get = ok.clone();
        get.method = "TRACE".into();
        assert!(get.validate().is_err());
    }

    #[test]
    fn an_unclassed_action_commits_and_a_changed_tool_must_be_classed_again() {
        let mut publish = action("/v1/posts", json!([]));
        publish.name = "publish".into();
        let mut draft = action("/v1/drafts", json!([]));
        draft.class = Some(Class::Reversible);
        let mut reply = action("/v1/replies", json!([]));
        reply.name = "reply".into();
        reply.class = Some(Class::Reversible);
        reply.max_level = Some(0);
        let integration = Integration {
            name: "typefully".into(),
            description: String::new(),
            base_url: "https://api.typefully.com".into(),
            headers: Default::default(),
            actions: vec![publish, draft, reply],
        };
        let schema = json!({"type": "object", "properties": {"q": {"type": "string"}}});
        let mut server: McpServer = serde_json::from_value(json!({"name": "stripe"})).unwrap();
        server.tools = vec![McpToolSpec {
            name: "refund".into(),
            description: String::new(),
            input_schema: schema.clone(),
        }];
        server.classes.insert(
            "refund".into(),
            ToolClass {
                class: Class::Read,
                max_level: None,
                schema: schema_hash(&schema),
                confirmed_at: Utc::now(),
            },
        );
        let find = |all: &[Declared], id: &str| all.iter().find(|d| d.id == id).cloned().unwrap();
        let all1 = all(
            std::slice::from_ref(&integration),
            std::slice::from_ref(&server),
        );
        assert_eq!(find(&all1, "airth.adopt_change").ceiling, 1);
        assert_eq!(find(&all1, "airth.launch_mission").ceiling, 3);
        let p = find(&all1, "typefully.publish");
        assert_eq!(
            (p.class, p.ceiling, p.confirmed),
            (Class::Commits, 1, false)
        );
        assert_eq!(find(&all1, "typefully.create_draft").ceiling, 3);
        assert_eq!(find(&all1, "typefully.reply").ceiling, 0);
        assert_eq!(find(&all1, "stripe.refund").class, Class::Read);
        // The server widens the tool: the founder's class no longer holds.
        server.tools[0].input_schema =
            json!({"type": "object", "properties": {"q": {}, "amount": {}}});
        let r = find(&all(&[integration], &[server]), "stripe.refund");
        assert_eq!(
            (r.class, r.ceiling, r.confirmed),
            (Class::Commits, 1, false)
        );
    }
}
