//! What agents can do, and developing and evaluating new skills in a
//! scratch workspace.

use super::*;

impl Hub {
    /// The capabilities an agent dispatched by this hub has.
    pub fn skills(&self) -> Vec<Skill> {
        tools::realm_tools(RealmTools {
            memory: MemoryStore::new("/dev/null"),
            source: String::new(),
            plan: PlanHandle::default(),
            integrations: Vec::new(),
            skills: SkillStore::new("/nonexistent"),
            inbox: self.home.inbox(),
            realm: String::new(),
            developing: None,
        })
        .iter()
        .map(|t| {
            let spec = t.spec();
            Skill {
                name: spec.name,
                description: spec.description,
                capability: t.capability(),
            }
        })
        .collect()
    }

    pub(super) fn skill_scratch(&self, realm: &Realm, run: &str) -> PathBuf {
        realm.cache_dir().join("skill-tests").join(run)
    }

    /// Start developing a skill: a run whose workspace is the skill's draft
    /// folder. The skill is the one a run proposed, or a new one the owner
    /// describes (`description`). Saved, not started.
    pub fn develop_skill(
        &self,
        realm: &Realm,
        name: &str,
        description: Option<&str>,
    ) -> Result<State> {
        if realm.config.archived {
            bail!("realm '{}' is archived", realm.name());
        }
        let store = realm.skills();
        let record = match (store.get(name), description) {
            (Ok(record), _) => record,
            (Err(_), Some(description)) => {
                store
                    .propose(SkillRecord {
                        name: name.to_string(),
                        status: SkillStatus::Proposed,
                        description: description.trim().to_string(),
                        reason: String::new(),
                        spec: String::new(),
                        proposed_by: "owner".into(),
                        created_at: Utc::now(),
                        dev_run: None,
                        evaluation: None,
                        installed: None,
                    })?
                    .0
            }
            (Err(e), None) => return Err(e),
        };
        if let Some(run) = &record.dev_run
            && self.is_live(realm, run)
        {
            bail!("skill '{name}' is already being developed (run {run})");
        }
        let brief = skill::development_brief(&record);
        let mut state = State::new(&brief, Some(&realm.runs_dir()));
        state.objective = format!("Develop the skill {name}");
        state.realm = realm.name().to_string();
        state.skill = Some(name.to_string());
        state.messages.push(Message::user_text(brief));
        state.save()?;
        let run = state.id.clone();
        store.update(name, |r| {
            r.status = SkillStatus::Developing;
            r.dev_run = Some(run);
        })?;
        Ok(state)
    }

    /// Evaluate the skill a development run built, and tell the owner.
    pub(super) fn skill_run_ended(
        &self,
        realm: &Realm,
        state: &State,
        cancel: &CancelToken,
    ) -> Result<()> {
        let Some(name) = &state.skill else {
            return Ok(());
        };
        let store = realm.skills();
        let draft = store.draft_dir(name);
        let scratch = self.skill_scratch(realm, &state.id);
        let eval = skill::evaluate(
            &draft,
            name,
            &scratch,
            &self.denied_to_skills(realm)?,
            cancel,
        )?;
        let _ = std::fs::remove_dir_all(&scratch);
        let passed = eval.passed();
        let summary = eval.describe();
        store.update(name, |r| {
            r.status = if passed {
                SkillStatus::Ready
            } else {
                SkillStatus::Failed
            };
            r.evaluation = Some(eval);
        })?;
        let (kind, title) = if passed {
            (
                Kind::Review,
                format!("Skill {name} is ready for your review ({summary})"),
            )
        } else {
            (
                Kind::Failed,
                format!("Skill {name} failed its evaluation ({summary})"),
            )
        };
        self.home.inbox().push(
            kind,
            realm.name(),
            Some(&state.id),
            &title,
            state.report.as_deref().unwrap_or_default(),
        )?;
        Ok(())
    }

    /// Evaluate a skill's draft again (after the owner or a run changed it).
    pub fn evaluate_skill(&self, realm: &Realm, name: &str) -> Result<SkillRecord> {
        let store = realm.skills();
        store.get(name)?;
        let scratch = realm
            .cache_dir()
            .join("skill-tests")
            .join(format!("owner-{name}"));
        let eval = skill::evaluate(
            &store.draft_dir(name),
            name,
            &scratch,
            &self.denied_to_skills(realm)?,
            &CancelToken::default(),
        )?;
        let _ = std::fs::remove_dir_all(&scratch);
        store.update(name, |r| {
            let installed = r.installed.as_ref().is_some_and(|i| i.hash == eval.hash);
            r.status = match (eval.passed(), installed) {
                (true, true) => SkillStatus::Installed,
                (true, false) => SkillStatus::Ready,
                (false, _) => SkillStatus::Failed,
            };
            r.evaluation = Some(eval);
        })
    }
}

/// Something an agent can do, as exposed to clients.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub capability: Capability,
}
