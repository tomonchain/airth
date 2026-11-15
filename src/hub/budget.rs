//! Monthly caps on model cost (decision 139): the business's, over all its
//! tasks and routines, and an agent's, over its tasks. A task starts with
//! no more than what is left; once a cap is reached, no task starts and
//! scheduled routines wait, until the next month or a higher cap. The
//! founder hears it once a month, in Needs you.

use super::*;
use crate::business::store::Store;
use chrono::{Datelike, TimeZone};
use serde_json::{Value, json};

/// A cap and where the month stands against it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cap {
    /// None: the business's; else the agent's.
    pub agent: Option<String>,
    pub cap: f64,
    pub spent: f64,
}

impl Cap {
    pub fn left(&self) -> f64 {
        (self.cap - self.spent).max(0.0)
    }

    fn whose(&self) -> String {
        self.agent
            .as_ref()
            .map_or("the business's".into(), |a| format!("{a}'s"))
    }
}

/// The first instant of `now`'s month, UTC.
pub fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now)
}

impl Hub {
    /// What `realm` spent on models since the start of `now`'s month: its
    /// tasks' calls and, for the business, its routines' too. For `agent`,
    /// its tasks only: a routine's calls count toward the business.
    pub fn month_spent(
        &self,
        realm: &Realm,
        agent: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<f64> {
        let since = month_start(now);
        let prices = crate::prices::Prices::load(self.home());
        let mut spent = 0.0;
        for state in self.all_runs(realm)? {
            if agent.is_some() && state.agent.as_deref() != agent {
                continue;
            }
            spent += state
                .usage
                .iter()
                .filter(|c| c.at >= since)
                .map(|c| crate::usage::call_cost(c, &prices))
                .sum::<f64>();
        }
        if agent.is_none() && realm.dir().join(crate::business::store::FILE).exists() {
            spent += Store::of(realm)?
                .model_costs_since(since)?
                .iter()
                .map(|(_, _, c)| c)
                .sum::<f64>();
        }
        Ok(spent)
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
        let mut caps = Vec::new();
        if let Some(cap) = crate::business::monthly_cap(&root, None) {
            caps.push(Cap {
                agent: None,
                cap,
                spent: self.month_spent(realm, None, now)?,
            });
        }
        if let Some(a) = agent
            && let Some(cap) = crate::business::monthly_cap(&root, Some(a))
        {
            caps.push(Cap {
                agent: Some(a.to_string()),
                cap,
                spent: self.month_spent(realm, Some(a), now)?,
            });
        }
        Ok(caps)
    }

    /// What work for `agent` may still spend this month: None with no cap.
    /// When a cap is reached, the founder is told once this month, and the
    /// error says which cap and how to go on.
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
            "{} monthly budget is reached: ${:.2} spent of ${:.2}. No task starts and scheduled routines wait until {}, or until the budget is raised on the Finance page.",
            capitalise(&tight.whose()),
            tight.spent,
            tight.cap,
            next.format("%-d %B"),
        );
        let title = match &tight.agent {
            Some(a) => format!("{a}'s monthly budget reached"),
            None => "Monthly budget reached".to_string(),
        };
        self.tell_once_this_month(realm, &title, &detail, tight.agent.as_deref(), now)?;
        bail!("{detail}")
    }

    /// One incident a month for one cap: a routine waiting each hour must
    /// not fill Needs you.
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
            .queue(&["pending", "done"])?
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
        let mut agents = Vec::new();
        for (name, _) in crate::business::bots(&root)? {
            agents.push(json!({
                "agent": name,
                "cap": crate::business::monthly_cap(&root, Some(&name)),
                "spent": self.month_spent(realm, Some(&name), now)?,
            }));
        }
        Ok(json!({
            "month": month_start(now).format("%Y-%m").to_string(),
            "cap": crate::business::monthly_cap(&root, None),
            "spent": self.month_spent(realm, None, now)?,
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
