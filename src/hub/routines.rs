//! Routines as graphs (decision 49), from the hub: each is checked against
//! the realm's declared actions and the levels in its `business.db`, and a
//! dry run reads through the realm's runtime and asks the realm's model.

use super::*;
use crate::business::store::Store;
use crate::pipeline::run::Io;
use crate::routine::{Checks, Graph};
use serde_json::json;

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
        let workspace = Workspace::new(&root)?
            .with_secrets(realm.command_env()?, self.redactions(realm)?)
            .with_network(!realm.config.offline);
        let mut runtime = Runtime::new(
            tools::pipeline_tools(realm.config.integrations.clone()),
            PermissionGate::new(realm.config.policy.clone(), approver),
            workspace,
            CancelToken::default(),
        );
        let mut calls = 0;
        let mut fetch = |url: &str| {
            calls += 1;
            super::pipelines::fetched(runtime.invoke(
                &format!("{name}-{calls}"),
                "http_request",
                &json!({"method": "GET", "url": url}),
            ))
        };
        let mut model = None;
        let mut ask = |request: &crate::models::ModelRequest| {
            if model.is_none() {
                model = Some(self.model_for(realm).context("no model available")?);
            }
            model.as_ref().expect("set above").complete(request)
        };
        let mut io = Io {
            fetch: &mut fetch,
            ask: &mut ask,
            prices: crate::prices::Prices::load(self.home()),
        };
        let mut store = Store::of(realm)?;
        let dry = crate::routine::run::Dry {
            root: &root,
            store: &mut store,
            io: &mut io,
            integrations: &realm.config.integrations,
        };
        crate::routine::run::dry_run(name, &graph, dry, Utc::now())
    }
}
