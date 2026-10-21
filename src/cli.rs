//! CLI commands that manage what Airth knows: realms, the owner profile, a
//! realm's memory and credentials. Only the owner reaches these; the agent has
//! no tool that touches them.

use crate::Ctx;
use crate::home::NewRealm;
use crate::isolation::Isolation;
use crate::owner::LIST_FIELDS;
use crate::permissions::{Capability, Rule};
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum RepoCmd {
    /// List the realm's repositories
    List,
    /// Connect a repository: a URL to clone, or a folder on this machine
    Add {
        /// https://github.com/owner/name, git@…, or a path
        source: String,
        /// Its name in the realm (default: from the URL or folder)
        #[arg(long)]
        name: Option<String>,
        /// Where to clone it (default: next to the realm's folder)
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Forget a repository (its files stay)
    Remove { name: String },
    /// Set the commands that verify a change in this repository (none to clear)
    Checks { name: String, commands: Vec<String> },
    /// How changes in this repository are delivered: adopt or pr
    Delivery {
        name: String,
        #[arg(value_parser = ["adopt", "pr"])]
        mode: String,
    },
}

#[derive(Subcommand)]
pub enum RealmCmd {
    /// List realms
    List,
    /// Create a realm
    Create {
        /// Short name, e.g. personal, acme, freelance
        name: String,
        /// Directory the realm works in (default: a private folder in the Airth home)
        #[arg(long)]
        root: Option<PathBuf>,
        /// What this realm is about; the agent sees it
        #[arg(long, default_value = "")]
        description: String,
        /// none, or worktree (default for git repositories: each run gets its own worktree)
        #[arg(long)]
        isolation: Option<String>,
    },
    /// Show a realm's settings (default: the current realm)
    Show { name: Option<String> },
    /// Make a realm the default
    Use { name: String },
    /// Archive a realm (or bring it back with --undo)
    Archive {
        name: String,
        #[arg(long)]
        undo: bool,
    },
    /// Set the rule for a capability: airth realm policy execute allow
    Policy {
        /// read, write, execute or memory
        capability: String,
        /// allow, ask or deny
        rule: String,
    },
    /// Set the commands that verify a run's change (none to clear)
    Checks { commands: Vec<String> },
    /// The git repositories the realm's chats work in
    #[command(subcommand)]
    Repo(RepoCmd),
    /// How chats' changes reach the repository: adopt (merged here by you)
    /// or pr (pull requests on GitHub, opened with the realm's GITHUB_TOKEN)
    Delivery {
        #[arg(value_parser = ["adopt", "pr"])]
        mode: String,
    },
    /// Whether this realm's runs see your owner profile
    ShareProfile {
        #[arg(value_parser = ["on", "off"])]
        value: String,
    },
    /// Describe the realm (the agent sees this)
    Describe { description: String },
}

#[derive(Subcommand)]
pub enum OwnerCmd {
    /// Show the profile
    Show,
    /// Set your name or the "about" text
    Set {
        #[arg(value_parser = ["name", "about"])]
        field: String,
        value: String,
    },
    /// Add an item to preferences, goals, commitments or boundaries
    Add {
        #[arg(value_parser = LIST_FIELDS)]
        field: String,
        item: String,
    },
    /// Remove an item (by its number in `owner show`)
    Rm {
        #[arg(value_parser = LIST_FIELDS)]
        field: String,
        number: usize,
    },
    /// Edit the profile as JSON in $EDITOR
    Edit,
}

#[derive(Subcommand)]
pub enum NorthStarCmd {
    /// Show the realm's North Star, its milestones and progress
    Show,
    /// Set (or reword) the North Star; milestones are kept
    Set {
        goal: String,
        /// Why it matters, or what success looks like
        #[arg(long, default_value = "")]
        why: String,
    },
    /// Forget the North Star and its milestones
    Clear,
    /// Set a milestone's status yourself
    Milestone {
        /// Its id, e.g. m2
        id: String,
        #[arg(value_parser = ["todo", "doing", "done", "dropped"])]
        status: String,
    },
    /// Have Airth advance it on a schedule ("weekdays 09:00", "every 3h"), or "off"
    Daily { every: String },
}

#[derive(Subcommand)]
pub enum MemoryCmd {
    /// List what the realm remembers, with where each memory came from
    List,
    /// Add a memory
    Add { text: String },
    /// Delete a memory by id
    Rm { id: String },
}

#[derive(Subcommand)]
pub enum SecretCmd {
    /// List the realm's credential names (values are never shown)
    List,
    /// Set a credential; the value is read from stdin without echo
    Set {
        key: String,
        /// A model setting (ANTHROPIC_API_KEY, AIRTH_MODEL...) for every realm
        /// without its own; needed by the background service
        #[arg(long)]
        all_realms: bool,
    },
    /// Remove a credential
    Rm {
        key: String,
        #[arg(long)]
        all_realms: bool,
    },
}

#[derive(Subcommand)]
pub enum IntegrationCmd {
    /// List the realm's integrations
    List,
    /// Add (or replace) an integration: requests under BASE_URL get these headers
    #[command(
        after_help = "Example:\n  airth secret set GITHUB_TOKEN\n  airth integration add github https://api.github.com \\\n    --header \"Authorization: Bearer [secret:GITHUB_TOKEN]\" \\\n    --description \"GitHub REST API\"\n\nPut credentials in headers as [secret:NAME], never as plain values."
    )]
    Add {
        name: String,
        base_url: String,
        /// "Name: value", repeatable; [secret:NAME] is filled in from the realm's credentials
        #[arg(long = "header", short = 'H')]
        headers: Vec<String>,
        /// What the service is for (the agent sees this)
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Remove an integration
    Rm { name: String },
}

#[derive(Subcommand)]
pub enum ObjectiveCmd {
    /// List the realm's objectives
    List,
    /// Add (or replace) an objective Airth pursues on its own
    #[command(
        after_help = "Examples:\n  airth objective add triage \"Triage new issues and post a summary\" \\\n    --every \"weekdays 08:30\" --allow network:tracker --allow write --max-minutes 20\n  airth objective add inbox \"File the notes dropped in inbox/\" --watch inbox --allow write\n  airth objective add deploy-check \"Check the deploy that just ran\" --webhook\n\nRuns may do what the realm allows plus --allow grants without asking; anything else\nwaits --wait minutes for you (in the app), then is denied."
    )]
    Add {
        id: String,
        objective: String,
        /// Run on a schedule: "every 30m", "daily 08:30", "weekdays 09:00", "mon,thu 18:00"
        #[arg(long, group = "trigger")]
        every: Option<String>,
        /// Run when files under this realm path change
        #[arg(long, group = "trigger")]
        watch: Option<String>,
        /// Run when something on this machine POSTs to its webhook URL
        #[arg(long, group = "trigger")]
        webhook: bool,
        /// Approve in advance: write, execute, memory, network,
        /// network:<integration or host>, skill:<name>. Repeatable
        #[arg(long = "allow")]
        allow: Vec<String>,
        /// Minutes a run waits for you on anything else, before it is denied
        #[arg(long, default_value_t = 10)]
        wait: u32,
        #[arg(long)]
        max_iterations: Option<u32>,
        #[arg(long)]
        max_tokens: Option<u64>,
        #[arg(long)]
        max_minutes: Option<u32>,
        /// The realm repository its runs work in (default: the realm's root)
        #[arg(long)]
        repo: Option<String>,
    },
    /// Show an objective, with its webhook URL if it has one
    Show {
        id: String,
    },
    /// Start an objective's run now, in this terminal
    Run {
        id: String,
        #[arg(short, long)]
        verbose: bool,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Rm {
        id: String,
    },
}

pub fn objective(ctx: &Ctx, command: ObjectiveCmd) -> Result<bool> {
    use crate::objective::{Objective, Trigger};
    let realm = ctx.realm()?;
    let store = realm.objectives();
    match command {
        ObjectiveCmd::List => {
            let list = store.list()?;
            if list.is_empty() {
                eprintln!(
                    "realm {} has no objectives (airth objective add --help)",
                    realm.name()
                );
            }
            for o in list {
                let next = o
                    .next_at
                    .map(|t| {
                        format!(
                            "next {}",
                            t.with_timezone(&chrono::Local).format("%a %d %b %H:%M")
                        )
                    })
                    .unwrap_or_default();
                println!(
                    "{:<18} {:<8} {:<28} {:<24} last: {}",
                    o.id,
                    if o.enabled { "on" } else { "off" },
                    o.trigger.describe(),
                    next,
                    o.last_status.as_deref().unwrap_or("never run")
                );
            }
        }
        ObjectiveCmd::Add {
            id,
            objective,
            every,
            watch,
            webhook,
            allow,
            wait,
            max_iterations,
            max_tokens,
            max_minutes,
            repo,
        } => {
            if let Some(name) = &repo {
                realm.repo(Some(name))?;
            }
            let trigger = match (every, watch, webhook) {
                (Some(every), _, _) => Trigger::Schedule { every },
                (_, Some(path), _) => Trigger::Watch { path },
                (_, _, true) => Trigger::Webhook {
                    token: crate::ui::random_token()?,
                },
                _ => Trigger::Manual,
            };
            let approvals = allow
                .iter()
                .map(|a| parse_grant(&realm, a))
                .collect::<Result<Vec<_>>>()?;
            store.save(Objective {
                id: id.clone(),
                objective,
                trigger,
                approvals,
                budget: crate::agent::Budget {
                    max_iterations,
                    max_tokens,
                    max_minutes,
                },
                approval_wait_minutes: wait,
                enabled: true,
                created_at: chrono::Utc::now(),
                repo,
                project: None,
                next_at: None,
                fingerprint: None,
                last_run: None,
                last_at: None,
                last_status: None,
            })?;
            eprintln!(
                "\x1b[32m✓\x1b[0m objective {id} saved in realm {}",
                realm.name()
            );
            show_objective(ctx, &realm, &id)?;
            if crate::ui::running_url(&ctx.hub()).is_none() {
                eprintln!(
                    "\x1b[33m!\x1b[0m Airth is not running in the background, so it will not start on its own: run `airth serve`, or `airth service install`"
                );
            }
        }
        ObjectiveCmd::Show { id } => show_objective(ctx, &realm, &id)?,
        ObjectiveCmd::Run { id, verbose } => {
            return crate::run_objective(ctx, &realm, &id, verbose);
        }
        ObjectiveCmd::Enable { id } | ObjectiveCmd::Disable { id }
            if !store.list()?.iter().any(|o| o.id == id) =>
        {
            bail!("no objective '{id}'")
        }
        ObjectiveCmd::Enable { id } => {
            store.update(&id, |o| {
                o.enabled = true;
                o.next_at = None;
                o.fingerprint = None;
            })?;
            eprintln!("enabled {id}");
        }
        ObjectiveCmd::Disable { id } => {
            store.update(&id, |o| o.enabled = false)?;
            eprintln!("disabled {id}");
        }
        ObjectiveCmd::Rm { id } => {
            if !store.remove(&id)? {
                bail!("no objective '{id}'");
            }
            eprintln!("removed {id}");
        }
    }
    Ok(true)
}

fn show_objective(ctx: &Ctx, realm: &crate::realm::Realm, id: &str) -> Result<()> {
    use crate::objective::Trigger;
    let o = realm.objectives().get(id)?;
    println!("objective    {}", o.id);
    println!("goal         {}", o.objective);
    println!("trigger      {}", o.trigger.describe());
    if let Trigger::Webhook { token } = &o.trigger {
        let port = crate::ui::running_url(&ctx.hub())
            .and_then(|u| {
                u.split(':')
                    .nth(2)
                    .and_then(|p| p.split('/').next())
                    .map(String::from)
            })
            .unwrap_or_else(|| "<port>".into());
        println!(
            "webhook      curl -X POST --data @payload.json http://127.0.0.1:{port}/hooks/{}/{}/{token}",
            realm.name(),
            o.id
        );
    }
    let approvals: Vec<String> = o
        .approvals
        .iter()
        .map(|g| match &g.scope {
            Some(s) => format!("{} ({s})", g.capability.as_str()),
            None => g.capability.as_str().to_string(),
        })
        .collect();
    println!(
        "approvals    {}",
        if approvals.is_empty() {
            "(none: only what the realm allows)".to_string()
        } else {
            approvals.join(", ")
        }
    );
    println!(
        "waits        {} minutes for you, then denies",
        o.approval_wait_minutes
    );
    println!("budget       {}", o.budget.describe());
    println!("enabled      {}", if o.enabled { "yes" } else { "no" });
    if let Some(at) = o.last_at {
        println!(
            "last run     {} · {} · {}",
            o.last_run.as_deref().unwrap_or("?"),
            at.with_timezone(&chrono::Local).format("%a %d %b %H:%M"),
            o.last_status.as_deref().unwrap_or("?")
        );
    }
    Ok(())
}

/// `write`, `network`, `network:github` (an integration) or `network:example.com` (a host).
fn parse_grant(realm: &crate::realm::Realm, text: &str) -> Result<crate::permissions::Grant> {
    let (cap, scope) = match text.split_once(':') {
        Some((c, s)) => (c, Some(s.trim())),
        None => (text, None),
    };
    let capability = Capability::parse(cap.trim())?;
    let scope = match scope {
        None => None,
        Some(s) if capability == Capability::Skill => Some(s.to_string()),
        Some(s) if capability != Capability::Network => {
            bail!("only network and skill approvals take a scope ('{s}')")
        }
        Some(s) if realm.config.integrations.iter().any(|i| i.name == s) => {
            Some(format!("integration {s}"))
        }
        Some(s) => Some(s.to_string()),
    };
    Ok(crate::permissions::Grant { capability, scope })
}

#[derive(clap::Args)]
pub struct InboxArgs {
    /// Show read notifications too
    #[arg(long)]
    all: bool,
    /// Whether the Mac app bounces its Dock icon when a run waits for you
    #[arg(long, value_parser = ["on", "off"])]
    desktop: Option<String>,
}

pub fn inbox(ctx: &Ctx, args: InboxArgs) -> Result<bool> {
    let home = ctx.home();
    if let Some(d) = args.desktop {
        home.set_desktop_notifications(d == "on")?;
        eprintln!("Dock alerts {d}");
        return Ok(true);
    }
    let inbox = home.inbox();
    let list: Vec<_> = inbox
        .list()?
        .into_iter()
        .filter(|n| args.all || !n.read)
        .collect();
    if list.is_empty() {
        eprintln!("no new notifications");
    }
    for n in &list {
        println!(
            "\x1b[1m{}\x1b[0m \x1b[2m{} · {}{}\x1b[0m",
            n.title,
            n.realm,
            n.created_at
                .with_timezone(&chrono::Local)
                .format("%a %d %b %H:%M"),
            n.run
                .as_deref()
                .map(|r| format!(" · run {r}"))
                .unwrap_or_default()
        );
        for line in n.body.lines().take(6) {
            println!("  {line}");
        }
    }
    inbox.mark_read(None)?;
    Ok(true)
}

pub fn summary(ctx: &Ctx, hours: i64) -> Result<bool> {
    let since = chrono::Utc::now() - chrono::Duration::hours(hours);
    let realms = crate::scheduler::summary(&ctx.hub(), since)?;
    if realms.is_empty() {
        eprintln!("nothing happened in the last {hours} hours");
    }
    for r in realms {
        println!(
            "\x1b[1m{}\x1b[0m  {} run(s): {} completed, {} failed, {} stopped",
            r.realm, r.runs, r.completed, r.failed, r.stopped
        );
        for l in &r.recent {
            let by = l
                .origin
                .as_deref()
                .map(|o| format!(" [{o}]"))
                .unwrap_or_default();
            println!("  ✓ {}{by}: {}", l.objective, l.report);
        }
        for l in &r.to_review {
            println!("  ⧗ to review: {} (airth review {})", l.objective, l.id);
        }
        for l in &r.unfinished {
            println!("  ↻ {}: {} (airth resume {})", l.status, l.objective, l.id);
        }
        for o in r.objectives.iter().filter(|o| o.enabled) {
            let next = o
                .next_at
                .map(|t| {
                    format!(
                        ", next {}",
                        t.with_timezone(&chrono::Local).format("%a %H:%M")
                    )
                })
                .unwrap_or_default();
            println!("  ◷ {} ({}{next})", o.id, o.trigger);
        }
    }
    Ok(true)
}

#[derive(Subcommand)]
pub enum SkillCmd {
    /// The realm's skills: proposed, in development, ready, installed
    List,
    /// A skill's details, evaluation and files (--source prints them)
    Show {
        name: String,
        #[arg(long)]
        source: bool,
    },
    /// Have Airth develop (or rework) a skill, in this terminal
    Develop {
        name: String,
        /// What it should do, for a skill nobody proposed yet
        #[arg(long)]
        description: Option<String>,
        /// What to change, for a skill already developed (e.g. a failing test)
        #[arg(long)]
        feedback: Option<String>,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run a skill's evaluation again
    Evaluate { name: String },
    /// Enable a skill for the realm's runs (after reviewing it)
    Install {
        name: String,
        /// Don't ask for confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Disable an installed skill (its draft is kept)
    Uninstall { name: String },
    /// Decline a proposed or developed skill
    Reject { name: String },
    /// Delete a skill: record, draft and installed copy
    Rm { name: String },
}

pub fn skill(ctx: &Ctx, command: SkillCmd) -> Result<bool> {
    use crate::skill::SkillStatus;
    let realm = ctx.realm()?;
    let store = realm.skills();
    match command {
        SkillCmd::List => {
            let list = store.list()?;
            if list.is_empty() {
                eprintln!(
                    "realm {} has no skills yet (runs propose them; or: airth skill develop <name> --description ...)",
                    realm.name()
                );
            }
            for r in list {
                let eval = r
                    .evaluation
                    .as_ref()
                    .map(|e| e.describe())
                    .unwrap_or_default();
                println!(
                    "{:<20} {:<10} {:<22} {}",
                    r.name,
                    r.status.as_str(),
                    eval,
                    r.description.lines().next().unwrap_or("")
                );
            }
        }
        SkillCmd::Show { name, source } => show_skill(&realm, &name, source)?,
        SkillCmd::Develop {
            name,
            description,
            feedback,
            verbose,
        } => {
            return crate::develop_skill(
                ctx,
                &realm,
                &name,
                description.as_deref(),
                feedback.as_deref(),
                verbose,
            );
        }
        SkillCmd::Evaluate { name } => {
            let r = ctx.hub().evaluate_skill(&realm, &name)?;
            print!("{}", r.evaluation.map(|e| e.report()).unwrap_or_default());
        }
        SkillCmd::Install { name, yes } => {
            store.get(&name)?;
            if !yes {
                show_skill(&realm, &name, false)?;
                eprint!(
                    "\nInstall {name}? Runs in realm {} will be able to use it (under the realm's skill rule: {}). Review its files first with --source. [y/N] ",
                    realm.name(),
                    realm.config.policy.skill.as_str()
                );
                std::io::stderr().flush()?;
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line)?;
                if !matches!(line.trim(), "y" | "yes") {
                    eprintln!("not installed");
                    return Ok(false);
                }
            }
            store.install(&name)?;
            eprintln!(
                "\x1b[32m✓\x1b[0m installed {name}: runs in realm {} can use it",
                realm.name()
            );
        }
        SkillCmd::Uninstall { name } => {
            store.uninstall(&name)?;
            eprintln!("uninstalled {name} (its draft is kept)");
        }
        SkillCmd::Reject { name } => {
            store.update(&name, |r| r.status = SkillStatus::Rejected)?;
            eprintln!("rejected {name}");
        }
        SkillCmd::Rm { name } => {
            if !store.remove(&name)? {
                bail!("no skill '{name}'");
            }
            eprintln!("removed {name}");
        }
    }
    Ok(true)
}

fn show_skill(realm: &crate::realm::Realm, name: &str, source: bool) -> Result<()> {
    let store = realm.skills();
    let r = store.get(name)?;
    println!("skill        {}", r.name);
    println!("status       {}", r.status.as_str());
    println!("does         {}", r.description);
    if !r.reason.is_empty() {
        println!("why          {}", r.reason);
    }
    println!("proposed by  {}", r.proposed_by.replace("run:", "run "));
    if let Some(run) = &r.dev_run {
        println!("developed in run {run}");
    }
    if let Some(i) = &r.installed {
        println!(
            "installed    {}",
            i.at.with_timezone(&chrono::Local).format("%a %d %b %H:%M")
        );
    }
    if let Some(e) = &r.evaluation {
        print!("{}", e.report());
    }
    let draft = store.draft_dir(name);
    if let Ok(files) = crate::skill::list_files(&draft) {
        println!(
            "files        {}",
            files
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        if source {
            for f in files {
                println!("\n\x1b[1m── {} ──\x1b[0m", f.display());
                match std::fs::read_to_string(draft.join(&f)) {
                    Ok(text) => println!("{text}"),
                    Err(_) => println!("(binary)"),
                }
            }
        }
    }
    Ok(())
}

pub fn integration(ctx: &Ctx, command: IntegrationCmd) -> Result<bool> {
    let mut realm = ctx.realm()?;
    match command {
        IntegrationCmd::List => {
            if realm.config.integrations.is_empty() {
                eprintln!(
                    "realm {} has no integrations (airth integration add ...)",
                    realm.name()
                );
            }
            for i in &realm.config.integrations {
                println!("{:<16} {}", i.name, i.base_url);
                if !i.description.is_empty() {
                    println!("{:<16} {}", "", i.description);
                }
                for (k, v) in &i.headers {
                    println!("{:<16} {k}: {v}", "");
                }
            }
        }
        IntegrationCmd::Add {
            name,
            base_url,
            headers,
            description,
        } => {
            let mut map = std::collections::BTreeMap::new();
            for h in headers {
                let (k, v) = h
                    .split_once(':')
                    .with_context(|| format!("headers look like \"Name: value\", not '{h}'"))?;
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
            let integration = crate::integration::Integration {
                name,
                description,
                base_url,
                headers: map,
            };
            let missing: Vec<String> = integration
                .secret_names()
                .into_iter()
                .filter(|k| realm.secret(k).is_none())
                .collect();
            let name = integration.name.clone();
            realm.set_integration(integration)?;
            eprintln!(
                "\x1b[32m✓\x1b[0m integration {name} saved in realm {}",
                realm.name()
            );
            for k in missing {
                eprintln!(
                    "\x1b[33m!\x1b[0m it uses {k}, which is not set yet: airth secret set {k}"
                );
            }
        }
        IntegrationCmd::Rm { name } => {
            if !realm.remove_integration(&name)? {
                bail!("no integration '{name}' in realm {}", realm.name());
            }
            eprintln!("removed {name}");
        }
    }
    Ok(true)
}

pub fn realm(ctx: &Ctx, command: RealmCmd) -> Result<bool> {
    let home = ctx.home();
    match command {
        RealmCmd::List => {
            let default = home.default_realm();
            let current = ctx.realm().ok().map(|r| r.name().to_string());
            let realms = home.realms()?;
            if realms.is_empty() {
                eprintln!("no realms yet: airth realm create <name> [--root <dir>]");
            }
            for r in realms {
                let mut tags = Vec::new();
                if current.as_deref() == Some(r.name()) {
                    tags.push("current");
                }
                if default.as_deref() == Some(r.name()) {
                    tags.push("default");
                }
                if r.config.archived {
                    tags.push("archived");
                }
                let root = r
                    .config
                    .root
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(private files)".into());
                println!(
                    "{} {:<16} {:<40} {}",
                    if current.as_deref() == Some(r.name()) {
                        "*"
                    } else {
                        " "
                    },
                    r.name(),
                    root,
                    tags.join(", ")
                );
            }
        }
        RealmCmd::Create {
            name,
            root,
            description,
            isolation,
        } => {
            let isolation = match isolation.as_deref() {
                None => None,
                Some("none") => Some(Isolation::None),
                Some("worktree") => Some(Isolation::Worktree),
                Some(other) => bail!("unknown isolation '{other}' (none, worktree)"),
            };
            let realm = home.create_realm(NewRealm {
                name,
                description,
                root,
                isolation,
            })?;
            eprintln!("\x1b[32m✓\x1b[0m created realm {}", realm.name());
            if let Some(root) = &realm.config.root {
                let imported = ctx.hub().import_runs(&realm, root)?;
                if imported > 0 {
                    eprintln!(
                        "  imported {imported} run(s) from {}/.airth/runs",
                        root.display()
                    );
                }
            }
            print_realm(ctx, &realm)?;
        }
        RealmCmd::Show { name } => {
            let realm = match name {
                Some(n) => home.realm(&n)?,
                None => ctx.realm()?,
            };
            print_realm(ctx, &realm)?;
        }
        RealmCmd::Use { name } => {
            home.set_default_realm(&name)?;
            eprintln!("default realm: {name}");
        }
        RealmCmd::Archive { name, undo } => {
            let mut realm = home.realm(&name)?;
            realm.config.archived = !undo;
            realm.save()?;
            eprintln!(
                "realm {name} {}",
                if undo { "restored" } else { "archived" }
            );
        }
        RealmCmd::Policy { capability, rule } => {
            let mut realm = ctx.realm()?;
            let capability = Capability::parse(&capability)?;
            let rule = Rule::parse(&rule)?;
            realm.config.policy.set(capability, rule);
            realm.save()?;
            eprintln!(
                "realm {}: {} → {}",
                realm.name(),
                capability.as_str(),
                rule.as_str()
            );
        }
        RealmCmd::Checks { commands } => {
            let mut realm = ctx.realm()?;
            realm.config.checks = if commands == ["none"] {
                Vec::new()
            } else {
                commands
            };
            realm.save()?;
            print_realm(ctx, &realm)?;
        }
        RealmCmd::Repo(cmd) => {
            let mut realm = ctx.realm()?;
            match cmd {
                RepoCmd::List => {}
                RepoCmd::Add { source, name, path } => {
                    let repo = ctx.hub().connect_repo(
                        &mut realm,
                        &source,
                        name.as_deref(),
                        path.as_deref(),
                    )?;
                    eprintln!(
                        "\x1b[32m✓\x1b[0m connected {} ({})",
                        repo.name,
                        repo.path.display()
                    );
                }
                RepoCmd::Remove { name } => realm.remove_repo(&name)?,
                RepoCmd::Checks { name, commands } => {
                    let commands = if commands == ["none"] {
                        Vec::new()
                    } else {
                        commands
                    };
                    realm.update_repo(&name, Some(commands), None)?;
                }
                RepoCmd::Delivery { name, mode } => {
                    realm.update_repo(
                        &name,
                        None,
                        Some(crate::isolation::Delivery::parse(&mode)?),
                    )?;
                }
            }
            for r in realm.repos() {
                println!(
                    "{:<16} {:<13} {}{}",
                    r.name,
                    r.delivery.as_str(),
                    r.path.display(),
                    if r.main { "  (the realm's folder)" } else { "" }
                );
                if !r.checks.is_empty() {
                    println!("{:<16} checks: {}", "", r.checks.join(" · "));
                }
            }
        }
        RealmCmd::Delivery { mode } => {
            let mut realm = ctx.realm()?;
            let delivery = crate::isolation::Delivery::parse(&mode)?;
            if delivery == crate::isolation::Delivery::PullRequest {
                if realm.config.isolation != Isolation::Worktree {
                    bail!("pull requests need a realm whose chats work in their own worktree");
                }
                crate::github::Remote::of(&realm.root()?)?;
                if realm.secret(crate::github::TOKEN).is_none() {
                    eprintln!(
                        "\x1b[33mnote:\x1b[0m add the token: airth secret set {} --realm {}",
                        crate::github::TOKEN,
                        realm.name()
                    );
                }
            }
            realm.config.delivery = delivery;
            realm.save()?;
            print_realm(ctx, &realm)?;
        }
        RealmCmd::ShareProfile { value } => {
            let mut realm = ctx.realm()?;
            realm.config.share_owner_profile = value == "on";
            realm.save()?;
            eprintln!("realm {}: owner profile {value}", realm.name());
        }
        RealmCmd::Describe { description } => {
            let mut realm = ctx.realm()?;
            realm.config.description = description.trim().to_string();
            realm.save()?;
        }
    }
    Ok(true)
}

fn print_realm(ctx: &Ctx, realm: &crate::realm::Realm) -> Result<()> {
    let c = &realm.config;
    println!("realm        {}", c.name);
    if !c.description.is_empty() {
        println!("description  {}", c.description);
    }
    println!("root         {}", realm.root()?.display());
    println!("isolation    {}", c.isolation.as_str());
    if c.isolation == Isolation::Worktree {
        println!("delivery     {}", c.delivery.as_str());
    }
    let policy: Vec<String> = Capability::ALL
        .iter()
        .map(|cap| format!("{} {}", cap.as_str(), c.policy.rule(*cap).as_str()))
        .collect();
    println!("policy       {}", policy.join(" · "));
    println!(
        "checks       {}",
        if c.checks.is_empty() {
            "(none)".to_string()
        } else {
            c.checks.join(" · ")
        }
    );
    println!(
        "profile      {}",
        if c.share_owner_profile {
            "shared"
        } else {
            "not shared"
        }
    );
    println!("memories     {}", realm.memory().list()?.len());
    let secrets: Vec<String> = realm.secrets()?.into_iter().map(|(k, _)| k).collect();
    println!(
        "credentials  {}",
        if secrets.is_empty() {
            "(none)".to_string()
        } else {
            secrets.join(", ")
        }
    );
    let integrations: Vec<&str> = c.integrations.iter().map(|i| i.name.as_str()).collect();
    println!(
        "integrations {}",
        if integrations.is_empty() {
            "(none)".to_string()
        } else {
            integrations.join(", ")
        }
    );
    println!("runs         {}", ctx.hub().runs(realm)?.len());
    if c.archived {
        println!("(archived)");
    }
    Ok(())
}

pub fn owner(ctx: &Ctx, command: OwnerCmd) -> Result<bool> {
    let home = ctx.home();
    let mut owner = home.owner()?;
    match command {
        OwnerCmd::Show => {
            if owner.is_empty() {
                eprintln!(
                    "The profile is empty. Start with:\n  airth owner set name \"Your Name\"\n  airth owner add goals \"...\""
                );
                return Ok(true);
            }
            println!("name         {}", owner.name);
            println!("about        {}", owner.about);
            for field in LIST_FIELDS {
                let items = owner.list_mut(field).expect("known field");
                println!("{field}");
                for (i, item) in items.iter().enumerate() {
                    println!("  {}. {item}", i + 1);
                }
            }
            return Ok(true);
        }
        OwnerCmd::Set { field, value } => match field.as_str() {
            "name" => owner.name = value.trim().to_string(),
            _ => owner.about = value.trim().to_string(),
        },
        OwnerCmd::Add { field, item } => {
            owner
                .list_mut(&field)
                .expect("validated by clap")
                .push(item.trim().to_string());
        }
        OwnerCmd::Rm { field, number } => {
            let items = owner.list_mut(&field).expect("validated by clap");
            if number == 0 || number > items.len() {
                bail!("{field} has {} item(s)", items.len());
            }
            items.remove(number - 1);
        }
        OwnerCmd::Edit => {
            let path =
                std::env::temp_dir().join(format!("airth-owner-{}.json", std::process::id()));
            std::fs::write(&path, serde_json::to_string_pretty(&owner)?)?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$0\""))
                .arg(&path)
                .status()
                .context("cannot start $EDITOR")?;
            let text = std::fs::read_to_string(&path)?;
            let _ = std::fs::remove_file(&path);
            if !status.success() {
                bail!("editor exited with {status}; profile unchanged");
            }
            owner = serde_json::from_str(&text).context("invalid profile; unchanged")?;
        }
    }
    home.save_owner(&owner)?;
    eprintln!("\x1b[32m✓\x1b[0m profile saved");
    Ok(true)
}

pub fn north_star(ctx: &Ctx, command: NorthStarCmd) -> Result<bool> {
    let realm = ctx.realm()?;
    let store = realm.north_star();
    match command {
        NorthStarCmd::Show => match store.get()? {
            None => eprintln!(
                "realm {} has no North Star. Set one: airth northstar set \"...\"",
                realm.name()
            ),
            Some(star) => {
                let (done, total) = star.progress();
                println!("\x1b[1m★ {}\x1b[0m", star.goal);
                if !star.why.is_empty() {
                    println!("  \x1b[2m{}\x1b[0m", star.why);
                }
                if star.milestones.is_empty() {
                    println!("no milestones yet: brainstorm them in the app (airth ui)");
                } else {
                    println!(
                        "{done}/{total} milestones done\n{}",
                        star.render_milestones()
                    );
                }
                if let Ok(o) = realm.objectives().get(crate::northstar::OBJECTIVE_ID) {
                    println!("\x1b[2mAirth advances it: {}\x1b[0m", o.trigger.describe());
                }
            }
        },
        NorthStarCmd::Set { goal, why } => {
            store.set_goal(&goal, &why)?;
            eprintln!("North Star set for realm {}", realm.name());
        }
        NorthStarCmd::Clear => {
            if !store.clear()? {
                bail!("realm {} has no North Star", realm.name());
            }
            ctx.hub().schedule_north_star(&realm, None)?;
            eprintln!("North Star cleared");
        }
        NorthStarCmd::Milestone { id, status } => {
            let star = store.update(
                &id,
                Some(crate::northstar::Status::parse(&status)?),
                None,
                None,
            )?;
            let (done, total) = star.progress();
            eprintln!("{id} is {status} ({done}/{total} done)");
        }
        NorthStarCmd::Daily { every } => {
            let off = every.trim() == "off";
            ctx.hub()
                .schedule_north_star(&realm, (!off).then_some(every.as_str()))?;
            eprintln!(
                "{}",
                if off {
                    "Airth no longer advances the North Star on its own".to_string()
                } else {
                    format!(
                        "Airth advances the North Star: {every} (needs `airth service install` or `airth ui` running)"
                    )
                }
            );
        }
    }
    Ok(true)
}

pub fn memory(ctx: &Ctx, command: MemoryCmd) -> Result<bool> {
    let realm = ctx.realm()?;
    let store = realm.memory();
    match command {
        MemoryCmd::List => {
            let entries = store.list()?;
            if entries.is_empty() {
                eprintln!("realm {} remembers nothing yet", realm.name());
            }
            for e in entries {
                println!(
                    "{:<5} {}  \x1b[2m({}, {})\x1b[0m",
                    e.id,
                    e.text,
                    e.source,
                    e.created_at.format("%Y-%m-%d")
                );
            }
        }
        MemoryCmd::Add { text } => {
            let e = store.add(&text, "owner")?;
            eprintln!("remembered ({}) in realm {}", e.id, realm.name());
        }
        MemoryCmd::Rm { id } => {
            if !store.remove(&id)? {
                bail!("no memory '{id}' in realm {}", realm.name());
            }
            eprintln!("forgot {id}");
        }
    }
    Ok(true)
}

pub fn secret(ctx: &Ctx, command: SecretCmd) -> Result<bool> {
    match command {
        SecretCmd::Set {
            key,
            all_realms: true,
        } => {
            let value = read_secret(&format!("{key} for all realms: "))?;
            ctx.home().set_provider_setting(&key, &value)?;
            eprintln!("\x1b[32m✓\x1b[0m saved {key} for every realm without its own");
            return Ok(true);
        }
        SecretCmd::Rm {
            key,
            all_realms: true,
        } => {
            if !ctx.home().remove_provider_setting(&key)? {
                bail!("no setting '{key}' for all realms");
            }
            eprintln!("removed {key}");
            return Ok(true);
        }
        _ => {}
    }
    let realm = ctx.realm()?;
    match command {
        SecretCmd::List => {
            let secrets = realm.secrets()?;
            if secrets.is_empty() {
                eprintln!("realm {} has no credentials", realm.name());
            }
            for (k, _) in secrets {
                println!("{k}");
            }
            for (k, _) in ctx.home().provider_settings()? {
                println!("{k} \x1b[2m(all realms)\x1b[0m");
            }
        }
        SecretCmd::Set { key, .. } => {
            let value = read_secret(&format!("{key} for realm {}: ", realm.name()))?;
            realm.set_secret(&key, &value)?;
            eprintln!(
                "\x1b[32m✓\x1b[0m saved {key} (only runs in realm {} can use it)",
                realm.name()
            );
        }
        SecretCmd::Rm { key, .. } => {
            if !realm.remove_secret(&key)? {
                bail!("no credential '{key}' in realm {}", realm.name());
            }
            eprintln!("removed {key}");
        }
    }
    Ok(true)
}

/// Read a line from stdin, without echo when it is a terminal.
fn read_secret(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let _echo = EchoOff::new();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    eprintln!();
    let value = line.trim_end_matches(['\n', '\r']).to_string();
    if value.is_empty() {
        bail!("empty value; nothing saved");
    }
    Ok(value)
}

/// Turns terminal echo off until dropped.
struct EchoOff(#[cfg(unix)] Option<libc::termios>);

impl EchoOff {
    fn new() -> Self {
        #[cfg(unix)]
        {
            // SAFETY: tcgetattr/tcsetattr on stdin with a zeroed, then filled, termios.
            unsafe {
                if libc::isatty(0) == 1 {
                    let mut t: libc::termios = std::mem::zeroed();
                    if libc::tcgetattr(0, &mut t) == 0 {
                        let saved = t;
                        t.c_lflag &= !libc::ECHO;
                        libc::tcsetattr(0, libc::TCSANOW, &t);
                        return Self(Some(saved));
                    }
                }
            }
            Self(None)
        }
        #[cfg(not(unix))]
        Self()
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(saved) = &self.0 {
            // SAFETY: restoring the attributes read in `new`.
            unsafe {
                libc::tcsetattr(0, libc::TCSANOW, saved);
            }
        }
    }
}

#[derive(Subcommand)]
pub enum BenchCmd {
    /// Turn Airth Bench on (also in the app: Settings → Advanced)
    Enable,
    /// Turn Airth Bench off; results are kept
    Disable,
    /// List the suites and their tasks, and which can run on this machine
    List,
    /// Run a suite: each task in its own isolated workspace and realm
    Run {
        /// The suite (see `airth bench list`)
        #[arg(default_value = "core")]
        suite: String,
        /// Only these tasks (repeatable)
        #[arg(long = "task")]
        tasks: Vec<String>,
        /// Run each task this many times, to see how stable it is
        #[arg(long, default_value_t = 1)]
        repeat: u32,
        /// Use this model instead of the configured one: provider/model
        #[arg(long)]
        model: Option<String>,
        /// Stop starting tasks once the run has used this many tokens
        #[arg(long)]
        max_tokens: Option<u64>,
        /// Keep each task's workspace afterwards, to inspect it
        #[arg(long)]
        keep: bool,
        /// Show every model response and tool call
        #[arg(short, long)]
        verbose: bool,
    },
    /// List past benchmark runs
    Results,
    /// Show a run's results, or one task's trace
    Show {
        /// Run id, or "last"
        #[arg(default_value = "last")]
        run: String,
        /// Show this task's execution trace
        #[arg(long)]
        task: Option<String>,
        #[arg(long, default_value_t = 1)]
        attempt: u32,
    },
    /// Compare two runs (or a run with the baseline)
    Compare {
        /// The baseline run (or the candidate, when it is the only one)
        a: String,
        /// The candidate run
        b: Option<String>,
    },
    /// Set (or clear) the run others are compared with
    Baseline {
        run: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    /// Print a run's results as JSON or CSV
    Export {
        #[arg(default_value = "last")]
        run: String,
        #[arg(long, default_value = "json", value_parser = ["json", "csv"])]
        format: String,
    },
}

/// Prints each task's outcome; the run's own output only with `--verbose`.
struct BenchObserver(Option<crate::term::TerminalObserver>);

impl crate::agent::Observer for BenchObserver {
    fn response(&mut self, iteration: u32, response: &crate::models::ModelResponse) {
        if let Some(t) = &mut self.0 {
            t.response(iteration, response);
        }
    }
    fn tool_started(&mut self, name: &str, input: &serde_json::Value) {
        if let Some(t) = &mut self.0 {
            t.tool_started(name, input);
        }
    }
    fn tool_finished(&mut self, event: &crate::state::Event) {
        if let Some(t) = &mut self.0 {
            t.tool_finished(event);
        }
    }
}

fn pct(x: Option<f64>) -> String {
    x.map_or("—".into(), |x| format!("{:.0}%", x * 100.0))
}

pub fn bench(ctx: &Ctx, command: BenchCmd) -> Result<bool> {
    use crate::bench::{Bench, metrics::Summary, storage::RunOptions};
    let bench = Bench::new(ctx.home());
    match command {
        BenchCmd::Enable => {
            bench.set_enabled(true)?;
            eprintln!("Airth Bench is on: `airth bench run` runs the core suite");
        }
        BenchCmd::Disable => {
            bench.set_enabled(false)?;
            eprintln!("Airth Bench is off; past results are kept");
        }
        BenchCmd::List => {
            for suite in bench.suites() {
                println!(
                    "\x1b[1m{}\x1b[0m {}{}  {}",
                    suite.name,
                    suite.version,
                    if suite.builtin { " (built in)" } else { "" },
                    suite.description
                );
                for task in &suite.tasks {
                    let why = crate::bench::runner::unavailable(task)
                        .map(|w| format!("  \x1b[33mskipped here: {w}\x1b[0m"))
                        .unwrap_or_default();
                    println!(
                        "  {:<14} {:<9} {}{why}",
                        task.id,
                        task.category.as_str(),
                        task.title
                    );
                }
            }
            if !bench.enabled() {
                eprintln!("\x1b[2mAirth Bench is off: `airth bench enable` to run suites\x1b[0m");
            }
        }
        BenchCmd::Run {
            suite,
            tasks,
            repeat,
            model,
            max_tokens,
            keep,
            verbose,
        } => {
            let hub = ctx.hub();
            let options = RunOptions {
                tasks,
                repeat,
                model,
                max_tokens,
                keep,
            };
            let (mut run, suite) = bench.prepare(&hub, &suite, options)?;
            eprintln!(
                "\x1b[1mairth bench\x1b[0m {} {} · {} · {} task attempt(s) · run {}",
                suite.name, suite.version, run.environment.model, run.planned, run.id
            );
            if !run.environment.sandbox {
                eprintln!(
                    "\x1b[33mno OS sandbox on this platform: tasks that run commands are skipped\x1b[0m"
                );
            }
            let mut observer =
                BenchObserver(verbose.then(|| crate::term::TerminalObserver::new(false)));
            let mut printed = 0;
            let mut progress = |run: &crate::bench::storage::BenchRun| {
                for r in &run.results[printed..] {
                    let cost = r
                        .metrics
                        .estimated_cost_usd
                        .filter(|_| r.executed())
                        .map(|c| format!(" · ~${c:.4}"))
                        .unwrap_or_default();
                    let security = r
                        .security
                        .map(|s| {
                            format!(
                                " [{}]",
                                serde_json::to_value(s)
                                    .unwrap()
                                    .as_str()
                                    .unwrap_or_default()
                            )
                        })
                        .unwrap_or_default();
                    eprintln!(
                        "{:<8} {:<14} #{} {}{}{}{}",
                        r.outcome.colored(),
                        r.task,
                        r.attempt,
                        if r.executed() {
                            format!("{:.1}s", r.metrics.duration_ms as f64 / 1000.0)
                        } else {
                            String::new()
                        },
                        cost,
                        security,
                        if r.reason.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "  \x1b[2m{}\x1b[0m",
                                r.reason.lines().next().unwrap_or_default()
                            )
                        }
                    );
                }
                printed = run.results.len();
                if let Some(current) = &run.current {
                    eprintln!("\x1b[2m… {current}\x1b[0m");
                }
            };
            bench.execute(
                &hub,
                &mut run,
                &suite,
                crate::cancel_on_ctrl_c()?,
                &mut observer,
                &mut progress,
            )?;
            let s = Summary::of(&run);
            eprintln!("\n{}", crate::bench::one_line(&run));
            eprintln!(
                "recovery {} · est. cost per completed task {} · {} in / {} out tokens",
                pct(s.recovery_rate),
                s.avg_cost_usd
                    .map_or("unknown".into(), |c| format!("${c:.4}")),
                s.tokens.input,
                s.tokens.output
            );
            eprintln!(
                "\x1b[2mreport: {} · details: airth bench show {} · compare: airth bench compare {}\x1b[0m",
                bench.store().report_path(&run.id).display(),
                run.id,
                run.id
            );
            return Ok(!s.security_violation && s.passed == s.executed);
        }
        BenchCmd::Results => {
            let runs = bench.store().list()?;
            if runs.is_empty() {
                eprintln!("no benchmark run yet (see `airth bench run`)");
            }
            let baseline = bench.store().baseline();
            for run in runs {
                let s = Summary::of(&run);
                println!(
                    "{:<29} {:<8} {:<11} {:>5} {:>3}/{:<3} {:<28} {}{}",
                    run.id,
                    run.suite,
                    run.display_status(),
                    pct(s.success_rate),
                    s.passed,
                    s.executed,
                    run.environment.model,
                    run.started_at
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M"),
                    if s.security_violation {
                        "  \x1b[31msecurity violation\x1b[0m"
                    } else if baseline.as_deref() == Some(run.id.as_str()) {
                        "  (baseline)"
                    } else {
                        ""
                    }
                );
            }
        }
        BenchCmd::Show { run, task, attempt } => {
            let run = bench.store().find(&run)?;
            if let Some(task) = task {
                let trace = bench.store().trace(&run.id, &task, attempt)?;
                let r = &trace["result"];
                println!(
                    "\x1b[1m{task}\x1b[0m #{attempt}: {}  {}",
                    r["outcome"].as_str().unwrap_or("?"),
                    r["reason"].as_str().unwrap_or("")
                );
                println!("objective: {}", trace["objective"].as_str().unwrap_or(""));
                for c in r["checks"].as_array().into_iter().flatten() {
                    println!(
                        "  {} {} {}",
                        if c["passed"].as_bool() == Some(true) {
                            "\x1b[32m✓\x1b[0m"
                        } else {
                            "\x1b[31m✗\x1b[0m"
                        },
                        c["check"].as_str().unwrap_or(""),
                        c["detail"].as_str().unwrap_or("")
                    );
                }
                println!("\ntrace:");
                for e in trace["run"]["events"].as_array().into_iter().flatten() {
                    let result = e["result"].as_str().unwrap_or("");
                    println!(
                        "  {:<7} {} {}\n          \x1b[2m{}\x1b[0m",
                        e["status"].as_str().unwrap_or(""),
                        e["action"].as_str().unwrap_or(""),
                        crate::agent::summarize_args(
                            e["action"].as_str().unwrap_or(""),
                            &e["arguments"]
                        ),
                        result
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(160)
                            .collect::<String>()
                    );
                }
                if let Some(report) = trace["run"]["report"].as_str() {
                    println!(
                        "\nAirth's final reply (its claim, not the verified outcome):\n{report}"
                    );
                }
                for f in r["files"].as_array().into_iter().flatten() {
                    println!("  {}", f.as_str().unwrap_or(""));
                }
                return Ok(true);
            }
            print!("{}", crate::bench::report(&run));
        }
        BenchCmd::Compare { a, b } => {
            let c = match &b {
                Some(b) => bench.compare(Some(&a), b)?,
                None => bench.compare(None, &a)?,
            };
            println!(
                "{} ({}, {}) → {} ({}, {})",
                c.a.id,
                c.a.model,
                pct(c.a.summary.success_rate),
                c.b.id,
                c.b.model,
                pct(c.b.summary.success_rate)
            );
            for w in &c.caveats {
                println!("\x1b[33mnote:\x1b[0m {w}");
            }
            for f in &c.regressions {
                println!(
                    "\x1b[31m▼ {}\x1b[0m {}",
                    if f.confirmed {
                        "regression"
                    } else {
                        "possible regression"
                    },
                    f.message
                );
            }
            for f in &c.improvements {
                println!(
                    "\x1b[32m▲ {}\x1b[0m {}",
                    if f.confirmed {
                        "improvement"
                    } else {
                        "possible improvement"
                    },
                    f.message
                );
            }
            if c.regressions.is_empty() && c.improvements.is_empty() {
                println!("no difference in outcomes");
            }
            for t in c.tasks.iter().filter(|t| t.change != "unchanged") {
                let f =
                    |p: Option<(usize, usize)>| p.map_or("—".into(), |(a, b)| format!("{a}/{b}"));
                println!(
                    "  {:<14} {:>5} → {:<5} {}",
                    t.task,
                    f(t.a),
                    f(t.b),
                    t.change
                );
            }
            return Ok(!c.regressions.iter().any(|f| f.confirmed));
        }
        BenchCmd::Baseline { run, clear } => {
            if clear {
                bench.store().set_baseline(None)?;
                eprintln!("no baseline: runs are compared with the previous one of their suite");
            } else if let Some(run) = run {
                let id = bench.store().find(&run)?.id;
                bench.store().set_baseline(Some(&id))?;
                eprintln!("runs are now compared with {id}");
            } else {
                match bench.store().baseline() {
                    Some(id) => println!("{id}"),
                    None => eprintln!(
                        "no baseline: runs are compared with the previous one of their suite"
                    ),
                }
            }
        }
        BenchCmd::Export { run, format } => {
            let run = bench.store().find(&run)?;
            if format == "csv" {
                print!("{}", crate::bench::csv(&run));
            } else {
                println!("{}", serde_json::to_string_pretty(&run)?);
            }
        }
    }
    Ok(true)
}
