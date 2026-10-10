//! Terminal output for the CLI: prints a run's progress as it happens.

use crate::agent::{Observer, summarize_args};
use crate::models::{ContentBlock, ModelResponse};
use crate::state::{Event, EventStatus, State};
use chrono::{DateTime, Utc};
use serde_json::Value;

pub struct TerminalObserver {
    pub verbose: bool,
    /// The check run being printed, and how many of its results are printed.
    checks: Option<(DateTime<Utc>, usize)>,
}

impl TerminalObserver {
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            checks: None,
        }
    }
}

impl Observer for TerminalObserver {
    fn response(&mut self, iteration: u32, response: &ModelResponse) {
        eprintln!(
            "\x1b[2m── iteration {iteration} · {} · {} in / {} out tokens\x1b[0m",
            response.model, response.input_tokens, response.output_tokens
        );
        for block in &response.content {
            match block {
                ContentBlock::Reasoning { summary, .. } if !summary.trim().is_empty() => {
                    eprintln!("\x1b[2;3m{}\x1b[0m", summary.trim());
                }
                ContentBlock::Text { text } => println!("{}", text.trim()),
                _ => {}
            }
        }
    }

    fn tool_started(&mut self, name: &str, input: &Value) {
        eprintln!("\x1b[36m→ {name}\x1b[0m {}", summarize_args(name, input));
    }

    fn tool_finished(&mut self, event: &Event) {
        let (mark, color) = match event.status {
            EventStatus::Ok => ("✓ ok", "32"),
            EventStatus::Error => ("✗ error", "31"),
            EventStatus::Denied => ("⊘ denied", "33"),
        };
        let first_line = event.result.lines().next().unwrap_or("");
        eprintln!(
            "  \x1b[{color}m{mark}\x1b[0m \x1b[2m{} · {} bytes\x1b[0m",
            first_line.chars().take(100).collect::<String>(),
            event.result.len()
        );
        if self.verbose {
            for line in event.result.lines() {
                eprintln!("  \x1b[2m│ {line}\x1b[0m");
            }
        }
    }

    fn changed(&mut self, state: &State) {
        let Some(run) = state.change.as_ref().and_then(|c| c.checks.as_ref()) else {
            return;
        };
        let printed = match self.checks {
            Some((started, n)) if started == run.started_at => n,
            // Checks we see only once finished are from before this command.
            _ if run.finished => {
                self.checks = Some((run.started_at, run.results.len()));
                return;
            }
            _ => {
                eprintln!("\x1b[2m── checks\x1b[0m");
                0
            }
        };
        for r in run.results.iter().skip(printed) {
            if r.ok {
                eprintln!(
                    "  \x1b[32m✓\x1b[0m {} \x1b[2m({:.1}s)\x1b[0m",
                    r.command,
                    r.duration_ms as f64 / 1000.0
                );
            } else {
                eprintln!(
                    "  \x1b[31m✗\x1b[0m {} \x1b[2m({:.1}s)\x1b[0m",
                    r.command,
                    r.duration_ms as f64 / 1000.0
                );
                let lines: Vec<&str> = r.output.lines().collect();
                for line in &lines[lines.len().saturating_sub(30)..] {
                    eprintln!("  \x1b[2m│ {line}\x1b[0m");
                }
            }
        }
        self.checks = Some((run.started_at, run.results.len()));
    }
}
