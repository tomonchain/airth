//! Routines as graphs (decision 49), from the hub: each is checked against
//! the realm's declared actions and the levels in its `business.db`, and a
//! dry run reads through the realm's runtime and asks the realm's model.

use super::*;
use crate::business::store::Store;
use crate::routine::{Checks, Graph};

impl Hub {
    /// Call `f` with what the realm's routines are checked against.
    pub fn with_routine_checks<T>(
        &self,
        realm: &Realm,
        f: impl FnOnce(&Checks) -> Result<T>,
    ) -> Result<T> {
        let root = realm.root()?;
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        let actions = crate::actions::all(&realm.config.integrations, &servers);
        let store = Store::of(realm)?;
        let level = |agent: &str, action: &str| {
            let (ceiling, start) = actions
                .iter()
                .find(|a| a.id == action)
                .map_or((1, 1), |a| (a.ceiling, a.start));
            store.level(agent, action, ceiling, start).unwrap_or(1)
        };
        f(&Checks {
            root: &root,
            actions: &actions,
            level: &level,
        })
    }

    /// Run the routine `name` dry: reads and AI blocks for real, approvals
    /// taken as given, acts only prepared. Its report, block by block.
    pub fn dry_run_routine(
        &self,
        realm: &Realm,
        name: &str,
        approver: Box<dyn Approver>,
    ) -> Result<String> {
        let graph: Graph = self.with_routine_checks(realm, |c| crate::routine::load(c, name))?;
        let root = realm.root()?;
        self.with_business_io(realm, name, approver, Vec::new(), |store, io| {
            let dry = crate::routine::run::Dry {
                root: &root,
                store,
                io,
                integrations: &realm.config.integrations,
            };
            crate::routine::run::dry_run(name, &graph, dry, Utc::now())
        })
    }
}
