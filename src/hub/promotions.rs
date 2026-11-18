//! Earned autonomy (decision 153): when an agent's record on an action
//! earns it, and today's prompts agree with the founder on that record,
//! Airth proposes in Needs you that the action go one level up. Only the
//! founder's Allow sets the level.

use super::*;
use crate::business::store::Store;
use crate::business::store::autonomy::{DEFAULT_LEVEL, NewItem};
use crate::routine::Kind;
use crate::supervise::Locked;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Approvals without edit in a row, and the days they span at least.
const IN_A_ROW: usize = 12;
const OVER_DAYS: i64 = 7;

/// What a routine's replay found on the founder's answers (decision 151).
#[derive(Default)]
struct Replay {
    cases: usize,
    /// Cases where every keeping block does as the founder did.
    agree: usize,
    cost_usd: f64,
}

impl Hub {
    /// For each business, the promotions earned, at most once an hour;
    /// what happened, by business.
    pub fn review_promotions(&self) -> Result<Vec<String>> {
        static LAST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
        {
            let mut last = LAST.locked();
            if last.is_some_and(|at| at.elapsed() < Duration::from_secs(3600)) {
                return Ok(Vec::new());
            }
            *last = Some(Instant::now());
        }
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived || realm.root().is_err() {
                continue;
            }
            match self.propose_promotions(&realm) {
                Ok(done) => out.extend(done.iter().map(|d| format!("{}: {d}", realm.name()))),
                Err(e) => out.push(format!("{}: promotions: {e:#}", realm.name())),
            }
        }
        Ok(out)
    }

    /// Propose each promotion an agent earned in `realm`: 12 approvals
    /// without edit in a row over a week, under the action's ceiling, and a
    /// replay of the routines that brought them agreeing on every case.
    /// What was proposed or withheld.
    pub fn propose_promotions(&self, realm: &Realm) -> Result<Vec<String>> {
        let root = realm.root()?;
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        let declared = crate::actions::all(&realm.config.integrations, &servers);
        // The approval blocks guarding each agent's actions, and the
        // routines a replay can check: those with a keeping AI block.
        let mut guards: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
        let mut keeping = BTreeSet::new();
        for (name, graph) in self.with_routine_checks(realm, crate::routine::load_all)? {
            let Ok(graph) = graph else { continue };
            if graph.blocks.iter().any(|b| {
                matches!(&b.kind, Kind::Ai { keep, rank, .. } if keep.is_some() || rank.is_some())
            }) {
                keeping.insert(name.clone());
            }
            for (block, acts) in crate::routine::guarded(&graph) {
                for act in acts {
                    let at = (graph.agent.clone(), act);
                    guards
                        .entry(at)
                        .or_default()
                        .push((name.clone(), block.clone()));
                }
            }
        }
        let mut store = Store::of(realm)?;
        let mut out = Vec::new();
        for (agent, _) in crate::business::bots(&root)? {
            // A report or a suggestion is the founder's to judge, never
            // the agent's to earn.
            for d in declared
                .iter()
                .filter(|d| !crate::routine::for_founder(&d.id, &BTreeMap::new()))
            {
                let from = store.level(&agent, &d.id, d.ceiling, DEFAULT_LEVEL)?;
                if from >= d.ceiling {
                    continue;
                }
                let key = (agent.clone(), d.id.clone());
                let guarding = guards.get(&key).map(Vec::as_slice).unwrap_or_default();
                let record = store.track_record(&agent, &d.id, guarding)?;
                let date = |at: &Option<String>| {
                    at.as_deref()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|t| t.with_timezone(&Utc))
                };
                let (Some(first), Some(last)) = (date(&record.first), date(&record.last)) else {
                    continue;
                };
                // Nothing is proposed while the founder has yet to answer
                // something of the record, or the last proposal.
                if record.pending
                    || record.waiting
                    || record.count < IN_A_ROW
                    || last - first < chrono::Duration::days(OVER_DAYS)
                {
                    continue;
                }
                // What a routine's prompts would do today, on what the
                // founder approved at the blocks guarding the action: a
                // prompt changed since the run began does not ride on it.
                let mut replay: Option<Replay> = None;
                for (routine, ids) in record.cases.iter().filter(|(r, _)| keeping.contains(*r)) {
                    let Some(r) = self.replay(realm, routine, ids)? else {
                        continue;
                    };
                    let sum = replay.get_or_insert_with(Replay::default);
                    sum.cases += r.cases;
                    sum.agree += r.agree;
                    sum.cost_usd += r.cost_usd;
                }
                let to = from + 1;
                let title = super::orders::title(&root, &agent).unwrap_or_else(|| agent.clone());
                let window = cancel_minutes(realm, &d.id);
                let mut args =
                    proposal(&agent, &d.id, from, &record, replay.as_ref(), window, first);
                // Allow holds only if the level did not change since.
                args["stamp"] = store.level_stamp(&agent, &d.id)?.into();
                let ask = match to {
                    1 => format!("{title}: {} once you approve?", words(&d.id)),
                    _ => format!("{title}: {} without asking?", words(&d.id)),
                };
                let item = NewItem {
                    kind: "promotion",
                    title: &ask,
                    agent: Some(&agent),
                    routine: None,
                    run: None,
                    action: Some(&d.id),
                    args: &args.to_string(),
                    execute_at: None,
                    source: r#"{"show": "text"}"#,
                    model_call: None,
                };
                match replay.filter(|r| r.agree < r.cases) {
                    None => {
                        let id = store.propose(&item)?;
                        out.push(format!("proposed: {ask} (item {id})"));
                    }
                    Some(r) => {
                        let why = format!(
                            "today's prompts disagree with you on {} of {} cases",
                            r.cases - r.agree,
                            r.cases
                        );
                        store.withhold_promotion(&item, &why)?;
                        out.push(format!("not proposed: {ask} {why}"));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Ask the routine's AI blocks again about the founder's answers to
    /// `items`, and count the cases where every keeping block does as the
    /// founder did. A case a block could not answer does not agree. None if
    /// no keeping block was asked about them: there is nothing to check.
    fn replay(&self, realm: &Realm, routine: &str, items: &[i64]) -> Result<Option<Replay>> {
        let eval = self.evaluate_items(realm, routine, items)?;
        let keeps: Vec<_> = eval
            .blocks
            .iter()
            .filter(|b| b.kind == "keeps" && !b.cases.is_empty())
            .collect();
        if keeps.is_empty() {
            return Ok(None);
        }
        let mut ids: BTreeSet<i64> = eval
            .blocks
            .iter()
            .flat_map(|b| &b.cases)
            .map(|c| c.item)
            .collect();
        for block in &keeps {
            ids.retain(|id| {
                block
                    .cases
                    .iter()
                    .any(|c| c.item == *id && c.kept == Some(c.founder != "reject"))
            });
        }
        Ok(Some(Replay {
            cases: eval.cases,
            agree: ids.len(),
            cost_usd: eval.cost_usd,
        }))
    }

    /// The founder allowed promotion `item`: the agent's level for its
    /// action is set to the one proposed, never above its ceiling, unless
    /// the level changed since. What it did.
    pub(super) fn accept_promotion(
        &self,
        realm: &Realm,
        item: &crate::business::store::queue::Item,
    ) -> Result<String> {
        let (Some(agent), Some(action)) = (item.agent.as_deref(), item.action.as_deref()) else {
            bail!("item {} names no agent and action", item.id);
        };
        let level = |key: &str| item.args[key].as_u64().map(|l| l.min(3) as u8);
        let (Some(from), Some(to)) = (level("from"), level("to")) else {
            bail!("item {} names no level", item.id);
        };
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        let ceiling = crate::actions::all(&realm.config.integrations, &servers)
            .into_iter()
            .find(|d| d.id == action)
            .map(|d| d.ceiling)
            .context("this action is no longer declared: nothing changed")?;
        let mut store = Store::of(realm)?;
        // Any change since it was asked, even one undone, is the founder's
        // newer word: the record it rests on is no longer what they see.
        if store.level(agent, action, ceiling, DEFAULT_LEVEL)? != from
            || item.args["stamp"].as_i64() != Some(store.level_stamp(agent, action)?)
        {
            bail!(
                "its level changed since this was asked: nothing changed. Set it on the agent's page"
            );
        }
        let why = format!("promotion allowed in Needs you, item {}", item.id);
        store.set_level(agent, action, to, ceiling, "founder", &why)?;
        Ok(format!(
            "Allowed. {}",
            item.args["change"].as_str().unwrap_or_default()
        ))
    }
}

/// An action as the founder reads it: "create draft in Typefully".
fn words(action: &str) -> String {
    let (connector, name) = action.split_once('.').unwrap_or(("airth", action));
    let name = name.replace(['_', '-'], " ");
    let mut c = connector.chars();
    match c.next() {
        Some(first) if connector != "airth" => {
            format!("{name} in {}{}", first.to_uppercase(), c.as_str())
        }
        _ => name,
    }
}

/// The minutes a delayed action of `action` waits for a Cancel.
fn cancel_minutes(realm: &Realm, action: &str) -> u32 {
    action
        .split_once('.')
        .and_then(|(connector, name)| {
            let i = realm
                .config
                .integrations
                .iter()
                .find(|i| i.name == connector)?;
            i.actions.iter().find(|a| a.name == name)?.cancel_minutes
        })
        .unwrap_or(super::autonomy::CANCEL_MINUTES)
}

/// What a promotion item holds: the record, the replay and what changes,
/// in plain words for Needs you and Telegram.
fn proposal(
    agent: &str,
    action: &str,
    from: u8,
    record: &crate::business::store::promotions::Record,
    replay: Option<&Replay>,
    window: u32,
    first: DateTime<Utc>,
) -> Value {
    let to = from + 1;
    let said = format!(
        "{} approved as they were since {}",
        record.count,
        first.format("%-d %b")
    );
    let check = match replay {
        Some(r) => format!(
            "checked again with today's prompts: {} of {} match",
            r.agree, r.cases
        ),
        None => "no step to check again: the record alone".to_string(),
    };
    let change = match to {
        1 => "It will ask you here first, then do it once you approve.".to_string(),
        2 => format!("Each one will wait {window} minutes, with Cancel, then go."),
        _ => "Each one will go on its own, then be reported to you.".to_string(),
    };
    let text = format!("{said} · {check}\n{change}");
    json!({
        "agent": agent, "action": action, "from": from, "to": to,
        "count": record.count, "first": record.first, "last": record.last,
        "routines": record.routines,
        "replay": replay.map(|r| json!({"cases": r.cases, "agree": r.agree, "cost_usd": r.cost_usd})),
        "record": said, "check": check, "change": change, "text": text,
    })
}
