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
use chrono::{DateTime, Duration, Utc};
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
            // The page says it as "2h ago", from the founder's clock.
            let at = blocks.last().map(|b| b.started_at.clone());
            activity.push(json!({
                "agent": agent,
                "agent_title": title,
                "routine": routine,
                "sentence": sentence(graph, &blocks),
                "at": at,
            }));
        }
        let finance = crate::hub::finance::shown(realm, now);
        let runway = finance
            .as_ref()
            .and_then(|f| f.get("runway_months").cloned());
        let (state, why) = health(graphs.len(), &pending, runway.as_ref());
        let approved: i64 = week.routines.iter().map(|r| r.approved).sum();
        Ok(json!({
            "name": realm.name(),
            "description": realm.config.description,
            "needs": pending.len(),
            "health": {"state": state, "sentence": why},
            "headline": headline(graphs.len(), &pending, runway.as_ref(), approved),
            "first": first(&pending).map(|i| json!({
                "id": i.id, "kind": i.kind, "title": i.title, "agent": i.agent,
                "routine": i.routine, "created_at": i.created_at, "flag_reason": i.flag_reason,
            })),
            // A business with no routine has nothing Airth measures yet.
            "finance": finance,
            "vitals": if graphs.is_empty() { Vec::new() } else { vitals(&graphs, &week, &pending) },
            "time": crate::business::store::time::human(week.founder_seconds as u64),
            "cost_usd": week.cost_usd,
            "activity": activity,
        }))
    }
}

/// The one fact a business's home leads with, most pressing first: an
/// incident, a short runway, an item held by the check; else what the
/// week produced.
fn headline(
    routines: usize,
    pending: &[QueueItem],
    runway: Option<&Value>,
    approved: i64,
) -> String {
    let incidents: Vec<&QueueItem> = pending.iter().filter(|i| i.kind == "incident").collect();
    let held = pending
        .iter()
        .filter(|i| i.flag.as_deref() == Some("blocking"))
        .count();
    let months = runway.and_then(Value::as_f64).map(f64::floor);
    if routines == 0 {
        return "No routine runs yet.".into();
    }
    match (incidents.as_slice(), months, held) {
        ([one], _, _) => return format!("{}.", capital(&one.title)),
        (many, _, _) if many.len() > 1 => return format!("{} incidents to look at.", many.len()),
        (_, Some(m), _) if m < 6.0 => return format!("Runway {m} months."),
        (_, _, 1) => return "One item is held by the check.".into(),
        (_, _, n) if n > 1 => return format!("{n} items are held by the check."),
        _ => {}
    }
    let week = match approved {
        0 => "Nothing approved yet this week".to_string(),
        n => format!("{n} approved this week"),
    };
    match months {
        Some(m) => format!("{week}, runway {m} months."),
        None => format!("{week}."),
    }
}

/// The item in Needs you to handle first: an incident, then what the
/// check holds, a legal deadline, an approval, then the rest; the oldest
/// first within each.
fn first(pending: &[QueueItem]) -> Option<&QueueItem> {
    let rank = |i: &QueueItem| match (i.kind.as_str(), i.flag.as_deref()) {
        ("incident", _) => 0,
        (_, Some("blocking")) => 1,
        ("deadline", _) => 2,
        ("approval", _) => 3,
        ("prepared", _) => 4,
        ("delayed", _) => 5,
        _ => 6,
    };
    pending
        .iter()
        .min_by(|a, b| (rank(a), &a.created_at).cmp(&(rank(b), &b.created_at)))
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Health from the criteria that have data (decision 53): an open blocking
/// flag, and the runway once finance is known (`runway`: its months,
/// null for no spending; None when unknown). A runway under 6 months is at
/// risk whatever the rest; under 12, one to watch. The goal's pace is
/// unknown until the goal is measured.
fn health(
    routines: usize,
    pending: &[QueueItem],
    runway: Option<&Value>,
) -> (&'static str, String) {
    let held = pending
        .iter()
        .filter(|i| i.flag.as_deref() == Some("blocking"))
        .count();
    let months = runway.map(|r| r.as_f64());
    let unknown = match months {
        None => "Goal pace and runway unknown: no measured goal, no finance figures.",
        Some(_) => "Goal pace unknown: no measured goal.",
    };
    let held_text = match held {
        0 => "Nothing is held by the check.".to_string(),
        1 => "One item is held by the check.".to_string(),
        n => format!("{n} items are held by the check."),
    };
    let runway_text = match months {
        Some(Some(m)) => format!(" Runway {} months.", m.floor()),
        _ => String::new(),
    };
    let state = match (routines, held, months) {
        (0, _, _) => return ("not started", format!("No routine yet. {unknown}")),
        (_, _, Some(Some(m))) if m < 6.0 => "at risk",
        (_, _, Some(Some(m))) if m < 12.0 => "watch",
        (_, 0, _) => "healthy",
        _ => "watch",
    };
    (state, format!("{held_text}{runway_text} {unknown}"))
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

/// A run in one short sentence, from its record, by code: what it read,
/// kept, sent to the founder and did, or where it failed. The routine's
/// name is shown beside it, not in it.
fn sentence(graph: Option<&Graph>, blocks: &[BlockRow]) -> String {
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
    let failed = blocks.iter().position(|b| b.outcome == "failed");
    let done = &blocks[..failed.unwrap_or(blocks.len())];
    let mut parts = Vec::new();
    let read = done.iter().find(|b| kind(&b.block) == "read");
    if let Some(r) = read {
        parts.push(match r.items_out {
            1 => "read 1 item".to_string(),
            n => format!("read {n} items"),
        });
    }
    if let Some(a) = done.iter().find(|b| kind(&b.block) == "ai")
        && read.is_none_or(|r| a.items_out < r.items_out)
    {
        parts.push(format!("kept {}", a.items_out));
    }
    let asked: i64 = done
        .iter()
        .filter(|b| kind(&b.block) == "approval")
        .map(|b| b.items_in)
        .sum();
    if asked > 0 {
        parts.push(format!("sent {asked} to you"));
    }
    for act in done.iter().filter(|b| kind(&b.block) == "act") {
        parts.push(format!("acted on {} ({})", act.items_out, act.block));
    }
    let said = match parts.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    };
    match (failed.map(|i| &blocks[i].block), said.is_empty()) {
        (Some(at), true) => format!("Failed at {at}."),
        (Some(at), false) => format!("{}, then failed at {at}.", capital(&said)),
        (None, true) => "Ran, with nothing to do.".into(),
        (None, false) => format!("{}.", capital(&said)),
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

    /// Decision 53: the runway counts once a bank is read; under 6 months
    /// is at risk whatever the rest, under 12 to watch; unknown never fails.
    #[test]
    fn the_runway_sets_health_once_the_bank_is_read() {
        let state = |runway: Option<Value>| health(1, &[], runway.as_ref()).0;
        assert_eq!(state(None), "healthy");
        assert_eq!(state(Some(json!(null))), "healthy");
        assert_eq!(state(Some(json!(29.0))), "healthy");
        assert_eq!(state(Some(json!(10.0))), "watch");
        assert_eq!(state(Some(json!(5.0))), "at risk");
        let (_, why) = health(1, &[], Some(&json!(29.4)));
        assert_eq!(
            why,
            "Nothing is held by the check. Runway 29 months. Goal pace unknown: no measured goal."
        );
    }

    fn pending(kind: &str, flag: Option<&str>, created_at: &str) -> QueueItem {
        QueueItem {
            id: created_at.len() as i64,
            created_at: created_at.into(),
            kind: kind.into(),
            title: format!("{kind} item"),
            agent: None,
            routine: None,
            run: None,
            action: None,
            args: json!({}),
            original_args: json!({}),
            source: json!({}),
            model_call: None,
            execute_at: None,
            status: "pending".into(),
            revision: 1,
            flag: flag.map(Into::into),
            flag_reason: None,
            note: None,
        }
    }

    /// The home leads with the most pressing fact: an incident, a short
    /// runway, what the check holds; else what the week produced.
    #[test]
    fn the_home_leads_with_the_most_pressing_fact() {
        let incident = pending("incident", None, "2026-11-03T08:00:00Z");
        let held = pending("approval", Some("blocking"), "2026-11-03T07:00:00Z");
        assert_eq!(headline(0, &[], None, 0), "No routine runs yet.");
        assert_eq!(
            headline(1, &[held.clone(), incident.clone()], Some(&json!(3.0)), 4),
            "Incident item."
        );
        assert_eq!(
            headline(1, std::slice::from_ref(&held), Some(&json!(3.5)), 4),
            "Runway 3 months."
        );
        assert_eq!(
            headline(1, &[held], Some(&json!(29.0)), 4),
            "One item is held by the check."
        );
        assert_eq!(
            headline(1, &[], Some(&json!(29.4)), 4),
            "4 approved this week, runway 29 months."
        );
        assert_eq!(headline(1, &[], None, 0), "Nothing approved yet this week.");
    }

    /// Needs you's first item: an incident before what the check holds,
    /// before a deadline, before an approval; the oldest within a kind.
    #[test]
    fn the_first_item_is_the_most_pressing_then_the_oldest() {
        let late = pending("approval", None, "2026-11-03T09:00:00Z");
        let early = pending("approval", None, "2026-11-01T09:00:00Z");
        let deadline = pending("deadline", None, "2026-11-03T10:00:00Z");
        let held = pending("approval", Some("blocking"), "2026-11-03T11:00:00Z");
        let incident = pending("incident", None, "2026-11-03T12:00:00Z");
        let mut items = vec![late, early.clone()];
        assert_eq!(first(&items).unwrap().created_at, early.created_at);
        for (item, kind) in [
            (deadline, "deadline"),
            (held, "approval"),
            (incident, "incident"),
        ] {
            items.push(item);
            let f = first(&items).unwrap();
            assert_eq!(
                (f.kind.as_str(), f.created_at.as_str()),
                (kind, items.last().unwrap().created_at.as_str())
            );
        }
        assert!(first(&[]).is_none());
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
            sentence(Some(&graph), &ran),
            "Read 58 items, kept 3, sent 3 to you and acted on 1 (send)."
        );
        let failed = [row("news", "ok", 1, 58), row("send", "failed", 1, 0)];
        assert_eq!(
            sentence(Some(&graph), &failed),
            "Read 58 items, then failed at send."
        );
        assert_eq!(
            sentence(Some(&graph), &[row("news", "failed", 1, 0)]),
            "Failed at news."
        );
        assert_eq!(
            sentence(None, &[row("x", "ok", 1, 1)]),
            "Ran, with nothing to do."
        );
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
            "Read 58 items, kept 3 and sent 3 to you."
        );
        assert_eq!(section["activity"][0]["routine"], "posts");
        assert!(section["activity"][0]["at"].as_str().is_some());
        assert_eq!(section["headline"], "One item is held by the check.");
        assert_eq!(section["first"]["title"], "A post");
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
