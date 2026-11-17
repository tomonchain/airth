//! The business's own skills the founder approved (decision 149): a skill
//! is offered to agents only while its files hash to what was approved.

use super::Store;
use super::queue::log;
use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeMap;

/// A skill as the founder approved it.
#[derive(Debug, Clone, PartialEq)]
pub struct Approved {
    pub name: String,
    pub hash: String,
    /// Each file's hash, by path.
    pub files: BTreeMap<String, String>,
    pub skill_md: String,
}

impl Store {
    /// The skills approved, by name.
    pub fn approved_skills(&self) -> Result<BTreeMap<String, Approved>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, hash, files, skill_md FROM skills ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (name, hash, files, skill_md) = row?;
            let files = serde_json::from_str(&files).unwrap_or_default();
            out.insert(
                name.clone(),
                Approved {
                    name,
                    hash,
                    files,
                    skill_md,
                },
            );
        }
        Ok(out)
    }

    /// The founder approved `skill` through item `item`: it is offered from
    /// now on, as these files, and the item is done. One write, so a skill
    /// is never approved without its item saying so.
    pub fn approve_skill(&mut self, item: i64, skill: &Approved) -> Result<()> {
        self.write(|tx| {
            // Only through an answer of the founder's: an item for this
            // skill, as these files, that they approved.
            let asked: Option<(String, String)> = tx
                .query_row(
                    "SELECT json_extract(source, '$.skill'), json_extract(source, '$.hash') FROM queue
                     WHERE id = ?1 AND action = ?2 AND status = 'approved'",
                    params![item, SKILL],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if asked.as_ref() != Some(&(skill.name.clone(), skill.hash.clone())) {
                bail!("item {item} is not the founder's approval of {} as it now is", skill.name);
            }
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO skills (name, hash, files, skill_md, item, approved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(name) DO UPDATE SET hash = ?2, files = ?3, skill_md = ?4, item = ?5, approved_at = ?6",
                params![skill.name, skill.hash, serde_json::to_string(&skill.files)?, skill.skill_md, item, now],
            )?;
            tx.execute(
                "UPDATE queue SET status = 'done', revision = revision + 1 WHERE id = ?1",
                params![item],
            )?;
            log(tx, item, "done", "airth", None, "approved: the skill is offered")
        })
    }

    /// Whether the founder was ever asked about this skill as these files,
    /// whatever they answered: a version rejected is not asked again.
    pub fn skill_asked(&self, name: &str, hash: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM queue WHERE action = ?1 AND json_extract(source, '$.skill') = ?2
                 AND json_extract(source, '$.hash') = ?3 LIMIT 1",
                params![SKILL, name, hash],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

/// The action of an item asking the founder to approve a skill.
pub const SKILL: &str = "airth.skill";
