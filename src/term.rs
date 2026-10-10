//! Terminal output for the CLI: prints a run's progress as it happens.

use crate::agent::{Observer, summarize_args};
use crate::models::{ContentBlock, ModelResponse};
use crate::state::{Event, EventStatus};
use serde_json::Value;

pub struct TerminalObserver {
    pub verbose: bool,
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
}
