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
    /// A value fixed by the declaration: whatever a routine or an agent
    /// gives, this is what is sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
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
    /// The JSON body, as a template: a string `"{field}"` is where a body
    /// field goes. Only this goes in the body, so nothing undeclared (a
    /// date that would publish, say) can be added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    /// The header in which the service takes an idempotency key: Airth sends
    /// the action's key in it, the same on every attempt, so a write can be
    /// tried again after any failure of the service (decision 55).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_header: Option<String>,
}

/// The field a template string stands for: `"{text}"`.
fn slot(v: &Value) -> Option<&str> {
    v.as_str()?.strip_prefix('{')?.strip_suffix('}')
}

/// Every slot of a body template.
fn slots(template: &Value, out: &mut Vec<String>) {
    match template {
        Value::Object(map) => map.values().for_each(|v| slots(v, out)),
        Value::Array(list) => list.iter().for_each(|v| slots(v, out)),
        v => out.extend(slot(v).map(String::from)),
    }
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
        if let Some(template) = &self.body {
            let mut found = Vec::new();
            slots(template, &mut found);
            for f in self.fields.iter().filter(|f| f.place == Place::Body) {
                if !found.contains(&f.name) {
                    bail!(
                        "{}: body field '{}' has no {{{}}} in the body",
                        self.name,
                        f.name,
                        f.name
                    );
                }
            }
            if let Some(stray) = found.iter().find(|s| {
                !self
                    .fields
                    .iter()
                    .any(|f| &f.name == *s && f.place == Place::Body)
            }) {
                bail!("{}: the body's {{{stray}}} is not a body field", self.name);
            }
        }
        if self.max_level.is_some_and(|l| l > 3) {
            bail!("{}: a level is 0 to 3", self.name);
        }
        Ok(())
    }
}

impl HttpAction {
    /// The URL this action calls with `values` in its placeholders, checked
    /// under the integration's base (decision 49). A value is encoded, and
    /// refused if it holds `/` or is `.` or `..`: a field drafted from
    /// outside content cannot walk the request to another endpoint.
    pub fn url(
        &self,
        integration: &Integration,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<reqwest::Url> {
        let mut path = String::new();
        let mut rest = self.path.as_str();
        while let Some(open) = rest.find('{') {
            let close = rest[open..]
                .find('}')
                .context("a placeholder is not closed")?
                + open;
            let name = &rest[open + 1..close];
            let value = self
                .fixed(name)
                .or(values.get(name))
                .with_context(|| format!("{}: no value for {{{name}}}", self.name))?;
            if value.contains('/') || value == "." || value == ".." || value.is_empty() {
                bail!("{}: '{value}' cannot go in {{{name}}}", self.name);
            }
            path.push_str(&rest[..open]);
            for b in value.bytes() {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    path.push(b as char);
                } else {
                    path.push_str(&format!("%{b:02X}"));
                }
            }
            rest = &rest[close + 1..];
        }
        path.push_str(rest);
        integration.url_for(&path)
    }

    /// Whether a request is this action: same method, and a path whose
    /// segments are the template's, a `{placeholder}` standing for one.
    pub fn matches(&self, method: &str, path: &str) -> bool {
        let want: Vec<&str> = self.path.trim_matches('/').split('/').collect();
        let got: Vec<&str> = path.trim_matches('/').split('/').collect();
        let segment =
            |w: &&str, g: &&str| match w.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
                // A fixed value is the only one that matches.
                Some(name) => self.fixed(name).map_or(!g.is_empty(), |v| v == g),
                None => w == g,
            };
        self.method.eq_ignore_ascii_case(method)
            && want.len() == got.len()
            && want.iter().zip(&got).all(|(w, g)| segment(w, g))
    }

    /// The value the declaration fixes for `field`, if any.
    fn fixed(&self, field: &str) -> Option<&String> {
        self.fields.iter().find(|f| f.name == field)?.value.as_ref()
    }

    /// The body to send: the template with each slot filled from `values`
    /// (or the declaration's fixed value). None without a template.
    pub fn body(
        &self,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<Option<Value>> {
        fn fill(
            v: &Value,
            a: &HttpAction,
            values: &std::collections::BTreeMap<String, String>,
        ) -> Result<Value> {
            Ok(match v {
                Value::Object(map) => Value::Object(
                    map.iter()
                        .map(|(k, v)| Ok((k.clone(), fill(v, a, values)?)))
                        .collect::<Result<_>>()?,
                ),
                Value::Array(list) => Value::Array(
                    list.iter()
                        .map(|v| fill(v, a, values))
                        .collect::<Result<_>>()?,
                ),
                v => match slot(v) {
                    Some(name) => Value::String(
                        a.fixed(name)
                            .or(values.get(name))
                            .with_context(|| format!("{}: no value for {name}", a.name))?
                            .clone(),
                    ),
                    None => v.clone(),
                },
            })
        }
        self.body
            .as_ref()
            .map(|t| fill(t, self, values))
            .transpose()
    }

    /// Whether `body` has exactly the template's shape: the same keys and
    /// lengths, the template's own values, a string in each slot (the fixed
    /// value where there is one). Anything added is refused.
    pub fn accepts_body(&self, body: &Value) -> bool {
        fn same(t: &Value, b: &Value, a: &HttpAction) -> bool {
            match (t, b) {
                (Value::Object(t), Value::Object(b)) => {
                    t.len() == b.len()
                        && t.iter()
                            .all(|(k, v)| b.get(k).is_some_and(|w| same(v, w, a)))
                }
                (Value::Array(t), Value::Array(b)) => {
                    t.len() == b.len() && t.iter().zip(b).all(|(v, w)| same(v, w, a))
                }
                (t, b) => match slot(t) {
                    Some(name) => {
                        b.is_string() && a.fixed(name).is_none_or(|f| b.as_str() == Some(f))
                    }
                    None => t == b,
                },
            }
        }
        self.body.as_ref().is_none_or(|t| same(t, body, self))
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

/// Airth's own actions (decision 49), with their class and the level they
/// start at.
const AIRTH: &[(&str, Class)] = &[
    ("launch_mission", Class::Reversible),
    ("report", Class::Read),
    // An idea told to the founder (decision 119): nothing is done.
    ("suggest", Class::Read),
    ("create_task", Class::Reversible),
    // Merging code never goes above 1 (`CLAUDE.md`).
    ("adopt_change", Class::Commits),
    // The interlocutor's plan for the team (decisions 120 and 137).
    ("dispatch", Class::Reversible),
];

/// Airth's actions that start below their ceiling: a plan was always put to
/// the founder (decision 120), so it stays so until the founder raises it
/// (decision 137).
const STARTS_AT_ONE: &[&str] = &["dispatch"];

/// An action as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// `<connector>.<action>`.
    pub id: String,
    pub class: Class,
    pub ceiling: u8,
    /// Whether the founder classed it (always, for Airth's own).
    pub confirmed: bool,
    /// Its level before the founder sets one: its ceiling (decision 96).
    pub start: u8,
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
        // Reads and reversible writes are the agent's job: it acts, then
        // reports. What commits, or was never classed, waits for the founder
        // (decision 96).
        start: ceiling,
    }
}

/// Every action of a realm: Airth's, each integration's, each MCP tool's.
pub fn all(integrations: &[Integration], servers: &[McpServer]) -> Vec<Declared> {
    let mut out: Vec<Declared> = AIRTH
        .iter()
        .map(|(name, class)| {
            let mut d = declared(format!("airth.{name}"), Some(*class), None);
            if STARTS_AT_ONE.contains(name) {
                d.start = 1;
            }
            d
        })
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

    /// Decision 49: a value in a path placeholder cannot leave the action's
    /// endpoint.
    #[test]
    fn a_placeholder_value_is_encoded_and_cannot_walk_away() {
        let a = action(
            "/v1/sets/{set}/drafts",
            json!([{"name": "set", "place": "path"}]),
        );
        let i = Integration {
            free_reads: false,
            name: "typefully".into(),
            description: String::new(),
            base_url: "https://api.typefully.com".into(),
            headers: Default::default(),
            actions: vec![a.clone()],
        };
        let url = |v: &str| a.url(&i, &[("set".to_string(), v.to_string())].into());
        assert_eq!(
            url("acme 2").unwrap().as_str(),
            "https://api.typefully.com/v1/sets/acme%202/drafts"
        );
        assert_eq!(url("a%2Fb").unwrap().path(), "/v1/sets/a%252Fb/drafts");
        for bad in ["../admin", "..", ".", "a/b", "https://evil.example/x", ""] {
            assert!(url(bad).is_err(), "{bad}");
        }
        assert!(a.url(&i, &Default::default()).is_err());
    }

    /// Typefully v2's draft, as the founder declares it: the social set
    /// fixed, the post's text the only thing filled.
    fn typefully_draft() -> HttpAction {
        serde_json::from_value(json!({
            "name": "create_draft", "method": "POST", "class": "reversible",
            "path": "/v2/social-sets/{social_set_id}/drafts",
            "fields": [
                {"name": "social_set_id", "place": "path", "value": "123"},
                {"name": "text", "place": "body", "required": true}
            ],
            "body": {"platforms": {"x": {"enabled": true, "posts": [{"text": "{text}"}]}}}
        }))
        .unwrap()
    }

    /// A draft never carries `publish_at`, which would publish it: that is
    /// what makes `create_draft` reversible. Nor another social set.
    #[test]
    fn a_body_template_sends_only_what_it_declares() {
        let a = typefully_draft();
        a.validate().unwrap();
        // A routine's fields, say after an approval, with more than declared.
        let values: std::collections::BTreeMap<String, String> = [
            ("text", "ETH left the queue."),
            ("publish_at", "now"),
            ("social_set_id", "999"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .into();
        assert_eq!(
            a.body(&values).unwrap().unwrap(),
            json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "ETH left the queue."}]}}})
        );
        let i = Integration {
            free_reads: false,
            name: "typefully".into(),
            description: String::new(),
            base_url: "https://api.typefully.com".into(),
            headers: Default::default(),
            actions: vec![a.clone()],
        };
        assert_eq!(
            a.url(&i, &values).unwrap().path(),
            "/v2/social-sets/123/drafts"
        );
        assert!(a.matches("POST", "/v2/social-sets/123/drafts"));
        assert!(!a.matches("POST", "/v2/social-sets/999/drafts"));
        // A body a mission writes itself must be the template's shape.
        let ok = json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "Hi"}]}}});
        assert!(a.accepts_body(&ok));
        for bad in [
            json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "Hi"}]}}, "publish_at": "now"}),
            json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "Hi"}], "publish_at": "now"}}}),
            json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "Hi", "publish_at": "now"}]}}}),
            json!({"platforms": {"x": {"enabled": true, "posts": [{"text": "Hi"}, {"text": "Two"}]}}}),
            json!({"platforms": {"x": {"enabled": false, "posts": [{"text": "Hi"}]}}}),
            json!({"platforms": {"x": {"enabled": true, "posts": [{"text": 5}]}}}),
        ] {
            assert!(!a.accepts_body(&bad), "{bad}");
        }
        // A template and its fields must agree.
        let mut stray = a.clone();
        stray.body = Some(json!({"text": "{text}", "when": "{publish_at}"}));
        assert!(stray.validate().is_err());
        let mut missing = a.clone();
        missing.body = Some(json!({"posts": []}));
        assert!(missing.validate().is_err());
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
            free_reads: false,
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
        let report = find(&all1, "airth.report");
        assert_eq!((report.class, report.start), (Class::Read, 3));
        assert_eq!(find(&all1, "airth.create_task").start, 3);
        assert_eq!(find(&all1, "airth.adopt_change").start, 1);
        assert_eq!(find(&all1, "airth.launch_mission").ceiling, 3);
        let plan = find(&all1, "airth.dispatch");
        assert_eq!((plan.ceiling, plan.start), (3, 1));
        let p = find(&all1, "typefully.publish");
        assert_eq!(
            (p.class, p.ceiling, p.confirmed, p.start),
            (Class::Commits, 1, false, 1)
        );
        let d = find(&all1, "typefully.create_draft");
        assert_eq!((d.ceiling, d.start), (3, 3));
        let reply = find(&all1, "typefully.reply");
        assert_eq!((reply.ceiling, reply.start), (0, 0));
        assert_eq!(find(&all1, "stripe.refund").class, Class::Read);
        // The server widens the tool: the founder's class no longer holds.
        server.tools[0].input_schema =
            json!({"type": "object", "properties": {"q": {}, "amount": {}}});
        let r = find(&all(&[integration], &[server]), "stripe.refund");
        assert_eq!(
            (r.class, r.ceiling, r.confirmed, r.start),
            (Class::Commits, 1, false, 1)
        );
    }
}
