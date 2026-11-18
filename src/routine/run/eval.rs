//! A routine's AI blocks checked against what the founder decided
//! (decision 151). Each block is asked again, as a dry run, about the items
//! the founder answered, over the fields it was asked about then; code
//! compares. Nothing is sent and nothing reaches Needs you: no act runs, and
//! only the model calls are recorded, marked as a dry run.

use super::{APPROVED, Dry, cost, order, prompt, with_lessons};
use crate::business::store::queue::Answered;
use crate::pipeline::run::{Asking, Filters, Judged, REDO, field, field_text, judge_all};
use crate::routine::{Graph, Kind, Tier};
use anyhow::Result;
use serde::Serialize;

/// How far back the founder's answers are read, and how many at most.
pub const DAYS: i64 = 60;
pub const MAX: usize = 20;

/// Fields the engine sets on an item: never what a block was asked about.
const ENGINE: &[&str] = &["model_call", APPROVED, REDO];

#[derive(Debug, Serialize)]
pub struct Evaluation {
    pub routine: String,
    /// The founder's answers replayed.
    pub cases: usize,
    pub blocks: Vec<BlockEval>,
    /// What its model calls cost, in words, and in dollars.
    pub cost: String,
    pub cost_usd: f64,
    /// What was not measured, and why: a block, or a case.
    pub skipped: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BlockEval {
    pub id: String,
    pub prompt: String,
    /// `keeps`: it keeps or ranks, and is scored. `drafts`: it drafts one
    /// item at a time, and its drafts are shown beside the founder's.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
    pub cases: Vec<Case>,
}

/// One item the founder answered, and what the block does with it now.
#[derive(Debug, Serialize)]
pub struct Case {
    pub item: i64,
    pub at: String,
    pub title: String,
    /// What the founder did: `approve`, `edit`, `reject` or `redo`.
    pub founder: &'static str,
    /// Why they rejected it, or what they asked to change.
    pub said: String,
    /// The draft the founder read.
    pub draft: String,
    /// Their version of it, when they edited it.
    pub edited: String,
    /// A keeping block: whether it keeps the item now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<bool>,
    /// A ranking block: the score it gives the item now, when kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// A drafting block: its new draft.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

/// How a keeping block does on the founder's answers. What they approved,
/// edited or asked to redo, they wanted; what they rejected, they did not.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Score {
    pub wanted: usize,
    pub wanted_kept: usize,
    pub rejected: usize,
    pub rejected_dropped: usize,
    /// The share of cases where the block now does as the founder did.
    pub rate: Option<f64>,
    /// The same share before: the block kept every case, since each one
    /// reached the founder.
    pub before: Option<f64>,
}

/// Count, by code, how often a keeping block agrees with the founder; a
/// case it could not answer is left out.
pub fn score(cases: &[Case]) -> Score {
    let mut s = Score::default();
    for case in cases {
        let Some(kept) = case.kept else { continue };
        if case.founder == "reject" {
            s.rejected += 1;
            s.rejected_dropped += usize::from(!kept);
        } else {
            s.wanted += 1;
            s.wanted_kept += usize::from(kept);
        }
    }
    let all = s.wanted + s.rejected;
    if all > 0 {
        s.rate = Some((s.wanted_kept + s.rejected_dropped) as f64 / all as f64);
        s.before = Some(s.wanted as f64 / all as f64);
    }
    s
}

/// Ask each AI block of `graph` (named `name`) again about the items in
/// `answered`, one call each, and compare with what the founder did.
pub fn evaluate(name: &str, graph: &Graph, dry: Dry, answered: &[Answered]) -> Result<Evaluation> {
    let mut out = Evaluation {
        routine: name.to_string(),
        cases: answered.len(),
        blocks: Vec::new(),
        cost: String::new(),
        cost_usd: 0.0,
        skipped: Vec::new(),
    };
    if answered.is_empty() {
        out.skipped.push(format!(
            "nothing answered: no item of {name} reached you in the last {DAYS} days"
        ));
    }
    let ord = order(graph)?;
    // What the founder saw came out of the blocks before the approval.
    let approval = ord
        .iter()
        .rposition(|&i| matches!(graph.blocks[i].kind, Kind::Approval { .. }));
    let mut total = Judged::default();
    for (at, &i) in ord.iter().enumerate() {
        let block = &graph.blocks[i];
        let Kind::Ai {
            prompt: path,
            keep,
            rank,
            into,
            over,
            model,
            ..
        } = &block.kind
        else {
            continue;
        };
        if answered.is_empty() {
            break;
        }
        let into = into.as_deref().unwrap_or("answer");
        let keeps = keep.is_some() || rank.is_some();
        let skip = if approval.is_none_or(|a| at > a) {
            Some("comes after the approval: you never saw what it did")
        } else if !keeps && (into != "answer" || over.is_some()) {
            Some("neither keeps nor drafts one item at a time: nothing to compare with")
        } else {
            None
        };
        if let Some(why) = skip {
            out.skipped.push(format!("{}: {why}", block.id));
            continue;
        }
        // What this block and those after it wrote did not exist when it
        // was asked.
        let written: Vec<&str> = ord[at..]
            .iter()
            .filter_map(|&j| match &graph.blocks[j].kind {
                Kind::Ai { into, .. } => Some(into.as_deref().unwrap_or("answer")),
                _ => None,
            })
            .collect();
        // The founder's words on recent drafts (decision 116) are left out:
        // they quote the very answers the block is measured against.
        let p = prompt(dry.root, path, dry.context)?;
        let p = with_lessons(p, &crate::business::lessons(dry.root, &graph.agent));
        let asking = Asking {
            name,
            step: i + 1,
            prompt: &p,
            dry_run: true,
            into,
            list: over.is_some(),
            fast: *model == Some(Tier::Fast),
            covered: &[],
        };
        // Alone, an item is never second of its kind nor past a limit:
        // those compare it with its day's other items, which are not here.
        let filters = Filters {
            keep: keep.as_deref(),
            rank: None,
            distinct: &[],
            covered: &[],
            limit: None,
        };
        let mut cases = Vec::new();
        for a in answered {
            let mut item = a.item.args.as_object().cloned().unwrap_or_default();
            for f in written.iter().chain(ENGINE) {
                item.remove(*f);
            }
            let mut notes = Vec::new();
            let judged = judge_all(&asking, vec![item], &filters, dry.store, dry.io, &mut notes);
            let (kept, stats) = match judged {
                Ok(judged) => judged,
                Err(e) => {
                    out.skipped
                        .push(format!("{}: item {}: {e:#}", block.id, a.item.id));
                    continue;
                }
            };
            add(&mut total, &stats);
            // Not answered: dropped, or, for a block that writes elsewhere
            // than `answer`, let through without its answer, said only in
            // the notes. Either way the block's choice is unknown.
            let answered =
                stats.dropped.is_empty() && kept.first().is_none_or(|it| it.contains_key(into));
            if !answered {
                let why = match stats.dropped.first() {
                    Some((_, why)) => why.clone(),
                    None => notes.join("; "),
                };
                out.skipped
                    .push(format!("{}: item {}: {why}", block.id, a.item.id));
                continue;
            }
            let now = kept.first();
            let show = a.item.source["show"].as_str().unwrap_or_default();
            cases.push(Case {
                item: a.item.id,
                at: a.said.at.clone(),
                title: a.item.title.clone(),
                founder: a.said.kind,
                said: a.said.said.clone(),
                draft: a.said.draft.clone(),
                edited: a.said.edited.clone(),
                kept: keeps.then_some(now.is_some()),
                score: rank
                    .as_ref()
                    .and_then(|r| now.and_then(|it| field(it, &format!("{into}.{r}"))))
                    .and_then(|v| v.as_f64()),
                new: (!keeps).then(|| now.map(|it| field_text(it, show)).unwrap_or_default()),
            });
        }
        out.blocks.push(BlockEval {
            id: block.id.clone(),
            prompt: path.clone(),
            kind: if keeps { "keeps" } else { "drafts" },
            score: keeps.then(|| score(&cases)),
            cases,
        });
    }
    out.cost = cost(&total);
    out.cost_usd = total.cost_usd;
    Ok(out)
}

fn add(total: &mut Judged, stats: &Judged) {
    total.input_tokens += stats.input_tokens;
    total.output_tokens += stats.output_tokens;
    total.cost_usd += stats.cost_usd;
    total.unpriced += stats.unpriced;
}

impl std::fmt::Display for Evaluation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(
            f,
            "Evaluation of {}: {} item(s) you answered in the last {DAYS} days, asked again as a dry run.",
            self.routine, self.cases
        )?;
        let cut = |t: &str| -> String {
            let one = t.split_whitespace().collect::<Vec<_>>().join(" ");
            match one.chars().count() > 160 {
                true => format!("{}…", one.chars().take(160).collect::<String>()),
                false => one,
            }
        };
        for b in &self.blocks {
            match &b.score {
                Some(s) => writeln!(
                    f,
                    "{} ({}): does as you did on {} of {} ({}; before: {}). Keeps {} of the {} you wanted, drops {} of the {} you rejected.",
                    b.id,
                    b.prompt,
                    s.wanted_kept + s.rejected_dropped,
                    s.wanted + s.rejected,
                    percent(s.rate),
                    percent(s.before),
                    s.wanted_kept,
                    s.wanted,
                    s.rejected_dropped,
                    s.rejected
                )?,
                None => writeln!(
                    f,
                    "{} ({}): {} draft(s) to read beside yours.",
                    b.id,
                    b.prompt,
                    b.cases.len()
                )?,
            }
            if b.score.is_some() {
                continue;
            }
            for c in &b.cases {
                let yours = match c.founder {
                    "edit" => format!("edited to: {}", cut(&c.edited)),
                    "approve" => format!("approved: {}", cut(&c.draft)),
                    _ => format!("{} ({}): {}", c.founder, c.said.trim(), cut(&c.draft)),
                };
                writeln!(f, "  item {}, {}", c.item, c.title)?;
                writeln!(f, "    yours, {yours}")?;
                writeln!(f, "    new: {}", cut(c.new.as_deref().unwrap_or_default()))?;
            }
        }
        for s in &self.skipped {
            writeln!(f, "not measured: {s}")?;
        }
        write!(f, "Total: {}", self.cost)
    }
}

fn percent(rate: Option<f64>) -> String {
    rate.map_or("no case".into(), |r| format!("{:.0}%", r * 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(founder: &'static str, kept: Option<bool>) -> Case {
        Case {
            item: 1,
            at: String::new(),
            title: String::new(),
            founder,
            said: String::new(),
            draft: String::new(),
            edited: String::new(),
            kept,
            score: None,
            new: None,
        }
    }

    /// What the founder approved, edited or asked again, they wanted; a
    /// case the block could not answer counts for nothing.
    #[test]
    fn a_keeping_block_is_scored_on_the_founders_answers() {
        let cases = [
            case("approve", Some(true)),
            case("edit", Some(false)),
            case("redo", Some(true)),
            case("reject", Some(false)),
            case("reject", Some(false)),
            case("reject", Some(true)),
            case("reject", None),
        ];
        let s = score(&cases);
        assert_eq!(
            (s.wanted, s.wanted_kept, s.rejected, s.rejected_dropped),
            (3, 2, 3, 2)
        );
        assert_eq!(s.rate, Some(4.0 / 6.0));
        assert_eq!(s.before, Some(0.5), "before, it kept every case");
        assert_eq!(score(&[]), Score::default());
    }
}
