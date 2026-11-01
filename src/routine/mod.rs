//! Routines as graphs of six blocks (decision 49): `routines/<name>.json` in
//! the business folder, read-only to runs (decision 44). A graph is checked
//! whole when it is loaded, and refused, naming the block, if it is wrong:
//! its shape (no cycle, every block reachable), its fields, its prompts and
//! its agent.

use crate::objective::Schedule;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Where a business's routines live, at the root of its folder.
pub const DIR: &str = "routines";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    #[serde(default)]
    pub description: String,
    pub agent: String,
    pub trigger: Trigger,
    pub blocks: Vec<Block>,
}

/// What starts the routine: exactly one of these.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub schedule: Option<String>,
    pub webhook: Option<String>,
    pub manual: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Block {
    pub id: String,
    #[serde(flatten)]
    pub kind: Kind,
    /// Absent: the next block in the list. `null`: the run stops here.
    #[serde(default, deserialize_with = "explicit")]
    pub next: Option<Option<String>>,
}

/// Keeps `"next": null` apart from no `next` at all.
fn explicit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Kind {
    Read {
        source: String,
        #[serde(default)]
        feeds: Vec<String>,
        url: Option<String>,
        action: Option<String>,
        max_age_days: Option<u32>,
        dedupe: Option<String>,
        max: Option<usize>,
    },
    Ai {
        prompt: String,
        keep: Option<String>,
        rank: Option<String>,
        distinct: Option<String>,
        limit: Option<usize>,
    },
    Condition {
        #[serde(rename = "if")]
        test: Test,
        then: Option<String>,
        #[serde(rename = "else")]
        otherwise: String,
    },
    Approval {
        show: String,
        editable: Option<String>,
        expires_after: Option<u32>,
    },
    Act {
        action: String,
        #[serde(default)]
        fields: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Test {
    pub field: String,
    pub op: String,
    pub value: Option<Value>,
}

const OPS: &[&str] = &["==", "!=", "<", "<=", ">", ">=", "exists"];

/// What a graph is checked against: the business's folder.
pub struct Checks<'a> {
    pub root: &'a Path,
}

/// Load and check `routines/<name>.json`.
pub fn load(cx: &Checks, name: &str) -> Result<Graph> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("a routine's file name is lowercase letters, digits and dashes");
    }
    let path = cx.root.join(DIR).join(format!("{name}.json"));
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {DIR}/{name}.json"))?;
    let graph: Graph = serde_json::from_str(&text)
        .with_context(|| format!("{DIR}/{name}.json is not a valid routine"))?;
    check(cx, &graph).with_context(|| format!("{DIR}/{name}.json"))?;
    Ok(graph)
}

/// Every routine of a business, by name, each loaded or with why not.
pub fn load_all(cx: &Checks) -> Result<Vec<(String, Result<Graph>)>> {
    let dir = cx.root.join(DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).context("reading routines/"),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".json")
                .map(String::from)
        })
        .collect();
    names.sort();
    Ok(names
        .into_iter()
        .map(|n| {
            let g = load(cx, &n);
            (n, g)
        })
        .collect())
}

/// The blocks an item goes to after block `i`.
fn successors(graph: &Graph, i: usize) -> Result<Vec<usize>> {
    let index = |id: &str| {
        graph
            .blocks
            .iter()
            .position(|b| b.id == id)
            .with_context(|| {
                format!(
                    "block '{}' goes to '{id}', which does not exist",
                    graph.blocks[i].id
                )
            })
    };
    let following = || (i + 1 < graph.blocks.len()).then_some(i + 1);
    Ok(match (&graph.blocks[i].kind, &graph.blocks[i].next) {
        (
            Kind::Condition {
                then, otherwise, ..
            },
            _,
        ) => {
            let then = match then {
                Some(t) => Some(index(t)?),
                None => following(),
            };
            then.into_iter().chain([index(otherwise)?]).collect()
        }
        (_, None) => following().into_iter().collect(),
        (_, Some(None)) => Vec::new(),
        (_, Some(Some(id))) => vec![index(id)?],
    })
}

fn check(cx: &Checks, graph: &Graph) -> Result<()> {
    let t = &graph.trigger;
    // Events (mail received) come with agent mail, at milestone 7.
    let set = [
        t.schedule.is_some(),
        t.webhook.is_some(),
        t.manual == Some(true),
    ];
    if set.iter().filter(|s| **s).count() != 1 {
        bail!("the trigger is one of schedule, webhook or manual");
    }
    if let Some(s) = &t.schedule {
        Schedule::parse(s)?;
    }
    crate::business::job(cx.root, &graph.agent)
        .with_context(|| format!("the agent '{}'", graph.agent))?;
    if graph.blocks.is_empty() {
        bail!("a routine has at least one block");
    }
    for (i, b) in graph.blocks.iter().enumerate() {
        crate::realm::validate_name(&b.id)
            .with_context(|| format!("block {}: invalid id", i + 1))?;
        if graph.blocks[..i].iter().any(|o| o.id == b.id) {
            bail!("block id '{}' is used twice", b.id);
        }
        check_block(cx, b).with_context(|| format!("block '{}'", b.id))?;
    }
    // No cycle, and every block reachable from the first.
    let edges: Vec<Vec<usize>> = (0..graph.blocks.len())
        .map(|i| successors(graph, i))
        .collect::<Result<_>>()?;
    let mut state = vec![0u8; edges.len()];
    fn visit(i: usize, edges: &[Vec<usize>], state: &mut [u8], graph: &Graph) -> Result<()> {
        match state[i] {
            1 => bail!(
                "block '{}' leads back to itself: a routine has no loop",
                graph.blocks[i].id
            ),
            2 => return Ok(()),
            _ => {}
        }
        state[i] = 1;
        for &j in &edges[i] {
            visit(j, edges, state, graph)?;
        }
        state[i] = 2;
        Ok(())
    }
    visit(0, &edges, &mut state, graph)?;
    if let Some(i) = state.iter().position(|s| *s == 0) {
        bail!("block '{}' is never reached", graph.blocks[i].id);
    }
    Ok(())
}

/// A field of an item, dotted for nested ones: `answer.draft`.
fn path(p: &str) -> Result<()> {
    let segment = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    if !p.split('.').all(segment) {
        bail!("'{p}' is not a field like answer.draft");
    }
    Ok(())
}

/// A template: text with `{{field}}` where an item's field goes.
fn template(t: &str) -> Result<()> {
    let mut rest = t;
    while let Some(start) = rest.find("{{") {
        let end = rest[start..]
            .find("}}")
            .with_context(|| format!("'{t}' opens {{{{ without closing it"))?;
        path(rest[start + 2..start + end].trim())?;
        rest = &rest[start + end + 2..];
    }
    Ok(())
}

/// An action's id: `<connector>.<action>`.
fn action_name(id: &str) -> Result<()> {
    match id.split_once('.') {
        Some((c, a)) if !c.is_empty() && !a.is_empty() => Ok(()),
        _ => bail!("'{id}' is not an action like typefully.create_draft"),
    }
}

fn positive(what: &str, n: Option<usize>) -> Result<()> {
    if n == Some(0) {
        bail!("{what} is at least 1");
    }
    Ok(())
}

fn check_block(cx: &Checks, block: &Block) -> Result<()> {
    match &block.kind {
        Kind::Read {
            source,
            feeds,
            url,
            action,
            max_age_days,
            dedupe,
            max,
        } => {
            dedupe.as_deref().map(path).transpose()?;
            positive("max", *max)?;
            match source.as_str() {
                "rss" if feeds.is_empty() => bail!("an rss read needs feeds"),
                "web" if url.is_none() => bail!("a web read needs a url"),
                "connector" => {
                    action_name(
                        action
                            .as_deref()
                            .context("a connector read names its action")?,
                    )?;
                }
                "rss" | "web" | "business" => {}
                other => bail!("unknown source '{other}' (rss, web, connector, business)"),
            }
            if *max_age_days == Some(0) {
                bail!("max_age_days is at least 1");
            }
        }
        Kind::Ai {
            prompt,
            keep,
            rank,
            distinct,
            limit,
        } => {
            for p in [keep, rank, distinct].into_iter().flatten() {
                path(p)?;
            }
            positive("limit", *limit)?;
            let stem = prompt
                .strip_prefix("prompts/")
                .and_then(|p| p.strip_suffix(".md"))
                .filter(|s| !s.is_empty() && !s.contains("..") && !s.contains('/'))
                .context("a prompt is prompts/<name>.md")?;
            std::fs::read_to_string(cx.root.join(prompt))
                .with_context(|| format!("reading {prompt}"))?;
            let schema = format!("prompts/{stem}.schema.json");
            let text = std::fs::read_to_string(cx.root.join(&schema))
                .with_context(|| format!("reading {schema}"))?;
            let value: Value =
                serde_json::from_str(&text).with_context(|| format!("{schema} is not JSON"))?;
            if !value.is_object() {
                bail!("{schema} is not a JSON schema");
            }
        }
        Kind::Condition { test, .. } => {
            path(&test.field)?;
            if !OPS.contains(&test.op.as_str()) {
                bail!("unknown operator '{}' ({})", test.op, OPS.join(" "));
            }
            if (test.op == "exists") != test.value.is_none() {
                bail!("a condition compares to a value, except `exists`");
            }
        }
        Kind::Approval {
            show,
            editable,
            expires_after,
        } => {
            path(show)?;
            editable.as_deref().map(path).transpose()?;
            positive("expires_after", expires_after.map(|h| h as usize))?;
        }
        Kind::Act { action, fields } => {
            action_name(action)?;
            for (name, value) in fields {
                crate::realm::validate_name(name).with_context(|| format!("field '{name}'"))?;
                template(value).with_context(|| format!("field '{name}'"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
