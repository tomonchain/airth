//! Permission layer. Every tool invocation passes through `PermissionGate::check`.
//!
//! Each realm has its own policy, stored in its `realm.json` inside the Airth
//! home. The policy is loaded when a run starts and is not reachable from any
//! tool (the home is outside every workspace), so the agent cannot relax it.
//! Edits to the files that enforce it are flagged loudly at approval time.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

/// What a tool is capable of doing. More granular capabilities can be added later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Read,
    Write,
    Execute,
    /// Saving something to the realm's long-term memory.
    Memory,
    /// Sending requests to other machines (integrations, web APIs).
    Network,
    /// Bookkeeping inside the run itself (its plan). Touches nothing outside
    /// the run, so it is always allowed and not part of a policy.
    Internal,
}

impl Capability {
    /// The capabilities a realm's policy governs.
    pub const ALL: [Capability; 5] = [
        Capability::Read,
        Capability::Write,
        Capability::Execute,
        Capability::Memory,
        Capability::Network,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Read => "read",
            Capability::Write => "write",
            Capability::Execute => "execute",
            Capability::Memory => "memory",
            Capability::Network => "network",
            Capability::Internal => "internal",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match Self::ALL.into_iter().find(|c| c.as_str() == s) {
            Some(c) => Ok(c),
            None => bail!("unknown capability '{s}' (read, write, execute, memory, network)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    Allow,
    Ask,
    Deny,
}

impl Rule {
    pub fn as_str(self) -> &'static str {
        match self {
            Rule::Allow => "allow",
            Rule::Ask => "ask",
            Rule::Deny => "deny",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "allow" => Ok(Rule::Allow),
            "ask" => Ok(Rule::Ask),
            "deny" => Ok(Rule::Deny),
            _ => bail!("unknown rule '{s}' (allow, ask, deny)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub read: Rule,
    pub write: Rule,
    pub execute: Rule,
    pub memory: Rule,
    pub network: Rule,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            read: Rule::Allow,
            write: Rule::Ask,
            execute: Rule::Ask,
            memory: Rule::Ask,
            network: Rule::Ask,
        }
    }
}

impl Policy {
    pub fn rule(&self, capability: Capability) -> Rule {
        match capability {
            Capability::Read => self.read,
            Capability::Write => self.write,
            Capability::Execute => self.execute,
            Capability::Memory => self.memory,
            Capability::Network => self.network,
            Capability::Internal => Rule::Allow,
        }
    }

    pub fn set(&mut self, capability: Capability, rule: Rule) {
        *match capability {
            Capability::Read => &mut self.read,
            Capability::Write => &mut self.write,
            Capability::Execute => &mut self.execute,
            Capability::Memory => &mut self.memory,
            Capability::Network => &mut self.network,
            Capability::Internal => return,
        } = rule;
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
    /// What an "allow for this run" answer would cover beyond this request:
    /// None means every request of this capability, Some(x) only requests
    /// with the same scope (e.g. one integration or host).
    pub scope: Option<String>,
}

impl PermissionRequest {
    /// Whether the owner may approve this kind of request for the rest of the
    /// run. Requests with warnings are always asked one by one.
    pub fn grantable(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Plain-language description of what a grant for this request covers.
    pub fn grant_label(&self) -> String {
        match (&self.scope, self.capability) {
            (Some(scope), _) => format!("{} requests to {scope}", self.capability.as_str()),
            (None, Capability::Write) => "all file changes".into(),
            (None, Capability::Execute) => "all commands".into(),
            (None, c) => format!("all {} requests", c.as_str()),
        }
    }
}

/// Standing approval the owner gave during a run: later requests it covers
/// are approved without asking. Kept with the run, so it survives a resume,
/// and never outlives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub capability: Capability,
    #[serde(default)]
    pub scope: Option<String>,
}

impl Grant {
    fn covers(&self, request: &PermissionRequest) -> bool {
        self.capability == request.capability && self.scope == request.scope
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Approved,
    Denied(String),
}

pub enum Answer {
    Yes,
    /// Yes, and for every later request this one's grant covers.
    YesForRun,
    No(String),
}

pub trait Approver: Send {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer>;
}

pub struct PermissionGate {
    policy: Policy,
    approver: Box<dyn Approver>,
    grants: Vec<Grant>,
}

impl PermissionGate {
    pub fn new(policy: Policy, approver: Box<dyn Approver>) -> Self {
        Self {
            policy,
            approver,
            grants: Vec::new(),
        }
    }

    /// Start with the grants a run already holds (when it is resumed).
    pub fn with_grants(mut self, grants: Vec<Grant>) -> Self {
        self.grants = grants;
        self
    }

    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    pub fn check(&mut self, request: &PermissionRequest) -> Result<Verdict> {
        Ok(match self.policy.rule(request.capability) {
            Rule::Allow => Verdict::Allowed,
            Rule::Deny => Verdict::Denied(format!(
                "{} capability is disabled by policy",
                request.capability.as_str()
            )),
            Rule::Ask if request.grantable() && self.grants.iter().any(|g| g.covers(request)) => {
                Verdict::Approved
            }
            Rule::Ask => match self.approver.ask(request)? {
                Answer::Yes => Verdict::Approved,
                Answer::YesForRun => {
                    if request.grantable() {
                        self.grants.push(Grant {
                            capability: request.capability,
                            scope: request.scope.clone(),
                        });
                    }
                    Verdict::Approved
                }
                Answer::No(reason) => Verdict::Denied(reason),
            },
        })
    }
}

/// Files that enforce Airth's security boundaries: permissions, realm
/// isolation, the command sandbox and secret handling.
const SECURITY_CRITICAL: &[&str] = &[
    "src/permissions.rs",
    "src/runtime.rs",
    "src/sandbox.rs",
    "src/tools/mod.rs",
    "src/tools/shell.rs",
    "src/hub.rs",
    "src/realm.rs",
    "src/home.rs",
    "src/isolation.rs",
    "src/integration.rs",
    "src/tools/http.rs",
];

/// Paths whose modification would change how permissions are enforced.
pub fn is_permission_critical(path: &str) -> bool {
    SECURITY_CRITICAL.contains(&path.trim_start_matches("./"))
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
        if request.grantable() {
            writeln!(
                err,
                "└─ approve? [y = yes, a = yes to {} for this run, N, or type a reason to deny] ",
                request.grant_label()
            )?;
        } else {
            writeln!(err, "└─ approve? [y/N, or type a reason to deny] ")?;
        }
        err.flush()?;

        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line)? == 0 {
            return Ok(Answer::No("no approval received (stdin closed)".into()));
        }
        let answer = line.trim();
        Ok(match answer.to_lowercase().as_str() {
            "y" | "yes" => Answer::Yes,
            "a" | "all" | "always" if request.grantable() => Answer::YesForRun,
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
            scope: None,
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

    struct Script(Vec<Answer>);
    impl Approver for Script {
        fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
            Ok(if self.0.is_empty() {
                Answer::No("asked".into())
            } else {
                self.0.remove(0)
            })
        }
    }

    #[test]
    fn grants_cover_later_requests_of_the_same_scope_only() {
        let mut gate =
            PermissionGate::new(Policy::default(), Box::new(Script(vec![Answer::YesForRun])));
        let github = PermissionRequest {
            scope: Some("github".into()),
            ..req(Capability::Network)
        };
        assert_eq!(gate.check(&github).unwrap(), Verdict::Approved);
        assert_eq!(gate.check(&github).unwrap(), Verdict::Approved);
        let other = PermissionRequest {
            scope: Some("example.com".into()),
            ..req(Capability::Network)
        };
        assert!(matches!(gate.check(&other).unwrap(), Verdict::Denied(_)));
        assert!(matches!(
            gate.check(&req(Capability::Write)).unwrap(),
            Verdict::Denied(_)
        ));
        assert_eq!(gate.grants().len(), 1);
    }

    #[test]
    fn requests_with_warnings_are_never_granted() {
        let warned = PermissionRequest {
            warnings: vec!["security".into()],
            ..req(Capability::Write)
        };
        let mut gate =
            PermissionGate::new(Policy::default(), Box::new(Script(vec![Answer::YesForRun])))
                .with_grants(vec![Grant {
                    capability: Capability::Write,
                    scope: None,
                }]);
        // The existing grant does not apply: the approver is asked.
        assert_eq!(gate.check(&warned).unwrap(), Verdict::Approved);
        // ...and "for this run" on it records nothing new.
        assert_eq!(gate.grants().len(), 1);
        assert!(matches!(gate.check(&warned).unwrap(), Verdict::Denied(_)));
        // Plain writes are covered by the grant.
        assert_eq!(
            gate.check(&req(Capability::Write)).unwrap(),
            Verdict::Approved
        );
    }

    #[test]
    fn internal_capability_is_always_allowed() {
        let policy = Policy {
            read: Rule::Deny,
            ..Policy::default()
        };
        let mut gate = PermissionGate::new(policy, Box::new(Fixed(false)));
        assert_eq!(
            gate.check(&req(Capability::Internal)).unwrap(),
            Verdict::Allowed
        );
        assert!(Capability::parse("internal").is_err());
    }
}
