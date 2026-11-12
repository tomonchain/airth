//! Projects (decision 129): what the founder asked of the team, named and
//! described by the interlocutor, with milestones and the tasks of every
//! plan approved for it. Its steps are kept from the approval on, each with
//! its run once started, so a project shows what is still to come.

use super::Store;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Milestone {
    pub id: i64,
    pub title: String,
    /// A date, `YYYY-MM-DD`, when the plan gave one.
    pub due: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct Task {
    pub id: i64,
    pub milestone: Option<i64>,
    pub agent: String,
    pub task: String,
    /// Its run, once started.
    pub run: Option<String>,
    /// Why it never started: a task it waited for failed.
    pub skipped: Option<String>,
    /// How its run ended (`completed`, `failed`, …); none while it runs
    /// or before it starts.
    #[serde(default)]
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// `active` or `done`.
    pub status: String,
    /// The interlocutor that leads it.
    pub agent: String,
    /// The task it was planned in.
    pub run: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub milestones: Vec<Milestone>,
    pub tasks: Vec<Task>,
}

/// One step of an approved plan, to keep in its project.
pub struct NewTask<'a> {
    pub agent: &'a str,
    pub task: &'a str,
    /// The milestone it serves, by its place in [`Additions::milestones`].
    pub milestone: Option<usize>,
}

/// What an approved plan brings to a project.
pub struct Additions<'a> {
    /// Title and date. One of the same title as a milestone the project has
    /// is that milestone.
    pub milestones: &'a [(String, Option<String>)],
    pub tasks: &'a [NewTask<'a>],
}

impl Store {
    /// A new project with what its first plan brings: its id, and its
    /// tasks' ids in the plan's order.
    pub fn create_project(
        &mut self,
        name: &str,
        description: &str,
        agent: &str,
        run: Option<&str>,
        add: &Additions,
        now: DateTime<Utc>,
    ) -> Result<(i64, Vec<i64>)> {
        if name.trim().is_empty() {
            bail!("a project has a name");
        }
        self.write(|tx| {
            let now = now.to_rfc3339();
            tx.execute(
                "INSERT INTO projects (name, description, status, agent, run, created_at, updated_at)
                 VALUES (?1, ?2, 'active', ?3, ?4, ?5, ?5)",
                params![name.trim(), description.trim(), agent, run, now],
            )?;
            let id = tx.last_insert_rowid();
            let tasks = add_to(tx, id, add, &now)?;
            Ok((id, tasks))
        })
    }

    /// Another plan approved for project `id`: its milestones and tasks are
    /// added, and the project is active again. The new tasks' ids.
    pub fn add_to_project(
        &mut self,
        id: i64,
        add: &Additions,
        now: DateTime<Utc>,
    ) -> Result<Vec<i64>> {
        self.write(|tx| {
            let now = now.to_rfc3339();
            if tx.execute(
                "UPDATE projects SET status = 'active', updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )? == 0
            {
                bail!("there is no project {id}");
            }
            add_to(tx, id, add, &now)
        })
    }

    /// Project task `task` started, as run `run`.
    pub fn set_project_task_run(&mut self, task: i64, run: &str) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "UPDATE project_tasks SET run = ?2 WHERE id = ?1",
                params![task, run],
            )?;
            Ok(())
        })
    }

    /// Project task `task`'s run ended, as `outcome`.
    pub fn end_project_task(&mut self, task: i64, outcome: &str) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "UPDATE project_tasks SET outcome = ?2 WHERE id = ?1",
                params![task, outcome],
            )?;
            Ok(())
        })
    }

    /// Project task `task` will not start, and why.
    pub fn skip_project_task(&mut self, task: i64, why: &str) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "UPDATE project_tasks SET skipped = ?2 WHERE id = ?1",
                params![task, why],
            )?;
            Ok(())
        })
    }

    /// The founder's words for project `id`: its name, its description,
    /// whether it is done. What is not given stays.
    pub fn edit_project(
        &mut self,
        id: i64,
        name: Option<&str>,
        description: Option<&str>,
        status: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        if name.is_some_and(|n| n.trim().is_empty()) {
            bail!("a project has a name");
        }
        if let Some(s) = status
            && !matches!(s, "active" | "done")
        {
            bail!("a project is active or done, not '{s}'");
        }
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE projects SET name = COALESCE(?2, name), description = COALESCE(?3, description),
                 status = COALESCE(?4, status), updated_at = ?5 WHERE id = ?1",
                params![id, name.map(str::trim), description.map(str::trim), status, now.to_rfc3339()],
            )?;
            if changed == 0 {
                bail!("there is no project {id}");
            }
            Ok(())
        })
    }

    pub fn project(&self, id: i64) -> Result<Option<Project>> {
        let head = self
            .conn
            .query_row(
                "SELECT id, name, description, status, agent, run, created_at, updated_at FROM projects WHERE id = ?1",
                [id],
                head,
            )
            .optional()?;
        head.map(|p| self.filled(p)).transpose()
    }

    /// Every project, the one touched last first.
    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, status, agent, run, created_at, updated_at FROM projects
             ORDER BY updated_at DESC, id DESC",
        )?;
        let heads = stmt
            .query_map([], head)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        heads.into_iter().map(|p| self.filled(p)).collect()
    }

    fn filled(&self, mut p: Project) -> Result<Project> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, due FROM project_milestones WHERE project = ?1 ORDER BY id",
        )?;
        p.milestones = stmt
            .query_map([p.id], |r| {
                Ok(Milestone {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    due: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, milestone, agent, task, run, skipped, outcome FROM project_tasks WHERE project = ?1 ORDER BY id",
        )?;
        p.tasks = stmt
            .query_map([p.id], |r| {
                Ok(Task {
                    id: r.get(0)?,
                    milestone: r.get(1)?,
                    agent: r.get(2)?,
                    task: r.get(3)?,
                    run: r.get(4)?,
                    skipped: r.get(5)?,
                    outcome: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(p)
    }
}

fn head(r: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        status: r.get(3)?,
        agent: r.get(4)?,
        run: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        milestones: Vec::new(),
        tasks: Vec::new(),
    })
}

/// The plan's milestones (an existing one found by its title, its date
/// updated when the plan gives one) and tasks, in project `id`.
fn add_to(tx: &Transaction, id: i64, add: &Additions, now: &str) -> Result<Vec<i64>> {
    let mut milestones = Vec::new();
    for (title, due) in add.milestones {
        let title = title.trim();
        if title.is_empty() {
            bail!("a milestone has a title");
        }
        let known: Option<i64> = tx
            .query_row(
                "SELECT id FROM project_milestones WHERE project = ?1 AND lower(title) = lower(?2)",
                params![id, title],
                |r| r.get(0),
            )
            .optional()?;
        let m = match known {
            Some(m) => {
                tx.execute(
                    "UPDATE project_milestones SET due = COALESCE(?2, due) WHERE id = ?1",
                    params![m, due],
                )?;
                m
            }
            None => {
                tx.execute(
                    "INSERT INTO project_milestones (project, title, due) VALUES (?1, ?2, ?3)",
                    params![id, title, due],
                )?;
                tx.last_insert_rowid()
            }
        };
        milestones.push(m);
    }
    let mut tasks = Vec::new();
    for t in add.tasks {
        let milestone = match t.milestone {
            Some(i) => match milestones.get(i) {
                Some(m) => Some(*m),
                None => bail!("a task names a milestone the plan does not have"),
            },
            None => None,
        };
        tx.execute(
            "INSERT INTO project_tasks (project, milestone, agent, task, added_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, milestone, t.agent, t.task, now],
        )?;
        tasks.push(tx.last_insert_rowid());
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("business.db")).unwrap();
        (dir, store)
    }

    #[test]
    fn a_project_keeps_its_milestones_and_every_plans_tasks() {
        let (_dir, mut store) = store();
        let now = Utc::now();
        let milestones = [("The article".to_string(), Some("2026-12-15".to_string()))];
        let tasks = [
            NewTask {
                agent: "writer",
                task: "A recap article",
                milestone: Some(0),
            },
            NewTask {
                agent: "developer",
                task: "A banner",
                milestone: None,
            },
        ];
        let (id, ids) = store
            .create_project(
                " Year-end recap ",
                "Hedgen's 2026, told everywhere.",
                "chief-of-staff",
                Some("r1"),
                &Additions {
                    milestones: &milestones,
                    tasks: &tasks,
                },
                now,
            )
            .unwrap();
        assert_eq!(ids.len(), 2);
        store.set_project_task_run(ids[0], "r2").unwrap();
        store.end_project_task(ids[0], "completed").unwrap();
        store.skip_project_task(ids[1], "writer failed").unwrap();
        store
            .edit_project(id, None, None, Some("done"), now)
            .unwrap();

        // A later plan for it: the same milestone by its title, a new one,
        // and the project active again.
        let more_milestones = [
            ("the article".to_string(), None),
            ("Everywhere".to_string(), Some("2026-12-20".to_string())),
        ];
        let more = [
            NewTask {
                agent: "post-writer",
                task: "A LinkedIn post",
                milestone: Some(1),
            },
            NewTask {
                agent: "writer",
                task: "Sources first",
                milestone: Some(0),
            },
        ];
        store
            .add_to_project(
                id,
                &Additions {
                    milestones: &more_milestones,
                    tasks: &more,
                },
                now,
            )
            .unwrap();

        let p = store.project(id).unwrap().unwrap();
        assert_eq!(p.name, "Year-end recap");
        assert_eq!(p.status, "active");
        assert_eq!(p.run.as_deref(), Some("r1"));
        let titles: Vec<(&str, Option<&str>)> = p
            .milestones
            .iter()
            .map(|m| (m.title.as_str(), m.due.as_deref()))
            .collect();
        assert_eq!(
            titles,
            [
                ("The article", Some("2026-12-15")),
                ("Everywhere", Some("2026-12-20"))
            ]
        );
        assert_eq!(p.tasks.len(), 4);
        assert_eq!(p.tasks[0].run.as_deref(), Some("r2"));
        assert_eq!(p.tasks[0].outcome.as_deref(), Some("completed"));
        assert_eq!(p.tasks[2].outcome, None);
        assert_eq!(p.tasks[1].skipped.as_deref(), Some("writer failed"));
        assert_eq!(p.tasks[2].milestone, Some(p.milestones[1].id));
        assert_eq!(p.tasks[3].milestone, Some(p.milestones[0].id));
        assert_eq!(store.projects().unwrap().len(), 1);
    }

    #[test]
    fn a_project_refuses_what_it_cannot_hold() {
        let (_dir, mut store) = store();
        let now = Utc::now();
        let none = Additions {
            milestones: &[],
            tasks: &[],
        };
        assert!(
            store
                .create_project(" ", "", "cos", None, &none, now)
                .is_err()
        );
        let stray = [NewTask {
            agent: "writer",
            task: "x",
            milestone: Some(3),
        }];
        let bad = Additions {
            milestones: &[],
            tasks: &stray,
        };
        assert!(
            store
                .create_project("A", "", "cos", None, &bad, now)
                .is_err()
        );
        // Nothing of the refused project was kept.
        assert!(store.projects().unwrap().is_empty());
        let (id, _) = store
            .create_project("A", "", "cos", None, &none, now)
            .unwrap();
        assert!(
            store
                .edit_project(id, None, None, Some("paused"), now)
                .is_err()
        );
        assert!(store.edit_project(id, Some(""), None, None, now).is_err());
        assert!(
            store
                .edit_project(id + 1, Some("B"), None, None, now)
                .is_err()
        );
        assert!(store.add_to_project(id + 1, &none, now).is_err());
        store
            .edit_project(id, Some("B"), Some("Why"), None, now)
            .unwrap();
        let p = store.project(id).unwrap().unwrap();
        assert_eq!((p.name.as_str(), p.description.as_str()), ("B", "Why"));
    }
}
