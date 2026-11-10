//! The founder's devices (decision 118). A new phone or browser asks for
//! access; the founder sees a short code on it and on Telegram or another
//! device, and allows it there. The device then holds a key of its own,
//! which the founder can revoke alone, without touching the others.
//!
//! What a device asks with, and the keys it gets, are never stored as such:
//! only their SHA-256, in `devices.json` in the Airth home (mode 600). A key
//! is made at the moment the allowed device collects it, and shown once.

use crate::supervise::Locked;
use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FILE: &str = "devices.json";
/// How long a request waits to be answered, then collected.
const ASK_FOR: i64 = 10 * 60;
/// Requests waiting at once: a few devices, not a flood of messages.
const MAX_ASKING: usize = 3;
/// How often a device's last use is written down.
const SEEN_EVERY: i64 = 10 * 60;
/// No 0/O, 1/I, 2/Z, 5/S, 8/B: a code read aloud or on a small screen.
const CODE_LETTERS: &[u8] = b"ACDEFGHJKLMNPQRTUVWXY34679";

/// One read-modify-write at a time: the web server's threads, Telegram's.
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Short and public: how the founder and the interface name it.
    pub id: String,
    pub name: String,
    /// SHA-256 of its key.
    key: String,
    pub added: DateTime<Utc>,
    pub seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ask {
    pub id: String,
    pub name: String,
    /// What the device shows: the founder allows only the device showing it.
    pub code: String,
    pub at: DateTime<Utc>,
    pub allowed: bool,
    /// SHA-256 of the secret only the asking page holds.
    secret: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
    devices: Vec<Device>,
    asking: Vec<Ask>,
}

/// Where a request stands, for the device that made it.
#[derive(Debug, PartialEq)]
pub enum Standing {
    Waiting,
    /// Allowed: the device's key, given this once.
    Allowed(String),
    /// Refused, expired or unknown: the device asks again if it wants.
    Gone,
}

fn path(home: &Path) -> PathBuf {
    home.join(FILE)
}

fn load(home: &Path) -> State {
    std::fs::read_to_string(path(home))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(home: &Path, state: &State) -> Result<()> {
    crate::realm::write_private(&path(home), &serde_json::to_string_pretty(state)?)
}

fn hash(secret: &str) -> String {
    crate::update::sha256_hex(secret.as_bytes())
}

fn random() -> Result<String> {
    crate::ui::random_token()
}

fn open(ask: &Ask, now: DateTime<Utc>) -> bool {
    now - ask.at < Duration::seconds(ASK_FOR)
}

/// A device's name as it gives it: one short line.
fn clean(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    match name.trim() {
        "" => "A device".into(),
        n => n.into(),
    }
}

/// A new request for access, from a device that holds no key. Returns the
/// secret the device keeps to collect its key, and the code it shows.
pub fn ask(home: &Path, name: &str, now: DateTime<Utc>) -> Result<(String, String)> {
    let _guard = LOCK.locked();
    let mut state = load(home);
    state.asking.retain(|a| open(a, now));
    // The oldest waiting gives way: a device that asks again replaces itself.
    while state.asking.len() >= MAX_ASKING {
        state.asking.remove(0);
    }
    let secret = random()?;
    let seed = random()?;
    let code: String = seed
        .as_bytes()
        .chunks(2)
        .take(4)
        .map(|pair| {
            let n = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("00"), 16).unwrap_or(0);
            CODE_LETTERS[n as usize % CODE_LETTERS.len()] as char
        })
        .collect();
    state.asking.push(Ask {
        id: random()?[..8].to_string(),
        name: clean(name),
        code: code.clone(),
        at: now,
        allowed: false,
        secret: hash(&secret),
    });
    save(home, &state)?;
    Ok((secret, code))
}

/// The requests waiting for the founder's answer.
pub fn asking(home: &Path, now: DateTime<Utc>) -> Vec<Ask> {
    load(home)
        .asking
        .into_iter()
        .filter(|a| !a.allowed && open(a, now))
        .collect()
}

/// The founder's answer to request `id`: allow it, or refuse it. The
/// device's name, for the founder's confirmation.
pub fn answer(home: &Path, id: &str, allow: bool, now: DateTime<Utc>) -> Result<String> {
    let _guard = LOCK.locked();
    let mut state = load(home);
    let Some(at) = state
        .asking
        .iter()
        .position(|a| a.id == id && !a.allowed && open(a, now))
    else {
        bail!("this request is no longer waiting: the device can ask again");
    };
    let name = state.asking[at].name.clone();
    if allow {
        state.asking[at].allowed = true;
    } else {
        state.asking.remove(at);
    }
    save(home, &state)?;
    Ok(name)
}

/// Where the request the device holds `secret` for stands. Once allowed,
/// the device is added and its key made, given this once.
pub fn standing(home: &Path, secret: &str, now: DateTime<Utc>) -> Result<Standing> {
    let _guard = LOCK.locked();
    let mut state = load(home);
    let wanted = hash(secret);
    let Some(at) = state
        .asking
        .iter()
        .position(|a| crate::ui::constant_time_eq(&a.secret, &wanted) && open(a, now))
    else {
        return Ok(Standing::Gone);
    };
    if !state.asking[at].allowed {
        return Ok(Standing::Waiting);
    }
    let ask = state.asking.remove(at);
    let key = random()?;
    state.devices.push(Device {
        id: ask.id,
        name: ask.name,
        key: hash(&key),
        added: now,
        seen: Some(now),
    });
    save(home, &state)?;
    Ok(Standing::Allowed(key))
}

/// The device holding `key`, if it is one of the founder's: its last use is
/// written down now and then.
pub fn known(home: &Path, key: &str, now: DateTime<Utc>) -> Option<String> {
    if key.len() < 32 {
        return None;
    }
    let wanted = hash(key);
    let state = load(home);
    let device = state
        .devices
        .iter()
        .find(|d| crate::ui::constant_time_eq(&d.key, &wanted))?;
    let id = device.id.clone();
    if device
        .seen
        .is_none_or(|s| now - s > Duration::seconds(SEEN_EVERY))
    {
        let _guard = LOCK.locked();
        let mut state = load(home);
        if let Some(d) = state.devices.iter_mut().find(|d| d.id == id) {
            d.seen = Some(now);
            let _ = save(home, &state);
        }
    }
    Some(id)
}

pub fn list(home: &Path) -> Vec<Device> {
    load(home).devices
}

/// Revoke one device: its key stops working at once.
pub fn revoke(home: &Path, id: &str) -> Result<bool> {
    let _guard = LOCK.locked();
    let mut state = load(home);
    let before = state.devices.len();
    state.devices.retain(|d| d.id != id);
    save(home, &state)?;
    Ok(state.devices.len() < before)
}

/// Every device and request forgotten: with a new main key (decision 98),
/// nothing paired before keeps working.
pub fn forget_all(home: &Path) -> Result<()> {
    let _guard = LOCK.locked();
    save(home, &State::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + minutes * 60, 0).unwrap()
    }

    #[test]
    fn an_allowed_device_gets_a_key_of_its_own_once() {
        let home = tempfile::tempdir().unwrap();
        let (secret, code) = ask(home.path(), "iPhone · Safari", at(0)).unwrap();
        assert_eq!(code.len(), 4);
        assert_eq!(
            standing(home.path(), &secret, at(0)).unwrap(),
            Standing::Waiting
        );
        let waiting = asking(home.path(), at(1));
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].code, code);
        // Not the secret: the founder's side never sees it.
        assert!(!serde_json::to_string(&waiting).unwrap().contains(&secret));

        assert_eq!(
            answer(home.path(), &waiting[0].id, true, at(2)).unwrap(),
            "iPhone · Safari"
        );
        let Standing::Allowed(key) = standing(home.path(), &secret, at(2)).unwrap() else {
            panic!("not allowed")
        };
        assert_eq!(known(home.path(), &key, at(3)), Some(waiting[0].id.clone()));
        // Collected once: asking again gets nothing, and no key is kept as such.
        assert_eq!(
            standing(home.path(), &secret, at(3)).unwrap(),
            Standing::Gone
        );
        let stored = std::fs::read_to_string(path(home.path())).unwrap();
        assert!(!stored.contains(&key) && !stored.contains(&secret));
        assert!(asking(home.path(), at(3)).is_empty());
    }

    #[test]
    fn a_device_not_allowed_gets_nothing() {
        let home = tempfile::tempdir().unwrap();
        // Unknown, short or empty keys are nobody's.
        assert_eq!(known(home.path(), "", at(0)), None);
        assert_eq!(known(home.path(), &"0".repeat(32), at(0)), None);
        // A wrong secret learns nothing.
        let (secret, _) = ask(home.path(), "Mac · Chrome", at(0)).unwrap();
        assert_eq!(
            standing(home.path(), &random().unwrap(), at(0)).unwrap(),
            Standing::Gone
        );
        // Refused: gone, and it cannot be allowed after.
        let id = asking(home.path(), at(0))[0].id.clone();
        answer(home.path(), &id, false, at(1)).unwrap();
        assert_eq!(
            standing(home.path(), &secret, at(1)).unwrap(),
            Standing::Gone
        );
        assert!(answer(home.path(), &id, true, at(1)).is_err());
        // Expired: neither the founder nor the device can use it.
        let (late, _) = ask(home.path(), "Old tab", at(0)).unwrap();
        let id = asking(home.path(), at(0))[0].id.clone();
        assert!(answer(home.path(), &id, true, at(11)).is_err());
        assert_eq!(
            standing(home.path(), &late, at(11)).unwrap(),
            Standing::Gone
        );
        assert!(list(home.path()).is_empty());
    }

    #[test]
    fn an_allowed_request_left_uncollected_expires() {
        let home = tempfile::tempdir().unwrap();
        let (secret, _) = ask(home.path(), "iPad", at(0)).unwrap();
        let id = asking(home.path(), at(0))[0].id.clone();
        answer(home.path(), &id, true, at(1)).unwrap();
        assert_eq!(
            standing(home.path(), &secret, at(11)).unwrap(),
            Standing::Gone
        );
        assert!(list(home.path()).is_empty());
    }

    #[test]
    fn revoking_one_device_leaves_the_others() {
        let home = tempfile::tempdir().unwrap();
        let mut keys = Vec::new();
        for name in ["iPhone", "Mac"] {
            let (secret, _) = ask(home.path(), name, at(0)).unwrap();
            let id = asking(home.path(), at(0)).last().unwrap().id.clone();
            answer(home.path(), &id, true, at(0)).unwrap();
            let Standing::Allowed(key) = standing(home.path(), &secret, at(0)).unwrap() else {
                panic!("not allowed")
            };
            keys.push((id, key));
        }
        assert!(revoke(home.path(), &keys[0].0).unwrap());
        assert_eq!(known(home.path(), &keys[0].1, at(1)), None);
        assert_eq!(
            known(home.path(), &keys[1].1, at(1)),
            Some(keys[1].0.clone())
        );
        forget_all(home.path()).unwrap();
        assert_eq!(known(home.path(), &keys[1].1, at(1)), None);
    }

    #[test]
    fn requests_are_few_and_names_one_short_line() {
        let home = tempfile::tempdir().unwrap();
        for _ in 0..10 {
            ask(home.path(), "x", at(0)).unwrap();
        }
        assert_eq!(asking(home.path(), at(0)).len(), MAX_ASKING);
        assert_eq!(clean("  \n\u{1b}[31m "), "[31m");
        assert_eq!(clean(""), "A device");
        assert_eq!(clean(&"a".repeat(100)).len(), 40);
    }
}
