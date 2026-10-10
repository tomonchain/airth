mod agent;
mod hub;
mod models;
mod permissions;
mod runtime;
mod state;
mod tools;

use agent::AgentConfig;
use anyhow::Result;
use clap::{Parser, Subcommand};
use hub::Hub;
use permissions::TerminalApprover;
use state::{State, Status};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use tools::Workspace;

/// Airth — a local-first autonomous AI agent runtime.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the agent on an objective
    Run {
        /// What the agent should achieve, e.g. "Improve yourself"
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
    /// List the skills (tools) agents can use and the permission each needs
    Skills {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    /// List previous runs
    Runs {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    /// Check that the environment is ready
    Doctor {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
}

#[derive(clap::Args)]
struct RunOpts {
    /// Directory the agent may operate in
    #[arg(long, default_value = ".")]
    workspace: PathBuf,
    /// Maximum number of model calls
    #[arg(long, default_value_t = 50)]
    max_iterations: u32,
    /// Print full tool results
    #[arg(short, long)]
    verbose: bool,
}

impl RunOpts {
    fn hub(&self) -> Result<Hub> {
        Ok(Hub::new(Workspace::new(&self.workspace)?).with_model(models::from_env()?))
    }

    fn config(&self) -> AgentConfig {
        AgentConfig {
            max_iterations: self.max_iterations,
            verbose: self.verbose,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Cmd::Run { objective, opts }) => run(&objective, opts),
        Some(Cmd::Send { run, message, opts }) => send(&run, &message, opts),
        Some(Cmd::Skills { workspace }) => skills(&workspace),
        Some(Cmd::Runs { workspace }) => runs(&workspace),
        Some(Cmd::Doctor { workspace }) => doctor(&workspace),
        None => prompt_objective().and_then(|objective| {
            run(
                &objective,
                RunOpts {
                    workspace: ".".into(),
                    max_iterations: 50,
                    verbose: false,
                },
            )
        }),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
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

fn run(objective: &str, opts: RunOpts) -> Result<bool> {
    let hub = opts.hub()?;
    banner(&hub);
    eprintln!("objective: {objective}");
    let state = hub.dispatch(objective, opts.config(), Box::new(TerminalApprover))?;
    Ok(finish(&state))
}

fn send(run: &str, message: &str, opts: RunOpts) -> Result<bool> {
    let hub = opts.hub()?;
    banner(&hub);
    eprintln!("message → {run}: {message}");
    let state = hub.send(run, message, opts.config(), Box::new(TerminalApprover))?;
    Ok(finish(&state))
}

fn banner(hub: &Hub) {
    eprintln!(
        "\x1b[1mairth\x1b[0m {} · {} · {}",
        env!("CARGO_PKG_VERSION"),
        hub.model_name().unwrap_or_else(|| "no model".into()),
        hub.workspace().root().display()
    );
}

/// Print the outcome of a run; true if it completed.
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
    if let Some(path) = state.path() {
        eprintln!(
            "\x1b[2mstate: {}  (continue: airth send {} \"...\")\x1b[0m",
            path.display(),
            state.id
        );
    }
    state.status == Status::Completed
}

fn skills(workspace: &Path) -> Result<bool> {
    let hub = Hub::new(Workspace::new(workspace)?);
    for skill in hub.skills() {
        println!(
            "{:<16} {:<8} {}",
            skill.name,
            skill.capability.as_str(),
            skill.description.lines().next().unwrap_or("")
        );
    }
    Ok(true)
}

fn runs(workspace: &Path) -> Result<bool> {
    let hub = Hub::new(Workspace::new(workspace)?);
    let runs = hub.runs()?;
    if runs.is_empty() {
        eprintln!("no runs yet in {}", hub.runs_dir().display());
    }
    for run in runs {
        let objective = run.objective.lines().next().unwrap_or("");
        println!(
            "{:<24} {:<9} {:>3} it  {}",
            run.id,
            run.status.as_str(),
            run.iterations,
            objective.chars().take(80).collect::<String>()
        );
    }
    Ok(true)
}

fn doctor(workspace: &Path) -> Result<bool> {
    let mut ok = true;
    let mut check = |name: &str, result: Result<String, String>| match &result {
        Ok(detail) => println!("\x1b[32m✓\x1b[0m {name}: {detail}"),
        Err(detail) => {
            ok = false;
            println!("\x1b[31m✗\x1b[0m {name}: {detail}");
        }
    };

    let ws = Workspace::new(workspace).map_err(|e| e.to_string());
    check(
        "workspace",
        ws.as_ref()
            .map(|w| w.root().display().to_string())
            .map_err(Clone::clone),
    );
    if let Ok(ws) = &ws {
        check(
            "cargo project",
            if ws.root().join("Cargo.toml").exists() {
                Ok("Cargo.toml found".into())
            } else {
                Err("no Cargo.toml in workspace".into())
            },
        );
        let runs = ws.root().join(".airth/runs");
        check(
            "state directory",
            std::fs::create_dir_all(&runs)
                .map(|_| runs.display().to_string())
                .map_err(|e| e.to_string()),
        );
    }
    check("cargo", version_of("cargo"));
    check("git", version_of("git"));
    check(
        "model provider",
        models::from_env()
            .map(|m| m.name())
            .map_err(|e| e.to_string()),
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
