//! Earned autonomy (decision 153): an agent's track record on an action,
//! read from the founder's answers the queue already keeps, and the
//! promotions Airth proposed from it. Nothing new is stored for the record.

use super::Store;
use super::autonomy::NewItem;
use super::queue::log;
use anyhow::Result;
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeMap;

/// The founder's answers on what leads to one action of one agent, newest
/// first, since the last promotion proposed for it.
#[derive(Debug, Default, PartialEq)]
pub struct Record {
    /// Approved as they were, in a row.
    pub count: usize,
    /// When the oldest and the newest of them were made.
    pub first: Option<String>,
    pub last: Option<String>,
    /// How many each routine's approval brought: its replay asks as many.
    pub routines: BTreeMap<String, usize>,
    /// What ended the run, and when: `edit`, `rejection` or `incident`.
    pub ended_by: Option<(&'static str, String)>,
    /// A proposal for this agent and action waits for the founder.
    pub pending: bool,
}

impl Store {
    /// `agent`'s record on `action`: the items of the approval blocks that
    /// guard it, `guards` (routine, block), and the action's own items at
    /// level 1. An approval without edit adds one; an edit, a rejection, or
    /// an incident on the agent and action ends the run. Only answers after
    /// the last proposal count, so the founder is asked again only after
    /// another run.
    pub fn track_record(
        &self,
        agent: &str,
        action: &str,
        guards: &[(String, String)],
    ) -> Result<Record> {
        let mut out = Record::default();
        let last: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT created_at, status FROM queue WHERE kind = 'promotion'
                 AND agent = ?1 AND action = ?2 ORDER BY id DESC LIMIT 1",
                params![agent, action],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        out.pending = last.as_ref().is_some_and(|(_, s)| s == "pending");
        let floor = last.map(|(at, _)| at).unwrap_or_default();
        let incident: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(created_at) FROM queue WHERE kind = 'incident'
                 AND agent = ?1 AND action = ?2 AND created_at > ?3",
                params![agent, action, floor],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        // The founder's own word on each item: an approval by Airth (a step
        // that stepped aside) is none.
        let answers = "SELECT id, created_at, routine, status,
               EXISTS (SELECT 1 FROM queue_log l WHERE l.item = q.id AND l.by = 'founder'
                       AND l.action IN ('approved', 'overridden')),
               EXISTS (SELECT 1 FROM queue_log l WHERE l.item = q.id
                       AND l.action IN ('edited', 'flag lifted by edit'))
             FROM queue q WHERE kind = 'approval' AND agent = ?1 AND created_at > ?2";
        type Row = (i64, String, Option<String>, String, bool, bool);
        let read = |r: &rusqlite::Row| -> rusqlite::Result<Row> {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        };
        let mut rows: Vec<Row> = self
            .conn
            .prepare(&format!("{answers} AND action = ?3"))?
            .query_map(params![agent, floor, action], read)?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.conn.prepare(&format!(
            "{answers} AND routine = ?3 AND json_extract(source, '$.block') = ?4"
        ))?;
        for (routine, block) in guards {
            let found = stmt.query_map(params![agent, floor, routine, block], read)?;
            rows.extend(found.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        rows.sort_by(|a, b| (&b.1, b.0).cmp(&(&a.1, a.0)));
        rows.dedup_by_key(|r| r.0);
        for (_, at, routine, status, approved, edited) in rows {
            if let Some(when) = incident.as_ref().filter(|i| **i > at) {
                out.ended_by = Some(("incident", when.clone()));
                break;
            }
            let stop = match status.as_str() {
                "rejected" => Some("rejection"),
                "approved" | "done" | "failed" if edited => Some("edit"),
                "approved" | "done" | "failed" if approved => None,
                // Not answered, or not by the founder: it says nothing.
                _ => continue,
            };
            if let Some(what) = stop {
                out.ended_by = Some((what, at));
                break;
            }
            out.count += 1;
            out.last.get_or_insert_with(|| at.clone());
            out.first = Some(at);
            if let Some(r) = routine {
                *out.routines.entry(r).or_default() += 1;
            }
        }
        if out.ended_by.is_none()
            && let Some(when) = incident
        {
            out.ended_by = Some(("incident", when));
        }
        Ok(out)
    }

    /// A promotion Airth weighed and did not propose, its replay having
    /// disagreed with the founder: kept, never shown, so the run starts
    /// again after it (decision 153). Its id.
    pub fn withhold_promotion(&mut self, item: &NewItem, why: &str) -> Result<i64> {
        self.write(|tx| {
            tx.execute(
                "INSERT INTO queue (created_at, kind, title, agent, routine, run, action, args, original_args, source, status)
                 VALUES (?1, 'promotion', ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'expired')",
                params![Utc::now().to_rfc3339(), item.title, item.agent, item.routine, item.run,
                    item.action, item.args, item.source],
            )?;
            let id = tx.last_insert_rowid();
            log(tx, id, "withheld", "airth", None, why)?;
            Ok(id)
        })
    }

    /// Move an item's date, as tests age what they made.
    #[cfg(test)]
    pub fn backdate(&mut self, id: i64, at: chrono::DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE queue SET created_at = ?2 WHERE id = ?1",
            params![id, at.to_rfc3339()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;
    use crate::business::store::queue::Answer;
    use chrono::Duration;

    const ACTION: &str = "typefully.create_draft";

    fn guards() -> Vec<(String, String)> {
        vec![("posts".into(), "ok".into())]
    }

    /// A routine's draft at its approval block `block`, made `days` ago.
    fn draft(s: &mut Store, block: &str, days: i64) -> i64 {
        let id = s
            .propose(&NewItem {
                kind: "approval",
                title: "A post",
                agent: Some("writer"),
                routine: Some("posts"),
                run: Some("r1"),
                action: None,
                args: r#"{"answer": {"draft": "Hello"}}"#,
                execute_at: None,
                source: &format!(
                    r#"{{"block": "{block}", "show": "answer.draft", "editable": "answer.draft"}}"#
                ),
                model_call: None,
            })
            .unwrap();
        s.backdate(id, Utc::now() - Duration::days(days)).unwrap();
        id
    }

    /// The action's own item at level 1, or an incident on it.
    fn own(s: &mut Store, kind: &str, days: i64) -> i64 {
        let id = s
            .propose(&NewItem {
                kind,
                title: "writer: typefully.create_draft",
                agent: Some("writer"),
                routine: None,
                run: Some("t1"),
                action: Some(ACTION),
                args: "{}",
                execute_at: None,
                source: r#"{"tool": "http_request"}"#,
                model_call: None,
            })
            .unwrap();
        s.backdate(id, Utc::now() - Duration::days(days)).unwrap();
        id
    }

    fn approve(s: &mut Store, id: i64) {
        s.answer(id, None, Answer::Approve, "app", 1).unwrap();
    }

    fn open() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join(FILE)).unwrap();
        (dir, s)
    }

    #[test]
    fn approvals_count_newest_first_from_the_guarding_block_and_the_actions_own_items() {
        let (_dir, mut s) = open();
        for days in [9, 6, 3] {
            let id = draft(&mut s, "ok", days);
            approve(&mut s, id);
        }
        let own_item = own(&mut s, "approval", 1);
        approve(&mut s, own_item);
        // Another block, still pending, or another agent's: nothing.
        let other = draft(&mut s, "other", 2);
        approve(&mut s, other);
        draft(&mut s, "ok", 0);
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!(r.count, 4);
        assert!(r.first.unwrap() < r.last.clone().unwrap());
        assert!(
            r.last.unwrap() > (Utc::now() - Duration::days(2)).to_rfc3339(),
            "the newest is the action's own item"
        );
        assert_eq!(r.routines, BTreeMap::from([("posts".to_string(), 3)]));
        assert_eq!(r.ended_by, None);
        assert_eq!(
            s.track_record("reviewer", ACTION, &guards()).unwrap().count,
            0
        );
    }

    #[test]
    fn an_edit_a_rejection_or_an_incident_ends_the_run() {
        let (_dir, mut s) = open();
        let old = draft(&mut s, "ok", 10);
        approve(&mut s, old);
        let edited = draft(&mut s, "ok", 8);
        s.answer(edited, None, Answer::Edit("Mine".into()), "app", 1)
            .unwrap();
        approve(&mut s, edited);
        for days in [7, 6] {
            let id = draft(&mut s, "ok", days);
            approve(&mut s, id);
        }
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!((r.count, r.ended_by.unwrap().0), (2, "edit"));

        let rejected = draft(&mut s, "ok", 5);
        s.answer(rejected, None, Answer::Reject("no".into()), "app", 1)
            .unwrap();
        let after = draft(&mut s, "ok", 4);
        approve(&mut s, after);
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!((r.count, r.ended_by.unwrap().0), (1, "rejection"));

        own(&mut s, "incident", 3);
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!((r.count, r.ended_by.unwrap().0), (0, "incident"));
        let last = draft(&mut s, "ok", 2);
        approve(&mut s, last);
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!((r.count, r.ended_by.unwrap().0), (1, "incident"));
    }

    #[test]
    fn only_answers_after_the_last_proposal_count() {
        let (_dir, mut s) = open();
        for days in [9, 8] {
            let id = draft(&mut s, "ok", days);
            approve(&mut s, id);
        }
        let item = |title: &'static str| NewItem {
            kind: "promotion",
            title,
            agent: Some("writer"),
            routine: None,
            run: None,
            action: Some(ACTION),
            args: "{}",
            execute_at: None,
            source: "{}",
            model_call: None,
        };
        let proposed = s.propose(&item("Allow?")).unwrap();
        s.backdate(proposed, Utc::now() - Duration::days(7))
            .unwrap();
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!((r.count, r.pending), (0, true));
        s.answer(proposed, None, Answer::Reject("not now".into()), "app", 1)
            .unwrap();
        let id = draft(&mut s, "ok", 6);
        approve(&mut s, id);
        let r = s.track_record("writer", ACTION, &guards()).unwrap();
        assert_eq!((r.count, r.pending), (1, false));
        // A withheld one is a floor too, and never shown.
        s.withhold_promotion(&item("Allow?"), "the replay disagreed")
            .unwrap();
        assert_eq!(
            s.track_record("writer", ACTION, &guards()).unwrap().count,
            0
        );
        assert!(
            s.queue(&["pending"]).unwrap().is_empty(),
            "nothing waits for the founder"
        );
    }
}
