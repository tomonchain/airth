//! The realm's `business.db` (decision 31): business state that must stay
//! right under retries and concurrent writers. It lives in the realm's
//! directory in the Airth home, never in the business folder, so runs can
//! neither read nor change it. Every write is one immediate transaction;
//! schema changes are numbered migrations, and a database written by a
//! newer Airth is refused rather than guessed at.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::path::Path;
use std::time::Duration;

/// The database file, in the realm's directory.
pub const FILE: &str = "business.db";

/// Schema changes, in order. The database's `user_version` is the number
/// applied; a change is never edited once released, only followed by another.
const MIGRATIONS: &[&str] = &[
    // 1: the owner-time ledger, append-only: it is the pilot's evidence.
    "CREATE TABLE owner_time (
        id INTEGER PRIMARY KEY,
        at TEXT NOT NULL,
        activity TEXT NOT NULL,
        seconds INTEGER NOT NULL CHECK (seconds >= 0),
        note TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX owner_time_at ON owner_time(at);
    CREATE TRIGGER owner_time_no_update BEFORE UPDATE ON owner_time
        BEGIN SELECT RAISE(ABORT, 'the owner-time ledger is append-only'); END;
    CREATE TRIGGER owner_time_no_delete BEFORE DELETE ON owner_time
        BEGIN SELECT RAISE(ABORT, 'the owner-time ledger is append-only'); END;",
    // 2: pipelines. Each run leaves one row, its heartbeat, append-only;
    // what a run kept is remembered so the next one skips it.
    "CREATE TABLE pipeline_runs (
        id INTEGER PRIMARY KEY,
        pipeline TEXT NOT NULL,
        started_at TEXT NOT NULL,
        finished_at TEXT NOT NULL,
        ok INTEGER NOT NULL,
        items_read INTEGER NOT NULL,
        items_kept INTEGER NOT NULL,
        report TEXT,
        notes TEXT NOT NULL DEFAULT '',
        error TEXT
    );
    CREATE INDEX pipeline_runs_by_name ON pipeline_runs(pipeline, started_at);
    CREATE TRIGGER pipeline_runs_no_update BEFORE UPDATE ON pipeline_runs
        BEGIN SELECT RAISE(ABORT, 'pipeline runs are append-only'); END;
    CREATE TRIGGER pipeline_runs_no_delete BEFORE DELETE ON pipeline_runs
        BEGIN SELECT RAISE(ABORT, 'pipeline runs are append-only'); END;
    CREATE TABLE pipeline_seen (
        pipeline TEXT NOT NULL,
        key TEXT NOT NULL,
        first_seen TEXT NOT NULL,
        PRIMARY KEY (pipeline, key)
    );",
    // 3: every model call a pipeline makes, the evaluation dataset
    // (decision 26). `outcome` is filled when the owner decides on what the
    // call produced.
    "CREATE TABLE model_calls (
        id INTEGER PRIMARY KEY,
        at TEXT NOT NULL,
        pipeline TEXT NOT NULL,
        step INTEGER NOT NULL,
        prompt TEXT NOT NULL,
        prompt_version TEXT NOT NULL,
        model TEXT NOT NULL,
        input_tokens INTEGER NOT NULL,
        output_tokens INTEGER NOT NULL,
        cache_read_tokens INTEGER NOT NULL,
        cache_write_tokens INTEGER NOT NULL,
        cost_usd REAL,
        latency_ms INTEGER NOT NULL,
        valid INTEGER NOT NULL,
        error TEXT,
        outcome TEXT
    );
    CREATE INDEX model_calls_by_prompt ON model_calls(prompt_version, at);",
];

pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// The realm's database, created and brought up to date as needed.
    pub fn of(realm: &crate::realm::Realm) -> Result<Self> {
        Self::open(&realm.dir().join(FILE))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let mut conn =
            Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        // Another process (the service, a CLI command) may be writing.
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut conn).with_context(|| format!("migrating {}", path.display()))?;
        Ok(Self { conn })
    }

    /// One write, all or nothing. Immediate, so two writers never both read
    /// a state that only one of them may change.
    pub(crate) fn write<T>(&mut self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let applied: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let applied = usize::try_from(applied).unwrap_or(usize::MAX);
    if applied > MIGRATIONS.len() {
        bail!(
            "this database has schema {applied}, newer than this Airth knows ({}): update Airth",
            MIGRATIONS.len()
        );
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
        tx.execute_batch(sql)
            .with_context(|| format!("migration {}", i + 1))?;
    }
    tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(store: &Store) -> i64 {
        store
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn migrations_run_once_and_a_newer_database_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let store = Store::open(&path).unwrap();
        assert_eq!(version(&store), MIGRATIONS.len() as i64);
        drop(store);
        // Reopening applies nothing twice.
        let store = Store::open(&path).unwrap();
        assert_eq!(version(&store), MIGRATIONS.len() as i64);
        store
            .conn
            .pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1)
            .unwrap();
        drop(store);
        let err = Store::open(&path).err().unwrap();
        assert!(format!("{err:#}").contains("newer than this Airth"));
    }

    #[test]
    fn a_failed_write_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join(FILE)).unwrap();
        let result: Result<()> = store.write(|tx| {
            tx.execute(
                "INSERT INTO owner_time (at, activity, seconds) VALUES ('x', 'a', 1)",
                [],
            )?;
            bail!("failed half way")
        });
        assert!(result.is_err());
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM owner_time", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
