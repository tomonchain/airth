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
    ) -> Result<crate::routine::run::Report> {
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
        // Reports are gathered, then made one per block when the run ends
        // (decision 59).
        let reports: std::cell::RefCell<Vec<(String, BTreeMap<String, String>)>> =
            Default::default();
        let mut act = |action: &str,
                       fields: &BTreeMap<String, String>,
                       key: &str,
                       approved: bool|
         -> Result<String> {
            if action == "airth.report" {
                reports.borrow_mut().push((key.to_string(), fields.clone()));
                return Ok("reported in Needs you".into());
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
            // A declared body is the only one sent; without one, the body
            // fields as a flat object.
            let body = match declared_http.body(fields)? {
                Some(templated) => Some(templated),
                None => (!body.is_empty()).then_some(Value::Object(body)),
            };
            if let Some(body) = body {
                args["body"] = body.to_string().into();
                args["headers"] = json!({"Content-Type": "application/json"});
            }
            // The same key on every attempt: the service does it once.
            if let Some(header) =
                super::retries::idempotency_header(&realm.config.integrations, action)
            {
                args["headers"][header] = key.into();
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
                    item: None,
                    level: ceiling,
                    approved: true,
                    args: &args.to_string(),
                    daily_limit: u32::MAX,
                })?;
                let crate::business::store::autonomy::Begin::Started(journal) = begin else {
                    bail!("not done again: this act was already started under {key}");
                };
                let event = as_approved.invoke(key, "http_request", &args);
                // A failure that was tried is recorded below, with its retry.
                match event.status {
                    crate::state::EventStatus::Ok if super::incidents::succeeded(&event.result) => {
                        store.finish_action(journal, "confirmed", None)?
                    }
                    crate::state::EventStatus::Denied => {
                        store.finish_action(journal, "failed", Some(&event.result))?
                    }
                    _ => {}
                }
                event
            } else {
                within_levels.invoke(key, "http_request", &args)
            };
            let first = event.result.lines().next().unwrap_or_default().to_string();
            match event.status {
                crate::state::EventStatus::Ok if super::incidents::succeeded(&first) => Ok(first),
                // Held by the levels (proposed, queued, prepared): not a failure.
                crate::state::EventStatus::Denied => bail!("{}", event.result),
                // It was tried and did not work: tried again later, or an
                // incident (decision 55).
                _ => {
                    let detail = format!("{name}: {action} [{key}] failed: {}", event.result);
                    let failed = super::retries::Failed {
                        key,
                        action,
                        agent: Some(&graph.agent),
                        routine: Some(name),
                        status: event.status,
                        result: &event.result,
                        attempts: 1,
                        title: format!("{action} failed in {name}"),
                        detail: detail.clone(),
                    };
                    match self.action_failed(realm, &failed)? {
                        Some(at) => bail!("{detail}; tried again at {}", at.to_rfc3339()),
                        None => bail!("{detail}"),
                    }
                }
            }
        };
        let outcome = self.with_business_io(
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
                    .map(|r| r.to_string())
            },
        );
        // What was gathered is reported even when a later block failed.
        let reported = report(realm, name, &graph.agent, run, reports.take());
        match (outcome, reported) {
            (Ok(text), Ok(lines)) => Ok(format!("{text}\n{lines}").trim_end().to_string()),
            (Err(e), Ok(_)) => Err(e),
            (Ok(_), Err(e)) => Err(e.context("the run's reports were not recorded")),
            (Err(e), Err(r)) => Err(e.context(format!("and its reports were not recorded: {r:#}"))),
        }
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
        // The founder cancelled a delayed action: that action comes down a
        // level for its agent (decision 45).
        if let ("delayed", "rejected", Some(agent), Some(action)) = (
            item.kind.as_str(),
            item.status.as_str(),
            item.agent.clone(),
            item.action.clone(),
        ) {
            let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
            let declared = crate::actions::all(&realm.config.integrations, &servers);
            let (ceiling, start) = declared
                .iter()
                .find(|d| d.id == action)
                .map_or((1, 1), |d| (d.ceiling, d.start));
            let (from, to) =
                store.demote(&agent, &action, ceiling, start, "cancelled by the founder")?;
            return Ok((
                item,
                Some(format!(
                    "cancelled; {agent} now takes {action} at level {to} (was {from})"
                )),
            ));
        }
        if item.status != "approved" {
            return Ok((item, None));
        }
        if item.kind == "incident" || item.kind == "report" {
            store.close_item(id, "done", "seen by the founder")?;
            return Ok((item, Some("seen".into())));
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
            // One to try again stays approved until its last attempt.
            (_, None, Some(tool)) => match self.do_approved(realm, &item, tool) {
                Ok(Approved::Later(why)) => return Ok((item, Some(why))),
                Ok(Approved::Done(what)) => Ok(what),
                Err(e) => Err(e),
            },
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
    ) -> Result<Approved> {
        let action = item.action.as_deref().context("the item names no action")?;
        if tool != "http_request" {
            bail!(
                "{action} goes through {tool}: approve it, then do it yourself; Airth does approved calls of HTTP actions only, for now"
            );
        }
        let key = format!("item/{}", item.id);
        let mut args = item.args.clone();
        // The same key on every attempt: the service does it once.
        if let Some(header) = super::retries::idempotency_header(&realm.config.integrations, action)
        {
            args["headers"][header] = key.clone().into();
        }
        let mut store = Store::of(realm)?;
        let begin = store.begin_action(&crate::business::store::autonomy::NewAction {
            key: &key,
            action,
            connector: action.split('.').next().unwrap_or(action),
            agent: item.agent.as_deref(),
            routine: item.routine.as_deref(),
            run: item.run.as_deref(),
            item: Some(item.id),
            level: 1,
            approved: true,
            args: &args.to_string(),
            daily_limit: u32::MAX,
        })?;
        let crate::business::store::autonomy::Begin::Started(journal) = begin else {
            bail!("not done again: item {} was already acted on", item.id);
        };
        let event = self.unattended(realm)?.invoke(&key, tool, &args);
        if event.status == crate::state::EventStatus::Ok
            && super::incidents::succeeded(&event.result)
        {
            store.finish_action(journal, "confirmed", None)?;
            return Ok(Approved::Done(format!(
                "{action}: {}",
                event.result.lines().next().unwrap_or_default()
            )));
        }
        let failed = super::retries::Failed {
            key: &key,
            action,
            agent: item.agent.as_deref(),
            routine: item.routine.as_deref(),
            status: event.status,
            result: &event.result,
            attempts: 1,
            title: format!("{action} failed"),
            detail: format!(
                "{action}, approved in item {}, failed: {}",
                item.id, event.result
            ),
        };
        match self.action_failed(realm, &failed)? {
            Some(at) => Ok(Approved::Later(format!(
                "{action} failed: {}; tried again at {}",
                event.result.lines().next().unwrap_or_default(),
                at.to_rfc3339()
            ))),
            None => bail!("{}", event.result),
        }
    }

    /// Reports not seen within a week leave Needs you, across realms; how
    /// many, by realm.
    pub fn expire_reports(&self, now: DateTime<Utc>) -> Result<Vec<(String, usize)>> {
        let before = now - chrono::Duration::days(crate::business::store::reports::KEPT_DAYS);
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived || !realm.dir().join(crate::business::store::FILE).exists() {
                continue;
            }
            let n = Store::of(&realm)?.expire_reports(before)?;
            if n > 0 {
                out.push((realm.name().to_string(), n));
            }
        }
        Ok(out)
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
                let (status, detail) = match self.do_approved(&realm, &item, &tool) {
                    Ok(Approved::Done(what)) => ("done", what),
                    Ok(Approved::Later(why)) => {
                        out.push(format!("{}/{}: {why}", realm.name(), item.id));
                        continue;
                    }
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
                // A missing heartbeat is an incident (`CLAUDE.md`).
                let late = [
                    silent.map(|at| format!("The run started at {} left no trace: Airth stopped during it. Running it again now.", at.to_rfc3339())),
                    missed.map(|at| format!("It should have run at {}; Airth was not running then. Running it now, once.", at.to_rfc3339())),
                ];
                for detail in late.into_iter().flatten() {
                    let incident = super::incidents::Incident {
                        title: &format!("{name} did not run on time"),
                        detail: &detail,
                        agent: None,
                        routine: Some(&name),
                        action: None,
                        demote: false,
                    };
                    super::incidents::record(&realm, &incident)?;
                }
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
            let incident = super::incidents::Incident {
                title: &format!("{name} failed"),
                detail: &e,
                agent: None,
                routine: Some(name),
                action: None,
                demote: false,
            };
            if let Err(e) = super::incidents::record(realm, &incident) {
                eprintln!(
                    "routine {}/{name}: the incident was not recorded: {e:#}",
                    realm.name()
                );
            }
        }
    }
}

/// What became of an approved action now.
enum Approved {
    Done(String),
    /// It failed, and is tried again later (decision 55): why, and when.
    Later(String),
}

/// One report per block of what a run's `airth.report` acts gathered,
/// titled by its first entry's `title`; what it did, a line per report.
fn report(
    realm: &Realm,
    routine: &str,
    agent: &str,
    run: &str,
    gathered: Vec<(String, BTreeMap<String, String>)>,
) -> Result<String> {
    type Entries = Vec<(String, BTreeMap<String, String>)>;
    // An act's key ends with its block's id (routine/source/block).
    let mut by_block: Vec<(String, Option<String>, Entries)> = Vec::new();
    for (key, mut fields) in gathered {
        let block = key.rsplit('/').next().unwrap_or_default().to_string();
        let title = fields.remove("title");
        match by_block.iter_mut().find(|(b, _, _)| *b == block) {
            Some((_, _, entries)) => entries.push((key, fields)),
            None => by_block.push((block, title, vec![(key, fields)])),
        }
    }
    let mut lines = Vec::new();
    let mut store = Store::of(realm)?;
    for (block, title, entries) in by_block {
        let new = crate::business::store::reports::NewReport {
            title: title.as_deref().unwrap_or(routine),
            agent: Some(agent),
            routine,
            run,
            entries: &entries,
        };
        lines.push(match store.add_report(&new)? {
            Some(id) => format!("{block}: reported in Needs you (item {id})"),
            None => format!("{block}: already reported, nothing new"),
        });
    }
    Ok(lines.join("\n"))
}
