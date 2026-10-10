//! Permission layer. Every tool invocation passes through `PermissionGate::check`.
//!
//! The policy is fixed when the process starts and is not reachable from any
//! tool, so the agent cannot relax it at runtime. Edits to this file (or the
//! runtime that enforces it) are flagged loudly at approval time.

use anyhow::Result;
use std::io::{BufRead, Write};

/// What a tool is capable of doing. More granular capabilities can be added later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Read,
    Write,
    Execute,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Read => "read",
            Capability::Write => "write",
            Capability::Execute => "execute",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Allow,
    Ask,
    #[allow(dead_code)] // not used by the default v0.1 policy
    Deny,
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub read: Rule,
    pub write: Rule,
    pub execute: Rule,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            read: Rule::Allow,
            write: Rule::Ask,
            execute: Rule::Ask,
        }
    }
}

impl Policy {
    fn rule(&self, capability: Capability) -> Rule {
        match capability {
            Capability::Read => self.read,
            Capability::Write => self.write,
            Capability::Execute => self.execute,
        }
    }
}

/// A request for permission, shown to the human when approval is needed.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool: String,
    pub capability: Capability,
    /// Human-readable description of exactly what will happen (command, diff, ...).
    pub preview: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Approved,
    Denied(String),
}

pub enum Answer {
    Yes,
    No(String),
}

pub trait Approver {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer>;
}

pub struct PermissionGate {
    policy: Policy,
    approver: Box<dyn Approver>,
}

impl PermissionGate {
    pub fn new(policy: Policy, approver: Box<dyn Approver>) -> Self {
        Self { policy, approver }
    }

    pub fn check(&mut self, request: &PermissionRequest) -> Result<Verdict> {
        Ok(match self.policy.rule(request.capability) {
            Rule::Allow => Verdict::Allowed,
            Rule::Deny => Verdict::Denied(format!(
                "{} capability is disabled by policy",
                request.capability.as_str()
            )),
            Rule::Ask => match self.approver.ask(request)? {
                Answer::Yes => Verdict::Approved,
                Answer::No(reason) => Verdict::Denied(reason),
            },
        })
    }
}

/// Paths whose modification would change how permissions are enforced.
pub fn is_permission_critical(path: &str) -> bool {
    let p = path.trim_start_matches("./");
    p == "src/permissions.rs" || p == "src/runtime.rs"
}

/// Asks on the terminal. Reads answers from stdin.
pub struct TerminalApprover;

impl Approver for TerminalApprover {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        let mut err = std::io::stderr().lock();
        writeln!(err)?;
        writeln!(
            err,
            "┌─ approval required: {} ({})",
            request.tool,
            request.capability.as_str()
        )?;
        for w in &request.warnings {
            writeln!(err, "│ ⚠  {w}")?;
        }
        for line in request.preview.lines() {
            writeln!(err, "│ {line}")?;
        }
        writeln!(err, "└─ approve? [y/N, or type a reason to deny] ")?;
        err.flush()?;

        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line)? == 0 {
            return Ok(Answer::No("no approval received (stdin closed)".into()));
        }
        let answer = line.trim();
        Ok(match answer.to_lowercase().as_str() {
            "y" | "yes" => Answer::Yes,
            "" | "n" | "no" => Answer::No("denied by user".into()),
            _ => Answer::No(format!("denied by user: {answer}")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(bool);
    impl Approver for Fixed {
        fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
            Ok(if self.0 {
                Answer::Yes
            } else {
                Answer::No("no".into())
            })
        }
    }

    fn req(capability: Capability) -> PermissionRequest {
        PermissionRequest {
            tool: "t".into(),
            capability,
            preview: String::new(),
            warnings: vec![],
        }
    }

    #[test]
    fn default_policy_reads_automatically_and_asks_for_the_rest() {
        let mut gate = PermissionGate::new(Policy::default(), Box::new(Fixed(false)));
        assert_eq!(
            gate.check(&req(Capability::Read)).unwrap(),
            Verdict::Allowed
        );
        assert!(matches!(
            gate.check(&req(Capability::Write)).unwrap(),
            Verdict::Denied(_)
        ));
        assert!(matches!(
            gate.check(&req(Capability::Execute)).unwrap(),
            Verdict::Denied(_)
        ));

        let mut gate = PermissionGate::new(Policy::default(), Box::new(Fixed(true)));
        assert_eq!(
            gate.check(&req(Capability::Write)).unwrap(),
            Verdict::Approved
        );
    }

    #[test]
    fn deny_rule_never_consults_approver() {
        let policy = Policy {
            execute: Rule::Deny,
            ..Policy::default()
        };
        let mut gate = PermissionGate::new(policy, Box::new(Fixed(true)));
        assert!(matches!(
            gate.check(&req(Capability::Execute)).unwrap(),
            Verdict::Denied(_)
        ));
    }
}
