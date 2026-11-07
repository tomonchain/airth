//! A run's change to the business config, checked before the founder
//! reviews it (decision 93): what the config would refuse, or set aside,
//! once adopted. A run that broke it is sent back to correct its change,
//! twice at most; after that the founder sees why in the review.

use super::*;
use crate::isolation::Change;
use std::collections::BTreeSet;

/// The config's folders: a change touching none of them is not checked.
const CONFIG: &[&str] = &["business/", "agents/", "routines/", "prompts/"];
/// Times a run is sent back to correct its change.
pub(super) const FIXES: u32 = 2;

/// What the config says of a copy of the business folder: a whole-config
/// refusal, each agent file set aside, and each routine that does not load.
fn findings(hub: &Hub, realm: &Realm, root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    match crate::business::config::load(root, &realm.config.departments) {
        Ok(c) => out.extend(c.broken),
        Err(e) => {
            out.insert(format!("{e:#}"));
        }
    }
    let routines = hub.with_routine_checks_at(realm, root, crate::routine::load_all);
    match routines {
        Ok(all) => out.extend(
            all.into_iter()
                .filter_map(|(_, g)| g.err().map(|e| format!("{e:#}"))),
        ),
        Err(e) => {
            out.insert(format!("{e:#}"));
        }
    }
    out
}

impl Hub {
    /// What the run's change breaks in the business config that the
    /// checkout did not already have wrong; empty when it touches no config
    /// or works in another repository.
    pub(super) fn config_problems(&self, realm: &Realm, change: &Change) -> Vec<String> {
        let Ok(root) = realm.root() else {
            return Vec::new();
        };
        let same_repo = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        };
        let touches = change.files.iter().any(|line| {
            let path = line.rsplit('\t').next().unwrap_or(line);
            CONFIG.iter().any(|c| path.starts_with(c))
        });
        if !touches || !same_repo(&change.repo, &root) {
            return Vec::new();
        }
        let before = findings(self, realm, &root);
        findings(self, realm, &change.path)
            .into_iter()
            .filter(|f| !before.contains(f))
            .collect()
    }

    /// After a run completed: send it back to correct a change that breaks
    /// the config, while it has tries left. True when it was sent back.
    pub(super) fn send_back_config(&self, realm: &Realm, state: &mut State) -> Result<bool> {
        let Some(change) = state.change.as_ref().filter(|c| c.has_changes()) else {
            return Ok(false);
        };
        let problems = self.config_problems(realm, change);
        if problems.is_empty() {
            return Ok(false);
        }
        let list = problems
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        if state.config_fixes >= FIXES {
            state.report = Some(format!(
                "{}\n\nIts change to the business config still does not hold after {FIXES} corrections:\n{list}",
                state.report.as_deref().unwrap_or_default()
            ));
            state.save()?;
            return Ok(false);
        }
        state.config_fixes += 1;
        state.resume = Some(format!(
            "Your change to the business config does not hold: Airth would refuse it, or set part of it aside, once adopted.\n{list}\n\nCorrect your change so it holds, then finish as before."
        ));
        state.status = Status::Stopped;
        state.report = Some(format!(
            "Stopped: its change to the business config does not hold; it is correcting it ({} of {FIXES}).\n{list}",
            state.config_fixes
        ));
        state.save()?;
        Ok(true)
    }
}
