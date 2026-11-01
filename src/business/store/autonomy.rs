//! Autonomy (decision 45): each agent's level per action, and the history of
//! every change.

use super::Store;
use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};

/// A new action's level before the founder sets one.
pub const DEFAULT_LEVEL: u8 = 1;

impl Store {
    /// The agent's level for `action`, never above `ceiling`.
    pub fn level(&self, agent: &str, action: &str, ceiling: u8) -> Result<u8> {
        let set: Option<u8> = self
            .conn
            .query_row(
                "SELECT level FROM autonomy WHERE agent = ?1 AND action = ?2",
                params![agent, action],
                |r| r.get(0),
            )
            .optional()?;
        Ok(set.unwrap_or(DEFAULT_LEVEL).min(ceiling))
    }

    /// Set a level, as the founder (`by`) or an incident does. A level above
    /// the ceiling is refused, never stored.
    pub fn set_level(
        &mut self,
        agent: &str,
        action: &str,
        level: u8,
        ceiling: u8,
        by: &str,
        reason: &str,
    ) -> Result<()> {
        if level > ceiling {
            bail!("{action} cannot go above level {ceiling}");
        }
        self.write(|tx| {
            let from: Option<u8> = tx
                .query_row(
                    "SELECT level FROM autonomy WHERE agent = ?1 AND action = ?2",
                    params![agent, action],
                    |r| r.get(0),
                )
                .optional()?;
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO autonomy (agent, action, level, set_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (agent, action) DO UPDATE SET level = ?3, set_at = ?4, revision = revision + 1",
                params![agent, action, level, now],
            )?;
            tx.execute(
                "INSERT INTO autonomy_log (at, agent, action, from_level, to_level, by, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![now, agent, action, from, level, by, reason],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::store::FILE;

    #[test]
    fn levels_start_at_one_never_pass_the_ceiling_and_keep_their_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join(FILE)).unwrap();
        assert_eq!(s.level("writer", "blog.publish", 1).unwrap(), 1);
        assert_eq!(s.level("writer", "x.reply", 0).unwrap(), 0);
        assert!(
            s.set_level("writer", "blog.publish", 2, 1, "founder", "")
                .is_err()
        );
        s.set_level(
            "writer",
            "typefully.create_draft",
            3,
            3,
            "founder",
            "trusted",
        )
        .unwrap();
        assert_eq!(s.level("writer", "typefully.create_draft", 3).unwrap(), 3);
        // A lower ceiling applies at once, without rewriting the level.
        assert_eq!(s.level("writer", "typefully.create_draft", 1).unwrap(), 1);
        s.set_level(
            "writer",
            "typefully.create_draft",
            2,
            3,
            "incident",
            "failed",
        )
        .unwrap();
        let history: Vec<(Option<u8>, u8, String)> = s
            .conn
            .prepare("SELECT from_level, to_level, by FROM autonomy_log ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            history,
            vec![(None, 3, "founder".into()), (Some(3), 2, "incident".into())]
        );
    }
}
