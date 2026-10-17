//! A realm's North Star: the one goal the owner wants the realm's work to
//! serve, the milestones toward it (brainstormed with Airth), and which runs
//! moved each one. It is the realm's focus project (see projects.rs).
//!
//! Only the owner sets or changes the goal. Runs see it in their briefing and
//! record milestones and progress through the `north_star` tool, under the
//! realm's memory rule.

use crate::projects::ProjectStore;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The permission scope of North Star updates (capability `memory`).
pub const SCOPE: &str = "north star";
/// The scope of rewording the goal itself: never granted ahead (the
/// scheduled objective gets `SCOPE` only), so the owner approves each change.
pub const GOAL_SCOPE: &str = "north star goal";
/// The objective that advances the North Star on a schedule.
pub const OBJECTIVE_ID: &str = "north-star";

const MAX_GOAL: usize = 500;
const MAX_TITLE: usize = 200;
const MAX_NOTE: usize = 500;
const MAX_MILESTONES: usize = 12;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Todo,
    Doing,
    Done,
    Dropped,
}

impl Status {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "todo" => Status::Todo,
            "doing" => Status::Doing,
            "done" => Status::Done,
            "dropped" => Status::Dropped,
            _ => bail!("unknown milestone status '{s}' (todo, doing, done, dropped)"),
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Todo => "todo",
            Status::Doing => "doing",
            Status::Done => "done",
            Status::Dropped => "dropped",
        }
    }

    fn mark(self) -> &'static str {
        match self {
            Status::Todo => "[ ]",
            Status::Doing => "[~]",
            Status::Done => "[x]",
            Status::Dropped => "[-]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Milestone {
    /// Stable short id, "m1", "m2"...
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: Status,
    /// How the owner and Airth will know it is reached.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub done_when: String,
    /// The latest progress note.
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub done_at: Option<DateTime<Utc>>,
    /// Runs that moved this milestone.
    #[serde(default)]
    pub runs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NorthStar {
    pub goal: String,
    /// Why it matters, or what success looks like.
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    pub set_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NorthStar {
    /// Milestones done, and milestones that count (dropped ones do not).
    pub fn progress(&self) -> (usize, usize) {
        let counted = self
            .milestones
            .iter()
            .filter(|m| m.status != Status::Dropped);
        let total = counted.clone().count();
        (counted.filter(|m| m.status == Status::Done).count(), total)
    }

    /// The milestone to work on: the first one in progress, else the first open one.
    pub fn next(&self) -> Option<&Milestone> {
        let find = |s| self.milestones.iter().find(|m| m.status == s);
        find(Status::Doing).or_else(|| find(Status::Todo))
    }

    /// Whether a run moved one of the milestones.
    pub fn involves(&self, run: &str) -> bool {
        self.milestones
            .iter()
            .any(|m| m.runs.iter().any(|r| r == run))
    }

    /// The milestones as a checklist.
    pub fn render_milestones(&self) -> String {
        self.milestones
            .iter()
            .map(|m| {
                let mut line = format!("{} {}. {}", m.status.mark(), m.id, m.title);
                if !m.done_when.is_empty() {
                    line.push_str(&format!(" (done when: {})", m.done_when));
                }
                if !m.note.is_empty() {
                    line.push_str(&format!(" — {}", m.note));
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// What every run of the realm is told about it.
    pub fn prompt_section(&self) -> String {
        let mut out = format!(
            "The owner's North Star in this realm, the one goal the work here serves:\n{}\n",
            self.goal
        );
        if !self.why.trim().is_empty() {
            out.push_str(&format!("Why it matters: {}\n", self.why.trim()));
        }
        if self.milestones.is_empty() {
            out.push_str("No milestones yet. If the owner asks you to define them, propose 3 to 7 and record them with north_star (action set_milestones) once the owner agrees.\n");
        } else {
            let (done, total) = self.progress();
            out.push_str(&format!(
                "Milestones ({done} of {total} done):\n{}\n",
                self.render_milestones()
            ));
        }
        out.push_str("Keep it in sight. Do what the owner asks; when you have a choice, choose what moves the next open milestone forward, and say how your work relates to it. Do not take on unrelated work unprompted. When your run advances a milestone, record it with north_star (action update: the milestone id, a one-line note, and status doing, or done once its outcome is verified). The goal is the owner's: you may propose a sharper wording, and record it (north_star set_goal) only once they agree to it.");
        out
    }
}

/// The realm's North Star: a view of its focus project (see projects.rs).
pub struct NorthStarStore {
    projects: ProjectStore,
}

impl NorthStarStore {
    /// `path` is the realm's `northstar.json`; the projects live next to it.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let dir = path.as_ref().parent().unwrap_or(Path::new("/"));
        Self {
            projects: ProjectStore::new(dir),
        }
    }

    pub fn get(&self) -> Result<Option<NorthStar>> {
        Ok(self.projects.focus()?.map(|p| p.as_north_star()))
    }

    fn focus_id(&self) -> Result<String> {
        Ok(self
            .projects
            .focus()?
            .context("no North Star is set in this realm (only the owner sets it)")?
            .id)
    }

    /// Set (or reword) the goal. Milestones are kept. Without a North Star,
    /// this starts a project that is one.
    pub fn set_goal(&self, goal: &str, why: &str) -> Result<NorthStar> {
        let goal = goal.trim();
        if goal.is_empty() {
            bail!("the North Star is empty");
        }
        check_len("the North Star", goal, MAX_GOAL)?;
        check_len("the why", why.trim(), MAX_GOAL)?;
        self.projects.modify(|list| {
            let now = Utc::now();
            let index = match list.iter().position(|p| p.focus) {
                Some(i) => i,
                None => {
                    let mut p = crate::projects::new_project(
                        list,
                        &crate::projects::short_name(goal),
                        goal,
                        why,
                    )?;
                    p.focus = true;
                    list.push(p);
                    list.len() - 1
                }
            };
            let p = &mut list[index];
            if p.goal != goal {
                p.set_at = now;
            }
            p.goal = goal.to_string();
            p.why = why.trim().to_string();
            p.updated_at = now;
            Ok(p.as_north_star())
        })
    }

    /// Stop having a North Star; false if there was none. Its project stays,
    /// with its milestones and tasks.
    pub fn clear(&self) -> Result<bool> {
        if self.projects.focus()?.is_none() {
            return Ok(false);
        }
        self.projects.set_focus(None)?;
        Ok(true)
    }

    /// Replace the milestones (title, done when), in order. A milestone with
    /// the same title as an existing one keeps its id, status and history.
    pub fn set_milestones(&self, items: &[(String, String)]) -> Result<NorthStar> {
        check_milestones(items)?;
        let id = self.focus_id()?;
        Ok(self.projects.set_milestones(&id, items)?.as_north_star())
    }

    /// Record progress on a milestone: a new status, a note, and the run.
    pub fn update(
        &self,
        id: &str,
        status: Option<Status>,
        note: Option<&str>,
        run: Option<&str>,
    ) -> Result<NorthStar> {
        if let Some(note) = note {
            check_len("the note", note.trim(), MAX_NOTE)?;
        }
        let project = self.focus_id()?;
        Ok(self
            .projects
            .update_milestone(&project, id, status, note, run)?
            .as_north_star())
    }
}

/// Milestones as given (title, done when) are valid.
pub(crate) fn check_milestones(items: &[(String, String)]) -> Result<()> {
    if items.is_empty() || items.len() > MAX_MILESTONES {
        bail!("give between 1 and {MAX_MILESTONES} milestones");
    }
    for (title, done_when) in items {
        if title.trim().is_empty() {
            bail!("a milestone has no title");
        }
        check_len("a milestone title", title.trim(), MAX_TITLE)?;
        check_len("a milestone's done when", done_when.trim(), MAX_NOTE)?;
    }
    Ok(())
}

/// Replace `milestones` with `items`, in order; one with the same title as
/// before keeps its id, status and history.
pub(crate) fn merge_milestones(milestones: &mut Vec<Milestone>, items: &[(String, String)]) {
    let key = |t: &str| t.trim().to_lowercase();
    let mut old = std::mem::take(milestones);
    let mut next_id = old
        .iter()
        .filter_map(|m| m.id.strip_prefix('m')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    for (title, done_when) in items {
        let milestone = match old.iter().position(|m| key(&m.title) == key(title)) {
            Some(i) => {
                let mut m = old.remove(i);
                m.title = title.trim().to_string();
                if !done_when.trim().is_empty() {
                    m.done_when = done_when.trim().to_string();
                }
                m
            }
            None => {
                next_id += 1;
                Milestone {
                    id: format!("m{next_id}"),
                    title: title.trim().to_string(),
                    status: Status::Todo,
                    done_when: done_when.trim().to_string(),
                    note: String::new(),
                    done_at: None,
                    // Credited to runs that move it, not to the one defining it.
                    runs: Vec::new(),
                }
            }
        };
        milestones.push(milestone);
    }
}

/// Record progress on one of `milestones`: a new status, a note, and the run.
pub(crate) fn update_milestone(
    milestones: &mut [Milestone],
    id: &str,
    status: Option<Status>,
    note: Option<&str>,
    run: Option<&str>,
) -> Result<()> {
    let ids: Vec<String> = milestones.iter().map(|m| m.id.clone()).collect();
    let m = milestones
        .iter_mut()
        .find(|m| m.id == id)
        .with_context(|| {
            if ids.is_empty() {
                "there are no milestones yet: brainstorm them first".to_string()
            } else {
                format!("no milestone '{id}' (milestones: {})", ids.join(", "))
            }
        })?;
    if let Some(status) = status {
        if status == Status::Done && m.status != Status::Done {
            m.done_at = Some(Utc::now());
        } else if status != Status::Done {
            m.done_at = None;
        }
        m.status = status;
    }
    if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
        m.note = note.to_string();
    }
    if let Some(run) = run
        && !m.runs.iter().any(|r| r == run)
    {
        m.runs.push(run.to_string());
    }
    Ok(())
}

fn check_len(what: &str, text: &str, max: usize) -> Result<()> {
    if text.chars().count() > max {
        bail!("{what} is too long (at most {max} characters)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(titles: &[&str]) -> Vec<(String, String)> {
        titles
            .iter()
            .map(|t| (t.to_string(), String::new()))
            .collect()
    }

    #[test]
    fn milestones_track_progress_and_keep_their_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = NorthStarStore::new(dir.path().join("northstar.json"));
        assert!(store.get().unwrap().is_none());
        assert!(store.update("m1", None, None, None).is_err(), "no goal yet");
        assert!(store.set_goal("  ", "").is_err());

        store
            .set_goal("100 kit sales by December", "Fund the next product")
            .unwrap();
        let star = store
            .set_milestones(&items(&["Landing page live", "First sale", "SEO funnel"]))
            .unwrap();
        assert_eq!(star.progress(), (0, 3));
        assert_eq!(star.next().unwrap().id, "m1");

        store
            .update(
                "m1",
                Some(Status::Done),
                Some("Deployed on Vercel"),
                Some("r2"),
            )
            .unwrap();
        let star = store
            .update("m2", Some(Status::Doing), None, Some("r2"))
            .unwrap();
        assert_eq!(star.progress(), (1, 3));
        assert_eq!(star.next().unwrap().id, "m2");
        // Runs that moved a milestone are credited, not the one that listed them.
        assert!(star.involves("r2") && !star.involves("r1") && !star.involves("r3"));
        assert!(star.milestones[0].done_at.is_some());
        assert!(
            store
                .update("m9", None, None, None)
                .unwrap_err()
                .to_string()
                .contains("m1, m2, m3")
        );

        // Reworking the list keeps what matches, drops the rest, adds new ids.
        let star = store
            .set_milestones(&items(&["first sale", "Newsletter", "Landing page live"]))
            .unwrap();
        let ids: Vec<&str> = star.milestones.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["m2", "m4", "m1"]);
        assert_eq!(star.milestones[0].status, Status::Doing);
        assert_eq!(star.milestones[2].note, "Deployed on Vercel");

        // Rewording the goal keeps the milestones; dropped ones do not count.
        store.set_goal("150 kit sales by December", "").unwrap();
        let star = store
            .update("m4", Some(Status::Dropped), None, None)
            .unwrap();
        assert_eq!(star.goal, "150 kit sales by December");
        assert_eq!(star.progress(), (1, 2));

        let section = star.prompt_section();
        assert!(section.contains("150 kit sales"), "{section}");
        assert!(
            section.contains("[x] m1. Landing page live — Deployed on Vercel"),
            "{section}"
        );
        assert!(section.contains("1 of 2 done"), "{section}");

        assert!(store.set_milestones(&[]).is_err());
        assert!(store.clear().unwrap());
        assert!(!store.clear().unwrap());
    }
}
