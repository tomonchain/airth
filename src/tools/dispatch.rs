//! The founder's interlocutor passes an order on to the team (decision
//! 120). The tool only proposes a plan, one task per agent; the founder
//! approves it in Needs you, and only then does each agent get its task,
//! within its own scope. Nothing of the interlocutor's rights passes on.

use super::{Tool, Workspace};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

/// The most tasks one plan holds.
pub const MAX_STEPS: usize = 6;
/// The most milestones one plan names.
pub const MAX_MILESTONES: usize = 6;

/// One agent's part of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub agent: String,
    pub task: String,
    /// The agents of earlier steps this one waits for: it starts once they
    /// have finished, with what they delivered.
    pub after: Vec<String>,
    /// The milestone it serves, by its place in the plan's milestones.
    pub milestone: Option<usize>,
}

/// A milestone of the project (decision 129).
#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub title: String,
    /// `YYYY-MM-DD`.
    pub due: Option<String>,
}

/// A plan as the founder approves it: the project it makes, or the name
/// of the one it adds to, and its steps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Proposal {
    pub name: String,
    pub description: String,
    pub milestones: Vec<Milestone>,
    pub steps: Vec<Step>,
}

/// Put the plan to the founder, for a new project or the open project
/// given; what it says back to the agent.
pub type Propose = Arc<dyn Fn(&Proposal, Option<i64>) -> Result<String> + Send + Sync>;

/// An agent the interlocutor asks the founder to hire (decision 136).
#[derive(Debug, Clone, PartialEq)]
pub struct Hiring {
    /// Its file's name: `agents/<name>.md`.
    pub name: String,
    pub title: String,
    pub department: String,
    /// Its job, as its file's body.
    pub job: String,
    /// Why the team needs it.
    pub why: String,
}

/// Put the hiring to the founder; what it says back to the agent.
pub type Hire = Arc<dyn Fn(&Hiring) -> Result<String> + Send + Sync>;

pub struct Dispatch {
    /// The agents a plan may name, with a line on each one's job.
    pub team: Vec<(String, String)>,
    /// The open projects a plan may add to: number and name.
    pub projects: Vec<(i64, String)>,
    pub propose: Propose,
    pub hire: Hire,
}

pub struct ProposePlan(pub Arc<Dispatch>);

/// The interlocutor asks the founder to hire an agent (decision 136): it
/// only proposes; the founder approves, edits the job or rejects.
pub struct ProposeAgent(pub Arc<Dispatch>);

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_string()
}

fn proposal(input: &Value, d: &Dispatch) -> Result<(Proposal, Option<i64>)> {
    let project = match input["project"].as_i64() {
        Some(id) => match d.projects.iter().find(|(p, _)| *p == id) {
            Some(p) => Some(p.clone()),
            None => bail!("there is no open project {id}: leave `project` out for a new one"),
        },
        None => None,
    };
    // `why` was the plan's line before plans were projects.
    let name = match &project {
        Some((_, name)) => name.clone(),
        None => Some(text(&input["name"]))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| text(&input["why"])),
    };
    let mut milestones = Vec::new();
    for m in input["milestones"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let due = Some(text(&m["due"])).filter(|d| !d.is_empty());
        if let Some(d) = &due
            && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err()
        {
            bail!("a milestone's date is YYYY-MM-DD, not '{d}'");
        }
        milestones.push(Milestone {
            title: text(&m["title"]),
            due,
        });
    }
    let mut steps = Vec::new();
    for s in input["steps"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let milestone = match text(&s["milestone"]) {
            t if t.is_empty() => None,
            t => match milestones
                .iter()
                .position(|m| m.title.eq_ignore_ascii_case(&t))
            {
                Some(i) => Some(i),
                None => bail!("'{t}' is not one of the plan's milestones"),
            },
        };
        steps.push(Step {
            agent: text(&s["agent"]),
            task: text(&s["task"]),
            after: s["after"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(text)
                .filter(|a| !a.is_empty())
                .collect(),
            milestone,
        });
    }
    let p = Proposal {
        name,
        description: text(&input["description"]),
        milestones,
        steps,
    };
    let names: Vec<&str> = d.team.iter().map(|(n, _)| n.as_str()).collect();
    check(&p, &names)?;
    Ok((p, project.map(|(id, _)| id)))
}

/// A plan the founder can run: a name, agents of the team, a task each, at
/// most [`MAX_STEPS`], each step waiting only for agents of earlier steps,
/// and the steps listed milestone by milestone, in the milestones' order.
pub fn check(p: &Proposal, team: &[&str]) -> Result<()> {
    if p.name.is_empty() {
        bail!("name the project in a few words");
    }
    if p.milestones.len() > MAX_MILESTONES {
        bail!("a plan names at most {MAX_MILESTONES} milestones");
    }
    if p.milestones.iter().any(|m| m.title.is_empty()) {
        bail!("a milestone has a title");
    }
    let steps = &p.steps;
    if steps.is_empty() {
        bail!("a plan gives at least one task");
    }
    if steps.len() > MAX_STEPS {
        bail!("a plan gives at most {MAX_STEPS} tasks: split the order");
    }
    for (i, s) in steps.iter().enumerate() {
        if !team.contains(&s.agent.as_str()) {
            bail!(
                "'{}' is not one of the agents work can be passed to",
                s.agent
            );
        }
        if s.task.is_empty() {
            bail!("say what {} must do", s.agent);
        }
        if s.milestone.is_some_and(|m| m >= p.milestones.len()) {
            bail!(
                "{}'s task names a milestone the plan does not have",
                s.agent
            );
        }
        if i > 0 && s.milestone < steps[i - 1].milestone {
            bail!(
                "list the tasks milestone by milestone, in the milestones' order: those of no milestone first"
            );
        }
        for a in &s.after {
            if !steps[..i].iter().any(|e| &e.agent == a) {
                bail!(
                    "{} can only wait for an agent of an earlier step, not '{a}'",
                    s.agent
                );
            }
        }
    }
    Ok(())
}

/// The plan back from its text, as the founder may have edited it: the
/// project's name on the first line, its description under it; a
/// milestone a line `## Title (by 2026-12-15)`; a step a line,
/// `- agent: task` or `- agent (after a, b): task`, serving the milestone
/// above it; a line that is not a step goes on the task above it.
pub fn parse(text: &str) -> Proposal {
    let mut p = Proposal::default();
    let mut head: Vec<&str> = Vec::new();
    for line in text.lines() {
        if let Some(title) = line.trim().strip_prefix("## ") {
            let (title, due) = match title
                .trim()
                .strip_suffix(')')
                .and_then(|t| t.rsplit_once("(by "))
            {
                Some((t, d)) if chrono::NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").is_ok() => {
                    (t.trim(), Some(d.trim().to_string()))
                }
                _ => (title.trim(), None),
            };
            p.milestones.push(Milestone {
                title: title.to_string(),
                due,
            });
            continue;
        }
        let milestone = p.milestones.len().checked_sub(1);
        let step = line.trim_start().strip_prefix("- ").and_then(|rest| {
            let (who, task) = rest.split_once(':')?;
            let (agent, after) = match who.split_once('(') {
                Some((agent, waits)) => {
                    let waits = waits.trim().trim_end_matches(')').trim();
                    let names = waits.strip_prefix("after").unwrap_or(waits);
                    (
                        agent.trim(),
                        names
                            .split(',')
                            .map(|a| a.trim().to_string())
                            .filter(|a| !a.is_empty())
                            .collect(),
                    )
                }
                None => (who.trim(), Vec::new()),
            };
            let simple = !agent.is_empty()
                && agent
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            simple.then(|| Step {
                agent: agent.to_string(),
                task: task.trim().to_string(),
                after,
                milestone,
            })
        });
        match (step, p.steps.last_mut()) {
            (Some(step), _) => p.steps.push(step),
            (None, Some(last)) if !line.trim().is_empty() => {
                last.task = format!("{}\n{}", last.task, line.trim());
            }
            (None, None) if !line.trim().is_empty() && p.milestones.is_empty() => {
                head.push(line.trim())
            }
            _ => {}
        }
    }
    if let Some((name, rest)) = head.split_first() {
        p.name = name.to_string();
        p.description = rest.join("\n");
    }
    p
}

/// The plan as the founder reads it: its name and description, then one
/// task a line, those of no milestone first, then each milestone's under it.
pub fn render(p: &Proposal) -> String {
    let line = |s: &Step| match s.after.as_slice() {
        [] => format!("- {}: {}", s.agent, s.task),
        after => format!("- {} (after {}): {}", s.agent, after.join(", "), s.task),
    };
    let mut out = vec![p.name.clone()];
    if !p.description.is_empty() {
        out.push(p.description.clone());
    }
    let loose: Vec<String> = p
        .steps
        .iter()
        .filter(|s| s.milestone.is_none())
        .map(line)
        .collect();
    if !loose.is_empty() {
        out.push(String::new());
        out.extend(loose);
    }
    for (i, m) in p.milestones.iter().enumerate() {
        out.push(String::new());
        out.push(match &m.due {
            Some(due) => format!("## {} (by {due})", m.title),
            None => format!("## {}", m.title),
        });
        out.extend(p.steps.iter().filter(|s| s.milestone == Some(i)).map(line));
    }
    out.join("\n")
}

fn hiring(input: &Value, d: &Dispatch) -> Result<Hiring> {
    let h = Hiring {
        name: text(&input["name"]).to_lowercase(),
        title: text(&input["title"]),
        department: text(&input["department"]),
        job: text(&input["job"]),
        why: text(&input["why"]),
    };
    crate::realm::validate_name(&h.name).map_err(|_| {
        anyhow::anyhow!("an agent's name is lowercase letters, digits and dashes (e.g. sales-lead)")
    })?;
    if d.team.iter().any(|(n, _)| *n == h.name) {
        bail!(
            "{} is already an agent of the team: give its task in a plan instead",
            h.name
        );
    }
    for (what, value) in [("title", &h.title), ("job", &h.job), ("why", &h.why)] {
        if value.is_empty() {
            bail!("say the agent's {what}");
        }
    }
    if h.title.contains('\n') {
        bail!("a title is one line");
    }
    Ok(h)
}

impl Tool for ProposePlan {
    fn spec(&self) -> ToolSpec {
        let team: Vec<String> = self
            .0
            .team
            .iter()
            .map(|(name, job)| format!("- {name}: {job}"))
            .collect();
        let projects = match self.0.projects.as_slice() {
            [] => "There is no open project.".to_string(),
            open => format!(
                "The open projects, by number:\n{}",
                open.iter()
                    .map(|(id, name)| format!("- {id}: {name}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        };
        ToolSpec {
            name: "propose_plan".into(),
            description: format!(
                "Pass the founder's order on to the agents whose job it is, as a project: name it in a few words and describe it in two lines (what it is for, what done looks like). For work spread over weeks, give it milestones with their dates, and say which milestone each task serves; list the tasks milestone by milestone. One task per agent, each with everything it must know (it sees nothing of this conversation). A step that needs another's result (a thread linking to an article) names it in `after`: it starts once that one has finished, with what it delivered. When the order adds to an open project, give its number in `project` instead of a new name. The plan goes to the founder in Needs you, who may edit it; once they approve it, each agent works on its task within its own tools and limits, and you are told when all have ended. After proposing, end your turn, saying in two lines what you planned. Do what is yours yourself; pass on only what another agent's job covers.\n\nThe agents:\n{}\n\n{projects}",
                team.join("\n")
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "The project's name, a few words, as the founder will read it"},
                    "description": {"type": "string", "description": "Two lines: what the project is for, and what done looks like"},
                    "project": {"type": "integer", "description": "The number of the open project this plan adds to; leave out for a new project"},
                    "milestones": {
                        "type": "array",
                        "maxItems": MAX_MILESTONES,
                        "description": "Only for work spread over weeks; empty otherwise",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string", "description": "What is reached, in a few words"},
                                "due": {"type": "string", "description": "YYYY-MM-DD, when there is a date"}
                            },
                            "required": ["title"]
                        }
                    },
                    "steps": {
                        "type": "array",
                        "maxItems": MAX_STEPS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "agent": {"type": "string", "enum": self.0.team.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()},
                                "task": {"type": "string", "description": "What this agent must do and deliver, complete"},
                                "after": {"type": "array", "items": {"type": "string"}, "description": "Agents of earlier steps whose result this one needs; empty when it can start at once"},
                                "milestone": {"type": "string", "description": "The title of the milestone this task serves, if any"}
                            },
                            "required": ["agent", "task"]
                        }
                    }
                },
                "required": ["name", "steps"]
            }),
        }
    }

    // It only puts a plan to the founder; nothing starts before they approve.
    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let (p, _) = proposal(input, &self.0)?;
        Ok(format!("propose to the founder:\n{}", render(&p)))
    }

    fn execute(&self, input: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        let (p, project) = proposal(input, &self.0)?;
        (self.0.propose)(&p, project)
    }
}

impl Tool for ProposeAgent {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "propose_agent".into(),
            description: "Ask the founder to hire a new agent, when the goal or the founder's order needs a job no agent of the team covers, and only then: never to share work an agent already does. Say why in two lines. Write its job as its file's body: its mission, how it works, what it delivers and how often, what it never does. The founder approves, edits the job or rejects it in Needs you. A new agent starts with no tools and no routine: the founder gives it its connectors and skills. After proposing, end your turn, saying in one line whom you proposed.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "A short name for its file, lowercase with dashes (sales-lead)"},
                    "title": {"type": "string", "description": "Its title, as the founder will read it (Sales lead)"},
                    "department": {"type": "string", "description": "leadership, marketing, sales, support, tech, operations, finance, legal or data"},
                    "job": {"type": "string", "description": "Its job, in Markdown: mission, how it works, what it delivers, what it never does"},
                    "why": {"type": "string", "description": "Two lines: what the team cannot do without it, and for which goal"}
                },
                "required": ["name", "title", "department", "job", "why"]
            }),
        }
    }

    // It only puts the hiring to the founder; nothing is written before
    // they approve.
    fn capability(&self) -> Capability {
        Capability::Internal
    }

    fn preview(&self, input: &Value, _: &Workspace) -> Result<String> {
        let h = hiring(input, &self.0)?;
        Ok(format!(
            "propose to the founder to hire {} ({})",
            h.title, h.name
        ))
    }

    fn execute(&self, input: &Value, _: &Workspace, _: &CancelToken) -> Result<String> {
        let h = hiring(input, &self.0)?;
        (self.0.hire)(&h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch() -> Dispatch {
        Dispatch {
            team: vec![
                ("writer".into(), "the blog".into()),
                ("developer".into(), "the site".into()),
            ],
            projects: vec![(3, "Year-end recap".into())],
            propose: Arc::new(|_, _| Ok(String::new())),
            hire: Arc::new(|_| Ok(String::new())),
        }
    }

    #[test]
    fn a_hiring_has_a_new_name_a_title_a_job_and_a_reason() {
        let d = dispatch();
        let ok = json!({"name": "Sales-Lead", "title": "Sales lead", "department": "sales", "job": "Find the firms.", "why": "Nobody sells."});
        let h = hiring(&ok, &d).unwrap();
        assert_eq!(
            (h.name.as_str(), h.title.as_str()),
            ("sales-lead", "Sales lead")
        );
        // An agent of the team is given a task, not hired again.
        let mut taken = ok.clone();
        taken["name"] = json!("writer");
        assert!(format!("{:#}", hiring(&taken, &d).unwrap_err()).contains("already"));
        let mut odd = ok.clone();
        odd["name"] = json!("../x");
        assert!(hiring(&odd, &d).is_err());
        for field in ["title", "job", "why"] {
            let mut blank = ok.clone();
            blank[field] = json!(" ");
            assert!(hiring(&blank, &d).is_err(), "{field}");
        }
        let mut two = ok.clone();
        two["title"] = json!("Sales\nlead");
        assert!(hiring(&two, &d).is_err());
    }

    #[test]
    fn a_plan_names_only_the_team_and_says_what_each_does() {
        let d = dispatch();
        let ok = json!({"name": "Year-end", "steps": [{"agent": "writer", "task": "A recap article"}, {"agent": "developer", "task": "A banner", "after": ["writer"]}]});
        let (p, project) = proposal(&ok, &d).unwrap();
        assert_eq!((p.steps.len(), project), (2, None));
        let text = render(&p);
        assert_eq!(
            text,
            "Year-end\n\n- writer: A recap article\n- developer (after writer): A banner"
        );
        // The text reads back as the same plan.
        assert_eq!(parse(&text), p);
        let stranger = json!({"name": "x", "steps": [{"agent": "chief-of-staff", "task": "x"}]});
        assert!(proposal(&stranger, &d).is_err());
        let blank = json!({"name": "x", "steps": [{"agent": "writer", "task": " "}]});
        assert!(proposal(&blank, &d).is_err());
        assert!(proposal(&json!({"name": "x", "steps": []}), &d).is_err());
        // A project has a name.
        assert!(proposal(&json!({"steps": [{"agent": "writer", "task": "x"}]}), &d).is_err());
        let many: Vec<Value> = (0..7)
            .map(|_| json!({"agent": "writer", "task": "x"}))
            .collect();
        assert!(proposal(&json!({"name": "x", "steps": many}), &d).is_err());
        // A step waits only for an earlier one: never itself, a later one or a stranger.
        let late = json!({"name": "x", "steps": [{"agent": "writer", "task": "x", "after": ["developer"]}, {"agent": "developer", "task": "y"}]});
        assert!(proposal(&late, &d).is_err());
    }

    #[test]
    fn a_project_has_milestones_and_a_plan_may_add_to_an_open_one() {
        let d = dispatch();
        let input = json!({
            "name": "Year-end recap",
            "description": "Hedgen's 2026, told everywhere.\nDone when the article and its thread are out.",
            "milestones": [{"title": "The article", "due": "2026-12-15"}, {"title": "Everywhere"}],
            "steps": [
                {"agent": "writer", "task": "Sources", "milestone": ""},
                {"agent": "writer", "task": "A recap article", "milestone": "the article"},
                {"agent": "developer", "task": "A banner", "after": ["writer"], "milestone": "Everywhere"}
            ]
        });
        let (p, _) = proposal(&input, &d).unwrap();
        assert_eq!(
            p.steps.iter().map(|s| s.milestone).collect::<Vec<_>>(),
            [None, Some(0), Some(1)]
        );
        let text = render(&p);
        assert_eq!(
            text,
            "Year-end recap\nHedgen's 2026, told everywhere.\nDone when the article and its thread are out.\n\n- writer: Sources\n\n## The article (by 2026-12-15)\n- writer: A recap article\n\n## Everywhere\n- developer (after writer): A banner"
        );
        assert_eq!(parse(&text), p);

        // Tasks out of their milestones' order, a stranger milestone, a bad date.
        let mut shuffled = input.clone();
        shuffled["steps"].as_array_mut().unwrap().swap(1, 2);
        shuffled["steps"][2]["after"] = json!([]);
        shuffled["steps"][1]["after"] = json!([]);
        assert!(proposal(&shuffled, &d).is_err());
        let mut stranger = input.clone();
        stranger["steps"][1]["milestone"] = json!("Launch");
        assert!(proposal(&stranger, &d).is_err());
        let mut late = input.clone();
        late["milestones"][0]["due"] = json!("15/12/2026");
        assert!(proposal(&late, &d).is_err());

        // Adding to open project 3 takes its name; an unknown one is refused.
        let more = json!({"project": 3, "name": "ignored", "steps": [{"agent": "writer", "task": "A LinkedIn post"}]});
        let (p, project) = proposal(&more, &d).unwrap();
        assert_eq!((p.name.as_str(), project), ("Year-end recap", Some(3)));
        assert!(
            proposal(
                &json!({"project": 4, "steps": [{"agent": "writer", "task": "x"}]}),
                &d
            )
            .is_err()
        );
    }

    #[test]
    fn the_founders_edit_of_a_plan_is_read_back() {
        let edited = "Year-end\nFor the year's last week.\n\n- writer: A recap article\n  of 800 words\n\n## Out (by 2026-12-20)\n- developer (after writer, ): A banner\n\n## Later (by soon)\n";
        let p = parse(edited);
        assert_eq!(
            (p.name.as_str(), p.description.as_str()),
            ("Year-end", "For the year's last week.")
        );
        assert_eq!(p.steps.len(), 2);
        assert_eq!(p.steps[0].task, "A recap article\nof 800 words");
        assert_eq!(
            (p.steps[1].after.as_slice(), p.steps[1].milestone),
            (&["writer".to_string()][..], Some(0))
        );
        // A date that is not one stays in the title.
        assert_eq!(
            p.milestones[1],
            Milestone {
                title: "Later (by soon)".into(),
                due: None
            }
        );
        assert_eq!(p.milestones[0].due.as_deref(), Some("2026-12-20"));
        check(&p, &["writer", "developer"]).unwrap();
        // A line removed is a step removed; a plan written before projects
        // reads its order line as the name.
        let p = parse("Year-end\n\n- developer: A banner");
        assert_eq!((p.name.as_str(), p.steps.len()), ("Year-end", 1));
    }
}
