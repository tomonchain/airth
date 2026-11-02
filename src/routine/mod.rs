//! Routines as graphs of six blocks (decision 49): `routines/<name>.json` in
//! the business folder, read-only to runs (decision 44). A graph is checked
//! whole when it is loaded, and refused, naming the block, if it is wrong:
//! its shape (no cycle, every block reachable), its references (blocks,
//! prompts, declared actions, the agent), and the rule that content from
//! outside never reaches an `act` without an `approval` on the way, unless
//! that action's level for the agent is at least 2.

use crate::objective::Schedule;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Where a business's routines live, at the root of its folder.
pub const DIR: &str = "routines";

pub mod run;
pub mod week;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    #[serde(default)]
    pub description: String,
    pub agent: String,
    pub trigger: Trigger,
    pub blocks: Vec<Block>,
}

/// What starts the routine: exactly one of these.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub schedule: Option<String>,
    pub webhook: Option<String>,
    pub manual: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Block {
    pub id: String,
    #[serde(flatten)]
    pub kind: Kind,
    /// Absent: the next block in the list. `null`: the run stops here.
    #[serde(
        default,
        deserialize_with = "explicit",
        skip_serializing_if = "Option::is_none"
    )]
    pub next: Option<Option<String>>,
}

/// A field, or a list of them.
fn one_or_many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Either::deserialize(d)? {
        Either::One(f) => vec![f],
        Either::Many(fs) => fs,
    })
}

/// Keeps `"next": null` apart from no `next` at all.
fn explicit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// How an AI block asks about its items.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Over {
    List,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Kind {
    Read {
        source: String,
        /// What a `business` read reads: `week`, how the routines ran in
        /// the last seven days.
        records: Option<String>,
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
        /// One field, or several: only the best item of each value goes on.
        #[serde(default, deserialize_with = "one_or_many")]
        distinct: Vec<String>,
        limit: Option<usize>,
        /// Where the answer goes: `answer` when absent, another name to keep
        /// an earlier block's, as a check does (decision 58).
        #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
        into: Option<String>,
        /// `list`: every item in one call, so the model can compare them
        /// (decision 60). One call per item when absent.
        #[serde(skip_serializing_if = "Option::is_none")]
        over: Option<Over>,
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
        /// The field holding a check's answer (`flag`, `reason`): the item
        /// goes to the queue with that quality flag (decision 58).
        #[serde(skip_serializing_if = "Option::is_none")]
        flag: Option<String>,
    },
    Act {
        action: String,
        #[serde(default)]
        fields: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Test {
    pub field: String,
    pub op: String,
    pub value: Option<Value>,
}

const OPS: &[&str] = &["==", "!=", "<", "<=", ">", ">=", "exists"];

/// Fields the engine sets on an item; an AI block's answer cannot take
/// their place.
const RESERVED: &[&str] = &["source_id", "approved_by_founder", "model_call", "result"];

/// What a graph is checked against: the business's folder, its declared
/// actions, and the routine's agent's level for each action.
pub struct Checks<'a> {
    pub root: &'a Path,
    pub actions: &'a [crate::actions::Declared],
    /// The level of (agent, action), clamped to its ceiling.
    pub level: &'a dyn Fn(&str, &str) -> u8,
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
    check_flags(graph, &edges)?;
    check_outside(cx, graph, &edges)
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

fn positive(what: &str, n: Option<usize>) -> Result<()> {
    if n == Some(0) {
        bail!("{what} is at least 1");
    }
    Ok(())
}

fn check_block(cx: &Checks, block: &Block) -> Result<()> {
    let declared = |action: &str| {
        cx.actions
            .iter()
            .find(|a| a.id == action)
            .with_context(|| format!("'{action}' is not a declared action (see `airth actions`)"))
    };
    match &block.kind {
        Kind::Read {
            source,
            records,
            feeds,
            url,
            action,
            max_age_days,
            dedupe,
            max,
        } => {
            dedupe.as_deref().map(path).transpose()?;
            positive("max", *max)?;
            match (source.as_str(), records.as_deref()) {
                ("business", Some("week")) => {}
                ("business", Some(other)) => bail!("unknown records '{other}' (week)"),
                ("business", None) => bail!("a business read names its records (week)"),
                (_, Some(_)) => bail!("only a business read names records"),
                _ => {}
            }
            match source.as_str() {
                "rss" if feeds.is_empty() => bail!("an rss read needs feeds"),
                "web" if url.is_none() => bail!("a web read needs a url"),
                "connector" => {
                    let a = declared(
                        action
                            .as_deref()
                            .context("a connector read names its action")?,
                    )?;
                    if a.class != crate::actions::Class::Read {
                        bail!("'{}' is not a read action", a.id);
                    }
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
            into,
            over,
        } => {
            // A check judges each draft alone: one cannot sway another's.
            if over.is_some() && into.is_some() {
                bail!("a block with 'as' judges each item alone: no 'over'");
            }
            for p in [keep, rank].into_iter().flatten().chain(distinct) {
                path(p)?;
            }
            if let Some(into) = into {
                crate::realm::validate_name(into).with_context(|| format!("as '{into}'"))?;
                if RESERVED.contains(&into.as_str()) {
                    bail!("as '{into}': Airth keeps that field for itself");
                }
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
            flag,
        } => {
            path(show)?;
            flag.as_deref().map(path).transpose()?;
            editable.as_deref().map(path).transpose()?;
            positive("expires_after", expires_after.map(|h| h as usize))?;
        }
        Kind::Act { action, fields } => {
            declared(action)?;
            for (name, value) in fields {
                crate::realm::validate_name(name).with_context(|| format!("field '{name}'"))?;
                template(value).with_context(|| format!("field '{name}'"))?;
            }
        }
    }
    Ok(())
}

/// An approval's `flag` names the answer of a check that comes before it
/// on every way to it: an item reaching it unchecked would look checked.
fn check_flags(graph: &Graph, edges: &[Vec<usize>]) -> Result<()> {
    for (i, block) in graph.blocks.iter().enumerate() {
        let Kind::Approval {
            flag: Some(flag), ..
        } = &block.kind
        else {
            continue;
        };
        // The blocks reaching this one without passing a check named `flag`.
        let names = |b: &Block| matches!(&b.kind, Kind::Ai { into: Some(n), .. } if n == flag);
        let mut seen = vec![false; graph.blocks.len()];
        let mut todo = vec![0];
        while let Some(j) = todo.pop() {
            if seen[j] || names(&graph.blocks[j]) {
                continue;
            }
            if j == i {
                bail!(
                    "block '{}': its flag '{flag}' is not the answer of a check (an ai block with \"as\": \"{flag}\") on every way to it",
                    block.id
                );
            }
            seen[j] = true;
            todo.extend(&edges[j]);
        }
    }
    Ok(())
}

/// Content from outside (a read of rss, web or a connector, a mission's
/// result, a webhook's body) reaches no `act` without an `approval` on the
/// way, unless the agent's level for that action is at least 2.
fn check_outside(cx: &Checks, graph: &Graph, edges: &[Vec<usize>]) -> Result<()> {
    let mut seen = HashSet::new();
    let mut todo = vec![(0usize, graph.trigger.webhook.is_some())];
    while let Some((i, outside)) = todo.pop() {
        if !seen.insert((i, outside)) {
            continue;
        }
        let after = match &graph.blocks[i].kind {
            Kind::Read { source, .. } => outside || source != "business",
            Kind::Approval { .. } => false,
            Kind::Act { action, fields } => {
                // A task for the founder is the founder's to judge; one for
                // an agent becomes its instructions, so it is checked.
                let for_founder = action == "airth.create_task"
                    && fields.get("assignee").map(String::as_str) == Some("founder");
                if outside && !for_founder && (cx.level)(&graph.agent, action) < 2 {
                    bail!(
                        "block '{}': content from outside reaches {action} without an approval, and {} may not take it alone (level {})",
                        graph.blocks[i].id,
                        graph.agent,
                        (cx.level)(&graph.agent, action)
                    );
                }
                outside || action == "airth.launch_mission"
            }
            _ => outside,
        };
        todo.extend(edges[i].iter().map(|&j| (j, after)));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
