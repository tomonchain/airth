//! Running a routine's graph (decision 49), for now as a dry run: reads and
//! AI blocks run for real, an approval is answered yes without asking, and
//! an act is only prepared, never done. Items flow block by block, in the
//! graph's order, a condition sending each to one side.

use super::{Graph, Kind, Test, successors};
use crate::business::store::Store;
use crate::integration::Integration;
use crate::pipeline::Prompt;
use crate::pipeline::run::{Asking, Filters, Io, Item, field, field_text, judge_all, read_rss};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Where an item keeps the id of what it was read from.
const SOURCE: &str = "source_id";

/// An act's idempotency key: the routine, the item's source id and the
/// block (decision 49). Never the content: a retried run whose AI block
/// drafts another text for the same item must not act twice.
pub fn key(routine: &str, item: &Item, block: &str) -> String {
    format!("{routine}/{}/{block}", field_text(item, SOURCE))
}

/// The id an item is known by where it was read: a feed item's link, or its
/// guid when it has no link.
fn source_id(item: &Item) -> String {
    ["link", "guid", "id"]
        .iter()
        .map(|k| field_text(item, k))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}

/// A template with each `{{field}}` replaced by the item's.
fn render(template: &str, item: &Item) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&field_text(item, rest[start + 2..start + end].trim()));
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

fn holds(test: &Test, item: &Item) -> bool {
    let got = field(item, &test.field).filter(|v| !v.is_null());
    let (Some(got), Some(want)) = (got, test.value.as_ref()) else {
        return test.op == "exists" && got.is_some();
    };
    let order = match (got.as_f64(), want.as_f64()) {
        (Some(a), Some(b)) => a.partial_cmp(&b),
        _ => Some(
            field_text(item, &test.field)
                .as_str()
                .cmp(want.as_str().unwrap_or("")),
        ),
    };
    use std::cmp::Ordering::*;
    matches!(
        (test.op.as_str(), order),
        ("==", Some(Equal))
            | ("!=", Some(Less | Greater))
            | ("<", Some(Less))
            | ("<=", Some(Less | Equal))
            | (">", Some(Greater))
            | (">=", Some(Greater | Equal))
    )
}

/// An AI block's prompt and the schema beside it, checked when the graph
/// was loaded.
fn prompt(root: &Path, path: &str) -> Result<Prompt> {
    let text =
        std::fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))?;
    let schema_path = path.replace(".md", ".schema.json");
    let schema = serde_json::from_str(&std::fs::read_to_string(root.join(&schema_path))?)
        .with_context(|| format!("{schema_path} is not JSON"))?;
    Ok(Prompt {
        path: path.to_string(),
        version: crate::update::sha256_hex(text.as_bytes()),
        text,
        schema,
    })
}

/// What an act would call: an integration's declared action as its method
/// and URL, anything else as its fields.
fn describe(
    action: &str,
    fields: &BTreeMap<String, String>,
    integrations: &[Integration],
) -> Result<String> {
    if let Some((connector, name)) = action.split_once('.')
        && let Some(i) = integrations.iter().find(|i| i.name == connector)
        && let Some(a) = i.actions.iter().find(|a| a.name == name)
    {
        return Ok(format!("{} {}", a.method, a.url(i, fields)?));
    }
    Ok(serde_json::to_string(fields)?)
}

/// The blocks in an order where each comes after every block leading to it.
fn order(graph: &Graph) -> Result<Vec<usize>> {
    fn visit(i: usize, graph: &Graph, done: &mut Vec<bool>, out: &mut Vec<usize>) -> Result<()> {
        if !done[i] {
            done[i] = true;
            for j in successors(graph, i)? {
                visit(j, graph, done, out)?;
            }
            out.push(i);
        }
        Ok(())
    }
    let (mut done, mut out) = (vec![false; graph.blocks.len()], Vec::new());
    visit(0, graph, &mut done, &mut out)?;
    out.reverse();
    Ok(out)
}

/// What a dry run needs: the business folder, its store, its reads and
/// model, and its integrations to show what acts would call.
pub struct Dry<'a, 'b> {
    pub root: &'a Path,
    pub store: &'a mut Store,
    pub io: &'a mut Io<'b>,
    pub integrations: &'a [Integration],
}

/// Run `graph` (named `name`) dry: its report, block by block.
pub fn dry_run(name: &str, graph: &Graph, dry: Dry, now: DateTime<Utc>) -> Result<String> {
    let mut inbox: Vec<Vec<Item>> = vec![Vec::new(); graph.blocks.len()];
    inbox[0].push(Item::new());
    let mut report = vec![format!(
        "Dry run of {name}: reads and AI blocks run, approvals are taken as given, acts are only prepared."
    )];
    for i in order(graph)? {
        let block = &graph.blocks[i];
        let items = std::mem::take(&mut inbox[i]);
        let (count, mut notes, mut lines) = (items.len(), Vec::new(), Vec::new());
        let (pass, other): (Vec<Item>, Vec<Item>) = match &block.kind {
            Kind::Read {
                source,
                feeds,
                max_age_days,
                dedupe,
                max,
                ..
            } => {
                if source != "rss" {
                    bail!("block '{}': a dry run reads rss only, for now", block.id);
                }
                if items.is_empty() {
                    (Vec::new(), Vec::new())
                } else {
                    let (_, mut read) = read_rss(feeds, *max_age_days, dry.io, now, &mut notes)?;
                    for item in &mut read {
                        let id = source_id(item);
                        item.insert(SOURCE.into(), id.into());
                    }
                    if let Some(by) = dedupe {
                        let keys: Vec<String> = read.iter().map(|it| field_text(it, by)).collect();
                        let seen = dry.store.seen(name, &keys)?;
                        let mut now_seen = HashSet::new();
                        read.retain(|it| {
                            let k = field_text(it, by);
                            !k.is_empty() && !seen.contains(&k) && now_seen.insert(k)
                        });
                    }
                    if let Some(max) = max {
                        read.truncate(*max);
                    }
                    (read, Vec::new())
                }
            }
            Kind::Ai {
                prompt: path,
                keep,
                rank,
                distinct,
                limit,
            } => {
                let p = prompt(dry.root, path)?;
                let asking = Asking {
                    name,
                    step: i + 1,
                    prompt: &p,
                };
                let filters = Filters {
                    keep: keep.as_deref(),
                    rank: rank.as_deref(),
                    distinct: distinct.as_deref(),
                    limit: *limit,
                };
                (
                    judge_all(&asking, items, &filters, dry.store, dry.io, &mut notes)?,
                    Vec::new(),
                )
            }
            Kind::Condition { test, .. } => items.into_iter().partition(|it| holds(test, it)),
            Kind::Approval { show, .. } => {
                for it in &items {
                    lines.push(format!(
                        "  would ask the founder: {}",
                        field_text(it, show).chars().take(200).collect::<String>()
                    ));
                }
                (items, Vec::new())
            }
            Kind::Act { action, fields } => {
                for it in &items {
                    let values: BTreeMap<String, String> = fields
                        .iter()
                        .map(|(k, v)| (k.clone(), render(v, it)))
                        .collect();
                    let what = describe(action, &values, dry.integrations)
                        .unwrap_or_else(|e| format!("refused: {e:#}"));
                    lines.push(format!(
                        "  would {action} [{}]: {what}",
                        key(name, it, &block.id)
                    ));
                }
                (items, Vec::new())
            }
        };
        report.push(format!(
            "{}: {count} in, {} out",
            block.id,
            pass.len() + other.len()
        ));
        report.extend(notes.iter().map(|n| format!("  note: {n}")));
        report.extend(lines);
        let next = successors(graph, i)?;
        match (&block.kind, next.as_slice()) {
            (Kind::Condition { .. }, [then, otherwise]) => {
                inbox[*then].extend(pass);
                inbox[*otherwise].extend(other);
            }
            (Kind::Condition { .. }, [otherwise]) => inbox[*otherwise].extend(other),
            _ => {
                for j in next {
                    inbox[j].extend(pass.iter().cloned());
                }
            }
        }
    }
    Ok(report.join("\n"))
}

#[cfg(test)]
mod tests;
