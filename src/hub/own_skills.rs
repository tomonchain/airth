//! The business's own skills (decision 149): a skill new or changed in
//! `skills/` is asked of the founder in Needs you, once per version, and
//! offered to agents only while its files are the ones approved.

use super::*;
use crate::business::skills::{self as own, Skill};
use crate::business::store::Store;
use crate::business::store::skills::{Approved, SKILL};
use crate::supervise::Locked;
use serde_json::json;
use std::collections::BTreeSet;

/// How much of a skill's `SKILL.md`, or of what changed in it, Needs you
/// shows; the rest is in the business folder.
const SHOWN: usize = 6_000;

impl Hub {
    /// The business's skills agents may be offered now: approved, as their
    /// files are.
    pub(super) fn offered_skills(&self, realm: &Realm) -> Result<Vec<Skill>> {
        let found = own::found(&realm.root()?);
        if found.is_empty() {
            return Ok(Vec::new());
        }
        let approved = Store::of(realm)?.approved_skills()?;
        let catalog = catalog_names(realm);
        Ok(found
            .into_iter()
            .filter(|s| !catalog.contains(&s.name))
            .filter(|s| approved.get(&s.name).is_some_and(|a| a.hash == s.hash))
            .collect())
    }

    /// Asks the founder about each skill new or changed since approved,
    /// once per version: an earlier ask still pending for it goes. How many
    /// were asked.
    pub fn ask_about_skills(&self, realm: &Realm) -> Result<usize> {
        let found = own::found(&realm.root()?);
        let mut store = Store::of(realm)?;
        // An ask about files no longer there goes, whatever comes next: its
        // approval would only fail.
        for old in store.queue(&["pending"])? {
            let stale = old.action.as_deref() == Some(SKILL)
                && !found.iter().any(|s| {
                    old.source["skill"] == s.name.as_str() && old.source["hash"] == s.hash.as_str()
                });
            if stale {
                store.expire_item(old.id, "its files changed since")?;
            }
        }
        if found.is_empty() {
            return Ok(0);
        }
        let approved = store.approved_skills()?;
        let catalog = catalog_names(realm);
        let mut asked = 0;
        for skill in found {
            let before = approved.get(&skill.name);
            if catalog.contains(&skill.name)
                || before.is_some_and(|a| a.hash == skill.hash)
                || store.skill_asked(&skill.name, &skill.hash)?
            {
                continue;
            }
            let title = match before {
                Some(_) => format!("Skill changed: {}", skill.name),
                None => format!("New skill: {}", skill.name),
            };
            let args = json!({
                "name": skill.name,
                "description": skill.description,
                "text": shown(&skill, before),
            })
            .to_string();
            let source =
                json!({"show": "text", "skill": skill.name, "hash": skill.hash}).to_string();
            store.propose(&crate::business::store::autonomy::NewItem {
                kind: "approval",
                title: &title,
                agent: None,
                routine: None,
                run: None,
                action: Some(SKILL),
                args: &args,
                execute_at: None,
                source: &source,
                model_call: None,
            })?;
            asked += 1;
        }
        Ok(asked)
    }

    /// For each business, the skills to ask about, at most once a minute;
    /// what happened, by business.
    pub fn review_skills(&self) -> Result<Vec<String>> {
        static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
        {
            let mut last = LAST.locked();
            if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)) {
                return Ok(Vec::new());
            }
            *last = Some(std::time::Instant::now());
        }
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived || realm.root().is_err() {
                continue;
            }
            match self.ask_about_skills(&realm) {
                Ok(0) => {}
                Ok(n) => out.push(format!("{}: {n} skill(s) to approve", realm.name())),
                Err(e) => out.push(format!("{}: skills: {e:#}", realm.name())),
            }
        }
        Ok(out)
    }

    /// The founder approved item `item`: the skill is offered from now on,
    /// if its files are still the ones they saw. What it did.
    pub(super) fn approve_skill(
        &self,
        realm: &Realm,
        item: &crate::business::store::queue::Item,
    ) -> Result<String> {
        let (Some(name), Some(hash)) =
            (item.source["skill"].as_str(), item.source["hash"].as_str())
        else {
            bail!("item {} names no skill", item.id);
        };
        if catalog_names(realm).contains(name) {
            bail!("a skill installed from the catalog is named {name}: rename the folder");
        }
        let Some(skill) = own::found(&realm.root()?)
            .into_iter()
            .find(|s| s.name == name)
        else {
            bail!("{name} is no longer in the business's skills");
        };
        if skill.hash != hash {
            self.ask_about_skills(realm)?;
            bail!("{name} changed since you saw it: its new version waits in Needs you");
        }
        Store::of(realm)?.approve_skill(
            item.id,
            &Approved {
                name: skill.name.clone(),
                hash: skill.hash.clone(),
                files: skill
                    .files
                    .iter()
                    .map(|f| (f.path.clone(), f.hash.clone()))
                    .collect(),
                skill_md: skill.skill_md().to_string(),
            },
        )?;
        Ok(format!(
            "{name} is offered to the agents whose file names it"
        ))
    }
}

fn catalog_names(realm: &Realm) -> BTreeSet<String> {
    crate::tools::catalog::installed(&realm.skills_dir())
        .into_iter()
        .map(|s| s.name)
        .collect()
}

/// What the founder reads to decide: what the skill is for, its files, and
/// its `SKILL.md` whole, or for a change, what changed.
fn shown(skill: &Skill, before: Option<&Approved>) -> String {
    let mut out = String::new();
    if !skill.description.is_empty() {
        out.push_str(&format!("{}\n\n", skill.description));
    }
    let paths = |pick: &dyn Fn(&own::File) -> bool| {
        skill
            .files
            .iter()
            .filter(|f| pick(f))
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match before {
        None => out.push_str(&format!("Files: {}\n", paths(&|_| true))),
        Some(a) => {
            let changed = paths(&|f| a.files.get(&f.path).is_some_and(|h| *h != f.hash));
            let added = paths(&|f| !a.files.contains_key(&f.path));
            let removed: Vec<&str> = a
                .files
                .keys()
                .filter(|p| !skill.files.iter().any(|f| &f.path == *p))
                .map(String::as_str)
                .collect();
            for (what, list) in [
                ("Changed", changed),
                ("New", added),
                ("Removed", removed.join(", ")),
            ] {
                if !list.is_empty() {
                    out.push_str(&format!("{what}: {list}\n"));
                }
            }
        }
    }
    if !skill.skipped.is_empty() {
        out.push_str(&format!(
            "Not read (binary, link or too big): {}\n",
            skill.skipped.join(", ")
        ));
    }
    let body = match before {
        Some(a) if a.skill_md == skill.skill_md() => "SKILL.md is unchanged.".to_string(),
        Some(a) => format!(
            "What changed in SKILL.md:\n{}",
            similar::TextDiff::from_lines(&a.skill_md, skill.skill_md())
                .unified_diff()
                .context_radius(2)
        ),
        None => format!("SKILL.md:\n{}", skill.skill_md()),
    };
    out.push('\n');
    out.push_str(&cut(&body, SHOWN));
    out
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}\n… (the rest is in the business folder)", &text[..at]),
        None => text.to_string(),
    }
}
