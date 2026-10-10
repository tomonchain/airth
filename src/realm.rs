//! A realm is an isolated area of the owner's life (personal, a job, a
//! company...). Each has its own directory in the Airth home:
//!
//! ```text
//! realms/<name>/
//!   realm.json    settings: root, policy, isolation, checks
//!   memory.json   long-term memory (see memory.rs)
//!   objectives.json  persistent objectives and their triggers (see objective.rs)
//!   secrets.env   credentials, KEY=VALUE, readable only by the owner
//!   runs/         run states and locks
//!   worktrees/    isolated working copies of runs
//!   files/        the workspace, for realms without a root of their own
//! ```
//!
//! Nothing in a realm is visible to another realm's runs: the hub denies
//! every run access to the Airth home and to other realms' roots.

use crate::integration::Integration;
use crate::isolation::Isolation;
use crate::memory::MemoryStore;
use crate::objective::ObjectiveStore;
use crate::permissions::Policy;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Secrets the model provider uses. They are never given to commands.
pub const PROVIDER_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_WORKSPACE_ID",
    "AIRTH_PROVIDER",
    "AIRTH_MODEL",
    "AIRTH_EFFORT",
    "AIRTH_MAX_TOKENS",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The directory the realm works in; None means the realm's own `files/`.
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub isolation: Isolation,
    /// Commands the runtime runs to verify a run's change before adoption.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Whether runs in this realm see the owner profile.
    #[serde(default = "yes")]
    pub share_owner_profile: bool,
    /// Services runs may call with the realm's credentials.
    #[serde(default)]
    pub integrations: Vec<Integration>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct Realm {
    dir: PathBuf,
    pub config: RealmConfig,
}

impl Realm {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("realm.json");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let config =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            config,
        })
    }

    pub(crate) fn create(dir: &Path, config: RealmConfig) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let realm = Self {
            dir: dir.to_path_buf(),
            config,
        };
        realm.save()?;
        Ok(realm)
    }

    pub fn save(&self) -> Result<()> {
        write_atomic(
            &self.dir.join("realm.json"),
            &serde_json::to_string_pretty(&self.config)?,
        )
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.dir.join("runs")
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.dir.join("worktrees")
    }

    /// Build cache shared by this realm's isolated runs.
    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }

    /// The directory runs work in (created if needed).
    pub fn root(&self) -> Result<PathBuf> {
        let root = match &self.config.root {
            Some(root) => root.clone(),
            None => {
                let files = self.dir.join("files");
                std::fs::create_dir_all(&files)?;
                files
            }
        };
        root.canonicalize()
            .with_context(|| format!("realm root {} is missing", root.display()))
    }

    pub fn memory(&self) -> MemoryStore {
        MemoryStore::new(self.dir.join("memory.json"))
    }

    pub fn objectives(&self) -> ObjectiveStore {
        ObjectiveStore::new(self.dir.join("objectives.json"))
    }

    pub(crate) fn secrets_path(&self) -> PathBuf {
        self.dir.join("secrets.env")
    }

    /// All credentials of this realm, in file order.
    pub fn secrets(&self) -> Result<Vec<(String, String)>> {
        read_env_file(&self.secrets_path())
    }

    pub fn secret(&self, key: &str) -> Option<String> {
        self.secrets()
            .ok()?
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .filter(|v| !v.is_empty())
    }

    /// Credentials given to this realm's commands (provider keys excluded).
    pub fn command_env(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .secrets()?
            .into_iter()
            .filter(|(k, _)| !PROVIDER_KEYS.contains(&k.as_str()))
            .collect())
    }

    /// Add an integration, or replace the one with the same name.
    pub fn set_integration(&mut self, integration: Integration) -> Result<()> {
        integration.validate()?;
        let list = &mut self.config.integrations;
        match list.iter_mut().find(|i| i.name == integration.name) {
            Some(existing) => *existing = integration,
            None => list.push(integration),
        }
        self.save()
    }

    /// Remove an integration; false if there was none with this name.
    pub fn remove_integration(&mut self, name: &str) -> Result<bool> {
        let before = self.config.integrations.len();
        self.config.integrations.retain(|i| i.name != name);
        if self.config.integrations.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    pub fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        set_env_entry(&self.secrets_path(), key, value)
    }

    /// Remove a secret; false if there was none with this key.
    pub fn remove_secret(&self, key: &str) -> Result<bool> {
        remove_env_entry(&self.secrets_path(), key)
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && name.starts_with(|c: char| c.is_ascii_alphanumeric());
    if !ok {
        bail!(
            "realm names use lowercase letters, digits, '-' and '_' (e.g. \"personal\", \"acme-corp\")"
        );
    }
    Ok(())
}

/// `KEY=value` lines, as in a realm's `secrets.env`.
pub(crate) fn read_env_file(path: &Path) -> Result<Vec<(String, String)>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .collect())
}

pub(crate) fn set_env_entry(path: &Path, key: &str, value: &str) -> Result<()> {
    validate_secret_key(key)?;
    if value.contains(['\n', '\r']) {
        bail!("secret values must fit on one line");
    }
    let mut entries = read_env_file(path)?;
    match entries.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value.to_string(),
        None => entries.push((key.to_string(), value.to_string())),
    }
    write_env_file(path, &entries)
}

pub(crate) fn remove_env_entry(path: &Path, key: &str) -> Result<bool> {
    let mut entries = read_env_file(path)?;
    let before = entries.len();
    entries.retain(|(k, _)| k != key);
    if entries.len() == before {
        return Ok(false);
    }
    write_env_file(path, &entries)?;
    Ok(true)
}

fn write_env_file(path: &Path, entries: &[(String, String)]) -> Result<()> {
    let mut text = String::from("# Airth credentials. Readable only by you.\n");
    for (k, v) in entries {
        text.push_str(&format!("{k}={v}\n"));
    }
    write_private(path, &text)
}

fn validate_secret_key(key: &str) -> Result<()> {
    let ok = key.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        bail!("secret names look like environment variables, e.g. GITHUB_TOKEN");
    }
    Ok(())
}

pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Write a file only the owner can read.
pub(crate) fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn realm(dir: &Path) -> Realm {
        Realm::create(
            &dir.join("work"),
            RealmConfig {
                name: "work".into(),
                description: String::new(),
                root: None,
                policy: Policy::default(),
                isolation: Isolation::None,
                checks: vec![],
                share_owner_profile: true,
                integrations: vec![],
                archived: false,
                created_at: Utc::now(),
            },
        )
        .unwrap()
    }

    #[test]
    fn secrets_round_trip_privately() {
        let dir = tempfile::tempdir().unwrap();
        let realm = realm(dir.path());
        realm.set_secret("GITHUB_TOKEN", "ghp_x=y").unwrap();
        realm.set_secret("ANTHROPIC_API_KEY", "sk-work").unwrap();
        realm.set_secret("GITHUB_TOKEN", "ghp_z").unwrap();
        assert_eq!(realm.secret("GITHUB_TOKEN").as_deref(), Some("ghp_z"));
        assert_eq!(
            realm.command_env().unwrap(),
            vec![("GITHUB_TOKEN".to_string(), "ghp_z".to_string())]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(realm.dir().join("secrets.env"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(realm.remove_secret("GITHUB_TOKEN").unwrap());
        assert!(!realm.remove_secret("GITHUB_TOKEN").unwrap());
        assert!(realm.set_secret("bad key", "x").is_err());
        assert!(realm.set_secret("K", "a\nb").is_err());
    }

    #[test]
    fn names_are_validated() {
        assert!(validate_name("acme-corp").is_ok());
        assert!(validate_name("Acme").is_err());
        assert!(validate_name("../x").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("-x").is_err());
    }

    #[test]
    fn realm_without_root_uses_its_files_dir() {
        let dir = tempfile::tempdir().unwrap();
        let realm = realm(dir.path());
        let root = realm.root().unwrap();
        assert!(root.ends_with("work/files"));
        assert!(root.is_dir());
    }
}
