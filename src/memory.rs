//! A realm's long-term memory: short facts that should outlive a run, each
//! with where it came from. Stored as JSON in the realm's directory; the owner
//! can list, add and delete entries (CLI or app), the agent can only add them
//! (with the `remember` tool, under the realm's `memory` policy).

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MAX_ENTRY_CHARS: usize = 1000;
pub const MAX_ENTRIES: usize = 300;

/// Serializes read-modify-write cycles within this process.
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEntry {
    pub id: String,
    pub text: String,
    /// Provenance: "owner", or "run:<id>" when the agent saved it.
    pub source: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MemoryStore {
    path: PathBuf,
}

impl MemoryStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn list(&self) -> Result<Vec<MemoryEntry>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    pub fn add(&self, text: &str, source: &str) -> Result<MemoryEntry> {
        let text = text.trim();
        if text.is_empty() {
            bail!("nothing to remember");
        }
        if text.chars().count() > MAX_ENTRY_CHARS {
            bail!("a memory is limited to {MAX_ENTRY_CHARS} characters; keep it to one fact");
        }
        let _guard = LOCK.lock().unwrap();
        let mut entries = self.list()?;
        if entries.len() >= MAX_ENTRIES {
            bail!("memory is full ({MAX_ENTRIES} entries); ask the owner to prune it");
        }
        if let Some(existing) = entries.iter().find(|e| e.text == text) {
            return Ok(existing.clone());
        }
        let entry = MemoryEntry {
            id: next_id(&entries),
            text: text.to_string(),
            source: source.to_string(),
            created_at: Utc::now(),
        };
        entries.push(entry.clone());
        self.write(&entries)?;
        Ok(entry)
    }

    /// Delete an entry; false if there was none with this id.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let _guard = LOCK.lock().unwrap();
        let mut entries = self.list()?;
        let before = entries.len();
        entries.retain(|e| e.id != id);
        if entries.len() == before {
            return Ok(false);
        }
        self.write(&entries)?;
        Ok(true)
    }

    fn write(&self, entries: &[MemoryEntry]) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(entries)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn next_id(entries: &[MemoryEntry]) -> String {
    let max = entries
        .iter()
        .filter_map(|e| e.id.strip_prefix('m')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("m{}", max + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_remove() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join("memory.json"));
        assert!(store.list().unwrap().is_empty());
        let a = store.add("Prefers short reports", "owner").unwrap();
        let b = store.add("Uses cargo nextest", "run:1").unwrap();
        assert_eq!((a.id.as_str(), b.id.as_str()), ("m1", "m2"));
        // Duplicates are not stored twice.
        assert_eq!(
            store.add("Prefers short reports", "run:2").unwrap().id,
            "m1"
        );
        assert!(store.remove("m1").unwrap());
        assert!(!store.remove("m1").unwrap());
        assert_eq!(store.list().unwrap(), vec![b]);
        assert_eq!(store.add("again", "owner").unwrap().id, "m3");
        assert!(store.add("  ", "owner").is_err());
    }
}
