//! The repositories a realm works in: connecting one by URL or folder,
//! and which one a run uses.

use super::*;

impl Hub {
    /// Where a new chat's worktree starts: the checkout's HEAD, or, in a
    /// realm that opens pull requests, its branch as it is on GitHub.
    pub(super) fn start_point(&self, realm: &Realm, repo: &Repo) -> String {
        let root = &repo.path;
        if repo.delivery == Delivery::PullRequest
            && let Ok((gh, remote)) = self.github(realm, root)
            && let Ok(branch) = isolation::current_branch(root).context("no branch")
        {
            match gh.fetch(root, &remote, &branch) {
                Ok(target) => return target,
                Err(e) => eprintln!("starting from the local checkout: {e:#}"),
            }
        }
        "HEAD".into()
    }

    /// Connect a repository to the realm: clone `source` (a URL) into
    /// `dest` (default: next to the realm's root, see `clone_dir`), or take
    /// `source` as an existing checkout on this machine.
    pub fn connect_repo(
        &self,
        realm: &mut Realm,
        source: &str,
        name: Option<&str>,
        dest: Option<&Path>,
    ) -> Result<Repo> {
        let source = source.trim();
        if source.is_empty() {
            bail!("give a repository URL or a folder");
        }
        let is_url = source.contains("://") || source.starts_with("git@");
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => n.to_string(),
            None => crate::realm::repo_name_from(source),
        };
        let path = if is_url {
            let dest = match dest {
                Some(d) => d.to_path_buf(),
                None => self.clone_dir(realm, &name)?,
            };
            if dest.exists() && std::fs::read_dir(&dest)?.next().is_some() {
                bail!("{} already exists and is not empty", dest.display());
            }
            self.check_repo_place(realm, &dest)?;
            match (Remote::parse(source), realm.secret(crate::github::TOKEN)) {
                (Some(remote), Some(token)) => {
                    self.github_client(&token).clone_into(&remote, &dest)?
                }
                _ => isolation::clone(source, &dest)?,
            }
            dest
        } else {
            let path = match (source.strip_prefix("~/"), std::env::var_os("HOME")) {
                (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
                _ => PathBuf::from(source),
            };
            self.check_repo_place(realm, &path)?;
            path
        };
        realm.add_repo(&name, &path, is_url.then(|| source.to_string()))
    }

    /// Where a cloned repository goes by default: next to the realm's root
    /// when the root is a repository itself, inside it otherwise, and under
    /// ~/Airth/<realm> for a realm without a folder of its own.
    fn clone_dir(&self, realm: &Realm, name: &str) -> Result<PathBuf> {
        let root = realm.root()?;
        Ok(if realm.config.root.is_none() {
            let home = std::env::var_os("HOME").context("no home folder")?;
            PathBuf::from(home)
                .join("Airth")
                .join(realm.name())
                .join(name)
        } else if isolation::is_repo_root(&root) {
            root.parent().unwrap_or(&root).join(name)
        } else {
            root.join(name)
        })
    }

    /// A repository may not live in Airth's home, nor in another realm.
    fn check_repo_place(&self, realm: &Realm, path: &Path) -> Result<()> {
        let abs = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let path = abs(path);
        if path.starts_with(abs(self.home.dir())) {
            bail!("a repository cannot live inside the Airth home");
        }
        for other in self.home.realms()? {
            if other.name() == realm.name() {
                continue;
            }
            let theirs = other
                .config
                .root
                .iter()
                .cloned()
                .chain(other.config.repos.iter().map(|r| r.path.clone()));
            for p in theirs {
                let p = abs(&p);
                if path.starts_with(&p) || p.starts_with(&path) {
                    bail!("{} belongs to realm '{}'", path.display(), other.name());
                }
            }
        }
        Ok(())
    }

    /// The repository a run works in, with its settings: found by its
    /// change's checkout, else the realm's own settings.
    pub fn repo_of(&self, realm: &Realm, state: &State) -> Repo {
        let found = state.change.as_ref().and_then(|c| realm.repo_at(&c.repo));
        found.unwrap_or_else(|| Repo {
            name: realm.name().to_string(),
            path: state
                .change
                .as_ref()
                .map(|c| c.repo.clone())
                .unwrap_or_default(),
            url: None,
            checks: realm.config.checks.clone(),
            delivery: realm.config.delivery,
            main: true,
        })
    }
}
