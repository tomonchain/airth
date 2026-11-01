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
use crate::realm::write_atomic;
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
            // The URL holds the token: never show it.
            .map_err(|e| anyhow::anyhow!("Telegram did not answer: {}", e.without_url()))?;
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
    let p = path(hub);
    write_atomic(&p, &serde_json::to_string_pretty(config)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
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
        "delayed" => "Will be done unless you cancel",
        "approval" if item.action.is_some() => "Asks to act",
        _ => "Needs you",
    };
    let action = item
        .action
        .as_deref()
        .map(|a| format!(" ({a})"))
        .unwrap_or_default();
    format!("{realm} · {what}{action}\n{}\n\n{whole}", item.title)
}

fn buttons(realm: &str, item: &Item) -> Value {
    let data = |op: &str| format!("{op}/{realm}/{}/{}", item.id, item.revision);
    let button = |label: &str, op: &str| json!({"text": label, "callback_data": data(op)});
    match item.kind.as_str() {
        "delayed" => json!([[button("Cancel", "r")]]),
        "incident" => json!([[button("Seen", "a")]]),
        "prepared" => json!([[button("Done", "a")]]),
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
                    at: Utc::now(),
                    revision: item.revision,
                },
            );
        }
    }
    for n in hub.home().inbox().list()? {
        if n.id > config.inbox_sent && n.kind == crate::notify::Kind::Failed {
            send(
                api,
                chat,
                &format!("{} · {}\n{}", n.realm, n.title, n.body),
                None,
            )?;
        }
        config.inbox_sent = config.inbox_sent.max(n.id);
    }
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
pub fn handle(hub: &Hub, api: &dyn Api, config: &mut Config, update: &Value) -> Result<()> {
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
    let Some(key) = config.editing.clone() else {
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
        "a" => Answer::Approve,
        _ => Answer::Reject("from Telegram".into()),
    };
    let text = match hub.answer_item(
        &realm,
        id,
        Some(revision),
        answer,
        "telegram",
        answer_seconds(&sent),
    ) {
        Ok((item, report)) => {
            let report = report.map(|r| format!("\n{r}")).unwrap_or_default();
            format!("{} · {}: {}{report}", realm.name(), item.status, item.title)
        }
        Err(e) => format!("Not done: {e:#}"),
    };
    send(api, chat, &text, None)?;
    Ok(())
}

/// Ask Telegram for what came, handle it, then send what waits.
pub fn turn(hub: &Hub, api: &dyn Api, config: &mut Config) -> Result<()> {
    let updates = api.call(
        "getUpdates",
        &json!({"offset": config.offset, "timeout": POLL_SECONDS, "allowed_updates": ["message", "callback_query"]}),
    )?;
    for update in updates.as_array().into_iter().flatten() {
        config.offset = update["update_id"].as_i64().unwrap_or_default() + 1;
        if let Err(e) = handle(hub, api, config, update) {
            eprintln!("telegram: {e:#}");
        }
    }
    push(hub, api, config)
}

/// The bot's loop, in the service: nothing while no bot is connected.
pub fn run(hub: Hub) {
    loop {
        let result = load(&hub).and_then(|mut config| {
            if config.token.is_empty() {
                std::thread::sleep(Duration::from_secs(60));
                return Ok(());
            }
            let token = config.token.clone();
            let outcome = turn(&hub, &Telegram(&token), &mut config);
            save(&hub, &config)?;
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
