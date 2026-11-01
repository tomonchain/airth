//! Delivering a change as a GitHub pull request, and following it until
//! it is merged or closed.

use super::*;

impl Hub {
    fn delivery_of(&self, realm: &Realm, change: &isolation::Change) -> Delivery {
        realm
            .repo_at(&change.repo)
            .map_or(realm.config.delivery, |r| r.delivery)
    }

    /// The ref holding a change's base branch as it is now: in the checkout,
    /// or fetched from GitHub in a realm that opens pull requests.
    pub(super) fn base_target(&self, realm: &Realm, change: &isolation::Change) -> Result<String> {
        let base = change
            .base_branch
            .as_deref()
            .context("this chat did not start from a branch")?;
        if self.delivery_of(realm, change) == Delivery::PullRequest {
            let (gh, remote) = self.github(realm, &change.repo)?;
            return gh.fetch(&change.repo, &remote, base);
        }
        Ok(format!("refs/heads/{base}"))
    }

    /// The realm's GitHub access, for realms that open pull requests.
    pub(super) fn github(&self, realm: &Realm, repo: &Path) -> Result<(GitHub, Remote)> {
        let token = realm.secret(crate::github::TOKEN).with_context(|| {
            format!(
                "add a GitHub token to the realm's secrets as {}",
                crate::github::TOKEN
            )
        })?;
        let remote = Remote::of(repo)?;
        Ok((self.github_client(&token), remote))
    }

    #[cfg(not(test))]
    pub(super) fn github_client(&self, token: &str) -> GitHub {
        GitHub::new(token)
    }

    /// Tests point GitHub at a local API and folder of repositories.
    #[cfg(test)]
    pub(super) fn github_client(&self, token: &str) -> GitHub {
        match TEST_GITHUB.lock().unwrap().as_ref() {
            Some((api, git)) => GitHub::at(token, api, git),
            None => GitHub::new(token),
        }
    }

    /// Push a run's change and open its pull request (or push to the one it
    /// has), in a realm that delivers changes that way. A change opens its
    /// pull request once its checks pass, unless `anyway`.
    pub fn deliver(&self, realm: &Realm, state: &mut State, anyway: bool) -> Result<()> {
        let repo = self.repo_of(realm, state);
        if repo.delivery != Delivery::PullRequest {
            return Ok(());
        }
        let Some(change) = state.change.as_mut() else {
            return Ok(());
        };
        let ready = change.status == ChangeStatus::Open
            && change.has_changes()
            && !change.files.is_empty()
            && change.merging.is_none()
            && change.conflicts.is_empty();
        let passed = repo.checks.is_empty() || change.checks_passed();
        if !ready || (change.pull_request.is_none() && !passed && !anyway) {
            return Ok(());
        }
        if change
            .pull_request
            .as_ref()
            .is_some_and(|p| p.pushed == change.head)
        {
            return Ok(());
        }
        let (gh, remote) = self.github(realm, &change.repo)?;
        let base = change
            .base_branch
            .clone()
            .context("this chat did not start from a branch")?;
        gh.push(&change.path, &remote, &change.branch)?;
        match change.pull_request.as_mut() {
            Some(pr) => pr.pushed = change.head.clone(),
            None => {
                let title = self.pull_request_title(realm, state);
                let body = pull_request_body(state);
                let change = state.change.as_mut().expect("checked above");
                let (number, url) = gh.open(&remote, &change.branch, &base, &title, &body)?;
                change.pull_request = Some(PullRequest {
                    number,
                    url,
                    pushed: change.head.clone(),
                });
            }
        }
        state.save()
    }

    /// A pull request's title, which also becomes its line in the release
    /// notes: the name the owner gave the chat, else one the fast model
    /// writes from the ask and the report (counted in the run), else the
    /// ask's first line.
    fn pull_request_title(&self, realm: &Realm, state: &mut State) -> String {
        if let Some(title) = realm.run_titles().get(&state.id) {
            return title.clone();
        }
        let fallback = first_line(&state.objective, 72);
        let Ok(connections) = self.connections_for(realm) else {
            return fallback;
        };
        let model = connections.fast.clone().unwrap_or(connections.main);
        let clip = |t: &str| t.chars().take(1500).collect::<String>();
        let messages = [Message::user_text(format!(
            "What the owner asked:\n<<<\n{}\n>>>\n\nWhat was done:\n<<<\n{}\n>>>\n\nFiles: {}",
            clip(&state.objective),
            clip(state.report.as_deref().unwrap_or_default()),
            state
                .change
                .as_ref()
                .map(|c| c.files.join(", "))
                .unwrap_or_default()
        ))];
        let request = ModelRequest {
            quick: true,
            ..ModelRequest::new(TITLE_PROMPT, &messages, &[])
        };
        let Ok(r) = model.complete(&request) else {
            return fallback;
        };
        state.input_tokens += r.input_tokens;
        state.output_tokens += r.output_tokens;
        state.usage.push(crate::state::Usage {
            at: Utc::now(),
            model: r.model.clone(),
            input: r.input_tokens,
            output: r.output_tokens,
            cache_read: r.cache_read_tokens,
            cache_write: r.cache_write_tokens,
            purpose: crate::state::Purpose::Title,
        });
        let text: String = r
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let title = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
            .trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == '*')
            .trim_end_matches('.')
            .trim()
            .to_string();
        if (8..=90).contains(&title.chars().count()) {
            title
        } else {
            fallback
        }
    }

    /// Follow a run's pull request: record it merged or closed, or keep its
    /// branch current with the base. Skips runs someone is driving.
    pub fn follow_pull_request(&self, realm: &Realm, run: &str) -> Result<PrEvent> {
        let mut state = self.load_run(realm, run)?;
        let Ok(_lock) = RunLock::acquire(&realm.runs_dir(), &state.id) else {
            return Ok(PrEvent::Nothing);
        };
        let Some(change) = state.change.as_mut() else {
            return Ok(PrEvent::Nothing);
        };
        let Some(pr) = change.pull_request.clone() else {
            return Ok(PrEvent::Nothing);
        };
        if change.status != ChangeStatus::Open || change.merging.is_some() {
            return Ok(PrEvent::Nothing);
        }
        let (gh, remote) = self.github(realm, &change.repo)?;
        let known_conflict = !change.conflicts.is_empty();
        let event = match gh.state(&remote, pr.number)? {
            PrState::Merged => {
                record_merged(&gh, &remote, change);
                PrEvent::Merged
            }
            PrState::Closed => {
                isolation::discard(change)?;
                let _ = gh.delete_branch(&remote, &change.branch);
                PrEvent::Closed
            }
            PrState::Open => {
                let base = change
                    .base_branch
                    .clone()
                    .context("this chat did not start from a branch")?;
                let target = gh.fetch(&change.repo, &remote, &base)?;
                match isolation::update(change, &target)? {
                    Update::UpToDate => PrEvent::Nothing,
                    Update::Updated => {
                        gh.push(&change.path, &remote, &change.branch)?;
                        if let Some(pr) = change.pull_request.as_mut() {
                            pr.pushed = change.head.clone();
                        }
                        PrEvent::Updated
                    }
                    Update::Conflict(_) => PrEvent::Conflict,
                }
            }
        };
        state.save()?;
        let title = match &event {
            PrEvent::Merged => Some((Kind::Completed, "merged")),
            PrEvent::Closed => Some((Kind::Stopped, "closed without merging")),
            PrEvent::Conflict if !known_conflict => {
                Some((Kind::Review, "conflicts with its base: Airth resolves them"))
            }
            _ => None,
        };
        if let Some((kind, what)) = title {
            self.home.inbox().push(
                kind,
                realm.name(),
                Some(&state.id),
                &format!("Pull request #{} {what}", pr.number),
                &first_line(&state.objective, 120),
            )?;
        }
        Ok(event)
    }

    /// Where a run's pull request stands on GitHub (None without one).
    pub fn pull_request_status(&self, realm: &Realm, run: &str) -> Result<Option<PrStatus>> {
        let state = self.load_run(realm, run)?;
        let Some(change) = state.change else {
            return Ok(None);
        };
        let Some(pr) = change.pull_request else {
            return Ok(None);
        };
        let (gh, remote) = self.github(realm, &change.repo)?;
        Ok(Some(gh.status(&remote, pr.number)?))
    }

    /// Squash-merge a run's pull request from Airth, once GitHub's checks
    /// pass on what was pushed, then record the change adopted. Airth checks
    /// this itself: the owner's token may be allowed to bypass the rules.
    pub fn merge_pull_request(&self, realm: &Realm, run: &str) -> Result<State> {
        let mut state = self.load_run(realm, run)?;
        let _lock = RunLock::acquire(&realm.runs_dir(), &state.id)?;
        let change = state
            .change
            .as_mut()
            .context("this run has no isolated change")?;
        let pr = change
            .pull_request
            .clone()
            .context("this change has no pull request")?;
        if change.status != ChangeStatus::Open {
            bail!("this change was already {}", change.status.as_str());
        }
        let (gh, remote) = self.github(realm, &change.repo)?;
        let status = gh.status(&remote, pr.number)?;
        match status.state {
            PrState::Merged => {}
            PrState::Closed => bail!("pull request #{} was closed", pr.number),
            PrState::Open => {
                if status.head != change.head {
                    bail!("GitHub has not received this chat's latest commit yet");
                }
                match status.merge_state.as_str() {
                    "dirty" => bail!("it conflicts with its base branch: resolve it first"),
                    "behind" => bail!(
                        "its base branch moved on: Airth brings it in within two minutes, then the checks run again"
                    ),
                    _ => {}
                }
                if status.checks != Checks::Passing {
                    bail!(
                        "the checks on GitHub {}",
                        match status.checks {
                            Checks::Failing => "failed",
                            Checks::Pending => "are still running",
                            _ => "have not run",
                        }
                    );
                }
                gh.merge(&remote, pr.number, &status.title, &status.head)?;
            }
        }
        record_merged(&gh, &remote, change);
        state.save()?;
        Ok(state)
    }

    /// Close a change's pull request and delete its branch on GitHub (best
    /// effort: the change is thrown away locally either way).
    pub(super) fn close_pull_request(&self, realm: &Realm, change: &isolation::Change) {
        let Some(pr) = &change.pull_request else {
            return;
        };
        if change.status != ChangeStatus::Open {
            return;
        }
        if let Ok((gh, remote)) = self.github(realm, &change.repo) {
            if let Err(e) = gh.close(&remote, pr.number) {
                eprintln!("could not close pull request #{}: {e:#}", pr.number);
            }
            let _ = gh.delete_branch(&remote, &change.branch);
        }
    }
}

/// A change's pull request was merged: the change is adopted, its branches
/// go, and the owner's checkout moves forward when it can.
fn record_merged(gh: &GitHub, remote: &Remote, change: &mut isolation::Change) {
    isolation::merged_elsewhere(change);
    let _ = gh.delete_branch(remote, &change.branch);
    if let Some(base) = change.base_branch.clone()
        && let Ok(target) = gh.fetch(&change.repo, remote, &base)
    {
        isolation::fast_forward(&change.repo, &base, &target);
    }
}

/// What following a pull request found.
#[derive(Debug, PartialEq, Eq)]
pub enum PrEvent {
    Nothing,
    Merged,
    Closed,
    /// The base moved on: merged in and pushed.
    Updated,
    /// The base moved on and conflicts: the chat must resolve it.
    Conflict,
}

#[cfg(test)]
pub(crate) static TEST_GITHUB: std::sync::Mutex<Option<(String, String)>> =
    std::sync::Mutex::new(None);

const TITLE_PROMPT: &str = "You write the title of a pull request, which also becomes its line in the release notes. \
One line in English, under 72 characters, saying what changed, in this style: \
\"Documents become a small code editor\", \"Chats can be archived or deleted from the sidebar\", \
\"Release notes leave out the version bump commit\". No prefix, no quotes, no final period. \
Answer with the title only.";

/// A pull request's description: what the chat reported, and its checks.
fn pull_request_body(state: &State) -> String {
    let mut body = state.report.clone().unwrap_or_default().trim().to_string();
    let config = crate::business::config_changes(
        state
            .change
            .as_ref()
            .map_or(&[][..], |c| c.files.as_slice()),
    );
    if !config.files.is_empty() {
        body.push_str(
            "\n\n**Business config changed**\nThis changes how the business runs: read these files before merging.\n",
        );
        if !config.agents.is_empty() {
            body.push_str(&format!("- Agents: {}\n", config.agents.join(", ")));
        }
        for f in &config.files {
            body.push_str(&format!("- `{f}`\n"));
        }
    }
    if let Some(checks) = state.change.as_ref().and_then(|c| c.checks.as_ref()) {
        body.push_str("\n\n**Checks**\n");
        for r in &checks.results {
            body.push_str(&format!(
                "- {} `{}`\n",
                if r.ok { "✓" } else { "✗" },
                r.command
            ));
        }
    }
    body.push_str(&format!(
        "\n---\nOpened by Airth from chat `{}` in realm `{}`.",
        state.id, state.realm
    ));
    body
}
