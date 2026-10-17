//! A realm's projects: goals the owner pursues there, each with milestones
//! (the stages) and tasks (the concrete work, on a board). One project may
//! be the realm's focus: its North Star (see northstar.rs, a view of it).
//! Kept in `realms/<name>/projects.json`; a North Star saved by an older
//! Airth (`northstar.json`) becomes the focus project on first read.
//!
//! Tasks Airth works on move by themselves: a chat started from a task puts
//! it in progress, its change waiting for review puts it in review, and the
//! change adopted (or its pull request merged) puts it done.

use crate::northstar::{Milestone, NorthStar, Status};
use crate::realm::write_atomic;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_NAME: usize = 80;
const MAX_TEXT: usize = 500;
const MAX_NOTES: usize = 4000;
const MAX_PROJECTS: usize = 50;
const MAX_TASKS: usize = 300;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    #[default]
    Active,
    Paused,
    Done,
}

impl ProjectStatus {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "active" => Self::Active,
            "paused" => Self::Paused,
            "done" => Self::Done,
            _ => bail!("unknown project status '{s}' (active, paused, done)"),
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Todo,
    Doing,
    Review,
    Done,
}

impl TaskStatus {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "todo" => Self::Todo,
            "doing" => Self::Doing,
            "review" => Self::Review,
            "done" => Self::Done,
            _ => bail!("unknown task status '{s}' (todo, doing, review, done)"),
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Review => "review",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// Stable short id in its project: "t1", "t2"...
    pub id: String,
    pub title: String,
    /// What it takes, what done means: the chat that works on it reads this.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default)]
    pub status: TaskStatus,
    /// The milestone it serves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    /// The realm repository its chat works in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The latest chat that worked on it, and all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<String>,
    /// The latest word on it (why it went back to do, for instance).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Short id, unique in the realm, from its name: "launch-site".
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub goal: String,
    /// Why it matters, or what success looks like.
    #[serde(default)]
    pub why: String,
    /// The realm's North Star.
    #[serde(default)]
    pub focus: bool,
    #[serde(default)]
    pub status: ProjectStatus,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    pub created_at: DateTime<Utc>,
    /// When the goal was last set or reworded.
    pub set_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    /// The project as a North Star (its goal and milestones).
    pub fn as_north_star(&self) -> NorthStar {
        NorthStar {
            goal: self.goal.clone(),
            why: self.why.clone(),
            milestones: self.milestones.clone(),
            set_at: self.set_at,
            updated_at: self.updated_at,
        }
    }

    pub fn task(&self, id: &str) -> Result<&Task> {
        self.tasks
            .iter()
            .find(|t| t.id == id)
            .with_context(|| format!("project '{}' has no task '{id}'", self.id))
    }

    /// What a chat of this project is told about it (and about its task).
    pub fn prompt_section(&self, task: Option<&str>) -> String {
        let mut out = String::new();
        if !self.focus {
            out.push_str(&format!(
                "This chat belongs to the project \"{}\": {}\n",
                self.name, self.goal
            ));
            if !self.why.trim().is_empty() {
                out.push_str(&format!("Why it matters: {}\n", self.why.trim()));
            }
            if !self.milestones.is_empty() {
                out.push_str(&format!(
                    "Its milestones:\n{}\n",
                    self.as_north_star().render_milestones()
                ));
            }
        }
        if let Some(t) = task.and_then(|t| self.task(t).ok()) {
            out.push_str(&format!(
                "You are working on its task {}: {}\n",
                t.id, t.title
            ));
            if let Some(m) = t
                .milestone
                .as_deref()
                .and_then(|m| self.milestones.iter().find(|x| x.id == m))
            {
                out.push_str(&format!("It serves the milestone {}: {}\n", m.id, m.title));
            }
            if !t.notes.trim().is_empty() {
                out.push_str(&format!("Details:\n{}\n", t.notes.trim()));
            }
            if !t.note.trim().is_empty() {
                out.push_str(&format!("Last time: {}\n", t.note.trim()));
            }
            out.push_str("Do this task, and only this task: finish it, verify it, and end with a short report the owner can review.");
        }
        out
    }
}

/// What a project's task becomes after something happened to its chat.
pub struct TaskUpdate<'a> {
    pub status: TaskStatus,
    pub run: Option<&'a str>,
    pub note: Option<String>,
}

static LOCK: Mutex<()> = Mutex::new(());

pub struct ProjectStore {
    path: PathBuf,
    /// A North Star saved by an older Airth, read once into the focus project.
    legacy: PathBuf,
}

impl ProjectStore {
    /// The store of the realm whose folder is `dir`.
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("projects.json"),
            legacy: dir.join("northstar.json"),
        }
    }

    pub fn list(&self) -> Result<Vec<Project>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.migrated(),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    /// The projects an older Airth kept as a North Star alone.
    fn migrated(&self) -> Result<Vec<Project>> {
        let text = match std::fs::read_to_string(&self.legacy) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let star: NorthStar = serde_json::from_str(&text)
            .with_context(|| format!("parsing {}", self.legacy.display()))?;
        let mut p = new_project(&[], &short_name(&star.goal), &star.goal, &star.why)?;
        p.focus = true;
        p.milestones = star.milestones;
        p.set_at = star.set_at;
        p.created_at = star.set_at;
        p.updated_at = star.updated_at;
        Ok(vec![p])
    }

    fn save(&self, list: &[Project]) -> Result<()> {
        write_atomic(&self.path, &serde_json::to_string_pretty(list)?)
    }

    pub fn get(&self, id: &str) -> Result<Project> {
        self.list()?
            .into_iter()
            .find(|p| p.id == id)
            .with_context(|| format!("no project '{id}' in this realm"))
    }

    /// The realm's North Star, if one project is the focus.
    pub fn focus(&self) -> Result<Option<Project>> {
        Ok(self.list()?.into_iter().find(|p| p.focus))
    }

    /// Change the projects under the lock, and save them.
    pub fn modify<T>(&self, f: impl FnOnce(&mut Vec<Project>) -> Result<T>) -> Result<T> {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut list = self.list()?;
        let out = f(&mut list)?;
        self.save(&list)?;
        Ok(out)
    }

    /// Change one project under the lock.
    pub fn update<T>(&self, id: &str, f: impl FnOnce(&mut Project) -> Result<T>) -> Result<T> {
        self.modify(|list| {
            let p = list
                .iter_mut()
                .find(|p| p.id == id)
                .with_context(|| format!("no project '{id}' in this realm"))?;
            let out = f(p)?;
            p.updated_at = Utc::now();
            Ok(out)
        })
    }

    pub fn create(&self, name: &str, goal: &str, why: &str) -> Result<Project> {
        self.modify(|list| {
            if list.len() >= MAX_PROJECTS {
                bail!("a realm holds at most {MAX_PROJECTS} projects");
            }
            let p = new_project(list, name, goal, why)?;
            list.push(p.clone());
            Ok(p)
        })
    }

    /// Rename a project or change its goal, why or status.
    pub fn edit(
        &self,
        id: &str,
        name: Option<&str>,
        goal: Option<&str>,
        why: Option<&str>,
        status: Option<ProjectStatus>,
    ) -> Result<Project> {
        self.update(id, |p| {
            if let Some(name) = name {
                let name = name.trim();
                if name.is_empty() {
                    bail!("a project needs a name");
                }
                check_len("the name", name, MAX_NAME)?;
                p.name = name.to_string();
            }
            if let Some(goal) = goal {
                check_len("the goal", goal.trim(), MAX_TEXT)?;
                if p.goal != goal.trim() {
                    p.set_at = Utc::now();
                }
                p.goal = goal.trim().to_string();
            }
            if let Some(why) = why {
                check_len("the why", why.trim(), MAX_TEXT)?;
                p.why = why.trim().to_string();
            }
            if let Some(s) = status {
                p.status = s;
            }
            Ok(p.clone())
        })
    }

    /// Make a project the realm's North Star (None: no North Star).
    pub fn set_focus(&self, id: Option<&str>) -> Result<()> {
        self.modify(|list| {
            if let Some(id) = id
                && !list.iter().any(|p| p.id == id)
            {
                bail!("no project '{id}' in this realm");
            }
            for p in list.iter_mut() {
                p.focus = Some(p.id.as_str()) == id;
            }
            Ok(())
        })
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        self.modify(|list| {
            let before = list.len();
            list.retain(|p| p.id != id);
            if list.len() == before {
                bail!("no project '{id}' in this realm");
            }
            Ok(())
        })
    }

    /// Add a task to the project's board, to do.
    pub fn add_task(
        &self,
        project: &str,
        title: &str,
        notes: &str,
        milestone: Option<&str>,
        repo: Option<&str>,
    ) -> Result<Task> {
        let title = title.trim();
        if title.is_empty() {
            bail!("a task needs a title");
        }
        check_len("the title", title, MAX_TEXT)?;
        check_len("the notes", notes.trim(), MAX_NOTES)?;
        self.update(project, |p| {
            if p.tasks.len() >= MAX_TASKS {
                bail!("a project holds at most {MAX_TASKS} tasks");
            }
            if let Some(m) = milestone
                && !p.milestones.iter().any(|x| x.id == m)
            {
                bail!("project '{}' has no milestone '{m}'", p.id);
            }
            let n = p
                .tasks
                .iter()
                .filter_map(|t| t.id.strip_prefix('t')?.parse::<u32>().ok())
                .max()
                .unwrap_or(0);
            let now = Utc::now();
            let task = Task {
                id: format!("t{}", n + 1),
                title: title.to_string(),
                notes: notes.trim().to_string(),
                status: TaskStatus::Todo,
                milestone: milestone.map(String::from),
                repo: repo.map(String::from),
                run: None,
                runs: Vec::new(),
                note: String::new(),
                created_at: now,
                updated_at: now,
                done_at: None,
            };
            p.tasks.push(task.clone());
            Ok(task)
        })
    }

    /// Edit a task (fields left None stay; an empty milestone or repo clears it).
    #[allow(clippy::too_many_arguments)]
    pub fn edit_task(
        &self,
        project: &str,
        task: &str,
        title: Option<&str>,
        notes: Option<&str>,
        status: Option<TaskStatus>,
        milestone: Option<&str>,
        repo: Option<&str>,
    ) -> Result<Task> {
        self.update(project, |p| {
            let milestones: Vec<String> = p.milestones.iter().map(|m| m.id.clone()).collect();
            let t = p
                .tasks
                .iter_mut()
                .find(|t| t.id == task)
                .with_context(|| format!("no task '{task}'"))?;
            if let Some(title) = title {
                let title = title.trim();
                if title.is_empty() {
                    bail!("a task needs a title");
                }
                check_len("the title", title, MAX_TEXT)?;
                t.title = title.to_string();
            }
            if let Some(notes) = notes {
                check_len("the notes", notes.trim(), MAX_NOTES)?;
                t.notes = notes.trim().to_string();
            }
            if let Some(m) = milestone {
                if !m.is_empty() && !milestones.iter().any(|x| x == m) {
                    bail!("no milestone '{m}'");
                }
                t.milestone = (!m.is_empty()).then(|| m.to_string());
            }
            if let Some(r) = repo {
                t.repo = (!r.is_empty()).then(|| r.to_string());
            }
            if let Some(s) = status {
                set_status(t, s);
            }
            t.updated_at = Utc::now();
            Ok(t.clone())
        })
    }

    pub fn remove_task(&self, project: &str, task: &str) -> Result<()> {
        self.update(project, |p| {
            let before = p.tasks.len();
            p.tasks.retain(|t| t.id != task);
            if p.tasks.len() == before {
                bail!("no task '{task}'");
            }
            Ok(())
        })
    }

    /// Record what happened to a task's chat. A task the owner moved since
    /// (done by hand, say) is left alone unless its chat started again.
    pub fn record(&self, project: &str, task: &str, update: TaskUpdate) -> Result<()> {
        self.update(project, |p| {
            let t = p
                .tasks
                .iter_mut()
                .find(|t| t.id == task)
                .with_context(|| format!("no task '{task}'"))?;
            if let Some(run) = update.run {
                if t.run.as_deref() != Some(run) && update.status != TaskStatus::Doing {
                    // An older chat of the task: its news is stale.
                    return Ok(());
                }
                t.run = Some(run.to_string());
                if !t.runs.iter().any(|r| r == run) {
                    t.runs.push(run.to_string());
                }
            }
            set_status(t, update.status);
            if let Some(note) = update.note {
                t.note = note.chars().take(MAX_TEXT).collect();
            }
            t.updated_at = Utc::now();
            Ok(())
        })
    }

    /// Replace a project's milestones (title, done when), in order. One with
    /// the same title as before keeps its id, status and history.
    pub fn set_milestones(&self, project: &str, items: &[(String, String)]) -> Result<Project> {
        crate::northstar::check_milestones(items)?;
        self.update(project, |p| {
            crate::northstar::merge_milestones(&mut p.milestones, items);
            Ok(p.clone())
        })
    }

    /// Record progress on one of a project's milestones.
    pub fn update_milestone(
        &self,
        project: &str,
        id: &str,
        status: Option<Status>,
        note: Option<&str>,
        run: Option<&str>,
    ) -> Result<Project> {
        if let Some(note) = note {
            check_len("the note", note.trim(), MAX_TEXT)?;
        }
        self.update(project, |p| {
            crate::northstar::update_milestone(&mut p.milestones, id, status, note, run)?;
            Ok(p.clone())
        })
    }
}

fn set_status(t: &mut Task, s: TaskStatus) {
    if s == TaskStatus::Done && t.status != TaskStatus::Done {
        t.done_at = Some(Utc::now());
    } else if s != TaskStatus::Done {
        t.done_at = None;
    }
    t.status = s;
}

/// A new project, its id made from its name and unique among `existing`.
pub fn new_project(existing: &[Project], name: &str, goal: &str, why: &str) -> Result<Project> {
    let name = name.trim();
    if name.is_empty() {
        bail!("a project needs a name");
    }
    check_len("the name", name, MAX_NAME)?;
    check_len("the goal", goal.trim(), MAX_TEXT)?;
    check_len("the why", why.trim(), MAX_TEXT)?;
    let base: String = slug(name);
    let base = if base.is_empty() {
        "project".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while existing.iter().any(|p| p.id == id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    let now = Utc::now();
    Ok(Project {
        id,
        name: name.to_string(),
        goal: goal.trim().to_string(),
        why: why.trim().to_string(),
        focus: false,
        status: ProjectStatus::Active,
        milestones: Vec::new(),
        tasks: Vec::new(),
        created_at: now,
        set_at: now,
        updated_at: now,
    })
}

/// A short name for a project known by its goal only.
pub fn short_name(goal: &str) -> String {
    let goal = goal.trim();
    if goal.chars().count() <= 60 {
        return goal.to_string();
    }
    let cut: String = goal.chars().take(60).collect();
    match cut.rfind(' ') {
        Some(i) if i > 20 => format!("{}…", &cut[..i]),
        _ => format!("{cut}…"),
    }
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for word in text
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(4)
    {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(word);
    }
    out.chars().take(40).collect()
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

    #[test]
    fn projects_hold_tasks_that_move_with_their_chats() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        assert!(store.list().unwrap().is_empty());
        let site = store
            .create("Launch the site", "Site live by May", "")
            .unwrap();
        assert_eq!(site.id, "launch-the-site");
        let again = store.create("Launch the site", "", "").unwrap();
        assert_eq!(again.id, "launch-the-site-2");
        assert!(store.create("  ", "", "").is_err());

        store
            .set_milestones(&site.id, &[("Landing page".into(), String::new())])
            .unwrap();
        let t1 = store
            .add_task(
                &site.id,
                "Write the hero copy",
                "Short, in French",
                Some("m1"),
                None,
            )
            .unwrap();
        let t2 = store
            .add_task(&site.id, "Pick a domain", "", None, None)
            .unwrap();
        assert_eq!((t1.id.as_str(), t2.id.as_str()), ("t1", "t2"));
        assert!(store.add_task(&site.id, "x", "", Some("m9"), None).is_err());

        // A chat starts on t1, its change waits for review, then is adopted.
        let doing = |run, status| TaskUpdate {
            status,
            run: Some(run),
            note: None,
        };
        store
            .record(&site.id, "t1", doing("r1", TaskStatus::Doing))
            .unwrap();
        store
            .record(&site.id, "t1", doing("r1", TaskStatus::Review))
            .unwrap();
        // News from an older chat of the task is ignored.
        store
            .record(&site.id, "t1", doing("r0", TaskStatus::Todo))
            .unwrap();
        let t = store.get(&site.id).unwrap().tasks[0].clone();
        assert_eq!(
            (t.status, t.run.as_deref()),
            (TaskStatus::Review, Some("r1"))
        );
        store
            .record(&site.id, "t1", doing("r1", TaskStatus::Done))
            .unwrap();
        let p = store.get(&site.id).unwrap();
        assert!(p.tasks[0].done_at.is_some());
        let section = p.prompt_section(Some("t1"));
        assert!(
            section.contains("task t1: Write the hero copy"),
            "{section}"
        );
        assert!(section.contains("milestone m1: Landing page"), "{section}");
        assert!(section.contains("Short, in French"), "{section}");

        store
            .edit_task(
                &site.id,
                "t2",
                Some("Buy the domain"),
                None,
                Some(TaskStatus::Doing),
                None,
                Some("site"),
            )
            .unwrap();
        store.remove_task(&site.id, "t1").unwrap();
        let p = store.get(&site.id).unwrap();
        assert_eq!(p.tasks.len(), 1);
        assert_eq!(p.tasks[0].repo.as_deref(), Some("site"));

        store.set_focus(Some(&site.id)).unwrap();
        assert_eq!(store.focus().unwrap().unwrap().id, site.id);
        store.set_focus(None).unwrap();
        assert!(store.focus().unwrap().is_none());
        store.remove(&again.id).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn an_older_north_star_becomes_the_focus_project() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = r#"{"goal":"100 kit sales by December","why":"Fund the next product",
            "milestones":[{"id":"m1","title":"Shop live","status":"done","note":"","runs":["r1"]}],
            "set_at":"2026-10-10T10:00:00Z","updated_at":"2026-10-11T10:00:00Z"}"#;
        std::fs::write(dir.path().join("northstar.json"), legacy).unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.focus().unwrap().unwrap();
        assert_eq!(p.name, "100 kit sales by December");
        assert_eq!(p.id, "100-kit-sales-by");
        assert_eq!(p.why, "Fund the next product");
        assert_eq!(p.milestones[0].runs, vec!["r1"]);
        // Once saved, projects.json is what counts.
        store
            .add_task(&p.id, "Post on Instagram", "", None, None)
            .unwrap();
        std::fs::remove_file(dir.path().join("northstar.json")).unwrap();
        assert_eq!(store.focus().unwrap().unwrap().tasks.len(), 1);
    }
}
