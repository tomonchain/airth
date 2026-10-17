//! A realm is an isolated area of the owner's life (personal, a job, a
//! company...). Each has its own directory in the Airth home:
//!
//! ```text
//! realms/<name>/
//!   realm.json    settings: root, policy, isolation, checks
//!   memory.json   long-term memory (see memory.rs)
//!   objectives.json  persistent objectives and their triggers (see objective.rs)
//!   projects.json    projects, their milestones and tasks; the focus one is the North Star (see projects.rs)
//!   skills/       skills: proposals, drafts, installed copies (see skill.rs)
//!   secrets.env   credentials, KEY=VALUE, readable only by the owner
//!   runs/         run states and locks
//!   worktrees/    isolated working copies of runs
//!   files/        the workspace, for realms without a root of their own
//! ```
//!
//! Nothing in a realm is visible to another realm's runs: the hub denies
//! every run access to the Airth home and to other realms' roots.

use crate::integration::Integration;
use crate::isolation::{Delivery, Isolation};
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
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "XAI_API_KEY",
    "DEEPSEEK_API_KEY",
    "MISTRAL_API_KEY",
    "OPENROUTER_API_KEY",
    "AIRTH_PROVIDER",
    "AIRTH_MODEL",
    "AIRTH_EFFORT",
    "AIRTH_COMPACT_AT",
    "AIRTH_MAX_TOKENS",
    // Extra connections (see models/router.rs).
    "AIRTH_FAST_PROVIDER",
    "AIRTH_FAST_MODEL",
    "AIRTH_FAST_EFFORT",
    "AIRTH_FAST_MAX_TOKENS",
    "AIRTH_FALLBACK_PROVIDER",
    "AIRTH_FALLBACK_MODEL",
    "AIRTH_FALLBACK_EFFORT",
    "AIRTH_FALLBACK_MAX_TOKENS",
];

/// A git repository a realm's chats work in: each chat gets its own
/// worktree of it, checked with its checks and delivered its way.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repo {
    /// Short name, unique in the realm (letters, digits, - _ .).
    pub name: String,
    /// The checkout.
    pub path: PathBuf,
    /// Where it was cloned from, if Airth cloned it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Commands that verify a chat's change before it is delivered.
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(default)]
    pub delivery: Delivery,
    /// The realm's root itself (its settings are the realm's).
    #[serde(skip)]
    pub main: bool,
}

/// The checks Airth proposes for a project, from what it is built with.
pub fn detect_checks(dir: &Path) -> Vec<String> {
    if dir.join("Cargo.toml").exists() {
        vec!["cargo check --all-targets".into(), "cargo test".into()]
    } else if dir.join("package.json").exists() {
        let text = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
        let pm = if dir.join("pnpm-lock.yaml").exists() {
            "pnpm"
        } else if dir.join("yarn.lock").exists() {
            "yarn"
        } else {
            "npm"
        };
        let scripts: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        ["lint", "typecheck", "test"]
            .iter()
            .filter(|s| scripts["scripts"][**s].is_string())
            .map(|s| format!("{pm} run {s}"))
            .collect()
    } else if dir.join("go.mod").exists() {
        vec!["go vet ./...".into(), "go test ./...".into()]
    } else {
        Vec::new()
    }
}

/// A valid repository name: what it would be called from a URL or folder.
pub fn repo_name_from(source: &str) -> String {
    let last = source
        .trim()
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default();
    let last = last.strip_suffix(".git").unwrap_or(last);
    last.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_string()
}

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
    /// How isolated changes reach the repository: adopted here, or as pull requests.
    #[serde(default)]
    pub delivery: Delivery,
    /// More git repositories the realm's chats can work in, besides its root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<Repo>,
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

    /// The repositories the realm's chats can work in: its root first when
    /// it is one (with isolated chats), then the ones added to it.
    pub fn repos(&self) -> Vec<Repo> {
        let mut out = Vec::new();
        if self.config.isolation == Isolation::Worktree
            && let Ok(root) = self.root()
        {
            out.push(Repo {
                name: root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| self.name().to_string()),
                path: root,
                url: None,
                checks: self.config.checks.clone(),
                delivery: self.config.delivery,
                main: true,
            });
        }
        out.extend(self.config.repos.iter().cloned());
        out
    }

    /// The repository named `name`, or the realm's root repository when
    /// None (None if the root is not one).
    pub fn repo(&self, name: Option<&str>) -> Result<Option<Repo>> {
        let repos = self.repos();
        match name.map(str::trim).filter(|n| !n.is_empty()) {
            None => Ok(repos.into_iter().find(|r| r.main)),
            Some(name) => repos
                .into_iter()
                .find(|r| r.name == name)
                .map(Some)
                .with_context(|| format!("realm '{}' has no repository '{name}'", self.name())),
        }
    }

    /// The repository whose checkout is `path` (a change's `repo`).
    pub fn repo_at(&self, path: &Path) -> Option<Repo> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.repos()
            .into_iter()
            .find(|r| r.path.canonicalize().unwrap_or_else(|_| r.path.clone()) == path)
    }

    /// Add a repository (an existing checkout). Its checks are detected.
    pub fn add_repo(&mut self, name: &str, path: &Path, url: Option<String>) -> Result<Repo> {
        let name = name.trim();
        if name.is_empty()
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            || name.starts_with('.')
        {
            bail!("a repository name uses letters, digits, - _ and . only");
        }
        if self.repos().iter().any(|r| r.name == name) {
            bail!("this realm already has a repository named '{name}'");
        }
        let path = path
            .canonicalize()
            .with_context(|| format!("{} does not exist", path.display()))?;
        if !crate::isolation::is_repo_root(&path) {
            bail!("{} is not a git repository with commits", path.display());
        }
        if self.repo_at(&path).is_some() {
            bail!(
                "{} is already one of this realm's repositories",
                path.display()
            );
        }
        let repo = Repo {
            name: name.to_string(),
            checks: detect_checks(&path),
            path,
            url,
            delivery: Delivery::Adopt,
            main: false,
        };
        self.config.repos.push(repo.clone());
        self.save()?;
        Ok(repo)
    }

    /// Change a repository's checks and delivery (the root's are the realm's).
    pub fn update_repo(
        &mut self,
        name: &str,
        checks: Option<Vec<String>>,
        delivery: Option<Delivery>,
    ) -> Result<Repo> {
        let repo = self.repo(Some(name))?.expect("named");
        if let Some(Delivery::PullRequest) = delivery {
            crate::github::Remote::of(&repo.path)?;
        }
        if repo.main {
            if let Some(c) = checks {
                self.config.checks = c;
            }
            if let Some(d) = delivery {
                self.config.delivery = d;
            }
        } else {
            let r = self
                .config
                .repos
                .iter_mut()
                .find(|r| r.name == name)
                .expect("found above");
            if let Some(c) = checks {
                r.checks = c;
            }
            if let Some(d) = delivery {
                r.delivery = d;
            }
        }
        self.save()?;
        Ok(self.repo(Some(name))?.expect("named"))
    }

    /// Forget a repository (its files stay where they are).
    pub fn remove_repo(&mut self, name: &str) -> Result<()> {
        let before = self.config.repos.len();
        self.config.repos.retain(|r| r.name != name);
        if self.config.repos.len() == before {
            bail!("'{name}' is not a repository added to this realm");
        }
        self.save()
    }

    /// Whether any of the realm's repositories delivers pull requests.
    pub fn delivers_pull_requests(&self) -> bool {
        self.repos()
            .iter()
            .any(|r| r.delivery == Delivery::PullRequest)
    }

    pub fn memory(&self) -> MemoryStore {
        MemoryStore::new(self.dir.join("memory.json"))
    }

    pub fn skills(&self) -> crate::skill::SkillStore {
        crate::skill::SkillStore::new(self.dir.join("skills"))
    }

    pub fn objectives(&self) -> ObjectiveStore {
        ObjectiveStore::new(self.dir.join("objectives.json"))
    }

    pub fn projects(&self) -> crate::projects::ProjectStore {
        crate::projects::ProjectStore::new(&self.dir)
    }

    pub fn north_star(&self) -> crate::northstar::NorthStarStore {
        crate::northstar::NorthStarStore::new(self.dir.join("northstar.json"))
    }

    /// Names the owner gave runs, by run id. Kept apart from the runs, which a
    /// live run rewrites as it works; the objective itself never changes.
    pub fn run_titles(&self) -> std::collections::BTreeMap<String, String> {
        std::fs::read_to_string(self.dir.join("run-titles.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Name a run; an empty title goes back to the objective.
    pub fn set_run_title(&self, run: &str, title: &str) -> Result<()> {
        let mut titles = self.run_titles();
        let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            titles.remove(run);
        } else {
            titles.insert(run.to_string(), title.chars().take(120).collect());
        }
        let path = self.dir.join("run-titles.json");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&titles)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Ids of the runs the owner archived (hidden from the main list). Kept
    /// apart from the runs for the same reason as the titles.
    pub fn archived_runs(&self) -> std::collections::BTreeSet<String> {
        std::fs::read_to_string(self.dir.join("run-archived.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Archive a run, or bring it back.
    pub fn set_run_archived(&self, run: &str, archived: bool) -> Result<()> {
        let mut ids = self.archived_runs();
        if archived {
            ids.insert(run.to_string());
        } else {
            ids.remove(run);
        }
        let path = self.dir.join("run-archived.json");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&ids)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
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

    /// Credentials given to this realm's commands (provider keys excluded,
    /// and the GitHub token when the runtime uses it to open pull requests:
    /// chats never push themselves).
    pub fn command_env(&self) -> Result<Vec<(String, String)>> {
        let runtime_only = |k: &str| {
            PROVIDER_KEYS.contains(&k)
                || (k == crate::github::TOKEN && self.delivers_pull_requests())
        };
        Ok(self
            .secrets()?
            .into_iter()
            .filter(|(k, _)| !runtime_only(k))
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
                delivery: Default::default(),
                repos: vec![],
                share_owner_profile: true,
                integrations: vec![],
                archived: false,
                created_at: Utc::now(),
            },
        )
        .unwrap()
    }

    #[test]
    fn runs_can_be_renamed_and_unnamed() {
        let dir = tempfile::tempdir().unwrap();
        let r = realm(dir.path());
        assert!(r.run_titles().is_empty());
        r.set_run_title("run1", "  Kit   launch\n plan ").unwrap();
        assert_eq!(r.run_titles().get("run1").unwrap(), "Kit launch plan");
        r.set_run_title("run1", "   ").unwrap();
        assert!(!r.run_titles().contains_key("run1"));
    }

    #[test]
    fn repositories_are_added_named_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = realm(dir.path());
        assert!(r.repos().is_empty(), "a plain folder is not a repository");
        let site = crate::isolation::tests::repo();
        std::fs::write(
            site.path().join("package.json"),
            r#"{"scripts":{"test":"x","lint":"y"}}"#,
        )
        .unwrap();
        let repo = r.add_repo("site", site.path(), None).unwrap();
        assert_eq!(repo.checks, vec!["npm run lint", "npm run test"]);
        assert!(!repo.main);
        assert!(
            r.add_repo("site", site.path(), None).is_err(),
            "names are unique"
        );
        assert!(
            r.add_repo("other", site.path(), None).is_err(),
            "and so are folders"
        );
        assert!(r.add_repo("../x", site.path(), None).is_err());
        assert!(
            r.add_repo("plain", dir.path(), None).is_err(),
            "not a repository"
        );
        assert_eq!(r.repo_at(site.path()).unwrap().name, "site");
        assert!(
            r.repo(None).unwrap().is_none(),
            "the root is not a repository"
        );
        r.update_repo("site", Some(vec!["make".into()]), None)
            .unwrap();
        let reloaded = Realm::load(r.dir()).unwrap();
        assert_eq!(
            reloaded.repo(Some("site")).unwrap().unwrap().checks,
            vec!["make"]
        );
        assert!(reloaded.repo(Some("nope")).is_err());
        r.remove_repo("site").unwrap();
        assert!(r.repos().is_empty());
        assert!(site.path().exists(), "its files stay");
    }

    #[test]
    fn repository_names_come_from_urls_and_folders() {
        assert_eq!(
            repo_name_from("https://github.com/tomonchain/airth.git"),
            "airth"
        );
        assert_eq!(
            repo_name_from("git@github.com:acme/web-site.git"),
            "web-site"
        );
        assert_eq!(repo_name_from("/Users/me/Code/My App/"), "My-App");
    }

    #[test]
    fn runs_are_archived_and_restored() {
        let dir = tempfile::tempdir().unwrap();
        let r = realm(dir.path());
        assert!(r.archived_runs().is_empty());
        r.set_run_archived("run1", true).unwrap();
        assert!(r.archived_runs().contains("run1"));
        r.set_run_archived("run1", false).unwrap();
        assert!(r.archived_runs().is_empty());
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
