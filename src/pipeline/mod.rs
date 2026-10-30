//! Business pipelines (decision 30): a generic engine, and what each
//! business declares in its realm's `business/pipelines/`, one JSON file per
//! pipeline. A pipeline is checked whole when it is loaded: an unknown step,
//! a missing prompt or a bad schema refuses it before it runs, never half
//! way. A prompt's version is the hash of its content.
//!
//! ```json
//! {
//!   "description": "Morning shortlist on the editorial pillars",
//!   "schedule": "daily 07:00",
//!   "steps": [
//!     { "type": "rss", "feeds": ["https://example.com/feed.xml"] },
//!     { "type": "dedupe", "by": "link" },
//!     { "type": "judge", "prompt": "prompts/watch.md", "schema": "schemas/watch.json", "keep": "relevant" },
//!     { "type": "report", "title": "Daily watch" }
//!   ]
//! }
//! ```
//!
//! A step type enters this list only when a real pipeline needs it.

use crate::objective::Schedule;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// Where a business's pipelines live, under its `business/` folder.
pub const DIR: &str = "pipelines";

pub mod rss;
pub mod run;
pub mod schedule;
mod schema;

/// One step, as declared.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Read items from RSS or Atom feeds.
    Rss { feeds: Vec<String> },
    /// Drop items already seen in an earlier run, by one of their fields.
    Dedupe { by: String },
    /// Ask the model about each item: the prompt, and the JSON schema its
    /// answer must match. With `keep`, only items whose answer sets that
    /// boolean go on. With `rank`, a number in the answer, they go on best
    /// first, and with `limit`, only that many.
    Judge {
        prompt: String,
        schema: String,
        #[serde(default)]
        keep: Option<String>,
        #[serde(default)]
        rank: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    /// Put each item in the draft queue, for the owner to approve, edit or
    /// reject. `text` names the field holding the draft, `answer.<field>`
    /// for a field of the judge's answer; `kind` is what the business calls
    /// it (post, thread, reply).
    Queue { kind: String, text: String },
    /// Write what is left as a report for the owner: one line per item,
    /// grouped by a field (`group_by`, like `answer.pillar`), with another
    /// field under each (`detail`, like `answer.why`).
    Report {
        title: String,
        #[serde(default)]
        group_by: Option<String>,
        #[serde(default)]
        detail: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    description: String,
    #[serde(default)]
    schedule: Option<String>,
    steps: Vec<Step>,
}

/// A prompt file, read once when the pipeline is loaded, with the schema
/// its answers must match.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    /// Relative to the business folder.
    pub path: String,
    pub text: String,
    /// SHA-256 of the text: recorded with every call made with it.
    pub version: String,
    pub schema: Value,
}

/// A pipeline that passed every check.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub description: String,
    /// When it runs on its own, as written ("daily 07:00"); checked.
    pub schedule: Option<String>,
    pub steps: Vec<Step>,
    /// The prompts of its judge steps, in step order: the n-th judge step
    /// uses the n-th prompt.
    pub prompts: Vec<Prompt>,
}

/// Every pipeline file of a business (`business` is its folder), by name,
/// each loaded or with why it was refused. No folder, no pipelines.
pub fn load_all(business: &Path) -> Result<Vec<(String, Result<Pipeline>)>> {
    let dir = business.join(DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let loaded = load(business, &name);
        out.push((name, loaded));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// One pipeline of a business, checked.
pub fn load(business: &Path, name: &str) -> Result<Pipeline> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("a pipeline's file name is lowercase letters, digits and dashes");
    }
    let path = business.join(DIR).join(format!("{name}.json"));
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let file: File = serde_json::from_str(&text).context("not a valid pipeline")?;
    if let Some(schedule) = &file.schedule {
        Schedule::parse(schedule)?;
    }
    check_order(&file.steps)?;
    let mut prompts = Vec::new();
    for (i, step) in file.steps.iter().enumerate() {
        check_step(business, step, &mut prompts).with_context(|| format!("step {}", i + 1))?;
    }
    Ok(Pipeline {
        description: file.description,
        schedule: file.schedule,
        steps: file.steps,
        prompts,
    })
}

/// A pipeline reads first, reports last, and does each only once.
fn check_order(steps: &[Step]) -> Result<()> {
    let Some(Step::Rss { .. }) = steps.first() else {
        bail!("a pipeline starts by reading a source (rss)");
    };
    for (i, step) in steps.iter().enumerate().skip(1) {
        match step {
            Step::Rss { .. } => bail!("step {}: only the first step reads a source", i + 1),
            Step::Report { .. } if i + 1 != steps.len() => {
                bail!("step {}: a report is the last step", i + 1)
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_step(business: &Path, step: &Step, prompts: &mut Vec<Prompt>) -> Result<()> {
    match step {
        Step::Rss { feeds } => {
            if feeds.is_empty() {
                bail!("rss needs at least one feed");
            }
            for feed in feeds {
                let url = url::Url::parse(feed).with_context(|| format!("feed {feed}"))?;
                if !matches!(url.scheme(), "http" | "https") {
                    bail!("feed {feed}: only http and https feeds");
                }
            }
        }
        Step::Dedupe { by } => {
            if by.trim().is_empty() {
                bail!("dedupe needs the field it compares (by)");
            }
        }
        Step::Judge {
            prompt,
            schema,
            keep,
            rank,
            limit,
        } => {
            let text = std::fs::read_to_string(inside(business, prompt)?)
                .with_context(|| format!("prompt {prompt}"))?;
            if text.trim().is_empty() {
                bail!("prompt {prompt} is empty");
            }
            let schema_text = std::fs::read_to_string(inside(business, schema)?)
                .with_context(|| format!("schema {schema}"))?;
            let schema_value: Value = serde_json::from_str(&schema_text)
                .with_context(|| format!("schema {schema} is not JSON"))?;
            if schema_value["type"] != "object" {
                bail!("schema {schema}: the model answers with an object (\"type\": \"object\")");
            }
            if let Some(keep) = keep
                && schema_value["properties"][keep]["type"] != "boolean"
            {
                bail!("keep: {keep} must be a boolean property of {schema}");
            }
            if let Some(rank) = rank
                && !matches!(
                    schema_value["properties"][rank]["type"].as_str(),
                    Some("number" | "integer")
                )
            {
                bail!("rank: {rank} must be a number property of {schema}");
            }
            match limit {
                Some(0) => bail!("limit: keep at least one item"),
                Some(_) if rank.is_none() => bail!("limit keeps the best items: give rank too"),
                _ => {}
            }
            prompts.push(Prompt {
                path: prompt.clone(),
                version: crate::update::sha256_hex(text.as_bytes()),
                text,
                schema: schema_value,
            });
        }
        Step::Queue { kind, text } => {
            if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                bail!("a queue's kind is a lowercase word, like post or reply");
            }
            if !is_path(text) {
                bail!("queue: text names the field holding the draft, like answer.draft");
            }
        }
        Step::Report {
            title,
            group_by,
            detail,
        } => {
            if title.trim().is_empty() {
                bail!("a report needs a title");
            }
            for path in [group_by, detail].into_iter().flatten() {
                if !is_path(path) {
                    bail!("report: {path} is not a field, like answer.pillar");
                }
            }
        }
    }
    Ok(())
}

/// A field of an item, dotted for nested ones: `answer.draft`.
fn is_path(path: &str) -> bool {
    !path.split('.').any(str::is_empty)
}

/// A path a pipeline names, which must stay inside the business folder.
fn inside(business: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        bail!("{relative}: give a path inside the business folder, like prompts/watch.md");
    }
    Ok(business.join(path))
}

impl Step {
    /// One line for the owner.
    pub fn describe(&self, prompts: &[Prompt]) -> String {
        match self {
            Step::Rss { feeds } => format!("read {} feed(s)", feeds.len()),
            Step::Dedupe { by } => format!("drop items already seen (by {by})"),
            Step::Judge {
                prompt,
                schema,
                keep,
                rank,
                limit,
            } => {
                let version = prompts
                    .iter()
                    .find(|p| &p.path == prompt)
                    .map_or("", |p| &p.version[..12]);
                let keep = keep
                    .as_deref()
                    .map(|k| format!(", keep where {k}"))
                    .unwrap_or_default();
                let rank = match (rank, limit) {
                    (Some(r), Some(n)) => format!(", the {n} best by {r}"),
                    (Some(r), None) => format!(", best {r} first"),
                    _ => String::new(),
                };
                format!("judge with {prompt} ({version}), answers as {schema}{keep}{rank}")
            }
            Step::Queue { kind, text } => format!("queue each as a {kind} draft, from {text}"),
            Step::Report {
                title, group_by, ..
            } => match group_by {
                Some(g) => format!("report \"{title}\", by {g}"),
                None => format!("report \"{title}\""),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A business folder with a valid daily watch.
    fn business() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let b = dir.path();
        for (path, text) in [
            ("prompts/watch.md", "Is this item about on-chain flows?"),
            (
                "schemas/watch.json",
                r#"{"type": "object", "properties": {"relevant": {"type": "boolean"}}}"#,
            ),
            (
                "pipelines/daily-watch.json",
                r#"{"description": "Morning shortlist", "schedule": "daily 07:00", "steps": [
                    {"type": "rss", "feeds": ["https://example.com/feed.xml"]},
                    {"type": "dedupe", "by": "link"},
                    {"type": "judge", "prompt": "prompts/watch.md", "schema": "schemas/watch.json", "keep": "relevant"},
                    {"type": "report", "title": "Daily watch"}]}"#,
            ),
        ] {
            std::fs::create_dir_all(b.join(path).parent().unwrap()).unwrap();
            std::fs::write(b.join(path), text).unwrap();
        }
        dir
    }

    /// Replace the daily watch's steps and load it.
    fn with_steps(steps: &str) -> Result<Pipeline> {
        let dir = business();
        std::fs::write(
            dir.path().join("pipelines/daily-watch.json"),
            format!(r#"{{"steps": {steps}}}"#),
        )
        .unwrap();
        load(dir.path(), "daily-watch")
    }

    fn refusal(steps: &str) -> String {
        format!("{:#}", with_steps(steps).expect_err("refused"))
    }

    const RSS: &str = r#"{"type": "rss", "feeds": ["https://example.com/feed.xml"]}"#;

    #[test]
    fn loads_a_pipeline_with_its_schedule_and_prompt_versions() {
        let dir = business();
        let all = load_all(dir.path()).unwrap();
        assert_eq!(all.len(), 1);
        let p = all[0].1.as_ref().unwrap();
        assert_eq!(p.description, "Morning shortlist");
        assert_eq!(p.schedule.as_deref(), Some("daily 07:00"));
        assert_eq!(p.steps.len(), 4);
        assert_eq!(p.prompts.len(), 1);
        assert_eq!(
            p.prompts[0].version,
            crate::update::sha256_hex(b"Is this item about on-chain flows?")
        );
        // Changing the prompt changes its version.
        std::fs::write(dir.path().join("prompts/watch.md"), "Reworded.").unwrap();
        let again = load(dir.path(), "daily-watch").unwrap();
        assert_ne!(again.prompts[0].version, p.prompts[0].version);
        assert!(
            p.steps[2]
                .describe(&p.prompts)
                .contains(&p.prompts[0].version[..12])
        );
    }

    #[test]
    fn a_business_without_pipelines_has_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_all(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn refuses_unknown_steps_and_fields() {
        assert!(refusal(&format!(r#"[{RSS}, {{"type": "post_to_x"}}]"#)).contains("post_to_x"));
        assert!(
            refusal(&format!(
                r#"[{RSS}, {{"type": "dedupe", "by": "link", "x": 1}}]"#
            ))
            .contains("unknown field")
        );
    }

    #[test]
    fn refuses_steps_out_of_order() {
        assert!(refusal(r#"[{"type": "dedupe", "by": "link"}]"#).contains("starts by reading"));
        assert!(refusal("[]").contains("starts by reading"));
        assert!(refusal(&format!("[{RSS}, {RSS}]")).contains("only the first step"));
        assert!(
            refusal(&format!(
                r#"[{RSS}, {{"type": "report", "title": "x"}}, {{"type": "dedupe", "by": "link"}}]"#
            ))
            .contains("last step")
        );
    }

    #[test]
    fn refuses_bad_feeds_schedules_and_names() {
        assert!(refusal(r#"[{"type": "rss", "feeds": []}]"#).contains("at least one feed"));
        assert!(refusal(r#"[{"type": "rss", "feeds": ["file:///etc/passwd"]}]"#).contains("http"));
        let dir = business();
        std::fs::write(
            dir.path().join("pipelines/daily-watch.json"),
            format!(r#"{{"schedule": "whenever", "steps": [{RSS}]}}"#),
        )
        .unwrap();
        assert!(load(dir.path(), "daily-watch").is_err());
        assert!(load(dir.path(), "../daily-watch").is_err());
        assert!(load(dir.path(), "Daily").is_err());
    }

    #[test]
    fn a_queue_names_its_kind_and_where_the_draft_is() {
        let queue = |kind: &str, text: &str| {
            with_steps(&format!(
                r#"[{RSS}, {{"type": "queue", "kind": "{kind}", "text": "{text}"}}]"#
            ))
        };
        assert!(queue("post", "answer.draft").is_ok());
        assert!(format!("{:#}", queue("Post!", "answer.draft").unwrap_err()).contains("kind"));
        assert!(format!("{:#}", queue("post", "answer.").unwrap_err()).contains("field"));
    }

    #[test]
    fn rank_limit_and_report_fields_are_checked() {
        let judge = |extra: &str| {
            with_steps(&format!(
                r#"[{RSS}, {{"type": "judge", "prompt": "prompts/watch.md", "schema": "schemas/watch.json"{extra}}}]"#
            ))
        };
        // The test schema has only a boolean: nothing to rank by.
        assert!(format!("{:#}", judge(r#", "rank": "relevant""#).unwrap_err()).contains("number"));
        assert!(format!("{:#}", judge(r#", "limit": 5"#).unwrap_err()).contains("give rank"));
        let report = |extra: &str| {
            with_steps(&format!(
                r#"[{RSS}, {{"type": "report", "title": "t"{extra}}}]"#
            ))
        };
        assert!(report(r#", "group_by": "answer.pillar", "detail": "answer.why""#).is_ok());
        assert!(report(r#", "group_by": "answer.""#).is_err());
    }

    #[test]
    fn judge_files_must_exist_stay_inside_and_fit() {
        let judge = |prompt: &str, schema: &str, keep: &str| {
            refusal(&format!(
                r#"[{RSS}, {{"type": "judge", "prompt": "{prompt}", "schema": "{schema}"{keep}}}]"#
            ))
        };
        assert!(judge("prompts/missing.md", "schemas/watch.json", "").contains("missing.md"));
        assert!(judge("../secrets.env", "schemas/watch.json", "").contains("inside the business"));
        assert!(judge("/etc/passwd", "schemas/watch.json", "").contains("inside the business"));
        assert!(
            judge(
                "prompts/watch.md",
                "schemas/watch.json",
                r#", "keep": "score""#
            )
            .contains("boolean")
        );
        assert!(judge("prompts/watch.md", "prompts/watch.md", "").contains("not JSON"));
    }

    #[test]
    fn one_bad_pipeline_does_not_hide_the_others() {
        let dir = business();
        std::fs::write(dir.path().join("pipelines/broken.json"), "{").unwrap();
        std::fs::write(dir.path().join("pipelines/notes.txt"), "ignored").unwrap();
        let all = load_all(dir.path()).unwrap();
        let names: Vec<_> = all.iter().map(|(n, r)| (n.as_str(), r.is_ok())).collect();
        assert_eq!(names, vec![("broken", false), ("daily-watch", true)]);
    }
}
