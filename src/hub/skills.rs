//! What agents can do.

use super::*;

/// A tool runs can use, for listing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub capability: Capability,
}

impl Hub {
    /// The capabilities an agent dispatched by this hub has.
    pub fn skills(&self) -> Vec<Skill> {
        tools::realm_tools(RealmTools {
            memory: MemoryStore::new("/dev/null"),
            source: String::new(),
            plan: PlanHandle::default(),
            integrations: Vec::new(),
            skills_dir: PathBuf::from("/nonexistent"),
            allowed: None,
            delegation: None,
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
}
