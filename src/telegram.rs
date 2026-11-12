//! The founder's Telegram bot (decisions 28 and 52), from the branch
//! `business-autonomy`. Airth only makes outgoing requests: it long-polls
//! for the founder's messages and taps, so nothing on the Mac is reachable
//! from outside. Each item that waits for the founder, in every business,
//! is sent once, its business named first, with buttons; a text is never
//! cut, a long one going as several messages. Failures in the founder's
//! notifications are forwarded. Only the chat that paired, and the
//! founder's user id in it, are listened to.

use crate::business::store::Store;
use crate::business::store::queue::Item;
use crate::hub::Hub;
use crate::realm::write_private;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

const API: &str = "https://api.telegram.org";
/// How long one poll waits for a message (Telegram holds the request).
const POLL_SECONDS: u64 = 25;
/// Under Telegram's 4,096 characters per message, with room for the header.
const CHUNK: usize = 3900;

/// What Airth keeps about the bot: `telegram.json` in the home (mode 600).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub token: String,
    pub bot: String,
    /// The founder's chat and user id, once paired.
    pub chat: Option<i64>,
    pub user: Option<i64>,
    /// The code the pairing link carries, until the founder pairs.
    pub code: String,
    /// The next update to ask Telegram for.
    pub offset: i64,
    /// The last notification forwarded.
    pub inbox_sent: u64,
    /// What was sent for each item, by `realm/id`.
    pub sent: BTreeMap<String, Sent>,
    /// The item whose new text the bot waits for, after an Edit tap.
    pub editing: Option<String>,
    /// The item whose draft the founder asked to redo: their next reply to
    /// it says what should change.
    #[serde(default)]
    pub redoing: Option<String>,
    /// The item the founder tapped Reject on: their next reply to it says
    /// why, which its agent's next drafts hear (decision 116); Skip rejects
    /// it without a word.
    #[serde(default)]
    pub rejecting: Option<String>,
    /// How many times Telegram refused to take each item, by `realm/id`.
    pub refused: BTreeMap<String, u32>,
    /// The business the founder's plain messages go to (decision 109); the
    /// default business when none was chosen.
    pub business: Option<String>,
    /// The founder's tasks still at work, by `realm/run`: their answer goes
    /// to the chat when they end.
    pub talks: std::collections::BTreeSet<String>,
    /// The agents' answers in the chat, by message id: a reply to one goes
    /// on with its task (`realm/run`). The newest are kept.
    pub threads: BTreeMap<i64, String>,
    /// The devices asking for access the founder was asked about (decision 118).
    pub devices_sent: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sent {
    pub messages: Vec<i64>,
    /// When the founder was shown it: their time counts from here.
    pub at: DateTime<Utc>,
    pub revision: i64,
}

/// Telegram's Bot API, behind a trait so tests can stand in for it.
pub trait Api {
    fn call(&self, method: &str, body: &Value) -> Result<Value>;
}

/// The real API, with the bot's token.
pub struct Telegram<'a>(pub &'a str);

impl Api for Telegram<'_> {
    fn call(&self, method: &str, body: &Value) -> Result<Value> {
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(POLL_SECONDS + 15))
            .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
            .build()?
            .post(format!("{API}/bot{}/{method}", self.0))
            .json(body)
            .send()
            .map_err(|e| anyhow::anyhow!("Telegram did not answer: {}", describe(e, self.0)))?;
        let answer: Value = response.json().context("Telegram's answer is not JSON")?;
        if answer["ok"] != Value::Bool(true) {
            bail!(
                "Telegram refused {method}: {}",
                answer["description"].as_str().unwrap_or("no reason given")
            );
        }
        Ok(answer["result"].clone())
    }
}

/// Why a request failed, with each cause beneath it: reqwest's own message
/// is only "error sending request", which says nothing of a refused
/// connection, a name that did not resolve or a time-out. The URL holds the
/// token: it is left out, and the token is masked wherever it shows.
fn describe(e: reqwest::Error, token: &str) -> String {
    let e = e.without_url();
    let mut said = e.to_string();
    let mut cause = std::error::Error::source(&e);
    while let Some(c) = cause {
        let text = c.to_string();
        if !said.ends_with(&text) {
            said.push_str(": ");
            said.push_str(&text);
        }
        cause = c.source();
    }
    if token.is_empty() {
        said
    } else {
        said.replace(token, "<token>")
    }
}

/// Check the bot from end to end: Telegram answers for it, and, once
/// paired, a message reaches the founder's chat. What it says, or why not.
pub fn check(api: &dyn Api, config: &Config) -> Result<String> {
    let me = api.call("getMe", &json!({}))?;
    let bot = me["username"].as_str().unwrap_or("your bot");
    let Some(chat) = config.chat else {
        return Ok(format!(
            "@{bot} answers, but is not paired yet: open https://t.me/{bot}?start={}",
            config.code
        ));
    };
    api.call(
        "sendMessage",
        &json!({"chat_id": chat, "text": "Airth: a test message. Telegram works."}),
    )?;
    Ok(format!(
        "@{bot} answers, and a test message was sent to your chat"
    ))
}

fn path(hub: &Hub) -> PathBuf {
    hub.home().dir().join("telegram.json")
}

pub fn load(hub: &Hub) -> Result<Config> {
    match std::fs::read_to_string(path(hub)) {
        Ok(text) => Ok(serde_json::from_str(&text).context("reading telegram.json")?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn save(hub: &Hub, config: &Config) -> Result<()> {
    // Private from the first byte: the file holds the bot's token.
    write_private(&path(hub), &serde_json::to_string_pretty(config)?)
}

/// Save what the bot loop changed, unless the bot was forgotten or replaced
/// meanwhile (`airth telegram forget` or `connect` while the loop waited on
/// Telegram): that change wins (decision 108).
pub fn save_unless_changed(hub: &Hub, config: &Config) -> Result<()> {
    if load(hub)?.token != config.token {
        return Ok(());
    }
    save(hub, config)
}

/// After this many refusals of the same item, it is let go.
const REFUSALS: u32 = 3;

/// Whether Telegram took the request and refused it (a bad message), rather
/// than not answering (the network): only the first is let go.
fn refused(e: &anyhow::Error) -> bool {
    format!("{e:#}").starts_with("Telegram refused")
}

/// Check a bot's token and start pairing: the link the founder opens.
pub fn connect(hub: &Hub, api: &dyn Api, token: &str) -> Result<String> {
    let me = api
        .call("getMe", &json!({}))
        .context("this is not a working bot token")?;
    let config = Config {
        token: token.trim().to_string(),
        bot: me["username"].as_str().unwrap_or_default().to_string(),
        code: crate::ui::random_token()?,
        inbox_sent: hub
            .home()
            .inbox()
            .list()?
            .iter()
            .map(|n| n.id)
            .max()
            .unwrap_or(0),
        ..Config::default()
    };
    save(hub, &config)?;
    Ok(format!("https://t.me/{}?start={}", config.bot, config.code))
}

/// `text` in pieces Telegram takes, cut between lines when it can.
fn pieces(text: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    for line in text.split_inclusive('\n') {
        let mut line = line;
        while !line.is_empty() {
            let last = out.last_mut().expect("never empty");
            let room = CHUNK.saturating_sub(last.chars().count());
            if room == 0 {
                out.push(String::new());
                continue;
            }
            let take: String = line.chars().take(room).collect();
            if take.len() < line.len() && !last.is_empty() {
                out.push(String::new());
                continue;
            }
            last.push_str(&take);
            line = &line[take.len()..];
        }
    }
    out
}

/// Send `text`, the buttons under its last piece: the messages' ids.
fn send(api: &dyn Api, chat: i64, text: &str, buttons: Option<Value>) -> Result<Vec<i64>> {
    let parts = pieces(text);
    let mut ids = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let mut body = json!({"chat_id": chat, "text": part, "disable_web_page_preview": true});
        if let (Some(b), true) = (&buttons, i + 1 == parts.len()) {
            body["reply_markup"] = json!({"inline_keyboard": b});
        }
        ids.push(
            api.call("sendMessage", &body)?["message_id"]
                .as_i64()
                .unwrap_or_default(),
        );
    }
    Ok(ids)
}

/// What the founder reads for an item: its business first, then all of it.
fn item_text(realm: &str, item: &Item) -> String {
    let whole = match item.source["show"].as_str() {
        Some(path) => crate::pipeline::run::field_text(
            item.args.as_object().unwrap_or(&Default::default()),
            path,
        ),
        None => serde_json::to_string_pretty(&item.args).unwrap_or_default(),
    };
    let what = match item.kind.as_str() {
        "prepared" => "To do yourself",
        "incident" => "Incident",
        "report" => "Report",
        "idea" => "An idea",
        "deadline" => "Legal deadline",
        "delayed" => "Will be done unless you cancel",
        "approval" if item.action.as_deref() == Some("airth.dispatch") => "A plan for the team",
        "approval" if item.action.is_some() => "Asks to act",
        _ => "Needs you",
    };
    let action = item
        .action
        .as_deref()
        .map(|a| format!(" ({a})"))
        .unwrap_or_default();
    let flag = match (item.flag.as_deref(), item.flag_reason.as_deref()) {
        (Some("blocking"), why) => format!(
            "\n⛔ Flagged by the check: {}. Edit it, or Override to approve it as it is.",
            why.unwrap_or("no reason given")
        ),
        (Some("note"), why) => format!("\nNote from the check: {}", why.unwrap_or("none")),
        _ => String::new(),
    };
    // What the same run dropped (decision 61), so a missing post is seen.
    let dropped = item
        .note
        .as_deref()
        .map(|n| format!("\nAlso in this run: {n}"))
        .unwrap_or_default();
    // A draft's title is often its first words, cut: shown above the whole
    // text, it read as a second, broken copy. It is said only when it adds
    // something.
    let stem = item.title.trim_end_matches(['…', '.']).trim();
    let title = if stem.is_empty() || whole.trim_start().starts_with(stem) {
        String::new()
    } else {
        format!("\n{}", item.title)
    };
    // An idea says why it is told now (decision 119).
    let why = match item.args["why"].as_str() {
        Some(why) if item.kind == "idea" && !why.trim().is_empty() => format!("\n\nWhy now: {why}"),
        _ => String::new(),
    };
    format!("{realm} · {what}{action}{title}{flag}{dropped}\n\n{whole}{why}")
}

/// Whether an item's draft can be drafted again: a routine's draft.
fn redoable(item: &Item) -> bool {
    item.kind == "approval" && item.routine.is_some() && item.source["editable"].is_string()
}

fn buttons(realm: &str, item: &Item) -> Value {
    let data = |op: &str| format!("{op}/{realm}/{}/{}", item.id, item.revision);
    let button = |label: &str, op: &str| json!({"text": label, "callback_data": data(op)});
    match item.kind.as_str() {
        "delayed" => json!([[button("Cancel", "r")]]),
        // Decision 62: a failed action offers Retry; an ambiguous one asks
        // first (op "u"), then retries on "c".
        "incident" => match item.args["retry"].as_str() {
            Some("clear") => json!([[button("Retry", "t"), button("Seen", "a")]]),
            Some("ambiguous") => json!([[button("Retry…", "u"), button("Seen", "a")]]),
            _ => json!([[button("Seen", "a")]]),
        },
        "report" => json!([[button("Seen", "a")]]),
        "idea" => json!([[button("Useful", "a"), button("Not for me", "r")]]),
        // Decision 66: an overdue deadline stays until it is filed.
        "deadline" if item.args["overdue"] == true => json!([[button("Filed", "a")]]),
        "deadline" => json!([[button("Filed", "a"), button("Seen", "r")]]),
        "prepared" => json!([[button("Done", "a")]]),
        // Blocked by a check: no plain Approve (decision 58).
        _ if item.flag.as_deref() == Some("blocking") => {
            let mut row = vec![button("Override", "o")];
            if item.source["editable"].is_string() {
                row.push(button("Edit", "e"));
            }
            if redoable(item) {
                row.push(button("Redo", "n"));
            }
            row.push(button("Reject", "r"));
            json!([row])
        }
        _ if redoable(item) => {
            json!([[
                button("Approve", "a"),
                button("Edit", "e"),
                button("Redo", "n"),
                button("Reject", "r")
            ]])
        }
        _ if item.source["editable"].is_string() => {
            json!([[
                button("Approve", "a"),
                button("Edit", "e"),
                button("Reject", "r")
            ]])
        }
        _ => json!([[button("Approve", "a"), button("Reject", "r")]]),
    }
}

/// Send each item that waits for the founder and was not sent yet, then
/// the failures in the founder's notifications.
pub fn push(hub: &Hub, api: &dyn Api, config: &mut Config) -> Result<()> {
    let Some(chat) = config.chat else {
        return Ok(());
    };
    for realm in hub.home().realms()? {
        if !realm.dir().join(crate::business::store::FILE).exists() {
            continue;
        }
        for item in Store::of(&realm)?.queue(&["pending"])? {
            let key = format!("{}/{}", realm.name(), item.id);
            if config.sent.contains_key(&key) {
                continue;
            }
            let messages = match send(
                api,
                chat,
                &item_text(realm.name(), &item),
                Some(buttons(realm.name(), &item)),
            ) {
                Ok(messages) => messages,
                // A message Telegram will never take must not hold up every
                // item after it: after a few refusals it is let go, and the
                // founder told (decision 108). Not answering is tried again.
                Err(e) if refused(&e) => {
                    let n = config.refused.entry(key.clone()).or_default();
                    *n += 1;
                    if *n < REFUSALS {
                        continue;
                    }
                    config.refused.remove(&key);
                    let _ = hub.home().inbox().push(
                        crate::notify::Kind::Failed,
                        realm.name(),
                        None,
                        "Telegram refused an item",
                        &format!(
                            "Item {} waits for you in Needs you, in the app. Telegram refused it {REFUSALS} times: {e:#}",
                            item.id
                        ),
                    );
                    Vec::new()
                }
                Err(e) => return Err(e),
            };
            config.refused.remove(&key);
            config.sent.insert(
                key,
                Sent {
                    messages,
                    at: Utc::now(),
                    revision: item.revision,
                },
            );
        }
    }
    // A device asking for access (decision 118): the founder allows it here
    // only when its code is the one on their own device.
    let asking = crate::devices::asking(hub.home().dir(), Utc::now());
    for ask in &asking {
        if config.devices_sent.contains(&ask.id) {
            continue;
        }
        let button = |label: &str, op: &str| json!({"text": label, "callback_data": format!("{op}/device/{}", ask.id)});
        let text = format!(
            "A device asks for access to Airth: {}\nCode on its screen: {}\nAllow it only if this code is on a device you hold.",
            ask.name, ask.code
        );
        match send(
            api,
            chat,
            &text,
            Some(json!([[button("Allow", "da"), button("Refuse", "dr")]])),
        ) {
            Ok(_) => {}
            // It stays in the app, under Settings, Phone.
            Err(e) if refused(&e) => eprintln!("telegram: device request refused: {e:#}"),
            Err(e) => return Err(e),
        }
        config.devices_sent.insert(ask.id.clone());
    }
    config
        .devices_sent
        .retain(|id| asking.iter().any(|a| &a.id == id));
    for n in hub.home().inbox().list()? {
        if n.id > config.inbox_sent && n.kind == crate::notify::Kind::Failed {
            match send(
                api,
                chat,
                &format!("{} · {}\n{}", n.realm, n.title, n.body),
                None,
            ) {
                Ok(_) => {}
                // It stays in the app's notifications.
                Err(e) if refused(&e) => eprintln!("telegram: notice {} refused: {e:#}", n.id),
                Err(e) => return Err(e),
            }
        }
        config.inbox_sent = config.inbox_sent.max(n.id);
    }
    answers(hub, api, config)?;
    Ok(())
}

/// Whether `message` is the founder's own, in the paired chat: not
/// forwarded, from the founder's user id.
fn from_founder(config: &Config, message: &Value) -> bool {
    let forwarded = ["forward_origin", "forward_from", "forward_date"]
        .iter()
        .any(|k| !message[k].is_null());
    !forwarded
        && config
            .chat
            .is_some_and(|c| message["chat"]["id"].as_i64() == Some(c))
        && config
            .user
            .is_some_and(|u| message["from"]["id"].as_i64() == Some(u))
}

/// One update from Telegram. Before pairing, only the pairing link counts;
/// after, only the founder's messages and taps, as answers to the queue.
/// Starts a task unwatched, as the service's runner does.
pub type Start<'a> = &'a dyn Fn(crate::realm::Realm, crate::state::State);

pub fn handle(
    hub: &Hub,
    api: &dyn Api,
    config: &mut Config,
    update: &Value,
    start: Start,
) -> Result<()> {
    if !update["callback_query"].is_null() {
        return tap(hub, api, config, &update["callback_query"]);
    }
    let message = &update["message"];
    if message.is_null() {
        return Ok(());
    }
    let text = message["text"].as_str().unwrap_or_default();
    if config.chat.is_none() {
        let paired = !config.code.is_empty()
            && text == format!("/start {}", config.code)
            && message["forward_date"].is_null();
        if let (true, Some(chat), Some(user)) = (
            paired,
            message["chat"]["id"].as_i64(),
            message["from"]["id"].as_i64(),
        ) {
            (config.chat, config.user, config.code) = (Some(chat), Some(user), String::new());
            send(
                api,
                chat,
                "Paired. What waits for you in your businesses will come here, with buttons.",
                None,
            )?;
        }
        return Ok(());
    }
    if !from_founder(config, message) {
        return Ok(());
    }
    let chat = config.chat.unwrap_or_default();
    // A post link, not as a reply to an item: the founder wants a reply
    // drafted (decision 63).
    if message["reply_to_message"].is_null() && crate::routine::x_post::link_in(text).is_some() {
        return reply_to_post(hub, api, chat, text);
    }
    // Decision 109: the founder talks to the agents. A reply to an agent's
    // answer goes on with its task; `/business` says or moves the business;
    // any other plain message is an order.
    if let Some(key) = message["reply_to_message"]["message_id"]
        .as_i64()
        .and_then(|id| config.threads.get(&id).cloned())
    {
        return go_on(hub, api, config, &key, text, start);
    }
    if let Some(rest) = text.strip_prefix("/business") {
        return choose_business(hub, api, config, rest.trim());
    }
    // The founder said why they reject an item: it is rejected with it.
    if let Some(key) = config.rejecting.clone() {
        let replied = message["reply_to_message"]["message_id"].as_i64();
        if let Some(sent) = config
            .sent
            .get(&key)
            .cloned()
            .filter(|s| replied.is_some_and(|r| s.messages.contains(&r)))
        {
            config.rejecting = None;
            let (realm, id) = item_of(hub, &key)?;
            let answer = crate::business::store::queue::Answer::Reject(text.trim().to_string());
            let said = match hub.answer_item(
                &realm,
                id,
                Some(sent.revision),
                answer,
                "telegram",
                answer_seconds(&sent),
            ) {
                Ok((item, report)) => answered(realm.name(), &item, report.as_deref()),
                Err(e) => format!("Not done: {e:#}"),
            };
            send(api, chat, &said, None)?;
            return Ok(());
        }
    }
    // The founder said what should change in a draft: it is drafted again.
    if let Some(key) = config.redoing.clone() {
        let replied = message["reply_to_message"]["message_id"].as_i64();
        if let Some(sent) = config
            .sent
            .get(&key)
            .cloned()
            .filter(|s| replied.is_some_and(|r| s.messages.contains(&r)))
        {
            config.redoing = None;
            let (realm, id) = item_of(hub, &key)?;
            send(api, chat, "Drafting it again…", None)?;
            let said = match hub.redo_item(
                &realm,
                id,
                Some(sent.revision),
                text,
                "telegram",
                answer_seconds(&sent),
            ) {
                // The new draft comes as its own item, with its buttons.
                Ok(report) => report,
                Err(e) => format!("Not redone: {e:#}"),
            };
            send(api, chat, &said, None)?;
            return Ok(());
        }
    }
    let Some(key) = config.editing.clone() else {
        if message["reply_to_message"].is_null()
            && !text.trim().is_empty()
            && !text.starts_with('/')
        {
            return talk(hub, api, config, text, start);
        }
        return Ok(());
    };
    // The new text counts only as a reply to that item's message.
    let replied = message["reply_to_message"]["message_id"].as_i64();
    let sent = config.sent.get(&key).cloned();
    let Some(sent) = sent.filter(|s| replied.is_some_and(|r| s.messages.contains(&r))) else {
        send(
            api,
            chat,
            "To change the item, reply to its message with the new text.",
            None,
        )?;
        return Ok(());
    };
    let (realm, id) = item_of(hub, &key)?;
    let seconds = answer_seconds(&sent);
    let answer = crate::business::store::queue::Answer::Edit(text.to_string());
    config.editing = None;
    match hub.answer_item(&realm, id, Some(sent.revision), answer, "telegram", seconds) {
        Ok((item, _)) => {
            let messages = send(
                api,
                chat,
                &item_text(realm.name(), &item),
                Some(buttons(realm.name(), &item)),
            )?;
            config.sent.insert(
                key,
                Sent {
                    messages,
                    at: sent.at,
                    revision: item.revision,
                },
            );
        }
        Err(e) => {
            send(api, chat, &format!("Not changed: {e:#}"), None)?;
        }
    }
    Ok(())
}

/// Draft a reply to the post the founder sent: in the business named first
/// in the message, else the only one with a reply routine. The draft comes
/// next, as an item to approve; a post too thin to answer is said.
fn reply_to_post(hub: &Hub, api: &dyn Api, chat: i64, text: &str) -> Result<()> {
    let mut replying = Vec::new();
    for realm in hub.home().realms()? {
        if realm.config.archived {
            continue;
        }
        let routines = hub.with_routine_checks(&realm, crate::routine::load_all)?;
        if routines.iter().any(|(_, g)| {
            g.as_ref()
                .is_ok_and(|g| g.trigger.telegram.as_deref() == Some("x_post"))
        }) {
            replying.push(realm);
        }
    }
    let first = text.split_whitespace().next().unwrap_or_default();
    let named = replying
        .iter()
        .position(|r| r.name() == first.trim_end_matches(':'));
    let (realm, text) = match (named, replying.as_slice()) {
        (Some(i), _) => (&replying[i], text.trim_start()[first.len()..].trim_start()),
        (None, [only]) => (only, text),
        (None, []) => {
            send(
                api,
                chat,
                "No business has a reply routine yet (trigger {\"telegram\": \"x_post\"}).",
                None,
            )?;
            return Ok(());
        }
        (None, many) => {
            let names: Vec<&str> = many.iter().map(|r| r.name()).collect();
            let ask = format!(
                "Which business? Start the message with its name: {}.",
                names.join(", ")
            );
            send(api, chat, &ask, None)?;
            return Ok(());
        }
    };
    let said = match hub.reply_to_post(realm, text) {
        Ok(_) => format!(
            "{} · reply drafted: it comes next, for your approval.",
            realm.name()
        ),
        Err(e) => format!(
            "{} · {}",
            realm.name(),
            crate::routine::run::one_line(&format!("{e:#}"))
        ),
    };
    send(api, chat, &said, None)?;
    Ok(())
}

/// The realm and item id a sent key names.
fn item_of(hub: &Hub, key: &str) -> Result<(crate::realm::Realm, i64)> {
    let (name, id) = key.rsplit_once('/').context("not an item")?;
    let realm = hub
        .home()
        .realms()?
        .into_iter()
        .find(|r| r.name() == name)
        .context("no such business")?;
    Ok((realm, id.parse()?))
}

/// From the moment the founder was shown the item to now.
fn answer_seconds(sent: &Sent) -> u64 {
    crate::business::time::answer_seconds((Utc::now() - sent.at).to_std().unwrap_or_default())
}

/// A tap on one of an item's buttons: `op/realm/id/revision`.
fn tap(hub: &Hub, api: &dyn Api, config: &mut Config, query: &Value) -> Result<()> {
    let ours = config
        .user
        .is_some_and(|u| query["from"]["id"].as_i64() == Some(u))
        && config
            .chat
            .is_some_and(|c| query["message"]["chat"]["id"].as_i64() == Some(c));
    if !ours {
        return Ok(());
    }
    // Stops the button's spinner, whatever happens next.
    api.call(
        "answerCallbackQuery",
        &json!({"callback_query_id": query["id"]}),
    )?;
    let chat = config.chat.unwrap_or_default();
    let data = query["data"].as_str().unwrap_or_default();
    let mut parts = data.splitn(2, '/');
    let (op, rest) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    if let Some(id) = rest.strip_prefix("device/") {
        let allow = match op {
            "da" => true,
            "dr" => false,
            _ => return Ok(()),
        };
        let said = match crate::devices::answer(hub.home().dir(), id, allow, Utc::now()) {
            Ok(name) if allow => format!("{name} is allowed: it opens Airth in a few seconds."),
            Ok(name) => format!("{name} is refused."),
            Err(e) => format!("{e:#}"),
        };
        // The buttons go: a request is answered once.
        let _ = api.call(
            "editMessageReplyMarkup",
            &json!({"chat_id": chat, "message_id": query["message"]["message_id"], "reply_markup": {"inline_keyboard": []}}),
        );
        send(api, chat, &said, None)?;
        return Ok(());
    }
    let Some((key, revision)) = rest.rsplit_once('/') else {
        return Ok(());
    };
    let revision: i64 = revision.parse()?;
    let (realm, id) = item_of(hub, key)?;
    let Some(sent) = config.sent.get(key).cloned() else {
        return Ok(());
    };
    use crate::business::store::queue::Answer;
    let answer = match op {
        "e" => {
            if sent.revision != revision {
                send(
                    api,
                    chat,
                    "That button is out of date: use the item's latest message.",
                    None,
                )?;
                return Ok(());
            }
            config.editing = Some(key.to_string());
            config.redoing = None;
            config.rejecting = None;
            let last = sent.messages.last().copied().unwrap_or_default();
            let body = json!({"chat_id": chat, "text": "Reply to the item's message with the new text.",
                "reply_to_message_id": last, "reply_markup": {"force_reply": true}});
            let asked = api.call("sendMessage", &body)?["message_id"]
                .as_i64()
                .unwrap_or_default();
            // A reply to this question is a reply to the item.
            if let Some(s) = config.sent.get_mut(key) {
                s.messages.push(asked);
            }
            return Ok(());
        }
        "n" => {
            if sent.revision != revision {
                send(
                    api,
                    chat,
                    "That button is out of date: use the item's latest message.",
                    None,
                )?;
                return Ok(());
            }
            config.redoing = Some(key.to_string());
            config.editing = None;
            config.rejecting = None;
            let last = sent.messages.last().copied().unwrap_or_default();
            let body = json!({"chat_id": chat, "text": "What should change? Reply here: shorter, funnier, another angle…",
                "reply_to_message_id": last, "reply_markup": {"force_reply": true}});
            let asked = api.call("sendMessage", &body)?["message_id"]
                .as_i64()
                .unwrap_or_default();
            // A reply to this question is a reply to the item.
            if let Some(s) = config.sent.get_mut(key) {
                s.messages.push(asked);
            }
            return Ok(());
        }
        "u" => {
            let data = format!("c/{key}/{revision}");
            let body = json!({"chat_id": chat,
                "text": "It may have been done. Check that nothing was created (in the service itself) before you retry.",
                "reply_markup": {"inline_keyboard": [[{"text": "Nothing was created: retry", "callback_data": data}]]}});
            api.call("sendMessage", &body)?;
            return Ok(());
        }
        // Reject asks why first: the reason reaches the agent's next drafts
        // (decision 116). Skip rejects without one.
        "r" => {
            if sent.revision != revision {
                send(
                    api,
                    chat,
                    "That button is out of date: use the item's latest message.",
                    None,
                )?;
                return Ok(());
            }
            config.rejecting = Some(key.to_string());
            config.editing = None;
            config.redoing = None;
            let last = sent.messages.last().copied().unwrap_or_default();
            let skip = format!("s/{key}/{revision}");
            let body = json!({"chat_id": chat, "text": "Why reject it? Reply here, and its agent hears it for its next drafts. Or skip.",
                "reply_to_message_id": last,
                "reply_markup": {"inline_keyboard": [[{"text": "Skip: reject it", "callback_data": skip}]]}});
            let asked = api.call("sendMessage", &body)?["message_id"]
                .as_i64()
                .unwrap_or_default();
            // A reply to this question is a reply to the item.
            if let Some(s) = config.sent.get_mut(key) {
                s.messages.push(asked);
            }
            return Ok(());
        }
        "a" => Answer::Approve,
        "o" => Answer::Override,
        "t" => Answer::Retry { checked: false },
        "c" => Answer::Retry { checked: true },
        "s" => {
            config.rejecting = None;
            Answer::Reject("from Telegram".into())
        }
        // An unknown button does nothing, never a rejection.
        _ => return Ok(()),
    };
    let text = match hub.answer_item(
        &realm,
        id,
        Some(revision),
        answer,
        "telegram",
        answer_seconds(&sent),
    ) {
        Ok((item, report)) => answered(realm.name(), &item, report.as_deref()),
        Err(e) => format!("Not done: {e:#}"),
    };
    send(api, chat, &text, None)?;
    Ok(())
}

/// What the bot says once an answer is taken: the item's new state and
/// what came of it. Approved but failed to send is never "done".
fn answered(realm: &str, item: &Item, report: Option<&str>) -> String {
    let report = report.map(|r| format!("\n{r}")).unwrap_or_default();
    let status = match (item.kind.as_str(), item.status.as_str()) {
        ("incident", "failed") => "retry failed",
        (_, "failed") => "approved, sending failed",
        (_, status) => status,
    };
    format!("{realm} · {status}: {}{report}", item.title)
}

/// Ask Telegram for what came, handle it, then send what waits.
/// The business the founder's messages go to: the one chosen, else the
/// default, else the first.
fn current_business(hub: &Hub, config: &Config) -> Result<crate::realm::Realm> {
    let realms: Vec<_> = hub
        .home()
        .realms()?
        .into_iter()
        .filter(|r| !r.config.archived)
        .collect();
    let wanted = config
        .business
        .clone()
        .or_else(|| hub.home().default_realm());
    realms
        .iter()
        .find(|r| Some(r.name()) == wanted.as_deref())
        .or(realms.first())
        .cloned()
        .context("there is no business yet")
}

fn choose_business(hub: &Hub, api: &dyn Api, config: &mut Config, name: &str) -> Result<()> {
    let chat = config.chat.unwrap_or_default();
    let names: Vec<String> = hub
        .home()
        .realms()?
        .into_iter()
        .filter(|r| !r.config.archived)
        .map(|r| r.name().to_string())
        .collect();
    let said = if name.is_empty() {
        let on = current_business(hub, config)?.name().to_string();
        format!(
            "Your messages go to {on}. Businesses: {}. Move with /business <name>.",
            names.join(", ")
        )
    } else if names.iter().any(|n| n == name) {
        config.business = Some(name.to_string());
        format!("Your messages now go to {name}.")
    } else {
        format!(
            "There is no business {name}. Businesses: {}.",
            names.join(", ")
        )
    };
    send(api, chat, &said, None)?;
    Ok(())
}

/// A plain message: an order to the business's agents, routed as the
/// command bar routes it (decision 64), run unwatched.
fn talk(hub: &Hub, api: &dyn Api, config: &mut Config, text: &str, start: Start) -> Result<()> {
    let chat = config.chat.unwrap_or_default();
    let realm = current_business(hub, config)?;
    let route = match hub.route_order(&realm, text) {
        Ok(route) => route,
        Err(e) => {
            send(
                api,
                chat,
                &format!(
                    "{} · I could not tell which agent should take it: {e:#}. Start with its name, like \"writer: …\".",
                    realm.name()
                ),
                None,
            )?;
            return Ok(());
        }
    };
    let state = hub.start_task(&realm, &route.agent, text)?;
    config
        .talks
        .insert(format!("{}/{}", realm.name(), state.id));
    send(
        api,
        chat,
        &format!("{} · {} is on it.", realm.name(), route.agent),
        None,
    )?;
    start(realm, state);
    Ok(())
}

/// A reply to an agent's answer: the same task goes on with it.
fn go_on(
    hub: &Hub,
    api: &dyn Api,
    config: &mut Config,
    key: &str,
    text: &str,
    start: Start,
) -> Result<()> {
    let chat = config.chat.unwrap_or_default();
    let (name, run) = key.split_once('/').context("unknown task")?;
    let realm = hub.home().realm(name)?;
    // A task that can no longer go on (its worktree cleared after a week,
    // its change merged) says so, rather than leave the reply unanswered.
    let state = match hub.follow_up(&realm, run, text) {
        Ok(state) => state,
        Err(e) => {
            config.threads.retain(|_, k| k != key);
            send(
                api,
                chat,
                &format!(
                    "{name} · That task cannot go on ({e:#}). Send a new message to start another."
                ),
                None,
            )?;
            return Ok(());
        }
    };
    let agent = state.agent.clone().unwrap_or_else(|| "the agent".into());
    config.talks.insert(key.to_string());
    send(api, chat, &format!("{name} · {agent} goes on."), None)?;
    start(realm, state);
    Ok(())
}

/// How many answers a reply can still reach.
const THREADS: usize = 200;

/// The founder's tasks that ended since: their answer goes to the chat. A
/// task waiting for the founder in Needs you is not over.
fn answers(hub: &Hub, api: &dyn Api, config: &mut Config) -> Result<()> {
    let chat = config.chat.unwrap_or_default();
    for key in config.talks.clone() {
        let Some((name, run)) = key.split_once('/') else {
            config.talks.remove(&key);
            continue;
        };
        let Ok(realm) = hub.home().realm(name) else {
            config.talks.remove(&key);
            continue;
        };
        let Ok(state) = hub.load_run(&realm, run) else {
            config.talks.remove(&key);
            continue;
        };
        let waiting = Store::of(&realm)?
            .queue(&["pending"])?
            .iter()
            .any(|i| i.run.as_deref() == Some(run));
        if hub.is_live(&realm, run) || state.status == crate::state::Status::Running || waiting {
            continue;
        }
        let agent = state.agent.clone().unwrap_or_else(|| "The agent".into());
        let body = match (&state.report, state.status) {
            (Some(report), _) if !report.trim().is_empty() => report.trim().to_string(),
            (_, crate::state::Status::Failed) => {
                "It could not finish: see the task in the app.".into()
            }
            (_, crate::state::Status::Stopped) => {
                "It stopped before finishing: see the task in the app.".into()
            }
            _ => "Done, with nothing to say.".into(),
        };
        let ids = send(api, chat, &format!("{name} · {agent}\n{body}"), None)?;
        for id in ids {
            config.threads.insert(id, key.clone());
        }
        while config.threads.len() > THREADS {
            config.threads.pop_first();
        }
        config.talks.remove(&key);
    }
    Ok(())
}

pub fn turn(hub: &Hub, api: &dyn Api, config: &mut Config, start: Start) -> Result<()> {
    let updates = api.call(
        "getUpdates",
        &json!({"offset": config.offset, "timeout": POLL_SECONDS, "allowed_updates": ["message", "callback_query"]}),
    )?;
    for update in updates.as_array().into_iter().flatten() {
        config.offset = update["update_id"].as_i64().unwrap_or_default() + 1;
        if let Err(e) = handle(hub, api, config, update, start) {
            eprintln!("telegram: {e:#}");
        }
    }
    push(hub, api, config)
}

/// The bot's loop, in the service: nothing while no bot is connected.
pub fn run(hub: Hub, start: Start) {
    loop {
        let result = load(&hub).and_then(|mut config| {
            if config.token.is_empty() {
                std::thread::sleep(Duration::from_secs(60));
                return Ok(());
            }
            let token = config.token.clone();
            let outcome = turn(&hub, &Telegram(&token), &mut config, start);
            save_unless_changed(&hub, &config)?;
            outcome
        });
        if let Err(e) = result {
            eprintln!("telegram: {e:#}");
            std::thread::sleep(Duration::from_secs(30));
        }
    }
}

#[cfg(test)]
mod tests;
