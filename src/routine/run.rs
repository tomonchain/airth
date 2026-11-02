//! Running a routine's graph (decision 49). Items flow block by block, in
//! the graph's order, a condition sending each to one side. A dry run reads
//! and asks the model for real, takes approvals as given and only prepares
//! acts. A real run puts each item that reaches an approval in the queue,
//! where it waits for the founder, and resumes after it with the founder's
//! answer; its acts go through the autonomy levels. An item without a
//! source id never reaches an act.

use super::{Graph, Kind, Test, successors};
use crate::business::store::Store;
use crate::business::store::routines::BlockRun;
use crate::integration::Integration;
use crate::pipeline::Prompt;
use crate::pipeline::run::{
    Asking, Filters, Io, Item, Judged, field, field_text, judge_all, read_rss,
};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Instant;

/// Where an item keeps the id of what it was read from.
pub(super) const SOURCE: &str = "source_id";
/// Set on an item the founder approved.
const APPROVED: &str = "approved_by_founder";

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
    // One field a line, as the founder would read them.
    Ok(fields
        .iter()
        .map(|(k, v)| format!("\n    {k}: {}", v.replace('\n', "\n      ")))
        .collect())
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

/// Where a run starts: at its trigger, or after an approval the founder
/// gave, with the item as the founder left it.
pub enum Start {
    Trigger,
    After { block: String, item: Item },
}

/// What does a real run's acts: it gets the action, its fields, the act's
/// key and whether the founder approved the item, and says what it did.
pub type Acts<'a> = dyn FnMut(&str, &BTreeMap<String, String>, &str, bool) -> Result<String> + 'a;

/// What a real run adds: its id, and what does its acts.
pub struct Real<'a> {
    pub run: &'a str,
    pub act: &'a mut Acts<'a>,
}

/// What a run did, block by block: as text for the terminal and the
/// heartbeat, as data for the app.
#[derive(Debug, Serialize)]
pub struct Report {
    pub title: String,
    pub blocks: Vec<BlockReport>,
    /// What its model calls cost, in words.
    pub cost: String,
    pub secs: f64,
}

#[derive(Debug, Serialize)]
pub struct BlockReport {
    pub id: String,
    pub items_in: usize,
    pub items_out: usize,
    pub secs: f64,
    /// What it did, one line each: what it kept, would ask or would do.
    pub lines: Vec<String>,
    /// What went wrong or was left out.
    pub notes: Vec<String>,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "{}", self.title)?;
        for b in &self.blocks {
            writeln!(
                f,
                "{}: {} in, {} out, {:.1}s",
                b.id, b.items_in, b.items_out, b.secs
            )?;
            for line in &b.lines {
                writeln!(f, "  {line}")?;
            }
            for note in &b.notes {
                writeln!(f, "  note: {note}")?;
            }
        }
        write!(f, "Total: {}, {:.1}s", self.cost, self.secs)
    }
}

/// Run `graph` (named `name`) dry: its report, block by block.
pub fn dry_run(name: &str, graph: &Graph, dry: Dry, now: DateTime<Utc>) -> Result<Report> {
    run(name, graph, dry, None, Start::Trigger, now)
}

/// Run `graph` (named `name`) from `start`, dry when `real` is None: its
/// report, block by block.
pub fn run(
    name: &str,
    graph: &Graph,
    dry: Dry,
    mut real: Option<Real>,
    start: Start,
    now: DateTime<Utc>,
) -> Result<Report> {
    let mut inbox: Vec<Vec<Item>> = vec![Vec::new(); graph.blocks.len()];
    match start {
        Start::Trigger => inbox[0].push(Item::new()),
        Start::After { block, item } => {
            let at = graph
                .blocks
                .iter()
                .position(|b| b.id == block)
                .ok_or_else(|| anyhow::anyhow!("{name} has no block '{block}' any more"))?;
            // The founder approved it: what follows is done as approved.
            let mut item = item;
            item.insert(APPROVED.into(), true.into());
            for j in successors(graph, at)? {
                inbox[j].push(item.clone());
            }
        }
    }
    let title = match &real {
        None => format!(
            "Dry run of {name}: reads and AI blocks run, approvals are taken as given, acts are only prepared."
        ),
        Some(r) => format!("Run {} of {name}.", r.run),
    };
    let mut blocks = Vec::new();
    let started = Instant::now();
    let mut total = Judged::default();
    for i in order(graph)? {
        let block = &graph.blocks[i];
        let items = std::mem::take(&mut inbox[i]);
        if items.is_empty() {
            continue;
        }
        let (count, mut notes, mut lines) = (items.len(), Vec::new(), Vec::new());
        let block_started = Instant::now();
        let block_at = Utc::now();
        // What the block's record says beyond its counts (a real run only).
        let (mut acts_failed, mut block_cost) = (0usize, None);
        let (pass, other): (Vec<Item>, Vec<Item>) = match &block.kind {
            Kind::Read { source, .. } if source == "business" => {
                // The graph check let only `records: week` through.
                let week = super::week::read(dry.root, dry.store, now)?;
                lines.push(format!(
                    "the week {} from the business's records",
                    field_text(&week, "week")
                ));
                (vec![week], Vec::new())
            }
            Kind::Read {
                source,
                feeds,
                max_age_days,
                dedupe,
                max,
                ..
            } => {
                if source != "rss" {
                    bail!(
                        "block '{}': a run reads rss and the business only, for now",
                        block.id
                    );
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
                    if let (Some(by), Some(_)) = (dedupe, &real) {
                        let keys: Vec<String> = read
                            .iter()
                            .map(|it| canonical(&field_text(it, by)))
                            .collect();
                        dry.store.mark_seen(name, &keys)?;
                    }
                    let cut = max.map_or(0, |m| read.len().saturating_sub(m));
                    read.truncate(read.len() - cut);
                    lines.push(format!(
                        "{read_count} read, {old} too old, {repeated} already seen or repeated, {cut} past the max: {} kept",
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
                into,
                over,
            } => {
                let p = prompt(dry.root, path)?;
                let into = into.as_deref().unwrap_or("answer");
                let asking = Asking {
                    name,
                    step: i + 1,
                    prompt: &p,
                    dry_run: real.is_none(),
                    into,
                    list: over.is_some(),
                };
                let filters = Filters {
                    keep: keep.as_deref(),
                    rank: rank.as_deref(),
                    distinct,
                    limit: *limit,
                };
                let (kept, stats) =
                    judge_all(&asking, items, &filters, dry.store, dry.io, &mut notes)
                        .with_context(|| format!("{} failed", block.id))?;
                let shown: Vec<&str> = rank.iter().chain(distinct).map(String::as_str).collect();
                for it in &kept {
                    let values: Vec<String> = shown
                        .iter()
                        .map(|f| format!("{f}: {}", field_text(it, &format!("{into}.{f}"))))
                        .collect();
                    lines.push(format!(
                        "kept: {} ({})",
                        field_text(it, "title"),
                        values.join(", ")
                    ));
                }
                lines.push(format!(
                    "set aside: {} not {}, {} invalid, {} by distinct {}, {} past the limit",
                    stats.not_kept,
                    keep.as_deref().unwrap_or("kept"),
                    stats.invalid,
                    stats.by_distinct,
                    distinct.join(", "),
                    stats.by_limit
                ));
                lines.push(cost(&stats));
                block_cost = Some(stats.cost_usd);
                total.input_tokens += stats.input_tokens;
                total.output_tokens += stats.output_tokens;
                total.cost_usd += stats.cost_usd;
                total.unpriced += stats.unpriced;
                (kept, Vec::new())
            }
            Kind::Condition { test, .. } => items.into_iter().partition(|it| holds(test, it)),
            Kind::Approval {
                show,
                editable,
                flag: check,
                ..
            } => {
                for it in &items {
                    let text = field_text(it, show);
                    let flag = check.as_deref().and_then(|c| quality_flag(it, c));
                    let flagged = flag
                        .as_ref()
                        .map(|(f, why)| format!(" [flagged {f}: {why}]"))
                        .unwrap_or_default();
                    let chars = text.chars().count();
                    let shown: String = text.chars().take(SHOWN).collect();
                    let cut = if chars > SHOWN {
                        format!(
                            " [cut here for this report: {chars} characters in all; the queue shows it whole]"
                        )
                    } else {
                        String::new()
                    };
                    match &real {
                        None => lines.push(format!("would ask the founder{flagged}: {shown}{cut}")),
                        Some(r) => {
                            let id = dry.store.propose_flagged(
                                &crate::business::store::autonomy::NewItem {
                                kind: "approval",
                                title: &text
                                    .lines()
                                    .next()
                                    .unwrap_or_default()
                                    .chars()
                                    .take(120)
                                    .collect::<String>(),
                                agent: Some(&graph.agent),
                                routine: Some(name),
                                run: Some(r.run),
                                action: None,
                                args: &Value::Object(it.clone()).to_string(),
                                execute_at: None,
                                source:
                                    &serde_json::json!({"block": block.id, "show": show, "editable": editable})
                                        .to_string(),
                                model_call: it.get("model_call").and_then(Value::as_i64),
                            },
                                flag.as_ref().map(|(f, why)| (*f, why.as_str())),
                            )?;
                            lines.push(format!(
                                "asked the founder (item {id}){flagged}: {shown}{cut}"
                            ));
                        }
                    }
                }
                // In a real run, an item waits in the queue for the founder.
                (if real.is_some() { Vec::new() } else { items }, Vec::new())
            }
            Kind::Act { action, fields } => {
                let mut done = Vec::new();
                for mut it in items {
                    // Its key would not tell this item from another: it
                    // could act twice, or never.
                    if field_text(&it, SOURCE).is_empty() {
                        notes.push(format!(
                            "set aside: '{}' has no source id, so it never reaches an act",
                            field_text(&it, "title")
                        ));
                        continue;
                    }
                    let values: BTreeMap<String, String> = fields
                        .iter()
                        .map(|(k, v)| (k.clone(), render(v, &it)))
                        .collect();
                    let key = key(name, &it, &block.id);
                    match real.as_mut() {
                        None => {
                            let what = describe(action, &values, dry.integrations)
                                .unwrap_or_else(|e| format!("refused: {e:#}"));
                            lines.push(format!("would {action} [{key}]: {what}"));
                        }
                        Some(r) => match (r.act)(
                            action,
                            &values,
                            &key,
                            it.get(APPROVED) == Some(&Value::Bool(true)),
                        ) {
                            Ok(what) => {
                                lines.push(format!("{action} [{key}]: {what}"));
                                it.insert("result".into(), what.into());
                            }
                            Err(e) => {
                                acts_failed += 1;
                                notes.push(format!("{action} [{key}] failed: {e:#}"));
                            }
                        },
                    }
                    done.push(it);
                }
                (done, Vec::new())
            }
        };
        if let Some(r) = &real {
            // Each block's execution, for the founder to follow the run step
            // by step (decision 49).
            let outcome = match &block.kind {
                Kind::Approval { .. } => "waiting",
                _ if acts_failed > 0 => "failed",
                _ => "ok",
            };
            dry.store.record_block(&BlockRun {
                routine: name,
                run: r.run,
                block: &block.id,
                started_at: block_at,
                outcome,
                items_in: count,
                items_out: pass.len() + other.len(),
                cost_usd: block_cost,
                error: (acts_failed > 0).then(|| notes.join("\n")),
            })?;
        }
        blocks.push(BlockReport {
            id: block.id.clone(),
            items_in: count,
            items_out: pass.len() + other.len(),
            secs: block_started.elapsed().as_secs_f64(),
            lines,
            notes,
        });
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
    Ok(Report {
        title,
        blocks,
        cost: cost(&total),
        secs: started.elapsed().as_secs_f64(),
    })
}

/// The quality flag a check's answer at `check` sets (decision 58), and
/// why. No answer there means the check could not run: the founder is told,
/// not blocked.
fn quality_flag(item: &Item, check: &str) -> Option<(&'static str, String)> {
    let Some(answer) = field(item, check).filter(|v| v.is_object()) else {
        return Some(("note", "not checked: the check gave no valid answer".into()));
    };
    let why = field_text(item, &format!("{check}.reason"));
    match answer["flag"].as_str() {
        Some("blocking") => Some(("blocking", why)),
        Some("note") => Some(("note", why)),
        Some("none") => None,
        _ => Some((
            "note",
            "not checked: the check's flag is not none, note or blocking".into(),
        )),
    }
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
        "cost: ${:.4}{unpriced}, {} tokens in, {} out",
        stats.cost_usd, stats.input_tokens, stats.output_tokens
    )
}

#[cfg(test)]
mod tests;
