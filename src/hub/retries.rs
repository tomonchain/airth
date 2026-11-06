//! Trying a failed action again (decision 55), only when a second attempt
//! cannot do it twice: the request never left, the service said it did not
//! do it, or the service takes an idempotency key. Anything else is an
//! incident. A failure of the service never demotes the agent; a refusal
//! of what the agent wrote does.

use super::*;
use crate::actions::Class;
use crate::business::store::Store;
use crate::integration::Integration;
use crate::state::EventStatus;
use crate::tools::{CUT, NOT_SENT};
use serde_json::Value;

const MAX_ATTEMPTS: u32 = 3;
/// Seconds to wait after the first attempt, then after the second.
const WAITS: [i64; 2] = [60, 300];
/// A `Retry-After` longer than this ends the retries.
const MAX_RETRY_AFTER: i64 = 15 * 60;

#[derive(Debug, PartialEq)]
pub enum Failure {
    /// The request never left: connection refused, DNS, TLS.
    NotSent,
    /// The service said it did not do it (429, 503), and when to come
    /// back, in seconds.
    NotDone(Option<i64>),
    /// It may have been done: cut after sending, 408, 502, 504, another 5xx.
    Ambiguous,
    /// The service refused what the agent wrote: a 4xx about the content.
    ByAgent,
    /// Anything else: the credentials, a redirect, Airth's own checks.
    Other,
}

/// What a call's outcome says about why it failed.
pub fn classify(status: EventStatus, result: &str) -> Failure {
    if status != EventStatus::Ok {
        return if result.starts_with(NOT_SENT) {
            Failure::NotSent
        } else if result.starts_with(CUT) {
            Failure::Ambiguous
        } else {
            Failure::Other
        };
    }
    let code: u16 = result
        .strip_prefix("HTTP ")
        .and_then(|r| r.get(..3))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    match code {
        429 | 503 => Failure::NotDone(retry_after(result)),
        401 | 403 | 407 => Failure::Other,
        408 | 500..=599 => Failure::Ambiguous,
        400..=499 => Failure::ByAgent,
        _ => Failure::Other,
    }
}

/// `Retry-After` in seconds, from the headers `http_request` shows.
fn retry_after(result: &str) -> Option<i64> {
    result
        .lines()
        .skip(1)
        .take_while(|l| !l.is_empty())
        .find_map(|l| l.strip_prefix("retry-after: "))
        .and_then(|v| v.trim().parse().ok())
}

/// Seconds to the next attempt after `attempts`, or why there is none.
pub fn next_attempt(
    class: Class,
    keyed: bool,
    failure: &Failure,
    attempts: u32,
) -> std::result::Result<i64, &'static str> {
    if class == Class::Commits {
        return Err("it commits the business, so Airth never tries it again alone");
    }
    match failure {
        Failure::NotSent | Failure::NotDone(_) => {}
        Failure::Ambiguous if class == Class::Read || keyed => {}
        Failure::Ambiguous => {
            return Err(
                "it may have been done; check that nothing was created before you retry it",
            );
        }
        Failure::ByAgent | Failure::Other => {
            return Err("trying again would fail the same way");
        }
    }
    if attempts >= MAX_ATTEMPTS {
        return Err("it failed three times");
    }
    match failure {
        Failure::NotDone(Some(after)) if *after > MAX_RETRY_AFTER => {
            Err("the service asked to wait more than 15 minutes")
        }
        Failure::NotDone(Some(after)) => Ok((*after).max(0)),
        _ => Ok(WAITS[attempts.clamp(1, 2) as usize - 1]),
    }
}

/// What the founder may do from the incident (decision 62): retry at once
/// when the failure proves nothing was done (`clear`), after checking when
/// it may have been (`ambiguous`: cut after sending, 5xx, 408, 409), and
/// never for what commits.
pub fn founder_retry(class: Class, failure: &Failure, result: &str) -> Option<&'static str> {
    if class == Class::Commits {
        return None;
    }
    let conflict = result.starts_with("HTTP 409");
    Some(match failure {
        Failure::Ambiguous => "ambiguous",
        Failure::ByAgent if conflict => "ambiguous",
        _ => "clear",
    })
}

/// The header in which `action`'s service takes an idempotency key, if
/// its declaration names one.
pub fn idempotency_header<'a>(integrations: &'a [Integration], action: &str) -> Option<&'a str> {
    let (connector, name) = action.split_once('.')?;
    integrations
        .iter()
        .find(|i| i.name == connector)?
        .actions
        .iter()
        .find(|a| a.name == name)?
        .idempotency_header
        .as_deref()
}

/// An action that failed, and what to tell the founder if it ends there.
pub struct Failed<'a> {
    pub key: &'a str,
    pub action: &'a str,
    pub agent: Option<&'a str>,
    pub routine: Option<&'a str>,
    pub status: EventStatus,
    pub result: &'a str,
    /// Attempts made, this one included.
    pub attempts: u32,
    pub title: String,
    pub detail: String,
}

impl Hub {
    /// Record a failed action: tried again later when that cannot do it
    /// twice, else an incident, which demotes the agent only when the
    /// service refused what the agent wrote. When it will be tried again.
    pub(super) fn action_failed(&self, realm: &Realm, f: &Failed) -> Result<Option<DateTime<Utc>>> {
        let failure = classify(f.status, f.result);
        let servers = crate::tools::mcp::McpStore::new(&realm.skills_dir()).list()?;
        let class = crate::actions::all(&realm.config.integrations, &servers)
            .iter()
            .find(|d| d.id == f.action)
            .map_or(Class::Commits, |d| d.class);
        let keyed = idempotency_header(&realm.config.integrations, f.action).is_some();
        let next = next_attempt(class, keyed, &failure, f.attempts);
        let at = next
            .as_ref()
            .ok()
            .map(|wait| Utc::now() + chrono::Duration::seconds(*wait));
        let journaled = Store::of(realm)?.action_failed(f.key, f.result, at)?;
        let why = match next {
            Ok(_) if journaled => return Ok(at),
            Ok(_) => "it is not in the action journal",
            Err(why) => why,
        };
        let retry = founder_retry(class, &failure, f.result);
        super::incidents::record(
            realm,
            &super::incidents::Incident {
                title: &f.title,
                detail: &format!("{}\n\nNot tried again: {why}.", f.detail),
                agent: f.agent,
                routine: f.routine,
                action: Some(f.action),
                demote: failure == Failure::ByAgent,
                retry: retry.map(|r| (f.key, r)),
            },
        )?;
        Ok(None)
    }

    /// A runtime that does declared HTTP actions as they are given, with
    /// nobody to ask: for what the founder approved, and its retries.
    pub(super) fn unattended(&self, realm: &Realm) -> Result<Runtime> {
        let root = realm.root()?;
        let workspace = Workspace::new(&root)?
            .with_secrets(realm.command_env()?, self.redactions(realm)?)
            .with_credentials(realm.integration_credentials()?)
            .with_network(!realm.config.offline);
        Ok(Runtime::new(
            tools::pipeline_tools(realm.config.integrations.clone()),
            PermissionGate::new(
                realm.config.policy.clone(),
                Box::new(super::pipelines::Unattended),
            ),
            workspace,
            CancelToken::default(),
        ))
    }

    /// Try again, once each, the failed actions whose time has come by
    /// `now`, across realms: what became of each, by realm and key. An
    /// approved item waits for its action's last attempt to be closed.
    pub fn retry_due(&self, now: DateTime<Utc>) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for realm in self.home.realms()? {
            if realm.config.archived || !realm.dir().join(crate::business::store::FILE).exists() {
                continue;
            }
            let mut store = Store::of(&realm)?;
            for r in store.due_retries(now)? {
                if !store.take_retry(r.id)? {
                    continue;
                }
                let args: Value = serde_json::from_str(&r.args)?;
                let event = self
                    .unattended(&realm)?
                    .invoke(&r.key, "http_request", &args);
                let attempts = r.attempts + 1;
                let what = if event.status == EventStatus::Ok
                    && super::incidents::succeeded(&event.result)
                {
                    store.finish_action(r.id, "confirmed", None)?;
                    if let Some(item) = r.item {
                        store.close_item(item, "done", &format!("done at attempt {attempts}"))?;
                    }
                    "done".to_string()
                } else {
                    let failed = Failed {
                        key: &r.key,
                        action: &r.action,
                        agent: r.agent.as_deref(),
                        routine: r.routine.as_deref(),
                        status: event.status,
                        result: &event.result,
                        attempts,
                        title: format!("{} failed", r.action),
                        detail: format!(
                            "{} [{}] failed at attempt {attempts}: {}",
                            r.action, r.key, event.result
                        ),
                    };
                    match self.action_failed(&realm, &failed)? {
                        Some(at) => format!("failed, tried again at {}", at.to_rfc3339()),
                        None => {
                            if let Some(item) = r.item {
                                store.close_item(item, "failed", &event.result)?;
                            }
                            "failed: an incident for the founder".into()
                        }
                    }
                };
                out.push(format!("{}/{}: {what}", realm.name(), r.key));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision 62: Retry at once when nothing was done, after checking
    /// when it may have been (409 included), never for what commits.
    #[test]
    fn the_founder_retries_what_proves_undone_and_checks_the_rest() {
        let ok = EventStatus::Ok;
        let retry = |class, result: &str| founder_retry(class, &classify(ok, result), result);
        for code in ["400", "401", "403", "404", "422"] {
            assert_eq!(
                retry(Class::Reversible, &format!("HTTP {code} X")),
                Some("clear"),
                "{code}"
            );
        }
        for code in ["408", "409", "502", "504"] {
            assert_eq!(
                retry(Class::Reversible, &format!("HTTP {code} X")),
                Some("ambiguous"),
                "{code}"
            );
        }
        let cut = "sent, no answer: request to x failed";
        assert_eq!(
            founder_retry(Class::Read, &classify(EventStatus::Error, cut), cut),
            Some("ambiguous")
        );
        assert_eq!(retry(Class::Commits, "HTTP 403 X"), None);
    }

    #[test]
    fn failures_are_told_apart() {
        let ok = EventStatus::Ok;
        let err = EventStatus::Error;
        assert_eq!(
            classify(err, "not sent: request to x failed"),
            Failure::NotSent
        );
        assert_eq!(
            classify(err, "sent, no answer: request to x failed"),
            Failure::Ambiguous
        );
        assert_eq!(classify(err, "network access is off"), Failure::Other);
        assert_eq!(
            classify(ok, "HTTP 429 Too Many Requests\nretry-after: 30\n\n{}"),
            Failure::NotDone(Some(30))
        );
        assert_eq!(
            classify(ok, "HTTP 503 Service Unavailable"),
            Failure::NotDone(None)
        );
        for code in ["502", "504", "500", "408"] {
            assert_eq!(
                classify(ok, &format!("HTTP {code} X")),
                Failure::Ambiguous,
                "{code}"
            );
        }
        for code in ["400", "404", "409", "422"] {
            assert_eq!(
                classify(ok, &format!("HTTP {code} X")),
                Failure::ByAgent,
                "{code}"
            );
        }
        for code in ["401", "403", "302"] {
            assert_eq!(
                classify(ok, &format!("HTTP {code} X")),
                Failure::Other,
                "{code}"
            );
        }
        // A body cannot pass for a header.
        assert_eq!(
            classify(ok, "HTTP 429 X\n\nretry-after: 1"),
            Failure::NotDone(None)
        );
    }

    #[test]
    fn only_what_cannot_happen_twice_is_tried_again() {
        use Class::*;
        let write = |f: &Failure, keyed| next_attempt(Reversible, keyed, f, 1);
        assert_eq!(write(&Failure::NotSent, false), Ok(60));
        assert_eq!(write(&Failure::NotDone(None), false), Ok(60));
        assert_eq!(write(&Failure::NotDone(Some(30)), false), Ok(30));
        assert!(write(&Failure::NotDone(Some(16 * 60)), false).is_err());
        assert!(write(&Failure::Ambiguous, false).is_err());
        assert_eq!(write(&Failure::Ambiguous, true), Ok(60));
        assert!(write(&Failure::ByAgent, true).is_err());
        assert_eq!(next_attempt(Read, false, &Failure::Ambiguous, 2), Ok(300));
        assert!(next_attempt(Read, false, &Failure::NotSent, 3).is_err());
        // Commits: never, key or not.
        for f in [Failure::NotSent, Failure::NotDone(None), Failure::Ambiguous] {
            assert!(next_attempt(Commits, true, &f, 1).is_err());
        }
    }
}
