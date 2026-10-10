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
    Set { key: String },
    /// Remove a credential
    Rm { key: String },
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
                root: root.clone(),
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
        }
        SecretCmd::Set { key } => {
            let value = read_secret(&format!("{key} for realm {}: ", realm.name()))?;
            realm.set_secret(&key, &value)?;
            eprintln!(
                "\x1b[32m✓\x1b[0m saved {key} (only runs in realm {} can use it)",
                realm.name()
            );
        }
        SecretCmd::Rm { key } => {
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
