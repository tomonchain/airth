//! Tasks (decision 64): an order from the founder, worked as one agent of
//! the business, with that agent's scope only and the business's task
//! budget. Started unwatched; what commits or asks waits in Needs you.

use super::*;

impl Hub {
    /// A task for `agent`: its job, then the founder's `order`, with the
    /// integrations, skills and repository its file declares, never the
    /// whole realm's, and the business's task budget. Saved, not driven.
    pub fn start_task(&self, realm: &Realm, agent: &str, order: &str) -> Result<State> {
        let root = realm.root()?;
        let job = crate::business::job(&root, agent)?;
        let scope = crate::business::scope(&root, agent)?;
        // One repository per task: the first its agent declares.
        let repo = scope.repos.first().map(String::as_str);
        let mut state = self.create_run_in(realm, order, repo)?;
        state.messages = vec![Message::user_text(format!(
            "You are {agent}, one of the business's agents. Your job, as the founder wrote it:\n{}\n\nThe founder's order:\n{}",
            job.text.trim(),
            order.trim()
        ))];
        state.tools = Some(scope.integrations.into_iter().chain(scope.skills).collect());
        let (minutes, cost) = crate::business::task_budget(&root);
        state.budget = crate::agent::Budget {
            max_minutes: Some(minutes),
            max_cost_usd: Some(cost),
            ..Default::default()
        };
        state.agent = Some(agent.to_string());
        state.save()?;
        Ok(state)
    }

    /// A task that stopped on its budget tells the founder why, in Needs
    /// you and on Telegram.
    pub(super) fn task_ended(&self, realm: &Realm, state: &State) -> Result<()> {
        let (Some(agent), Status::Stopped) = (&state.agent, state.status) else {
            return Ok(());
        };
        let report = state.report.as_deref().unwrap_or_default();
        if !report.starts_with("Stopped: reached its") {
            return Ok(());
        }
        super::incidents::record(
            realm,
            &super::incidents::Incident {
                title: &format!("Task stopped: {}", first_line(&state.objective, 60)),
                detail: &format!(
                    "{agent} stopped the task \"{}\": {}",
                    state.objective.trim(),
                    report.trim_start_matches("Stopped: ")
                ),
                agent: Some(agent),
                routine: None,
                action: None,
                demote: false,
                retry: None,
            },
        )?;
        Ok(())
    }
}

impl Hub {
    /// Tasks the founder allowed to go on, across realms, each ready to
    /// drive unwatched with the founder's message: taken once.
    pub fn tasks_to_resume(&self) -> Result<Vec<(Realm, State)>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived {
                continue;
            }
            // Only a stopped run that is not being driven can wait to resume.
            for summary in self.runs(&realm)? {
                if summary.status != Status::Stopped || summary.live {
                    continue;
                }
                let run = self.load_run(&realm, &summary.id)?;
                let Some(message) = run.resume.clone() else {
                    continue;
                };
                let mut state = self.follow_up(&realm, &run.id, &message)?;
                state.resume = None;
                state.save()?;
                out.push((realm.clone(), state));
            }
        }
        Ok(out)
    }
}
