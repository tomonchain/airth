//! Routines as graphs (decision 49), from the hub: each is checked against
//! the realm's declared actions and the levels in its `business.db`, and a
//! dry run reads through the realm's runtime and asks the realm's model.

use super::*;
use crate::business::store::Store;
use crate::pipeline::schedule::{Tick, tick};
use crate::routine::{Checks, Graph};
use serde_json::{Value, json};
use std::collections::BTreeMap;

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

impl Hub {
    /// Run the routine `name` for real, from `start`, unattended: its acts
    /// go through the agent's autonomy levels, and items that reach an
    /// approval wait in the queue. Its report, block by block.
    pub fn run_routine(
        &self,
        realm: &Realm,
        name: &str,
        run: &str,
        start: crate::routine::run::Start,
    ) -> Result<String> {
        let graph: Graph = self.with_routine_checks(realm, |c| crate::routine::load(c, name))?;
        let root = realm.root()?;
        let workspace = Workspace::new(&root)?
            .with_secrets(realm.command_env()?, self.redactions(realm)?)
            .with_network(!realm.config.offline);
        let runtime = || {
            Runtime::new(
                tools::pipeline_tools(realm.config.integrations.clone()),
                PermissionGate::new(
                    realm.config.policy.clone(),
                    Box::new(super::pipelines::Unattended),
                ),
                workspace.clone(),
                CancelToken::default(),
            )
        };
        // Acts on an item the founder approved are done as approved, once
        // (decision 45); the others go through the agent's levels.
        let authority =
            super::autonomy::BusinessAuthority::for_graph(realm, name, &graph.agent, run)?;
        let mut within_levels = runtime().with_authority(Box::new(authority));
        let mut as_approved = runtime();
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        let declared = crate::actions::all(&realm.config.integrations, &servers);
        let inbox = self.home.inbox();
        let mut act = |action: &str,
                       fields: &BTreeMap<String, String>,
                       key: &str,
                       approved: bool|
         -> Result<String> {
            if action == "airth.report" {
                let title = fields
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| name.to_string());
                let body: Vec<String> = fields
                    .iter()
                    .filter(|(k, _)| *k != "title")
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect();
                inbox.push(
                    crate::notify::Kind::Completed,
                    realm.name(),
                    None,
                    &title,
                    &body.join("\n"),
                )?;
                return Ok("reported".into());
            }
            let (connector, action_name) = action
                .split_once('.')
                .context("an action is <connector>.<action>")?;
            let integration = realm
                .config
                .integrations
                .iter()
                .find(|i| i.name == connector)
                .with_context(|| {
                    format!(
                        "{action}: only declared HTTP actions and airth.report are done for now"
                    )
                })?;
            let declared_http = integration
                .actions
                .iter()
                .find(|a| a.name == action_name)
                .with_context(|| format!("{action} is not declared"))?;
            let mut url = declared_http.url(integration, fields)?;
            let mut body = serde_json::Map::new();
            for f in &declared_http.fields {
                let Some(value) = fields.get(&f.name) else {
                    continue;
                };
                match f.place {
                    crate::actions::Place::Query => {
                        url.query_pairs_mut().append_pair(&f.name, value);
                    }
                    crate::actions::Place::Body => {
                        body.insert(f.name.clone(), value.clone().into());
                    }
                    crate::actions::Place::Path => {}
                }
            }
            let mut args = json!({"integration": connector, "method": declared_http.method, "url": url.as_str()});
            if !body.is_empty() {
                args["body"] = Value::Object(body).to_string().into();
                args["headers"] = json!({"Content-Type": "application/json"});
            }
            let event = if approved {
                let ceiling = declared
                    .iter()
                    .find(|d| d.id == action)
                    .map_or(1, |d| d.ceiling);
                if ceiling == 0 {
                    bail!("{action} is only ever prepared for the founder, who does it");
                }
                let mut store = Store::of(realm)?;
                let begin = store.begin_action(&crate::business::store::autonomy::NewAction {
                    key,
                    action,
                    connector,
                    agent: Some(&graph.agent),
                    routine: Some(name),
                    run: Some(run),
                    level: ceiling,
                    approved: true,
                    args: &args.to_string(),
                    daily_limit: u32::MAX,
                })?;
                let crate::business::store::autonomy::Begin::Started(journal) = begin else {
                    bail!("not done again: this act was already started under {key}");
                };
                let event = as_approved.invoke(key, "http_request", &args);
                let ok = event.status == crate::state::EventStatus::Ok;
                let error = (!ok).then(|| event.result.clone());
                store.finish_action(
                    journal,
                    if ok { "confirmed" } else { "failed" },
                    error.as_deref(),
                )?;
                event
            } else {
                within_levels.invoke(key, "http_request", &args)
            };
            match event.status {
                crate::state::EventStatus::Ok => {
                    Ok(event.result.lines().next().unwrap_or_default().to_string())
                }
                _ => bail!("{}", event.result),
            }
        };
        self.with_business_io(
            realm,
            name,
            Box::new(super::pipelines::Unattended),
            Vec::new(),
            |store, io| {
                let dry = crate::routine::run::Dry {
                    root: &root,
                    store,
                    io,
                    integrations: &realm.config.integrations,
                };
                let real = crate::routine::run::Real { run, act: &mut act };
                crate::routine::run::run(name, &graph, dry, Some(real), start, Utc::now())
            },
        )
    }

    /// The founder answers queue item `id` from `channel`. An approval of a
    /// routine's item resumes the routine after its approval block, with the
    /// item as the founder left it; the routine's report comes back.
    pub fn answer_item(
        &self,
        realm: &Realm,
        id: i64,
        revision: Option<i64>,
        answer: crate::business::store::queue::Answer,
        channel: &str,
        seconds: u64,
    ) -> Result<(crate::business::store::queue::Item, Option<String>)> {
        let mut store = Store::of(realm)?;
        let item = store.answer(id, revision, answer, channel, seconds)?;
        if item.status != "approved" {
            return Ok((item, None));
        }
        // Prepared for the founder (level 0): the founder did it; nothing
        // runs.
        if item.kind == "prepared" {
            store.close_item(id, "done", "done by the founder")?;
            return Ok((item, Some("marked done: the founder did it".into())));
        }
        let routine_block = item.source["block"].as_str();
        let outcome = match (&item.routine, routine_block, item.source["tool"].as_str()) {
            // A routine's item: the routine resumes after its approval.
            (Some(routine), Some(block), _) => {
                let Value::Object(fields) = item.args.clone() else {
                    bail!("item {id} does not hold the routine's item");
                };
                let start = crate::routine::run::Start::After {
                    block: block.to_string(),
                    item: fields,
                };
                let run = item
                    .run
                    .clone()
                    .unwrap_or_else(|| format!("{routine}-{id}"));
                self.run_routine(realm, routine, &run, start)
            }
            // An action a mission proposed: done as it was proposed, once.
            (_, None, Some(tool)) => self.do_approved(realm, &item, tool),
            _ => return Ok((item, None)),
        };
        let (status, detail) = match &outcome {
            Ok(report) => (
                "done",
                report.lines().last().unwrap_or_default().to_string(),
            ),
            Err(e) => ("failed", format!("{e:#}")),
        };
        store.close_item(id, status, &detail)?;
        let report = outcome.unwrap_or_else(|e| format!("failed: {e:#}"));
        let closed = store.queue(&[status])?.into_iter().find(|i| i.id == id);
        Ok((closed.unwrap_or(item), Some(report)))
    }

    /// Do the action the founder approved in queue item `item`, exactly as
    /// it was proposed, once: it is journaled under the item.
    fn do_approved(
        &self,
        realm: &Realm,
        item: &crate::business::store::queue::Item,
        tool: &str,
    ) -> Result<String> {
        let action = item.action.as_deref().context("the item names no action")?;
        if tool != "http_request" {
            bail!(
                "{action} goes through {tool}: approve it, then do it yourself; Airth does approved calls of HTTP actions only, for now"
            );
        }
        let key = format!("item/{}", item.id);
        let mut store = Store::of(realm)?;
        let begin = store.begin_action(&crate::business::store::autonomy::NewAction {
            key: &key,
            action,
            connector: action.split('.').next().unwrap_or(action),
            agent: item.agent.as_deref(),
            routine: item.routine.as_deref(),
            run: item.run.as_deref(),
            level: 1,
            approved: true,
            args: &item.args.to_string(),
            daily_limit: u32::MAX,
        })?;
        let crate::business::store::autonomy::Begin::Started(journal) = begin else {
            bail!("not done again: item {} was already acted on", item.id);
        };
        let root = realm.root()?;
        let workspace = Workspace::new(&root)?
            .with_secrets(realm.command_env()?, self.redactions(realm)?)
            .with_network(!realm.config.offline);
        let mut runtime = Runtime::new(
            tools::pipeline_tools(realm.config.integrations.clone()),
            PermissionGate::new(
                realm.config.policy.clone(),
                Box::new(super::pipelines::Unattended),
            ),
            workspace,
            CancelToken::default(),
        );
        let event = runtime.invoke(&key, tool, &item.args);
        let ok = event.status == crate::state::EventStatus::Ok;
        let error = (!ok).then_some(event.result.as_str());
        store.finish_action(journal, if ok { "confirmed" } else { "failed" }, error)?;
        if !ok {
            bail!("{}", event.result);
        }
        Ok(format!(
            "{action}: {}",
            event.result.lines().next().unwrap_or_default()
        ))
    }

    /// Do the delayed actions whose window has passed by `now`, across
    /// realms, each once; the founder cancelled the others by rejecting
    /// them. What was done, or why not, by realm and item.
    pub fn release_delayed(&self, now: DateTime<Utc>) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived || !realm.dir().join(crate::business::store::FILE).exists() {
                continue;
            }
            let mut store = Store::of(&realm)?;
            for item in store.due_delayed(now)? {
                if !store.release_item(item.id)? {
                    continue;
                }
                let tool = item.source["tool"]
                    .as_str()
                    .unwrap_or("http_request")
                    .to_string();
                let outcome = self.do_approved(&realm, &item, &tool);
                let (status, detail) = match &outcome {
                    Ok(what) => ("done", what.clone()),
                    Err(e) => ("failed", format!("{e:#}")),
                };
                store.close_item(item.id, status, &detail)?;
                out.push(format!("{}/{}: {status}: {detail}", realm.name(), item.id));
            }
        }
        Ok(out)
    }

    /// The routines whose schedule says they run at `now`, across realms,
    /// each claimed so the next call does not return it again. A late or
    /// silent run is told to the founder, as for pipelines.
    pub fn due_routines(&self, now: DateTime<Utc>) -> Result<Vec<(Realm, String)>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived || realm.root().is_err() {
                continue;
            }
            let graphs = self.with_routine_checks(&realm, crate::routine::load_all)?;
            let mut store = Store::of(&realm)?;
            for (name, graph) in graphs {
                let Some(schedule) = graph.ok().and_then(|g| g.trigger.schedule) else {
                    continue;
                };
                let schedule = Schedule::parse(&schedule)?;
                // Kept apart from pipelines of the same name.
                let key = format!("routine:{name}");
                let Tick::Run { missed, silent } = tick(&mut store, &key, &schedule, now)? else {
                    continue;
                };
                self.tell_late(&realm, &name, missed, silent)?;
                out.push((realm.clone(), name));
            }
        }
        Ok(out)
    }

    /// Run a routine its schedule started, and leave its heartbeat, so the
    /// next time knows it ended (decision 34).
    pub fn run_scheduled_routine(&self, realm: &Realm, name: &str) {
        let started = Utc::now();
        let run = format!("{name}-{}", started.format("%Y%m%dT%H%M%S"));
        let outcome = self.run_routine(realm, name, &run, crate::routine::run::Start::Trigger);
        let error = outcome.as_ref().err().map(|e| format!("{e:#}"));
        let report = outcome.ok();
        let beat = crate::business::store::pipelines::Heartbeat {
            pipeline: &format!("routine:{name}"),
            started,
            finished: Utc::now(),
            items_read: 0,
            items_kept: 0,
            report: report.as_deref(),
            notes: "",
            error: error.as_deref(),
        };
        let recorded = Store::of(realm).and_then(|mut s| s.record_run(&beat, &[]));
        if let Err(e) = recorded {
            eprintln!(
                "routine {}/{name}: its heartbeat was not recorded: {e:#}",
                realm.name()
            );
        }
        if let Some(e) = error {
            let _ = self.home.inbox().push(
                crate::notify::Kind::Failed,
                realm.name(),
                None,
                &format!("{name} failed"),
                &e,
            );
        }
    }
}
