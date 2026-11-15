//! An agent's package (decision 140): the agent, its routines and the
//! prompts they use, to take to another business. What belongs to this
//! business stays: its mailbox, its budget, its repositories, the
//! founder's lessons. No secret is in the business folder, so none can
//! leave with it.

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The package's format, checked when it comes back in.
pub const FORMAT: &str = "airth-agent/1";

/// Fields of an agent's file that belong to its business, never exported.
const LOCAL: &[&str] = &[
    "mailbox",
    "mail_server",
    "imap_port",
    "smtp_port",
    "monthly_max_cost_usd",
    "repos",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub format: String,
    /// The agent's name, its file's name.
    pub agent: String,
    pub title: String,
    /// Its files, by their path in the business folder.
    pub files: BTreeMap<String, String>,
}

/// The package of the agent `name`: its file, without what belongs to this
/// business; the routines it runs; the prompts and schemas their AI blocks
/// name.
pub fn export(root: &Path, name: &str) -> Result<Package> {
    crate::realm::validate_name(name).context("invalid agent name")?;
    let path = root.join(AGENTS).join(format!("{name}.md"));
    let text = std::fs::read_to_string(&path).with_context(|| format!("no agent {name}"))?;
    let (front, body) = split(&text);
    let front: String = front
        .lines()
        .filter(|l| {
            let key = l.split(':').next().unwrap_or_default().trim();
            !LOCAL.contains(&key)
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let title = crate::frontmatter::parse(&text)
        .text("title")
        .unwrap_or_else(|| name.to_string());
    let mut files = BTreeMap::new();
    files.insert(
        format!("{AGENTS}/{name}.md"),
        format!("{front}{}", without_lessons(body)),
    );
    let mut prompts = Vec::new();
    let dir = root.join("routines");
    if dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !file.ends_with(".json") {
                continue;
            }
            let text = std::fs::read_to_string(entry.path())?;
            let Ok(graph) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if graph["agent"].as_str() != Some(name) {
                continue;
            }
            named_prompts(&graph, &mut prompts);
            files.insert(format!("routines/{file}"), text);
        }
    }
    for prompt in prompts {
        let Some(stem) = prompt
            .strip_prefix("prompts/")
            .and_then(|p| p.strip_suffix(".md"))
        else {
            continue;
        };
        if !stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        {
            continue;
        }
        for rel in [prompt.clone(), format!("prompts/{stem}.schema.json")] {
            if let Ok(text) = std::fs::read_to_string(root.join(&rel)) {
                files.insert(rel, text);
            }
        }
    }
    Ok(Package {
        format: FORMAT.into(),
        agent: name.to_string(),
        title,
        files,
    })
}

/// Every `"prompt": "prompts/…"` a routine's blocks name, in order.
fn named_prompts(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                match (k.as_str(), v.as_str()) {
                    ("prompt", Some(p)) if p.starts_with("prompts/") => {
                        if !out.iter().any(|o| o == p) {
                            out.push(p.to_string());
                        }
                    }
                    _ => named_prompts(v, out),
                }
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| named_prompts(v, out)),
        _ => {}
    }
}

/// The body without the founder's lessons, which are this business's; the
/// sections after them stay.
fn without_lessons(body: &str) -> String {
    let Some(at) = body.find(LESSONS) else {
        return body.to_string();
    };
    let before = body[..at].trim_end();
    let rest = &body[at + LESSONS.len()..];
    match rest.find("\n## ") {
        Some(n) => format!("{before}\n\n{}", &rest[n + 1..]),
        None => format!("{before}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_package_holds_the_agent_its_routines_and_their_prompts_not_the_business() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "agents/support.md",
            "---\ntitle: Support\ndepartment: support\nintegrations: shop\nmailbox: support@n.example\nmail_server: mail.n.example\nmonthly_max_cost_usd: 20\nrepos: site\n---\nAnswer the buyers.\n\n## Lessons from the founder\n\n- Never promise a date.\n",
        );
        write(
            root,
            "routines/replies.json",
            r#"{"agent": "support", "blocks": [{"id": "draft", "type": "ai", "prompt": "prompts/reply.md"}]}"#,
        );
        write(
            root,
            "routines/other.json",
            r#"{"agent": "writer", "blocks": [{"id": "a", "type": "ai", "prompt": "prompts/article.md"}]}"#,
        );
        write(root, "prompts/reply.md", "Draft a reply.");
        write(root, "prompts/reply.schema.json", r#"{"type": "object"}"#);
        write(root, "prompts/article.md", "Write.");

        let p = export(root, "support").unwrap();
        assert_eq!((p.format.as_str(), p.title.as_str()), (FORMAT, "Support"));
        let names: Vec<&str> = p.files.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "agents/support.md",
                "prompts/reply.md",
                "prompts/reply.schema.json",
                "routines/replies.json"
            ]
        );
        let agent = &p.files["agents/support.md"];
        for gone in [
            "mailbox",
            "mail_server",
            "monthly_max_cost_usd",
            "repos",
            "Never promise",
        ] {
            assert!(!agent.contains(gone), "{gone} left with it: {agent}");
        }
        assert!(
            agent.contains("integrations: shop") && agent.contains("Answer the buyers."),
            "{agent}"
        );
        // No such agent, or a name that walks out of the folder.
        assert!(export(root, "ghost").is_err());
        assert!(export(root, "../agents").is_err());
    }

    #[test]
    fn lessons_leave_the_sections_after_them() {
        let body = "Job.\n\n## Lessons from the founder\n\n- One.\n\n## Tone\n\nPlain.\n";
        assert_eq!(without_lessons(body), "Job.\n\n## Tone\n\nPlain.\n");
        assert_eq!(without_lessons("Job.\n"), "Job.\n");
    }
}
