//! Monthly caps on model cost (decision 139): the business's, over its
//! agents' tasks and its routines, and an agent's, over its tasks. A task
//! starts with no more than what is left; once a cap is reached, no task
//! starts and routines wait, until the next month or a higher cap. The
//! founder hears it once a month, in Needs you.

use super::*;
use crate::business::store::Store;
use chrono::{Datelike, TimeZone};
use serde_json::{Value, json};
use std::collections::HashMap;

/// A cap and where the month stands against it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cap {
    /// None: the business's; else the agent's.
    pub agent: Option<String>,
    pub cap: f64,
    pub spent: f64,
    /// What tasks still running may spend of their own budgets: promised,
    /// so not given again.
    pub reserved: f64,
}

impl Cap {
    pub fn left(&self) -> f64 {
        (self.cap - self.spent - self.reserved).max(0.0)
    }

    fn whose(&self) -> String {
        self.agent
            .as_ref()
            .map_or("the business's".into(), |a| format!("{a}'s"))
    }
}

/// Work refused because a cap is reached: told apart from a failure, so a
/// routine waits instead of failing and a plan keeps what it started.
#[derive(Debug)]
pub struct CapReached(pub String);

impl std::fmt::Display for CapReached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CapReached {}

/// Whether `e`, or what it wraps, is a cap reached.
pub fn cap_reached(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<CapReached>())
}

/// One start at a time, from reading what the month leaves to saving the
/// run that holds its share of it: two starts at once would both see the
/// same room and together spend past the cap. One lock for the process,
/// as a plan's is: starts are rare, and one that makes a worktree holds it
/// a few seconds at most. A start from another process (`airth objective
/// run` in a terminal) is not covered.
pub(super) static STARTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The first instant of `now`'s month, UTC.
pub fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now)
}

/// The month's spend, read once: the business's and each agent's, and what
/// running tasks may still spend.
#[derive(Debug, Default)]
struct Month {
    business: f64,
    agents: HashMap<String, f64>,
    reserved: f64,
    reserved_by: HashMap<String, f64>,
}

impl Hub {
    fn month(&self, realm: &Realm, now: DateTime<Utc>) -> Result<Month> {
        let since = month_start(now);
        let prices = crate::prices::Prices::load(self.home());
        let mut m = Month::default();
        // Agents' and objectives' runs only: the founder's own chats are not
        // the business's work. An objective's run is the bot it ran for, or
        // the business's alone.
        for state in self.load_shared(realm)? {
            let agent = state
                .agent
                .clone()
                .or_else(|| state.origin.as_ref()?.bot.clone());
            if agent.is_none() && state.origin.is_none() {
                continue;
            }
            let spent: f64 = state
                .usage
                .iter()
                .filter(|c| c.at >= since)
                .map(|c| crate::usage::call_cost(c, &prices))
                .sum();
            m.business += spent;
            if let Some(agent) = &agent {
                *m.agents.entry(agent.clone()).or_default() += spent;
            }
            // A run holds what it may still spend while it is driven, or
            // saved and not yet driven; one killed midway stays Running on
            // disk, and promises nothing.
            let holds = state.iterations == 0 || self.is_live(realm, &state.id);
            if state.status == Status::Running && holds {
                let promised = state.budget.max_cost_usd.unwrap_or(0.0)
                    - crate::usage::run_cost(&state, &prices);
                let promised = promised.max(0.0);
                m.reserved += promised;
                if let Some(agent) = &agent {
                    *m.reserved_by.entry(agent.clone()).or_default() += promised;
                }
            }
        }
        if realm.dir().join(crate::business::store::FILE).exists() {
            m.business += Store::of(realm)?
                .model_costs_since(since)?
                .iter()
                .map(|(_, _, c)| c)
                .sum::<f64>();
        }
        Ok(m)
    }

    /// The caps that bear on work for `agent` (or the business's alone),
    /// with the month's spend against each.
    pub fn month_caps(
        &self,
        realm: &Realm,
        agent: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Cap>> {
        let root = realm.root()?;
        let business = crate::business::monthly_cap(&root, None);
        let own = agent.and_then(|a| crate::business::monthly_cap(&root, Some(a)));
        if business.is_none() && own.is_none() {
            return Ok(Vec::new());
        }
        let m = self.month(realm, now)?;
        let mut caps = Vec::new();
        if let Some(cap) = business {
            caps.push(Cap {
                agent: None,
                cap,
                spent: m.business,
                reserved: m.reserved,
            });
        }
        if let (Some(a), Some(cap)) = (agent, own) {
            caps.push(Cap {
                agent: Some(a.to_string()),
                cap,
                spent: m.agents.get(a).copied().unwrap_or(0.0),
                reserved: m.reserved_by.get(a).copied().unwrap_or(0.0),
            });
        }
        Ok(caps)
    }

    /// What work for `agent` may still spend this month: None with no cap.
    /// When a cap is reached, the founder is told once this month, and the
    /// error, a `CapReached`, says which cap and how to go on.
    pub(super) fn month_allows(
        &self,
        realm: &Realm,
        agent: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<f64>> {
        let caps = self.month_caps(realm, agent, now)?;
        let Some(tight) = caps.iter().min_by(|a, b| a.left().total_cmp(&b.left())) else {
            return Ok(None);
        };
        if tight.left() > 0.0 {
            return Ok(Some(tight.left()));
        }
        let next = month_start(month_start(now) + chrono::Duration::days(32));
        let detail = format!(
            "{} monthly budget is reached: ${:.2} spent of ${:.2}{}. No task starts and routines wait until {}, or until the budget is raised on the Finance page.",
            capitalise(&tight.whose()),
            tight.spent,
            tight.cap,
            if tight.reserved > 0.0 {
                format!(", ${:.2} more held by tasks still running", tight.reserved)
            } else {
                String::new()
            },
            next.format("%-d %B"),
        );
        let title = match &tight.agent {
            Some(a) => format!("{a}'s monthly budget reached"),
            None => "Monthly budget reached".to_string(),
        };
        self.tell_once_this_month(realm, &title, &detail, tight.agent.as_deref(), now)?;
        Err(CapReached(detail).into())
    }

    /// One incident a month for one cap, whatever the founder did with the
    /// last: a routine waiting each hour must not fill Needs you.
    fn tell_once_this_month(
        &self,
        realm: &Realm,
        title: &str,
        detail: &str,
        agent: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let since = month_start(now);
        let told = Store::of(realm)?
            .queue(&["pending", "done", "rejected", "approved", "failed"])?
            .iter()
            .any(|i| {
                i.kind == "incident"
                    && i.title == title
                    && DateTime::parse_from_rfc3339(&i.created_at)
                        .is_ok_and(|at| at.with_timezone(&Utc) >= since)
            });
        if told {
            return Ok(());
        }
        super::incidents::record(
            realm,
            &super::incidents::Incident {
                title,
                detail,
                agent,
                routine: None,
                run: None,
                action: None,
                demote: false,
                retry: None,
            },
        )?;
        Ok(())
    }

    /// The month's budgets for the Finance page: the business's and each
    /// agent's cap (None when unset) and what each spent.
    pub fn month_budgets(&self, realm: &Realm, now: DateTime<Utc>) -> Result<Value> {
        let root = realm.root()?;
        let m = self.month(realm, now)?;
        let agents: Vec<Value> = crate::business::bots(&root)?
            .into_iter()
            .map(|(name, _)| {
                json!({
                    "cap": crate::business::monthly_cap(&root, Some(&name)),
                    "spent": m.agents.get(&name).copied().unwrap_or(0.0),
                    "agent": name,
                })
            })
            .collect();
        Ok(json!({
            "month": month_start(now).format("%Y-%m").to_string(),
            "cap": crate::business::monthly_cap(&root, None),
            "spent": m.business,
            "agents": agents,
        }))
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}
