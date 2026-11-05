//! Airth's library (decision 74): teams of agents a business installs from
//! the Marketplace. Each team is a folder of `library/teams/`, embedded in
//! the binary by build.rs: its agents, routines and prompts, copied into the
//! business folder as they are, and the skills and connectors it needs.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::{Component, Path};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/library.rs"));
}

/// The folders of a team that go into the business folder, as they are.
const COPIED: &[&str] = &["agents", "routines", "prompts"];

pub struct Team {
    pub id: String,
    pub title: String,
    pub description: String,
    pub skills: Vec<String>,
    pub connectors: Vec<String>,
    /// What the founder does once it is installed, one step per line.
    pub setup: Vec<String>,
    /// Its files, by their path in the business folder.
    pub files: Vec<(String, &'static str)>,
}

/// Every team of the library, by id.
pub fn teams() -> Vec<Team> {
    teams_in(embedded::FILES)
}

fn teams_in(files: &[(&str, &'static str)]) -> Vec<Team> {
    let mut out: Vec<Team> = Vec::new();
    for (path, text) in files {
        let Some(rest) = path.strip_prefix("teams/") else {
            continue;
        };
        let Some((id, rel)) = rest.split_once('/') else {
            continue;
        };
        let at = match out.iter().position(|t| t.id == id) {
            Some(i) => i,
            None => {
                out.push(Team {
                    id: id.to_string(),
                    title: id.to_string(),
                    description: String::new(),
                    skills: Vec::new(),
                    connectors: Vec::new(),
                    setup: Vec::new(),
                    files: Vec::new(),
                });
                out.len() - 1
            }
        };
        let team = &mut out[at];
        if rel == "team.md" {
            let doc = crate::frontmatter::parse(text);
            team.title = doc.text("title").unwrap_or_else(|| id.to_string());
            team.description = doc.text("description").unwrap_or_default();
            team.skills = doc.list("skills");
            team.connectors = doc.list("connectors");
            team.setup = doc
                .body
                .lines()
                .filter_map(|l| l.trim().strip_prefix("- "))
                .map(str::to_string)
                .collect();
        } else if COPIED.iter().any(|d| rel.starts_with(&format!("{d}/"))) {
            team.files.push((rel.to_string(), *text));
        }
    }
    out
}

pub fn team(id: &str) -> Result<Team> {
    teams()
        .into_iter()
        .find(|t| t.id == id)
        .with_context(|| format!("no team '{id}' in the library"))
}

/// A team's agents: name, title, department, accessory and mission.
fn agents(team: &Team) -> Vec<Value> {
    team.files
        .iter()
        .filter_map(|(rel, text)| {
            let name = rel.strip_prefix("agents/")?.strip_suffix(".md")?;
            let doc = crate::frontmatter::parse(text);
            let mission = doc
                .body
                .split("## Mission")
                .nth(1)
                .and_then(|m| m.split("\n## ").next())
                .map(|m| m.trim().to_string());
            Some(json!({
                "name": name,
                "title": doc.text("title"),
                "department": doc.text("department"),
                "accessory": doc.text("accessory"),
                "mission": mission,
            }))
        })
        .collect()
}

/// A team as the Marketplace shows it; `installed` when every one of its
/// agents is already in the business at `root`.
pub fn shown(team: &Team, root: Option<&Path>) -> Value {
    let routines: Vec<Value> = team
        .files
        .iter()
        .filter_map(|(rel, text)| {
            let name = rel.strip_prefix("routines/")?.strip_suffix(".json")?;
            let graph: Value = serde_json::from_str(text).ok()?;
            Some(json!({
                "name": name,
                "agent": graph["agent"],
                "description": graph["description"],
                "schedule": graph["trigger"]["schedule"],
            }))
        })
        .collect();
    let installed = root.is_some_and(|root| {
        team.files
            .iter()
            .filter(|(rel, _)| rel.starts_with("agents/"))
            .all(|(rel, _)| root.join(rel).exists())
    });
    json!({
        "id": team.id,
        "title": team.title,
        "description": team.description,
        "agents": agents(team),
        "routines": routines,
        "skills": team.skills,
        "connectors": team.connectors,
        "setup": team.setup,
        "installed": installed,
    })
}

/// Copy a team's agents, routines and prompts into the business folder at
/// `root`. Nothing is written when any of its files is there already: a
/// team never overwrites what the founder has. Returns the files written.
pub fn install(root: &Path, team: &Team) -> Result<Vec<String>> {
    let mut there = Vec::new();
    for (rel, text) in &team.files {
        let path = Path::new(rel);
        if !path.components().all(|c| matches!(c, Component::Normal(_))) {
            bail!("{rel}: a team's file stays inside its folder");
        }
        if rel.starts_with("routines/") {
            serde_json::from_str::<crate::routine::Graph>(text)
                .with_context(|| format!("{rel} is not a valid routine"))?;
        }
        if root.join(path).exists() {
            there.push(rel.clone());
        }
    }
    if !there.is_empty() {
        bail!(
            "already in this business, so nothing was installed: {}",
            there.join(", ")
        );
    }
    let mut written = Vec::new();
    for (rel, text) in &team.files {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        crate::realm::write_atomic(&path, text)?;
        written.push(rel.clone());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every team of the library reads: a title, agents with a job, routines
    /// that are graphs of known agents, prompts their AI blocks name.
    #[test]
    fn the_library_is_well_formed() {
        let teams = teams();
        assert!(teams.iter().any(|t| t.id == "media"));
        assert!(teams.iter().any(|t| t.id == "ecommerce"));
        for team in &teams {
            assert_ne!(team.title, team.id, "{}: team.md has a title", team.id);
            assert!(!team.description.is_empty(), "{}", team.id);
            assert!(!team.setup.is_empty(), "{}: setup steps", team.id);
            let has = |rel: &str| team.files.iter().any(|(r, _)| r == rel);
            for (rel, text) in &team.files {
                if let Some(name) = rel
                    .strip_prefix("agents/")
                    .and_then(|r| r.strip_suffix(".md"))
                {
                    crate::realm::validate_name(name).unwrap();
                    let doc = crate::frontmatter::parse(text);
                    assert!(doc.text("title").is_some(), "{rel}");
                    let d = doc.text("department").unwrap();
                    // Teams ship in the seven departments of CLAUDE.md.
                    assert!(
                        [
                            "leadership",
                            "marketing",
                            "sales",
                            "support",
                            "product",
                            "operations",
                            "finance"
                        ]
                        .contains(&d.as_str()),
                        "{rel}"
                    );
                    assert!(doc.body.contains("## Mission"), "{rel}");
                }
                if rel.starts_with("routines/") {
                    let graph: crate::routine::Graph =
                        serde_json::from_str(text).unwrap_or_else(|e| panic!("{rel}: {e}"));
                    assert!(
                        has(&format!("agents/{}.md", graph.agent)),
                        "{rel}: its agent"
                    );
                    let raw: Value = serde_json::from_str(text).unwrap();
                    for block in raw["blocks"].as_array().unwrap() {
                        if let Some(prompt) = block["prompt"].as_str() {
                            assert!(has(prompt), "{rel}: {prompt}");
                            let schema = prompt.replace(".md", ".schema.json");
                            assert!(has(&schema), "{rel}: {schema}");
                        }
                    }
                }
            }
            let shown = shown(team, None);
            assert!(!shown["agents"].as_array().unwrap().is_empty());
            assert!(
                shown["agents"][0]["mission"]
                    .as_str()
                    .is_some_and(|m| !m.is_empty())
            );
        }
    }

    /// Installing copies the team into the business folder; its routines
    /// then load as any routine does. A file already there stops it all.
    #[test]
    fn a_team_installs_once_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let media = team("media").unwrap();
        assert_eq!(shown(&media, Some(root.path()))["installed"], false);
        let written = install(root.path(), &media).unwrap();
        assert!(written.contains(&"agents/watcher.md".to_string()));
        assert!(written.contains(&"prompts/post.md".to_string()));
        assert_eq!(shown(&media, Some(root.path()))["installed"], true);
        assert!(
            !crate::business::job(root.path(), "watcher")
                .unwrap()
                .text
                .is_empty()
        );
        let report = crate::routine::load(
            &crate::routine::Checks {
                root: root.path(),
                actions: &crate::actions::all(&[], &[]),
                level: &|_: &str, _: &str| 3,
            },
            "weekly-report",
        );
        assert!(report.is_ok(), "{report:?}");
        // Again: refused, and the founder's edit stays.
        std::fs::write(
            root.path().join("agents/watcher.md"),
            "---\ntitle: Mine\ndepartment: marketing\n---\nMine.",
        )
        .unwrap();
        let err = install(root.path(), &media).unwrap_err().to_string();
        assert!(err.contains("agents/watcher.md"), "{err}");
        assert!(
            std::fs::read_to_string(root.path().join("agents/watcher.md"))
                .unwrap()
                .contains("Mine")
        );
        // A second team with a shared agent (chief-of-staff) is refused too,
        // before anything of it is written.
        let shop = team("ecommerce").unwrap();
        assert!(install(root.path(), &shop).is_err());
        assert!(!root.path().join("agents/support.md").exists());
        assert!(team("nope").is_err());
    }

    #[test]
    fn a_path_leaving_the_team_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let bad = teams_in(&[
            ("teams/x/team.md", "---\ntitle: X\n---\n- step"),
            ("teams/x/agents/../../evil.md", "x"),
        ]);
        assert!(install(root.path(), &bad[0]).is_err());
        assert!(!root.path().join("../evil.md").exists());
    }
}
