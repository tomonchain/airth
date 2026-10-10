//! The owner profile: what Airth knows about the person it works for. Written
//! by the owner (CLI or app), never by the agent, and shared with a realm's
//! runs only if that realm allows it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Owner {
    pub name: String,
    /// Free text: who they are, what they do.
    pub about: String,
    pub preferences: Vec<String>,
    pub goals: Vec<String>,
    pub commitments: Vec<String>,
    /// Lines Airth must not cross, whatever the objective says.
    pub boundaries: Vec<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// The list fields, by name, for the CLI and the app.
pub const LIST_FIELDS: [&str; 4] = ["preferences", "goals", "commitments", "boundaries"];

impl Owner {
    pub fn list_mut(&mut self, field: &str) -> Option<&mut Vec<String>> {
        match field {
            "preferences" => Some(&mut self.preferences),
            "goals" => Some(&mut self.goals),
            "commitments" => Some(&mut self.commitments),
            "boundaries" => Some(&mut self.boundaries),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty()
            && self.about.trim().is_empty()
            && [
                &self.preferences,
                &self.goals,
                &self.commitments,
                &self.boundaries,
            ]
            .iter()
            .all(|l| l.iter().all(|i| i.trim().is_empty()))
    }

    /// The profile as given to the model, or None if there is nothing to say.
    pub fn prompt_section(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut out = String::new();
        if !self.name.trim().is_empty() {
            out.push_str(&format!("Name: {}\n", self.name.trim()));
        }
        if !self.about.trim().is_empty() {
            out.push_str(&format!("About: {}\n", self.about.trim()));
        }
        let lists = [
            ("Preferences", &self.preferences),
            ("Goals", &self.goals),
            ("Commitments", &self.commitments),
            ("Boundaries (never cross these)", &self.boundaries),
        ];
        for (title, items) in lists {
            let items: Vec<&String> = items.iter().filter(|i| !i.trim().is_empty()).collect();
            if !items.is_empty() {
                out.push_str(&format!("{title}:\n"));
                for item in items {
                    out.push_str(&format!("- {}\n", item.trim()));
                }
            }
        }
        Some(out.trim_end().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_section_skips_empty_parts() {
        assert_eq!(Owner::default().prompt_section(), None);
        let owner = Owner {
            name: "Sam".into(),
            boundaries: vec!["Never email clients without asking".into(), " ".into()],
            ..Owner::default()
        };
        assert_eq!(
            owner.prompt_section().unwrap(),
            "Name: Sam\nBoundaries (never cross these):\n- Never email clients without asking"
        );
    }
}
