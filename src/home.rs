//! The Airth home (`$AIRTH_HOME`, default `~/.airth`): everything Airth keeps
//! about its owner, local and readable only by them.
//!
//! ```text
//! owner.json          the owner profile (see owner.rs)
//! config.json         the default realm, notification settings
//! notifications.json  the owner's inbox (see notify.rs)
//! server.json         the running app's address, while it runs (see ui)
//! secrets.env         model provider settings for every realm (mode 600)
//! prices.json         the owner's model prices, optional (see prices.rs)
//! realms/<name>/      one directory per realm (see realm.rs)
//! ```
//!
//! No run can read or write here: the hub denies the whole home to every
//! workspace, except the run's own isolated worktree.

use crate::isolation::{self, Isolation};
use crate::notify::Inbox;
use crate::owner::Owner;
use crate::permissions::Policy;
use crate::realm::{
    PROVIDER_KEYS, Realm, RealmConfig, read_env_file, remove_env_entry, set_env_entry,
    validate_name, write_atomic,
};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    default_realm: Option<String>,
    /// Show notifications on the desktop too (macOS). Default on.
    desktop_notifications: Option<bool>,
}

/// What a new realm looks like.
pub struct NewRealm {
    pub name: String,
    pub description: String,
    pub root: Option<PathBuf>,
    /// None picks the default: a worktree per run if the root is a git repository.
    pub isolation: Option<Isolation>,
}

/// The home's folder: `$AIRTH_HOME`, or `~/.airth`. A development (debug)
/// build refuses `~/.airth` unless told `real_home`: opening a realm
/// migrates its database, and a build being worked on once migrated the
/// founder's real one under their running service.
fn home_dir(
    airth_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    debug: bool,
    real_home: bool,
) -> Result<PathBuf> {
    if let Some(dir) = airth_home {
        return Ok(PathBuf::from(dir));
    }
    if debug && !real_home {
        bail!(
            "a development build does not open your real home (~/.airth): set AIRTH_HOME to a copy, or pass --real-home"
        );
    }
    Ok(PathBuf::from(home.context("HOME is not set")?).join(".airth"))
}

#[derive(Debug, Clone)]
pub struct Home {
    dir: PathBuf,
}

impl Home {
    /// `$AIRTH_HOME`, or `~/.airth`; see `home_dir` for a development build.
    pub fn locate(real_home: bool) -> Result<Self> {
        let dir = home_dir(
            std::env::var_os("AIRTH_HOME").filter(|v| !v.is_empty()),
            std::env::var_os("HOME"),
            cfg!(debug_assertions),
            real_home,
        )?;
        Self::at(&dir)
    }

    pub fn at(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir.join("realms"))
            .with_context(|| format!("cannot create {}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Self {
            dir: dir.canonicalize()?,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    // ---- owner ----

    pub fn owner(&self) -> Result<Owner> {
        match std::fs::read_to_string(self.dir.join("owner.json")) {
            Ok(text) => serde_json::from_str(&text).context("parsing owner.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Owner::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_owner(&self, owner: &Owner) -> Result<()> {
        let mut owner = owner.clone();
        owner.updated_at = Some(Utc::now());
        write_atomic(
            &self.dir.join("owner.json"),
            &serde_json::to_string_pretty(&owner)?,
        )
    }

    // ---- realms ----

    fn config(&self) -> Config {
        std::fs::read_to_string(self.dir.join("config.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn default_realm(&self) -> Option<String> {
        self.config().default_realm
    }

    pub fn set_default_realm(&self, name: &str) -> Result<()> {
        self.realm(name)?;
        let mut config = self.config();
        config.default_realm = Some(name.to_string());
        self.save_config(&config)
    }

    fn save_config(&self, config: &Config) -> Result<()> {
        write_atomic(
            &self.dir.join("config.json"),
            &serde_json::to_string_pretty(config)?,
        )
    }

    /// Whether the Mac app bounces its Dock icon when a run waits for the owner.
    pub fn desktop_notifications(&self) -> bool {
        self.config().desktop_notifications.unwrap_or(true)
    }

    pub fn set_desktop_notifications(&self, on: bool) -> Result<()> {
        let mut config = self.config();
        config.desktop_notifications = Some(on);
        self.save_config(&config)
    }

    /// Model provider settings shared by all realms (a realm's own override
    /// them). Kept here so a background service does not depend on the
    /// shell's environment.
    pub fn provider_settings(&self) -> Result<Vec<(String, String)>> {
        read_env_file(&self.dir.join("secrets.env"))
    }

    pub fn set_provider_setting(&self, key: &str, value: &str) -> Result<()> {
        if !PROVIDER_KEYS.contains(&key) {
            bail!(
                "only model settings can be set for all realms ({}); other credentials belong to a realm",
                PROVIDER_KEYS.join(", ")
            );
        }
        set_env_entry(&self.dir.join("secrets.env"), key, value)
    }

    pub fn remove_provider_setting(&self, key: &str) -> Result<bool> {
        remove_env_entry(&self.dir.join("secrets.env"), key)
    }

    /// The owner's notifications.
    pub fn inbox(&self) -> Inbox {
        Inbox::new(self.dir.join("notifications.json"))
    }

    /// All realms, archived ones included, by name.
    pub fn realms(&self) -> Result<Vec<Realm>> {
        let mut realms = Vec::new();
        for entry in std::fs::read_dir(self.dir.join("realms"))? {
            let path = entry?.path();
            if path.join("realm.json").exists() {
                match Realm::load(&path) {
                    Ok(realm) => realms.push(realm),
                    Err(e) => eprintln!("warning: skipping realm {}: {e:#}", path.display()),
                }
            }
        }
        realms.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(realms)
    }

    pub fn realm(&self, name: &str) -> Result<Realm> {
        validate_name(name)?;
        let dir = self.dir.join("realms").join(name);
        if !dir.join("realm.json").exists() {
            bail!("no realm '{name}' (see `airth realm list`)");
        }
        Realm::load(&dir)
    }

    /// A folder a realm may work in: there, a folder, outside the Airth home,
    /// and no other realm's (`except` may keep its own).
    fn check_root(&self, root: &Path, except: Option<&str>) -> Result<PathBuf> {
        let root = root
            .canonicalize()
            .with_context(|| format!("{} does not exist", root.display()))?;
        if !root.is_dir() {
            bail!("{} is not a directory", root.display());
        }
        if root.starts_with(&self.dir) {
            bail!("a realm cannot live inside the Airth home");
        }
        if let Some(other) = self
            .realms()?
            .into_iter()
            .find(|r| Some(r.name()) != except && r.config.root.as_deref() == Some(root.as_path()))
        {
            bail!(
                "{} is already the root of realm '{}'",
                root.display(),
                other.name()
            );
        }
        Ok(root)
    }

    /// Point a realm at its folder's new place (Airth moved to another
    /// machine, or the folder moved): same rules as a new realm's folder.
    pub fn move_realm_root(&self, name: &str, root: &Path) -> Result<Realm> {
        let mut realm = self.realm(name)?;
        let root = self.check_root(root, Some(name))?;
        if realm.config.isolation == Isolation::Worktree && !isolation::is_repo_root(&root) {
            bail!(
                "{} is not a git repository with commits, and this realm works in worktrees",
                root.display()
            );
        }
        realm.config.root = Some(root);
        realm.save()?;
        Ok(realm)
    }

    pub fn create_realm(&self, new: NewRealm) -> Result<Realm> {
        validate_name(&new.name)?;
        let dir = self.dir.join("realms").join(&new.name);
        if dir.exists() {
            bail!("realm '{}' already exists", new.name);
        }
        let root = match &new.root {
            Some(root) => Some(self.check_root(root, None)?),
            None => None,
        };
        let is_repo = root.as_deref().is_some_and(isolation::is_repo_root);
        let isolation = match new.isolation {
            Some(Isolation::Worktree) if !is_repo => {
                bail!("worktree isolation needs a root that is a git repository with commits")
            }
            Some(i) => i,
            None if is_repo => Isolation::Worktree,
            None => Isolation::None,
        };
        let checks = root
            .as_deref()
            .filter(|_| is_repo)
            .map(crate::realm::detect_checks)
            .unwrap_or_default();
        let realm = Realm::create(
            &dir,
            RealmConfig {
                name: new.name.clone(),
                description: new.description.trim().to_string(),
                root,
                policy: Policy::default(),
                isolation,
                checks,
                delivery: Default::default(),
                repos: Vec::new(),
                share_owner_profile: true,
                integrations: Vec::new(),
                archived: false,
                departments: Vec::new(),
                offline: false,
                created_at: Utc::now(),
            },
        )?;
        if self.default_realm().is_none() {
            self.set_default_realm(&new.name)?;
        }
        Ok(realm)
    }

    /// The realm to use: the one named, else the one whose root holds `cwd`
    /// (the most specific), else the default one.
    pub fn resolve_realm(&self, name: Option<&str>, cwd: &Path) -> Result<Realm> {
        if let Some(name) = name {
            return self.realm(name);
        }
        let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        let realms = self.realms()?;
        let here = realms
            .iter()
            .filter(|r| !r.config.archived)
            .filter_map(|r| Some((r, r.config.root.as_ref()?)))
            .filter(|(_, root)| cwd.starts_with(root))
            .max_by_key(|(_, root)| root.components().count());
        if let Some((realm, _)) = here {
            return Ok(realm.clone());
        }
        if let Some(name) = self.default_realm()
            && let Some(realm) = realms.iter().find(|r| r.name() == name)
        {
            return Ok(realm.clone());
        }
        if realms.is_empty() {
            bail!(
                "no realm yet. Create one, for example:\n  airth realm create personal\n  airth realm create airth --root {}",
                cwd.display()
            );
        }
        bail!("no realm covers {}; pass --realm <name>", cwd.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A development build opens `~/.airth` only when told to; `AIRTH_HOME`
    /// and a release build open what they say.
    #[test]
    fn a_development_build_refuses_the_real_home() {
        let home = || Some("/Users/f".into());
        let e = home_dir(None, home(), true, false).unwrap_err();
        assert!(format!("{e}").contains("--real-home"), "{e}");
        let real = PathBuf::from("/Users/f/.airth");
        assert_eq!(home_dir(None, home(), true, true).unwrap(), real);
        assert_eq!(home_dir(None, home(), false, false).unwrap(), real);
        assert_eq!(
            home_dir(Some("/tmp/copy".into()), home(), true, false).unwrap(),
            PathBuf::from("/tmp/copy")
        );
    }

    fn new(name: &str, root: Option<&Path>) -> NewRealm {
        NewRealm {
            name: name.into(),
            description: String::new(),
            root: root.map(Path::to_path_buf),
            isolation: None,
        }
    }

    #[test]
    fn a_realm_moves_to_a_new_folder_under_the_same_rules_as_a_new_one() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        let (old, moved, taken) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        home.create_realm(new("acme", Some(old.path()))).unwrap();
        home.create_realm(new("other", Some(taken.path()))).unwrap();

        let realm = home.move_realm_root("acme", moved.path()).unwrap();
        assert_eq!(realm.root().unwrap(), moved.path().canonicalize().unwrap());
        assert_eq!(
            home.realm("acme").unwrap().root().unwrap(),
            moved.path().canonicalize().unwrap()
        );
        // Moving onto itself is allowed; onto another realm's folder, the
        // home, or a folder that is not there is not.
        assert!(home.move_realm_root("acme", moved.path()).is_ok());
        assert!(home.move_realm_root("acme", taken.path()).is_err());
        assert!(home.move_realm_root("acme", home.dir()).is_err());
        assert!(
            home.move_realm_root("acme", &moved.path().join("gone"))
                .is_err()
        );
        assert!(home.move_realm_root("nope", old.path()).is_err());
    }

    #[test]
    fn creates_and_resolves_realms() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        let work = tempfile::tempdir().unwrap();
        std::fs::create_dir(work.path().join("sub")).unwrap();

        assert!(home.resolve_realm(None, work.path()).is_err());
        home.create_realm(new("personal", None)).unwrap();
        let w = home.create_realm(new("work", Some(work.path()))).unwrap();
        assert_eq!(w.config.isolation, Isolation::None);
        assert!(home.create_realm(new("work", None)).is_err());
        assert!(home.create_realm(new("again", Some(work.path()))).is_err());
        assert!(home.create_realm(new("inside", Some(home.dir()))).is_err());

        // The first realm became the default; a realm's root wins inside it.
        assert_eq!(home.default_realm().as_deref(), Some("personal"));
        let cwd = work.path().join("sub");
        assert_eq!(home.resolve_realm(None, &cwd).unwrap().name(), "work");
        assert_eq!(
            home.resolve_realm(None, Path::new("/")).unwrap().name(),
            "personal"
        );
        assert_eq!(
            home.resolve_realm(Some("personal"), &cwd).unwrap().name(),
            "personal"
        );
        assert!(home.resolve_realm(Some("nope"), &cwd).is_err());
    }

    #[test]
    fn git_roots_get_worktree_isolation() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        let repo = crate::isolation::tests::repo();
        let realm = home.create_realm(new("code", Some(repo.path()))).unwrap();
        assert_eq!(realm.config.isolation, Isolation::Worktree);
    }

    #[test]
    fn owner_round_trips() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = Home::at(home_dir.path()).unwrap();
        assert!(home.owner().unwrap().is_empty());
        let mut owner = home.owner().unwrap();
        owner.name = "Sam".into();
        owner.preferences.push("Answer in French".into());
        home.save_owner(&owner).unwrap();
        let loaded = home.owner().unwrap();
        assert_eq!(loaded.preferences, vec!["Answer in French"]);
        assert!(loaded.updated_at.is_some());
    }
}
