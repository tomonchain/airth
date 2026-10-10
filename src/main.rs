mod agent;
mod cancel;
mod cli;
mod home;
mod hub;
mod isolation;
mod memory;
mod models;
mod owner;
mod permissions;
mod realm;
mod runtime;
mod sandbox;
mod state;
mod term;
mod tools;
mod ui;

use agent::AgentConfig;
use anyhow::{Context, Result};
use cancel::CancelToken;
use clap::{Parser, Subcommand};
use home::Home;
use hub::{Hub, RunHooks};
use permissions::TerminalApprover;
use realm::Realm;
use state::{State, Status};
use std::io::{BufRead, Write};
use std::process::{Command, ExitCode};

/// Airth — a persistent, local-first personal agent.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Realm to work in (default: the realm whose root holds the current
    /// directory, else the default realm). Also AIRTH_REALM.
    #[arg(long, global = true, env = "AIRTH_REALM")]
    realm: Option<String>,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the agent on an objective
    Run {
        /// What the agent should achieve
        objective: String,
        #[command(flatten)]
        opts: RunOpts,
    },
    /// Continue a previous run with a follow-up message
    Send {
        /// Run id (see `airth runs`), or "last"
        run: String,
        /// The message to send to the agent
        message: String,
        #[command(flatten)]
        opts: RunOpts,
    },
    /// List the realm's runs
    Runs,
    /// Show an isolated run's change: files, checks and diff
    Review {
        /// Run id, or "last"
        #[arg(default_value = "last")]
        run: String,
    },
    /// Run the realm's checks on a run's change again
    Check {
        #[arg(default_value = "last")]
        run: String,
    },
    /// Merge a run's change into your checkout (its checks must pass)
    Adopt { run: String },
    /// Throw a run's change away
    Discard { run: String },
    /// Open the Airth app in your browser (served locally)
    Ui {
        /// Port on 127.0.0.1 (falls back to a free port if taken)
        #[arg(long, default_value_t = 7457)]
        port: u16,
        /// Don't open a browser, just print the link
        #[arg(long)]
        no_open: bool,
        /// Maximum number of model calls per run or follow-up
        #[arg(long, default_value_t = 50)]
        max_iterations: u32,
    },
    /// Manage realms: isolated areas of your life Airth works in
    Realm {
        #[command(subcommand)]
        command: cli::RealmCmd,
    },
    /// Show or edit what Airth knows about you
    Owner {
        #[command(subcommand)]
        command: cli::OwnerCmd,
    },
    /// Show or edit the realm's long-term memory
    Memory {
        #[command(subcommand)]
        command: cli::MemoryCmd,
    },
    /// Manage the realm's credentials
    Secret {
        #[command(subcommand)]
        command: cli::SecretCmd,
    },
    /// List the skills (tools) agents can use and the permission each needs
    Skills,
    /// Check that the environment is ready
    Doctor,
}

#[derive(clap::Args)]
struct RunOpts {
    /// Maximum number of model calls
    #[arg(long, default_value_t = 50)]
    max_iterations: u32,
    /// Print full tool results
    #[arg(short, long)]
    verbose: bool,
}

impl RunOpts {
    fn config(&self) -> AgentConfig {
        AgentConfig {
            max_iterations: self.max_iterations,
        }
    }
}

/// The CLI's view of where it is: the home and the selected realm.
pub struct Ctx {
    home: Home,
    realm_name: Option<String>,
}

impl Ctx {
    pub fn home(&self) -> &Home {
        &self.home
    }

    pub fn realm(&self) -> Result<Realm> {
        let cwd = std::env::current_dir()?;
        self.home.resolve_realm(self.realm_name.as_deref(), &cwd)
    }

    pub fn hub(&self) -> Hub {
        Hub::new(self.home.clone())
    }
}

/// First Ctrl-C stops the run cleanly (the current command is killed and the
/// state saved); a second one quits immediately.
fn cancel_on_ctrl_c() -> Result<CancelToken> {
    let cancel = CancelToken::default();
    let c = cancel.clone();
    ctrlc::set_handler(move || {
        if c.is_cancelled() {
            std::process::exit(130);
        }
        c.cancel();
        eprintln!("\n\x1b[33mstopping after the current step… (Ctrl-C again to quit now)\x1b[0m");
    })
    .context("cannot install Ctrl-C handler")?;
    Ok(cancel)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = Home::locate().and_then(|home| {
        let ctx = Ctx {
            home,
            realm_name: cli.realm,
        };
        dispatch(&ctx, cli.command)
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(ctx: &Ctx, command: Option<Cmd>) -> Result<bool> {
    match command {
        Some(Cmd::Run { objective, opts }) => run(ctx, &objective, opts),
        Some(Cmd::Send { run, message, opts }) => send(ctx, &run, &message, opts),
        Some(Cmd::Runs) => runs(ctx),
        Some(Cmd::Review { run }) => review(ctx, &run),
        Some(Cmd::Check { run }) => check(ctx, &run),
        Some(Cmd::Adopt { run }) => {
            let realm = ctx.realm()?;
            let state = ctx.hub().adopt(&realm, &run)?;
            let repo = state.change.as_ref().map(|c| c.repo.display().to_string());
            eprintln!(
                "\x1b[32m✓\x1b[0m adopted run {} into {}",
                state.id,
                repo.unwrap_or_default()
            );
            Ok(true)
        }
        Some(Cmd::Discard { run }) => {
            let realm = ctx.realm()?;
            let state = ctx.hub().discard(&realm, &run)?;
            eprintln!("discarded the change of run {}", state.id);
            Ok(true)
        }
        Some(Cmd::Ui {
            port,
            no_open,
            max_iterations,
        }) => ui::serve(
            ctx.hub(),
            ui::UiOptions {
                port,
                open_browser: !no_open,
                max_iterations,
                realm: ctx.realm().ok().map(|r| r.name().to_string()),
            },
        )
        .map(|_| true),
        Some(Cmd::Realm { command }) => cli::realm(ctx, command),
        Some(Cmd::Owner { command }) => cli::owner(ctx, command),
        Some(Cmd::Memory { command }) => cli::memory(ctx, command),
        Some(Cmd::Secret { command }) => cli::secret(ctx, command),
        Some(Cmd::Skills) => skills(ctx),
        Some(Cmd::Doctor) => doctor(ctx),
        None => prompt_objective().and_then(|objective| {
            run(
                ctx,
                &objective,
                RunOpts {
                    max_iterations: 50,
                    verbose: false,
                },
            )
        }),
    }
}

fn prompt_objective() -> Result<String> {
    eprint!("Objective: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let objective = line.trim().to_string();
    if objective.is_empty() {
        anyhow::bail!("no objective given (usage: airth run \"<objective>\")");
    }
    Ok(objective)
}

fn run(ctx: &Ctx, objective: &str, opts: RunOpts) -> Result<bool> {
    let realm = ctx.realm()?;
    let hub = ctx.hub();
    banner(&hub, &realm);
    eprintln!("objective: {objective}");
    let mut observer = term::TerminalObserver::new(opts.verbose);
    let hooks = RunHooks {
        approver: Box::new(TerminalApprover),
        observer: &mut observer,
        cancel: cancel_on_ctrl_c()?,
    };
    let state = hub.dispatch(&realm, objective, opts.config(), hooks)?;
    Ok(finish(&state))
}

fn send(ctx: &Ctx, run: &str, message: &str, opts: RunOpts) -> Result<bool> {
    let realm = ctx.realm()?;
    let hub = ctx.hub();
    banner(&hub, &realm);
    eprintln!("message → {run}: {message}");
    let mut observer = term::TerminalObserver::new(opts.verbose);
    let hooks = RunHooks {
        approver: Box::new(TerminalApprover),
        observer: &mut observer,
        cancel: cancel_on_ctrl_c()?,
    };
    let state = hub.send(&realm, run, message, opts.config(), hooks)?;
    Ok(finish(&state))
}

fn banner(hub: &Hub, realm: &Realm) {
    let model = hub
        .model_for(realm)
        .map(|m| m.name())
        .unwrap_or_else(|_| "no model".into());
    let root = realm
        .root()
        .map(|r| r.display().to_string())
        .unwrap_or_default();
    eprintln!(
        "\x1b[1mairth\x1b[0m {} · realm \x1b[1m{}\x1b[0m · {} · {}{}",
        env!("CARGO_PKG_VERSION"),
        realm.name(),
        model,
        root,
        if realm.config.isolation == isolation::Isolation::Worktree {
            " (isolated worktree per run)"
        } else {
            ""
        }
    );
}

/// Print the outcome of a run; true if it completed (and its checks passed).
fn finish(state: &State) -> bool {
    eprintln!(
        "\n\x1b[1m{:?}\x1b[0m after {} iteration(s), {} tool call(s)",
        state.status,
        state.iterations,
        state.events.len()
    );
    if state.status != Status::Completed
        && let Some(report) = &state.report
    {
        eprintln!("{report}");
    }
    let mut ok = state.status == Status::Completed;
    if let Some(change) = &state.change
        && change.has_changes()
    {
        eprintln!("changed on branch {}:", change.branch);
        for f in &change.files {
            eprintln!("  {f}");
        }
        if let Some(checks) = &change.checks {
            ok &= change.checks_passed();
            eprintln!(
                "checks: {}",
                if change.checks_passed() {
                    "\x1b[32mpassed\x1b[0m"
                } else if checks.finished {
                    "\x1b[31mfailed\x1b[0m"
                } else {
                    "did not finish"
                }
            );
        }
        eprintln!(
            "\x1b[2mreview: airth review {0} · adopt: airth adopt {0} · discard: airth discard {0}\x1b[0m",
            state.id
        );
    }
    eprintln!("\x1b[2mcontinue: airth send {} \"...\"\x1b[0m", state.id);
    ok
}

fn print_change_summary(change: &isolation::Change) {
    if !change.has_changes() {
        eprintln!("no files changed");
        return;
    }
    eprintln!(
        "change on branch {} ({}):",
        change.branch,
        change.status.as_str()
    );
    for f in &change.files {
        eprintln!("  {f}");
    }
    match &change.checks {
        None => eprintln!("checks: not run"),
        Some(checks) => {
            for r in &checks.results {
                eprintln!(
                    "  {} {} ({:.1}s)",
                    if r.ok {
                        "\x1b[32m✓\x1b[0m"
                    } else {
                        "\x1b[31m✗\x1b[0m"
                    },
                    r.command,
                    r.duration_ms as f64 / 1000.0
                );
            }
            if !checks.finished {
                eprintln!("  (checks did not finish)");
            } else if checks.commit != change.head {
                eprintln!("  (checks ran on an older commit)");
            }
        }
    }
}

fn runs(ctx: &Ctx) -> Result<bool> {
    let realm = ctx.realm()?;
    let runs = ctx.hub().runs(&realm)?;
    if runs.is_empty() {
        eprintln!("no runs yet in realm {}", realm.name());
    }
    for run in runs {
        let objective = run.objective.lines().next().unwrap_or("");
        let change = match (run.change, run.checks_passed) {
            (Some(c), _) if c != isolation::ChangeStatus::Open => c.as_str(),
            (Some(_), Some(true)) => "checks ✓",
            (Some(_), Some(false)) => "checks ✗",
            _ => "",
        };
        println!(
            "{:<24} {:<11} {:>3} it  {:<10} {}",
            run.id,
            run.display_status(),
            run.iterations,
            change,
            objective.chars().take(70).collect::<String>()
        );
    }
    Ok(true)
}

fn review(ctx: &Ctx, run: &str) -> Result<bool> {
    let realm = ctx.realm()?;
    let hub = ctx.hub();
    let state = hub.load_run(&realm, run)?;
    let change = state
        .change
        .as_ref()
        .context("this run has no isolated change (its realm works in place)")?;
    eprintln!("\x1b[1m{}\x1b[0m  {}", state.id, state.objective);
    if let Some(report) = &state.report {
        eprintln!("\n{report}\n");
    }
    print_change_summary(change);
    if let Some(checks) = &change.checks
        && let Some(failed) = checks.results.iter().find(|r| !r.ok)
    {
        eprintln!("\n--- {} ---\n{}", failed.command, failed.output);
    }
    if change.has_changes() {
        println!("{}", hub.diff(&realm, run)?);
    }
    Ok(true)
}

fn check(ctx: &Ctx, run: &str) -> Result<bool> {
    let realm = ctx.realm()?;
    let mut observer = term::TerminalObserver::new(false);
    let state = ctx
        .hub()
        .check(&realm, run, &mut observer, &cancel_on_ctrl_c()?)?;
    let passed = state.change.as_ref().is_some_and(|c| c.checks_passed());
    eprintln!(
        "checks {}",
        if passed {
            "\x1b[32mpassed\x1b[0m"
        } else {
            "\x1b[31mfailed\x1b[0m"
        }
    );
    Ok(passed)
}

fn skills(ctx: &Ctx) -> Result<bool> {
    let policy = ctx.realm().map(|r| r.config.policy).unwrap_or_default();
    for skill in ctx.hub().skills() {
        println!(
            "{:<16} {:<8} {:<6} {}",
            skill.name,
            skill.capability.as_str(),
            policy.rule(skill.capability).as_str(),
            skill.description.lines().next().unwrap_or("")
        );
    }
    Ok(true)
}

fn doctor(ctx: &Ctx) -> Result<bool> {
    let mut ok = true;
    let mut check = |name: &str, result: Result<String, String>| match &result {
        Ok(detail) => println!("\x1b[32m✓\x1b[0m {name}: {detail}"),
        Err(detail) => {
            ok = false;
            println!("\x1b[31m✗\x1b[0m {name}: {detail}");
        }
    };
    check("home", Ok(ctx.home().dir().display().to_string()));
    let realm = ctx.realm();
    check(
        "realm",
        realm
            .as_ref()
            .map(|r| {
                let root = r
                    .root()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                format!(
                    "{} → {} (isolation: {})",
                    r.name(),
                    root,
                    r.config.isolation.as_str()
                )
            })
            .map_err(|e| format!("{e:#}")),
    );
    check(
        "command sandbox",
        if sandbox::available() {
            Ok("sandbox-exec: commands cannot reach other realms or Airth's data".into())
        } else {
            Err("not available on this platform: commands are not confined to the realm (approve them carefully)".into())
        },
    );
    check("git", version_of("git"));
    check("cargo", version_of("cargo"));
    check(
        "model provider",
        match &realm {
            Ok(r) => ctx
                .hub()
                .model_for(r)
                .map(|m| m.name())
                .map_err(|e| e.to_string()),
            Err(_) => models::from_env()
                .map(|m| m.name())
                .map_err(|e| e.to_string()),
        },
    );
    Ok(ok)
}

fn version_of(program: &str) -> Result<String, String> {
    Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| format!("not found ({e})"))
        .and_then(|o| {
            if o.status.success() {
                Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                Err(format!("exited with {}", o.status))
            }
        })
}
