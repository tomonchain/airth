//! The realm's `business.db` (decision 31): business state that must stay
//! right under retries and concurrent writers. This module is its one access
//! layer (decision 47): all of Airth's SQL is here, one file per area, and
//! the rest of Airth calls typed functions, never a query. It lives in the realm's
//! directory in the Airth home, never in the business folder, so runs can
//! neither read nor change it. Every write is one immediate transaction;
//! schema changes are numbered migrations, and a database written by a
//! newer Airth is refused rather than guessed at.

pub mod autonomy;
pub mod drafts;
pub mod pipelines;
pub mod queue;
pub mod reports;
pub mod retries;
pub mod routines;
pub mod status;
pub mod time;

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
    // 4: when each scheduled pipeline runs next, and when it was last
    // started, to notice a run that left no heartbeat.
    "CREATE TABLE pipeline_schedule (
        pipeline TEXT PRIMARY KEY,
        next_at TEXT NOT NULL,
        started_at TEXT
    );",
    // 5: the draft queue, and its log of every move (append-only).
    "CREATE TABLE drafts (
        id INTEGER PRIMARY KEY,
        created_at TEXT NOT NULL,
        pipeline TEXT NOT NULL,
        kind TEXT NOT NULL,
        text TEXT NOT NULL,
        original TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT '{}',
        model_call INTEGER REFERENCES model_calls(id),
        status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'published')),
        revision INTEGER NOT NULL DEFAULT 1
    );
    CREATE INDEX drafts_by_status ON drafts(status);
    CREATE TABLE draft_log (
        id INTEGER PRIMARY KEY,
        draft INTEGER NOT NULL REFERENCES drafts(id),
        at TEXT NOT NULL,
        action TEXT NOT NULL,
        detail TEXT NOT NULL DEFAULT ''
    );
    CREATE TRIGGER draft_log_no_update BEFORE UPDATE ON draft_log
        BEGIN SELECT RAISE(ABORT, 'the draft log is append-only'); END;
    CREATE TRIGGER draft_log_no_delete BEFORE DELETE ON draft_log
        BEGIN SELECT RAISE(ABORT, 'the draft log is append-only'); END;",
    // 6: what the model answered, valid or not: the call's evaluation needs it.
    "ALTER TABLE model_calls ADD COLUMN answer TEXT;",
    // 7: the queue, what waits for the founder (decisions 45 and 47): an
    // approval, an action prepared at level 0, one delayed at level 2, an
    // incident, a proposed promotion. Its log of every move is evidence.
    "CREATE TABLE queue (
        id INTEGER PRIMARY KEY,
        created_at TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('approval', 'prepared', 'delayed', 'incident', 'promotion')),
        title TEXT NOT NULL,
        agent TEXT,
        routine TEXT,
        run TEXT,
        action TEXT,
        args TEXT NOT NULL DEFAULT '{}',
        original_args TEXT NOT NULL DEFAULT '{}',
        source TEXT NOT NULL DEFAULT '{}',
        model_call INTEGER REFERENCES model_calls(id),
        due_at TEXT,
        execute_at TEXT,
        status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'cancelled', 'expired', 'done', 'failed')),
        revision INTEGER NOT NULL DEFAULT 1
    );
    CREATE INDEX queue_by_status ON queue(status, created_at);
    CREATE TABLE queue_log (
        id INTEGER PRIMARY KEY,
        item INTEGER NOT NULL REFERENCES queue(id),
        at TEXT NOT NULL,
        action TEXT NOT NULL,
        by TEXT NOT NULL CHECK (by IN ('founder', 'airth')),
        channel TEXT,
        detail TEXT NOT NULL DEFAULT ''
    );
    CREATE TRIGGER queue_log_no_update BEFORE UPDATE ON queue_log
        BEGIN SELECT RAISE(ABORT, 'the queue log is append-only'); END;
    CREATE TRIGGER queue_log_no_delete BEFORE DELETE ON queue_log
        BEGIN SELECT RAISE(ABORT, 'the queue log is append-only'); END;",
    // 8: the action journal: each action attempted, once per idempotency key
    // (routine, item's source id, block: decision 49). Its events are evidence.
    "CREATE TABLE actions (
        id INTEGER PRIMARY KEY,
        key TEXT NOT NULL UNIQUE,
        action TEXT NOT NULL,
        connector TEXT NOT NULL,
        agent TEXT,
        routine TEXT,
        run TEXT,
        item INTEGER REFERENCES queue(id),
        level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 3),
        approved INTEGER NOT NULL,
        args TEXT NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('in_flight', 'confirmed', 'failed', 'unknown')),
        attempts INTEGER NOT NULL DEFAULT 0,
        started_at TEXT NOT NULL,
        finished_at TEXT,
        error TEXT,
        revision INTEGER NOT NULL DEFAULT 1
    );
    CREATE INDEX actions_by_connector ON actions(connector, started_at);
    CREATE TABLE action_log (
        id INTEGER PRIMARY KEY,
        action INTEGER NOT NULL REFERENCES actions(id),
        at TEXT NOT NULL,
        status TEXT NOT NULL,
        detail TEXT NOT NULL DEFAULT ''
    );
    CREATE TRIGGER action_log_no_update BEFORE UPDATE ON action_log
        BEGIN SELECT RAISE(ABORT, 'the action log is append-only'); END;
    CREATE TRIGGER action_log_no_delete BEFORE DELETE ON action_log
        BEGIN SELECT RAISE(ABORT, 'the action log is append-only'); END;",
    // 9: each agent's autonomy level per action (decision 45), and the
    // history of every change, which is evidence.
    "CREATE TABLE autonomy (
        agent TEXT NOT NULL,
        action TEXT NOT NULL,
        level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 3),
        set_at TEXT NOT NULL,
        revision INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (agent, action)
    );
    CREATE TABLE autonomy_log (
        id INTEGER PRIMARY KEY,
        at TEXT NOT NULL,
        agent TEXT NOT NULL,
        action TEXT NOT NULL,
        from_level INTEGER,
        to_level INTEGER NOT NULL CHECK (to_level BETWEEN 0 AND 3),
        by TEXT NOT NULL CHECK (by IN ('founder', 'incident', 'default')),
        reason TEXT NOT NULL DEFAULT ''
    );
    CREATE TRIGGER autonomy_log_no_update BEFORE UPDATE ON autonomy_log
        BEGIN SELECT RAISE(ABORT, 'the autonomy history is append-only'); END;
    CREATE TRIGGER autonomy_log_no_delete BEFORE DELETE ON autonomy_log
        BEGIN SELECT RAISE(ABORT, 'the autonomy history is append-only'); END;",
    // 10: each block's execution in a routine run (decision 49): one row per
    // state, so a run that waits and resumes leaves its whole story.
    "CREATE TABLE routine_blocks (
        id INTEGER PRIMARY KEY,
        routine TEXT NOT NULL,
        run TEXT NOT NULL,
        block TEXT NOT NULL,
        item TEXT,
        started_at TEXT NOT NULL,
        finished_at TEXT,
        outcome TEXT NOT NULL CHECK (outcome IN ('ok', 'failed', 'skipped', 'waiting')),
        items_in INTEGER NOT NULL DEFAULT 0,
        items_out INTEGER NOT NULL DEFAULT 0,
        cost_usd REAL,
        error TEXT
    );
    CREATE INDEX routine_blocks_by_run ON routine_blocks(routine, run);
    CREATE TRIGGER routine_blocks_no_update BEFORE UPDATE ON routine_blocks
        BEGIN SELECT RAISE(ABORT, 'routine blocks are append-only'); END;
    CREATE TRIGGER routine_blocks_no_delete BEFORE DELETE ON routine_blocks
        BEGIN SELECT RAISE(ABORT, 'routine blocks are append-only'); END;",
    // 11: a dry run's model calls are marked, so they count neither in the
    // statistics nor as evidence.
    "ALTER TABLE model_calls ADD COLUMN dry_run INTEGER NOT NULL DEFAULT 0;",
    // 12: the founder's time on an answer says which item it answered, and
    // from where (the app, Telegram, the command line).
    "ALTER TABLE owner_time ADD COLUMN item INTEGER REFERENCES queue(id);
    ALTER TABLE owner_time ADD COLUMN channel TEXT;",
    // 13: a failed action waits here for its next attempt, if it may have
    // one (decision 55).
    "ALTER TABLE actions ADD COLUMN retry_at TEXT;",
    // 14: a report is an item of the queue (decision 59). SQLite cannot
    // change a check, so the table is rebuilt, ids kept; what refers to
    // `queue` (log, actions, ledger) is checked before the commit.
    "CREATE TABLE queue_new (
        id INTEGER PRIMARY KEY,
        created_at TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('approval', 'prepared', 'delayed', 'incident', 'promotion', 'report')),
        title TEXT NOT NULL,
        agent TEXT,
        routine TEXT,
        run TEXT,
        action TEXT,
        args TEXT NOT NULL DEFAULT '{}',
        original_args TEXT NOT NULL DEFAULT '{}',
        source TEXT NOT NULL DEFAULT '{}',
        model_call INTEGER REFERENCES model_calls(id),
        due_at TEXT,
        execute_at TEXT,
        status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'cancelled', 'expired', 'done', 'failed')),
        revision INTEGER NOT NULL DEFAULT 1
    );
    INSERT INTO queue_new SELECT id, created_at, kind, title, agent, routine, run, action, args,
        original_args, source, model_call, due_at, execute_at, status, revision FROM queue;
    DROP TABLE queue;
    ALTER TABLE queue_new RENAME TO queue;
    CREATE INDEX queue_by_status ON queue(status, created_at);",
];

pub struct Store {
    conn: Connection,
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
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        backup_before_migrating(&conn, path)?;
        migrate(&mut conn).with_context(|| format!("migrating {}", path.display()))?;
        Ok(Self { conn })
    }

    /// One write, all or nothing. Immediate, so two writers never both read
    /// a state that only one of them may change.
    fn write<T>(&mut self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}

/// Before a database is migrated, a copy of it as it was is kept beside it,
/// `business.db.<version>.bak` (decision 47). A new database has nothing to
/// keep.
fn backup_before_migrating(conn: &Connection, path: &Path) -> Result<()> {
    let applied: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if applied == 0 || applied as usize >= MIGRATIONS.len() {
        return Ok(());
    }
    let copy = path.with_extension(format!("db.{applied}.bak"));
    if !copy.exists() {
        // A consistent copy, WAL included, even while another process reads.
        conn.execute("VACUUM INTO ?1", [copy.to_string_lossy()])
            .with_context(|| format!("copying {} before migrating it", path.display()))?;
    }
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    // A table rebuilt keeps its name and ids, so what refers to it must not
    // be rewritten or emptied on the way: foreign keys are off while the
    // migrations run (they cannot change inside a transaction), and every
    // reference is checked before the commit, as SQLite advises.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let migrated = migrate_unchecked(conn);
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrated
}

fn migrate_unchecked(conn: &mut Connection) -> Result<()> {
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
    let broken: Vec<(String, i64)> = tx
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if let Some((table, row)) = broken.first() {
        bail!("row {row} of {table} refers to nothing after migrating: nothing was changed");
    }
    tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision 47: this module is the only access layer. SQL or `rusqlite`
    /// anywhere else in Airth's code fails this test.
    #[test]
    fn no_sql_outside_the_access_layer() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let layer = src.join("business/store");
        let marks = [
            "rusqlite",
            "\"SELECT ",
            "\"INSERT ",
            "\"UPDATE ",
            "\"DELETE FROM",
            "\"CREATE TABLE",
            ".conn.",
        ];
        let mut found = Vec::new();
        let mut dirs = vec![src];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path != layer && !path.ends_with("node_modules") {
                        dirs.push(path);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for mark in marks {
                        if text.contains(mark) {
                            found.push(format!("{}: {mark}", path.display()));
                        }
                    }
                }
            }
        }
        assert!(found.is_empty(), "SQL outside business/store: {found:#?}");
    }

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

    /// Decision 47: a database of the version before holding rows migrates
    /// with its rows, and a copy of it as it was is kept.
    #[test]
    fn an_older_database_keeps_its_rows_and_a_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..6] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 6).unwrap();
        conn.execute(
            "INSERT INTO owner_time (at, activity, seconds) VALUES ('2026-11-01', 'draft', 30)",
            [],
        )
        .unwrap();
        drop(conn);
        let store = Store::open(&path).unwrap();
        assert_eq!(version(&store), MIGRATIONS.len() as i64);
        let kept: i64 = store
            .conn
            .query_row("SELECT seconds FROM owner_time", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 30);
        let copy = Connection::open(dir.path().join("business.db.6.bak")).unwrap();
        let old: i64 = copy
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(old, 6);
    }

    /// Decision 59: a database of schema 13 with items, their log, an
    /// action and the founder's time on them keeps them all when the queue
    /// is rebuilt to take reports; an unknown kind is still refused.
    #[test]
    fn the_queue_takes_reports_and_keeps_what_refers_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        let tx = conn.transaction().unwrap();
        for sql in &MIGRATIONS[..13] {
            tx.execute_batch(sql).unwrap();
        }
        tx.pragma_update(None, "user_version", 13).unwrap();
        tx.execute_batch(
            "INSERT INTO queue (id, created_at, kind, title, status) VALUES (7, 't', 'approval', 'Post', 'pending'),
                (9, 't', 'incident', 'Down', 'done');
             INSERT INTO queue_log (item, at, action, by) VALUES (7, 't', 'proposed', 'airth'), (9, 't', 'proposed', 'airth');
             INSERT INTO actions (key, action, connector, item, level, approved, args, status, started_at)
                 VALUES ('r/link/send', 'typefully.create_draft', 'typefully', 7, 1, 1, '{}', 'confirmed', 't');
             INSERT INTO owner_time (at, activity, seconds, item, channel) VALUES ('t', 'answer', 12, 9, 'app');",
        )
        .unwrap();
        tx.commit().unwrap();
        drop(conn);
        let store = Store::open(&path).unwrap();
        assert_eq!(version(&store), MIGRATIONS.len() as i64);
        let c = &store.conn;
        let one = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(one("SELECT COUNT(*) FROM queue WHERE id IN (7, 9)"), 2);
        assert_eq!(one("SELECT COUNT(*) FROM queue_log"), 2);
        assert_eq!(one("SELECT item FROM actions"), 7);
        assert_eq!(one("SELECT item FROM owner_time"), 9);
        assert!(
            c.query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
                .is_err(),
            "no reference left dangling"
        );
        c.execute(
            "INSERT INTO queue (created_at, kind, title, status) VALUES ('t', 'report', 'Week', 'pending')",
            [],
        )
        .unwrap();
        let wrong = c.execute(
            "INSERT INTO queue (created_at, kind, title, status) VALUES ('t', 'memo', 'x', 'pending')",
            [],
        );
        assert!(wrong.is_err(), "the kind is still checked");
        // The log still refers to the queue: a log line for no item is refused.
        let orphan = c.execute(
            "INSERT INTO queue_log (item, at, action, by) VALUES (999, 't', 'x', 'airth')",
            [],
        );
        assert!(orphan.is_err(), "the log's reference to the queue holds");
        assert_eq!(
            one("SELECT COUNT(*) FROM sqlite_master WHERE name = 'queue_by_status'"),
            1
        );
    }

    /// A migration that would leave a reference to nothing changes
    /// nothing: the database stays at its version, rows as they were.
    #[test]
    fn a_migration_that_breaks_a_reference_is_refused_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..13] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 13).unwrap();
        // Written with references off, as an old bug might have.
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn.execute(
            "INSERT INTO queue_log (item, at, action, by) VALUES (42, 't', 'proposed', 'airth')",
            [],
        )
        .unwrap();
        drop(conn);
        let err = Store::open(&path).err().unwrap();
        assert!(format!("{err:#}").contains("refers to nothing"), "{err:#}");
        let conn = Connection::open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 13);
        let kind: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'queue'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!kind.contains("'report'"), "the queue was not rebuilt");
    }

    /// Decision 47: evidence is append-only by the database itself.
    #[test]
    fn evidence_refuses_updates_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join(FILE)).unwrap();
        let c = &store.conn;
        c.execute_batch(
            "INSERT INTO queue (created_at, kind, title, status) VALUES ('t', 'approval', 'Post', 'pending');
             INSERT INTO queue_log (item, at, action, by) VALUES (1, 't', 'proposed', 'airth');
             INSERT INTO actions (key, action, connector, level, approved, args, status, started_at)
                 VALUES ('r/link/send', 'typefully.create_draft', 'typefully', 1, 1, '{}', 'in_flight', 't');
             INSERT INTO action_log (action, at, status) VALUES (1, 't', 'in_flight');
             INSERT INTO autonomy_log (at, agent, action, to_level, by) VALUES ('t', 'writer', 'blog.publish', 1, 'default');
             INSERT INTO routine_blocks (routine, run, block, started_at, outcome) VALUES ('w', 'r1', 'news', 't', 'ok');",
        )
        .unwrap();
        for table in [
            "queue_log",
            "action_log",
            "autonomy_log",
            "routine_blocks",
            "owner_time",
            "pipeline_runs",
            "draft_log",
        ] {
            let update = c.execute(&format!("UPDATE {table} SET id = id"), []);
            let delete = c.execute(&format!("DELETE FROM {table}"), []);
            // Empty tables have nothing to refuse; the ones filled above must.
            let filled: i64 = c
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            if filled > 0 {
                assert!(update.is_err() && delete.is_err(), "{table}");
            }
        }
        // Records that change are not evidence, but an action runs once per key.
        let again = c.execute(
            "INSERT INTO actions (key, action, connector, level, approved, args, status, started_at)
                 VALUES ('r/link/send', 'typefully.create_draft', 'typefully', 1, 1, '{}', 'in_flight', 't')",
            [],
        );
        assert!(again.is_err(), "a second action under the same key");
        let bad = [
            "INSERT INTO queue (created_at, kind, title, status) VALUES ('t', 'chat', 'x', 'pending')",
            "INSERT INTO autonomy (agent, action, level, set_at) VALUES ('w', 'a', 4, 't')",
            "INSERT INTO queue_log (item, at, action, by) VALUES (99, 't', 'x', 'airth')",
        ];
        for sql in bad {
            assert!(c.execute(sql, []).is_err(), "{sql}");
        }
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
