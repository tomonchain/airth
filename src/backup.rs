//! A copy of the Airth home each day (decision 106): its businesses'
//! databases, settings, secrets and run history, the last seven kept in
//! `backups/` inside the home, out of the agents' reach like the rest of it.
//!
//! A business's own folder (its config and documents, in git) is not copied:
//! it lives outside the home, and its git remote is its copy.

use crate::business::store::{FILE, Store};
use crate::home::Home;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How many backups are kept.
const KEEP: usize = 7;
const NAME: &str = "%Y%m%d-%H%M%S";

/// Folders that are not copied: the backups themselves, update builds,
/// logs, and what runs rebuild (worktrees, build caches).
const SKIP: [&str; 5] = ["backups", "update", "logs", "worktrees", "cache"];

/// Copy the home now. A database is copied whole and consistent (SQLite's
/// own copy), even while the service writes to it; the rest file by file.
/// The copy is put together aside and appears complete or not at all.
pub fn make(home: &Home, now: DateTime<Local>) -> Result<PathBuf> {
    let root = home.dir().join("backups");
    private_dir(&root)?;
    let name = now.format(NAME).to_string();
    let partial = root.join(format!(".{name}.partial"));
    let _ = std::fs::remove_dir_all(&partial);
    let made = copy_tree(home.dir(), &partial).and_then(|()| {
        for realm in home.realms()? {
            let db = realm.dir().join(FILE);
            if db.exists() {
                let to = partial.join("realms").join(realm.name()).join(FILE);
                Store::open(&db)?
                    .copy_to(&to)
                    .with_context(|| format!("copying {}'s database", realm.name()))?;
            }
        }
        Ok(())
    });
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(e);
    }
    let done = root.join(&name);
    std::fs::rename(&partial, &done)?;
    prune(&root)?;
    Ok(done)
}

/// When the last backup was made, if any.
pub fn last(home: &Home) -> Option<NaiveDateTime> {
    names(&home.dir().join("backups"))
        .ok()?
        .iter()
        .filter_map(|n| NaiveDateTime::parse_from_str(n, NAME).ok())
        .max()
}

/// The service's backup: once a day, checked every hour.
pub fn run(home: Home) {
    loop {
        let due = last(&home)
            .is_none_or(|at| Local::now().naive_local() - at >= chrono::Duration::hours(24));
        if due {
            match make(&home, Local::now()) {
                Ok(path) => eprintln!("backup: {}", path.display()),
                Err(e) => eprintln!("backup failed: {e:#}"),
            }
        }
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// The finished backups, oldest first.
fn names(root: &Path) -> Result<Vec<String>> {
    let mut out: Vec<String> = std::fs::read_dir(root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    out.sort();
    Ok(out)
}

fn prune(root: &Path) -> Result<()> {
    let all = names(root)?;
    for old in &all[..all.len().saturating_sub(KEEP)] {
        std::fs::remove_dir_all(root.join(old))?;
    }
    Ok(())
}

/// Copy `from` into `to`: files and folders, not links, without the skipped
/// folders, and without databases (copied apart, consistently).
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    private_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let kind = entry.file_type()?;
        let target = to.join(&name);
        if kind.is_dir() {
            if !SKIP.iter().any(|s| name == *s) {
                copy_tree(&entry.path(), &target)?;
            }
        } else if kind.is_file() && !is_database(&name.to_string_lossy()) {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// A database file or one of its companions (write-ahead log, shared
/// memory, the copy kept before a migration).
fn is_database(name: &str) -> bool {
    [".db", ".db-wal", ".db-shm", ".db-journal"]
        .iter()
        .any(|end| name.ends_with(end))
        || (name.contains(".db.") && name.ends_with(".bak"))
}

/// A folder only the owner can open: a backup holds the secrets.
fn private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::NewRealm;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, day, 3, 0, 0).unwrap()
    }

    #[test]
    fn a_backup_holds_the_databases_settings_and_secrets_but_not_what_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        let realm = home
            .create_realm(NewRealm {
                name: "acme".into(),
                description: String::new(),
                root: None,
                isolation: None,
            })
            .unwrap();
        let entry = crate::business::store::time::Entry {
            at: chrono::Utc::now(),
            activity: "review".into(),
            seconds: 90,
            note: "kept".into(),
        };
        Store::of(&realm).unwrap().record_time(&entry).unwrap();
        std::fs::write(realm.dir().join("secrets.env"), "KEY=v").unwrap();
        for skipped in ["update", "logs"] {
            std::fs::create_dir_all(home.dir().join(skipped)).unwrap();
            std::fs::write(home.dir().join(skipped).join("big"), "x").unwrap();
        }
        std::fs::create_dir_all(realm.dir().join("worktrees/run1")).unwrap();

        let copy = make(&home, at(8)).unwrap();
        let r = copy.join("realms/acme");
        assert_eq!(
            std::fs::read_to_string(r.join("secrets.env")).unwrap(),
            "KEY=v"
        );
        assert!(r.join("realm.json").exists());
        assert!(!r.join("worktrees").exists());
        assert!(!copy.join("update").exists() && !copy.join("logs").exists());
        // The database copy opens, with what was in it.
        let kept = Store::open(&r.join(FILE))
            .unwrap()
            .time_since(chrono::Utc::now() - chrono::Duration::days(1))
            .unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].note, "kept");
        assert!(!r.join(format!("{FILE}-wal")).exists());
        // Not inside itself, and closed to others.
        assert!(!copy.join("backups").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&copy).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0);
        }
        assert_eq!(
            last(&home).unwrap(),
            at(8).naive_local(),
            "the last backup is known"
        );
    }

    #[test]
    fn the_last_seven_backups_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        for day in 1..=9 {
            make(&home, at(day)).unwrap();
        }
        let kept = names(&home.dir().join("backups")).unwrap();
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0], at(3).format(NAME).to_string());
        assert_eq!(last(&home).unwrap(), at(9).naive_local());
    }
}
