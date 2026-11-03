//! The HQ in the new interface (decision 53): one section per business,
//! never a total across them. Health, vitals, finance and latest activity
//! are put into words by code from what Airth already records, never by a
//! model. Vitals from a business's own MCP (`get_vitals`) and finance from
//! its bank (decision 54) come later; until then they say so.

use super::*;
use crate::business::store::queue::Item as QueueItem;
use crate::business::store::routines::BlockRow;
use crate::business::store::status::RoutinesWeek;
use crate::routine::{Graph, Kind};
use chrono::{DateTime, Duration, Local, Utc};
use std::collections::BTreeMap;

/// Runs shown under Latest activity.
const ACTIVITY: usize = 3;

impl App {
    /// The HQ's route; None when the request is not it.
    pub(super) fn hq_routes(
        &self,
        _request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["hq"]) => json_response(200, self.hq(Utc::now())?),
            _ => return Ok(None),
        }))
    }

    fn hq(&self, now: DateTime<Utc>) -> Result<Value> {
        let mut businesses = Vec::new();
        for realm in self.hub.home().realms()? {
            if realm.config.archived {
                continue;
            }
            businesses.push(self.hq_section(&realm, now)?);
        }
        let needs: u64 = businesses.iter().filter_map(|b| b["needs"].as_u64()).sum();
        let watch = businesses
            .iter()
            .filter(|b| b["health"]["state"] != "healthy" && b["health"]["state"] != "not started")
            .count();
        let headline = match (businesses.len(), watch) {
            (0, _) => "No business yet.".to_string(),
            (_, 0) => "Your businesses are healthy.".to_string(),
            (_, 1) => "One business to watch.".to_string(),
            (_, n) => format!("{n} businesses to watch."),
        };
        Ok(json!({"headline": headline, "needs": needs, "businesses": businesses}))
    }

    /// One business's section.
    fn hq_section(&self, realm: &Realm, now: DateTime<Utc>) -> Result<Value> {
        let store = Store::of(realm)?;
        let graphs: BTreeMap<String, Graph> = self
            .hub
            .with_routine_checks(realm, crate::routine::load_all)?
            .into_iter()
            .filter_map(|(name, g)| g.ok().map(|g| (name, g)))
            .collect();
        let week = store.routines_week(now - Duration::days(7))?;
        let pending = store.queue(&["pending"])?;
        let root = realm.root().ok();
        let mut activity = Vec::new();
        for (routine, run) in store.latest_runs(ACTIVITY)? {
            let blocks = store.run_blocks(&routine, &run)?;
            let graph = graphs.get(&routine);
            let agent = graph.map(|g| g.agent.clone()).unwrap_or_default();
            let title = root
                .as_ref()
                .and_then(|r| std::fs::read_to_string(r.join(format!("agents/{agent}.md"))).ok())
                .and_then(|t| crate::frontmatter::parse(&t).text("title"));
            let at = blocks
                .last()
                .and_then(|b| DateTime::parse_from_rfc3339(&b.started_at).ok())
                .map(|t| when(t.with_timezone(&Utc), now))
                .unwrap_or_default();
            activity.push(json!({
                "agent": agent,
                "agent_title": title,
                "sentence": sentence(&routine, graph, &blocks),
                "at": at,
            }));
        }
        let (state, why) = health(graphs.len(), &pending);
        Ok(json!({
            "name": realm.name(),
            "description": realm.config.description,
            "needs": pending.len(),
            "health": {"state": state, "sentence": why},
            // A business with no routine has nothing Airth measures yet.
            "vitals": if graphs.is_empty() { Vec::new() } else { vitals(&graphs, &week, &pending) },
            "time": crate::business::store::time::human(week.founder_seconds as u64),
            "cost_usd": week.cost_usd,
            "activity": activity,
        }))
    }
}

/// When a run happened, as the HQ says it: "07:30" today, else the weekday.
fn when(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let (at, now) = (at.with_timezone(&Local), now.with_timezone(&Local));
    if at.date_naive() == now.date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%a").to_string()
    }
}

/// Health from the criteria that have data (decision 53): the goal's pace
/// and the runway are unknown until a measured goal and a bank exist, so
/// only an open blocking flag can make a business one to watch.
fn health(routines: usize, pending: &[QueueItem]) -> (&'static str, String) {
    let unknown = "Goal pace and runway unknown: no measured goal, no bank connected.";
    let held = pending
        .iter()
        .filter(|i| i.flag.as_deref() == Some("blocking"))
        .count();
    match (routines, held) {
        (0, _) => ("not started", format!("No routine yet. {unknown}")),
        (_, 0) => (
            "healthy",
            format!("Nothing is held by the check. {unknown}"),
        ),
        (_, 1) => ("watch", format!("One item is held by the check. {unknown}")),
        (_, n) => (
            "watch",
            format!("{n} items are held by the check. {unknown}"),
        ),
    }
}

/// At most four vitals from what Airth records: what the founder approved
/// this week in each routine that asks them, what was dropped, and the
/// check's open flags. A vital is out of range only when something was
/// dropped or is held.
fn vitals(
    graphs: &BTreeMap<String, Graph>,
    week: &RoutinesWeek,
    pending: &[QueueItem],
) -> Vec<Value> {
    let asking = graphs.iter().filter(|(_, g)| {
        g.blocks
            .iter()
            .any(|b| matches!(b.kind, Kind::Approval { .. }))
    });
    let mut out: Vec<Value> = asking
        .take(2)
        .map(|(name, _)| {
            let approved = week
                .routines
                .iter()
                .find(|r| &r.routine == name)
                .map_or(0, |r| r.approved);
            json!({"label": name, "value": approved.to_string(), "note": "Approved this week", "out": false})
        })
        .collect();
    let dropped: i64 = week.dropped.iter().map(|d| d.count).sum();
    out.push(json!({
        "label": "Dropped this week",
        "value": dropped.to_string(),
        "note": week.dropped.first().map_or("Nothing dropped".to_string(), |d| d.reason.clone()),
        "out": dropped > 0,
    }));
    let flagged: Vec<&QueueItem> = pending
        .iter()
        .filter(|i| matches!(i.flag.as_deref(), Some("blocking" | "note")))
        .collect();
    let held = flagged
        .iter()
        .find(|i| i.flag.as_deref() == Some("blocking"));
    out.push(json!({
        "label": "Quality",
        "value": match flagged.len() { 0 => "No flag".to_string(), 1 => "1 flag".to_string(), n => format!("{n} flags") },
        "note": flagged.first().and_then(|i| i.flag_reason.clone()).unwrap_or_else(|| "Nothing to check".into()),
        "out": held.is_some(),
    }));
    out.truncate(4);
    out
}

/// A run in one sentence, from its record, by code: what it read, kept,
/// sent to the founder and did, or where it failed.
fn sentence(routine: &str, graph: Option<&Graph>, blocks: &[BlockRow]) -> String {
    let kind = |id: &str| {
        graph
            .and_then(|g| g.blocks.iter().find(|b| b.id == id))
            .map(|b| match &b.kind {
                Kind::Read { .. } => "read",
                Kind::Ai { .. } => "ai",
                Kind::Condition { .. } => "condition",
                Kind::Approval { .. } => "approval",
                Kind::Act { .. } => "act",
            })
            .unwrap_or("")
    };
    if let Some(failed) = blocks.iter().find(|b| b.outcome == "failed") {
        return format!("{routine} failed at {}", failed.block);
    }
    let mut parts = Vec::new();
    let read = blocks.iter().find(|b| kind(&b.block) == "read");
    if let Some(r) = read {
        parts.push(format!("read {}", r.items_out));
    }
    if let Some(a) = blocks.iter().find(|b| kind(&b.block) == "ai")
        && read.is_none_or(|r| a.items_out < r.items_out)
    {
        parts.push(format!("kept {}", a.items_out));
    }
    let asked: i64 = blocks
        .iter()
        .filter(|b| kind(&b.block) == "approval")
        .map(|b| b.items_in)
        .sum();
    if asked > 0 {
        parts.push(format!("{asked} sent to you"));
    }
    for act in blocks.iter().filter(|b| kind(&b.block) == "act") {
        parts.push(format!("{}: {} done", act.block, act.items_out));
    }
    match parts.is_empty() {
        true => format!("{routine} ran"),
        false => format!("{routine} {}", parts.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::autonomy::NewItem;
    use crate::business::store::queue::Answer;
    use crate::business::store::routines::BlockRun;

    const POSTS: &str = r#"{"agent": "writer", "trigger": {"manual": true}, "blocks": [
        {"id": "news", "type": "read", "source": "rss", "feeds": ["https://e.com/feed"]},
        {"id": "draft", "type": "ai", "prompt": "prompts/post.md"},
        {"id": "ok", "type": "approval", "show": "answer.draft"},
        {"id": "send", "type": "act", "action": "airth.report", "fields": {"title": "x"}}]}"#;

    fn row(block: &str, outcome: &str, items_in: i64, items_out: i64) -> BlockRow {
        BlockRow {
            block: block.into(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            outcome: outcome.into(),
            items_in,
            items_out,
            cost_usd: None,
            error: None,
        }
    }

    /// Decision 53: a run's activity sentence is built by code from its
    /// record, never by a model.
    #[test]
    fn a_runs_sentence_is_built_from_its_record() {
        let graph: Graph = serde_json::from_str(POSTS).unwrap();
        let ran = [
            row("news", "ok", 1, 58),
            row("draft", "ok", 58, 3),
            row("ok", "waiting", 3, 0),
            row("send", "ok", 1, 1),
        ];
        assert_eq!(
            sentence("daily-posts", Some(&graph), &ran),
            "daily-posts read 58, kept 3, 3 sent to you, send: 1 done"
        );
        let failed = [row("news", "ok", 1, 58), row("send", "failed", 1, 0)];
        assert_eq!(
            sentence("daily-posts", Some(&graph), &failed),
            "daily-posts failed at send"
        );
        assert_eq!(sentence("gone", None, &[row("x", "ok", 1, 1)]), "gone ran");
    }

    /// The HQ: one section per business; health from the criteria with
    /// data (here the check's flags), the rest said unknown; vitals from
    /// what Airth records; a business with no routine is not started.
    #[test]
    fn the_hq_shows_each_business_from_its_records() {
        let dir = tempfile::tempdir().unwrap();
        let home = crate::home::Home::at(dir.path()).unwrap();
        let realm = |name: &str| {
            home.create_realm(crate::home::NewRealm {
                name: name.into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap()
        };
        let (media, auctions) = (realm("media"), realm("auctions"));
        let root = media.root().unwrap();
        for (path, text) in [
            ("agents/writer.md", "---\ntitle: Post writer\n---\nDraft."),
            ("prompts/post.md", "Draft."),
            ("prompts/post.schema.json", r#"{"type": "object"}"#),
            ("routines/posts.json", POSTS),
        ] {
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), text).unwrap();
        }
        let mut store = Store::of(&media).unwrap();
        for (block, outcome, items_in, items_out) in [
            ("news", "ok", 1, 58),
            ("draft", "ok", 58, 3),
            ("ok", "waiting", 3, 0),
        ] {
            store
                .record_block(&BlockRun {
                    routine: "posts",
                    run: "r1",
                    block,
                    started_at: Utc::now(),
                    outcome,
                    items_in,
                    items_out,
                    cost_usd: None,
                    error: None,
                    dropped: None,
                })
                .unwrap();
        }
        let item = |store: &mut Store, flag: Option<(&str, &str)>| {
            store
                .propose_flagged(
                    &NewItem {
                        kind: "approval",
                        title: "A post",
                        agent: Some("writer"),
                        routine: Some("posts"),
                        run: Some("r1"),
                        action: None,
                        args: "{}",
                        execute_at: None,
                        source: "{}",
                        model_call: None,
                    },
                    flag,
                    None,
                )
                .unwrap()
        };
        let approved = item(&mut store, None);
        store
            .answer(approved, None, Answer::Approve, "telegram", 5)
            .unwrap();
        item(&mut store, Some(("blocking", "a call to buy")));
        let app = super::super::tests::test_app(home, 0);
        let hq = app.hq(Utc::now()).unwrap();
        assert_eq!(hq["headline"], "One business to watch.");
        assert_eq!(hq["needs"], 1);
        let section = hq["businesses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "media")
            .unwrap();
        assert_eq!(section["health"]["state"], "watch");
        assert!(
            section["health"]["sentence"]
                .as_str()
                .unwrap()
                .starts_with("One item is held by the check. Goal pace and runway unknown")
        );
        assert_eq!(section["vitals"][0]["label"], "posts");
        assert_eq!(section["vitals"][0]["value"], "1");
        assert_eq!(section["vitals"][1]["label"], "Dropped this week");
        assert_eq!(section["vitals"][2]["value"], "1 flag");
        assert_eq!(section["vitals"][2]["out"], true);
        assert_eq!(section["vitals"][2]["note"], "a call to buy");
        assert_eq!(
            section["activity"][0]["sentence"],
            "posts read 58, kept 3, 3 sent to you"
        );
        assert_eq!(section["activity"][0]["agent_title"], "Post writer");
        let quiet = hq["businesses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "auctions")
            .unwrap();
        assert_eq!(quiet["health"]["state"], "not started");
        assert!(quiet["activity"].as_array().unwrap().is_empty());
        assert!(quiet["vitals"].as_array().unwrap().is_empty());
        let _ = auctions;
    }
}
