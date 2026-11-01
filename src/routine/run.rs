//! Running a routine's graph (decision 49), for now as a dry run: reads and
//! AI blocks run for real, an approval is answered yes without asking, and
//! an act is only prepared, never done. Items flow block by block, in the
//! graph's order, a condition sending each to one side.

use super::{Graph, Kind, Test, successors};
use crate::business::store::Store;
use crate::integration::Integration;
use crate::pipeline::Prompt;
use crate::pipeline::run::{
    Asking, Filters, Io, Item, Judged, field, field_text, judge_all, read_rss,
};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Instant;

/// Where an item keeps the id of what it was read from.
const SOURCE: &str = "source_id";

/// An act's idempotency key: the routine, the item's source id and the
/// block (decision 49). Never the content: a retried run whose AI block
/// drafts another text for the same item must not act twice.
pub fn key(routine: &str, item: &Item, block: &str) -> String {
    format!("{routine}/{}/{block}", field_text(item, SOURCE))
}

/// Query parameters that only track where a reader came from.
const TRACKING: &[&str] = &[
    "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "yclid", "mc_cid", "mc_eid",
    "igshid", "_hsenc", "_hsmi", "mkt_tok", "ref_src", "spm", "cmpid", "s_cid",
];

/// A link as one article is known, whatever brought the reader: without
/// tracking parameters (`utm_*` and the like) or fragment. Anything that is
/// not an http(s) URL is kept as it is.
pub fn canonical(link: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(link.trim()) else {
        return link.trim().to_string();
    };
    if !matches!(url.scheme(), "http" | "https") {
        return link.trim().to_string();
    }
    url.set_fragment(None);
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !k.starts_with("utm_") && !TRACKING.contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept);
    }
    url.to_string()
}

/// The id an item is known by where it was read: a feed item's link, or its
/// guid when it has no link, made canonical.
fn source_id(item: &Item) -> String {
    let id = ["link", "guid", "id"]
        .iter()
        .map(|k| field_text(item, k))
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    canonical(&id)
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
    let started = Instant::now();
    let mut total = Judged::default();
    for i in order(graph)? {
        let block = &graph.blocks[i];
        let items = std::mem::take(&mut inbox[i]);
        let (count, mut notes, mut lines) = (items.len(), Vec::new(), Vec::new());
        let block_started = Instant::now();
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
                    let (read_count, mut read) =
                        read_rss(feeds, *max_age_days, dry.io, now, &mut notes)?;
                    let old = read_count - read.len();
                    for item in &mut read {
                        let id = source_id(item);
                        item.insert(SOURCE.into(), id.into());
                    }
                    let before = read.len();
                    if let Some(by) = dedupe {
                        let id = |it: &Item| canonical(&field_text(it, by));
                        let keys: Vec<String> = read.iter().map(id).collect();
                        let seen = dry.store.seen(name, &keys)?;
                        let mut now_seen = HashSet::new();
                        read.retain(|it| {
                            let k = id(it);
                            !k.is_empty() && !seen.contains(&k) && now_seen.insert(k)
                        });
                    }
                    let repeated = before - read.len();
                    let cut = max.map_or(0, |m| read.len().saturating_sub(m));
                    read.truncate(read.len() - cut);
                    lines.push(format!(
                        "  {read_count} read, {old} too old, {repeated} already seen or repeated, {cut} past the max: {} kept",
                        read.len()
                    ));
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
                    distinct,
                    limit: *limit,
                };
                let (kept, stats) =
                    judge_all(&asking, items, &filters, dry.store, dry.io, &mut notes)?;
                let shown: Vec<&str> = rank.iter().chain(distinct).map(String::as_str).collect();
                for it in &kept {
                    let values: Vec<String> = shown
                        .iter()
                        .map(|f| format!("{f}: {}", field_text(it, &format!("answer.{f}"))))
                        .collect();
                    lines.push(format!(
                        "  kept: {} ({})",
                        field_text(it, "title"),
                        values.join(", ")
                    ));
                }
                lines.push(format!(
                    "  set aside: {} not {}, {} invalid, {} by distinct {}, {} past the limit",
                    stats.not_kept,
                    keep.as_deref().unwrap_or("kept"),
                    stats.invalid,
                    stats.by_distinct,
                    distinct.join(", "),
                    stats.by_limit
                ));
                lines.push(cost(&stats));
                total.input_tokens += stats.input_tokens;
                total.output_tokens += stats.output_tokens;
                total.cost_usd += stats.cost_usd;
                total.unpriced += stats.unpriced;
                (kept, Vec::new())
            }
            Kind::Condition { test, .. } => items.into_iter().partition(|it| holds(test, it)),
            Kind::Approval { show, .. } => {
                for it in &items {
                    let text = field_text(it, show);
                    let chars = text.chars().count();
                    let shown: String = text.chars().take(SHOWN).collect();
                    let cut = if chars > SHOWN {
                        format!(
                            " [cut here for this report: {chars} characters in all; the queue shows it whole]"
                        )
                    } else {
                        String::new()
                    };
                    lines.push(format!("  would ask the founder: {shown}{cut}"));
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
            "{}: {count} in, {} out, {:.1}s",
            block.id,
            pass.len() + other.len(),
            block_started.elapsed().as_secs_f64()
        ));
        report.extend(lines);
        report.extend(notes.iter().map(|n| format!("  note: {n}")));
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
    report.push(format!(
        "Total: {}, {:.1}s",
        cost(&total).trim(),
        started.elapsed().as_secs_f64()
    ));
    Ok(report.join("\n"))
}

/// Characters of an approval's text shown in a report.
const SHOWN: usize = 400;

/// What model calls cost, as a report line.
fn cost(stats: &Judged) -> String {
    let unpriced = match stats.unpriced {
        0 => String::new(),
        n => format!(" (+{n} call(s) to a model without a known price)"),
    };
    format!(
        "  cost: ${:.4}{unpriced}, {} tokens in, {} out",
        stats.cost_usd, stats.input_tokens, stats.output_tokens
    )
}

#[cfg(test)]
mod tests;
