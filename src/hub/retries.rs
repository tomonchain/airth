//! Why an action failed (decision 55): whether the request left, whether
//! the service said it did not do it, or whether it refused what the agent
//! wrote. Only that last one demotes the agent.

use crate::state::EventStatus;
use crate::tools::{CUT, NOT_SENT};

#[derive(Debug, PartialEq)]
pub enum Failure {
    /// The request never left: connection refused, DNS, TLS.
    NotSent,
    /// The service said it did not do it: 429, 503.
    NotDone,
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
        429 | 503 => Failure::NotDone,
        401 | 403 | 407 => Failure::Other,
        408 | 500..=599 => Failure::Ambiguous,
        400..=499 => Failure::ByAgent,
        _ => Failure::Other,
    }
}

/// Whether a failed call takes its agent's action down a level: only when
/// the service refused what the agent wrote, never for its own failures.
pub fn demotes(status: EventStatus, result: &str) -> bool {
    classify(status, result) == Failure::ByAgent
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(classify(ok, "HTTP 429 Too Many Requests"), Failure::NotDone);
        assert_eq!(
            classify(ok, "HTTP 503 Service Unavailable"),
            Failure::NotDone
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
    }
}
