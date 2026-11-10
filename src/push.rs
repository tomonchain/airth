//! Notifications on the founder's phone (decision 117): what waits in Needs
//! you, and what failed, sent as Web Push to the Airth app on the phone's
//! home screen, as Telegram sends it to the bot.
//!
//! The push service (Apple's or Google's) only carries the message: it is
//! encrypted for the phone (RFC 8291) and signed by Airth (VAPID, RFC 8292),
//! with the cryptography Airth already ships (aws-lc-rs). The keys and the
//! phones' subscriptions stay in the Airth home, readable by the founder only.

use crate::business::store::Store;
use crate::home::Home;
use crate::tools::remote::base64url;
use anyhow::{Context, Result, bail};
use aws_lc_rs::{aead, agreement, hkdf, rand, signature};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;

const KEY: &str = "push-key.pk8";
const FILE: &str = "push.json";
/// The most subscriptions kept: a few phones and browsers, not a crowd.
const MAX_SUBSCRIPTIONS: usize = 8;

/// A phone (or browser) that asked for notifications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    /// The phone's public key (P-256, uncompressed), base64url.
    pub p256dh: String,
    /// The phone's 16-byte secret, base64url.
    pub auth: String,
    pub added: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
    subscriptions: Vec<Subscription>,
    /// What was pushed already (`realm/id` for an item, `device/id` for a
    /// device asking for access), so nothing is pushed twice.
    sent: BTreeSet<String>,
    /// The notifications already looked at: only newer ones are pushed.
    notices_seen: u64,
}

fn load(home: &Home) -> State {
    std::fs::read_to_string(home.dir().join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(home: &Home, state: &State) -> Result<()> {
    crate::realm::write_private(
        &home.dir().join(FILE),
        &serde_json::to_string_pretty(state)?,
    )
}

/// Airth's signing key for push (VAPID), made once and kept private.
fn signing_key(home: &Home) -> Result<signature::EcdsaKeyPair> {
    let path = home.dir().join(KEY);
    let alg = &signature::ECDSA_P256_SHA256_FIXED_SIGNING;
    if let Ok(text) = std::fs::read_to_string(&path) {
        let der = decode(text.trim()).context("the push key is not base64url")?;
        return signature::EcdsaKeyPair::from_pkcs8(alg, &der)
            .map_err(|_| anyhow::anyhow!("{KEY} is not a key Airth can read"));
    }
    let der = signature::EcdsaKeyPair::generate_pkcs8(alg, &rand::SystemRandom::new())
        .map_err(|_| anyhow::anyhow!("cannot make a push key"))?;
    crate::realm::write_private(&path, &base64url(der.as_ref()))?;
    signature::EcdsaKeyPair::from_pkcs8(alg, der.as_ref())
        .map_err(|_| anyhow::anyhow!("cannot read the push key just made"))
}

/// The public key a phone subscribes with (base64url, uncompressed P-256).
pub fn public_key(home: &Home) -> Result<String> {
    use signature::KeyPair;
    Ok(base64url(signing_key(home)?.public_key().as_ref()))
}

pub fn subscriptions(home: &Home) -> Vec<Subscription> {
    load(home).subscriptions
}

/// Keep a phone's subscription; the same endpoint again replaces it.
pub fn subscribe(home: &Home, endpoint: &str, p256dh: &str, auth: &str) -> Result<()> {
    let url = url::Url::parse(endpoint).context("not an address")?;
    anyhow::ensure!(url.scheme() == "https", "a push address is https");
    // A key that is not a point on the curve would fail every push: refused now.
    let point = decode(p256dh).filter(|k| k.len() == 65);
    let probe = agreement::PrivateKey::generate(&agreement::ECDH_P256)
        .map_err(|_| anyhow::anyhow!("cannot make a key"))?;
    anyhow::ensure!(
        point.is_some_and(|k| agreement::agree(
            &probe,
            agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, &k),
            aws_lc_rs::error::Unspecified,
            |_| Ok(()),
        )
        .is_ok()),
        "not a phone's public key"
    );
    anyhow::ensure!(
        decode(auth).map(|a| a.len()) == Some(16),
        "not a phone's secret"
    );
    let mut state = load(home);
    state.subscriptions.retain(|s| s.endpoint != endpoint);
    state.subscriptions.push(Subscription {
        endpoint: endpoint.to_string(),
        p256dh: p256dh.to_string(),
        auth: auth.to_string(),
        added: Utc::now().to_rfc3339(),
    });
    let extra = state.subscriptions.len().saturating_sub(MAX_SUBSCRIPTIONS);
    state.subscriptions.drain(..extra);
    // What already waits is not pushed to a phone that just subscribed.
    if state.notices_seen == 0 {
        state.notices_seen = home.inbox().list()?.iter().map(|n| n.id).max().unwrap_or(0);
    }
    save(home, &state)
}

pub fn unsubscribe(home: &Home, endpoint: &str) -> Result<bool> {
    let mut state = load(home);
    let before = state.subscriptions.len();
    state.subscriptions.retain(|s| s.endpoint != endpoint);
    save(home, &state)?;
    Ok(state.subscriptions.len() < before)
}

/// base64url, with or without padding, into bytes.
fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// An HKDF output length, for aws-lc-rs.
struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(prk: &hkdf::Prk, info: &[u8], len: usize) -> Result<Vec<u8>> {
    let mut out = vec![0; len];
    prk.expand(&[info], Len(len))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| anyhow::anyhow!("key derivation failed"))?;
    Ok(out)
}

/// `payload` encrypted for one phone (RFC 8291, `aes128gcm`), with the
/// sender's key pair `ours` (its private key and public key) and `salt`:
/// the body of the push request.
fn encrypt(
    payload: &[u8],
    ours: (&agreement::PrivateKey, &[u8]),
    phone_key: &[u8],
    auth: &[u8],
    salt: &[u8; 16],
) -> Result<Vec<u8>> {
    let (private, public) = ours;
    let shared = agreement::agree(
        private,
        agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, phone_key),
        aws_lc_rs::error::Unspecified,
        |s| Ok(s.to_vec()),
    )
    .map_err(|_| anyhow::anyhow!("the phone's key does not agree"))?;
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(phone_key);
    key_info.extend_from_slice(public);
    let prk_key = hkdf::Salt::new(hkdf::HKDF_SHA256, auth).extract(&shared);
    let ikm = expand(&prk_key, &key_info, 32)?;
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&ikm);
    let cek = expand(&prk, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = expand(&prk, b"Content-Encoding: nonce\0", 12)?;
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek)
            .map_err(|_| anyhow::anyhow!("bad content key"))?,
    );
    // One record: the payload, then the last record's delimiter.
    let mut record = payload.to_vec();
    record.push(2);
    key.seal_in_place_append_tag(
        aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| anyhow::anyhow!("bad nonce"))?,
        aead::Aad::empty(),
        &mut record,
    )
    .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let mut body = salt.to_vec();
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(public.len() as u8);
    body.extend_from_slice(public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// The VAPID header's token for the push service at `audience`.
fn vapid_token(key: &signature::EcdsaKeyPair, audience: &str, now: i64) -> Result<String> {
    let header = base64url(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = base64url(
        json!({"aud": audience, "exp": now + 12 * 3600, "sub": "https://github.com/tomonchain/airth"})
            .to_string()
            .as_bytes(),
    );
    let signed = format!("{header}.{claims}");
    let sig = key
        .sign(&rand::SystemRandom::new(), signed.as_bytes())
        .map_err(|_| anyhow::anyhow!("cannot sign the push token"))?;
    Ok(format!("{signed}.{}", base64url(sig.as_ref())))
}

/// What happened to one push.
enum Sent {
    Delivered,
    /// The phone is gone (unsubscribed or reinstalled): forget it.
    Gone,
}

/// Push `message` (JSON: title, body, url) to one phone.
fn send(home: &Home, sub: &Subscription, message: &Value) -> Result<Sent> {
    use signature::KeyPair;
    let key = signing_key(home)?;
    let phone_key = decode(&sub.p256dh).context("bad phone key")?;
    let auth = decode(&sub.auth).context("bad phone secret")?;
    let rng = rand::SystemRandom::new();
    let ours = agreement::PrivateKey::generate(&agreement::ECDH_P256)
        .map_err(|_| anyhow::anyhow!("cannot make a message key"))?;
    let ours_public = ours
        .compute_public_key()
        .map_err(|_| anyhow::anyhow!("cannot read the message key"))?;
    let mut salt = [0u8; 16];
    rand::SecureRandom::fill(&rng, &mut salt).map_err(|_| anyhow::anyhow!("no randomness"))?;
    let body = encrypt(
        message.to_string().as_bytes(),
        (&ours, ours_public.as_ref()),
        &phone_key,
        &auth,
        &salt,
    )?;
    let url = url::Url::parse(&sub.endpoint)?;
    let audience = url.origin().ascii_serialization();
    let token = vapid_token(&key, &audience, Utc::now().timestamp())?;
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?
        .post(url)
        .header(
            "Authorization",
            format!(
                "vapid t={token}, k={}",
                base64url(key.public_key().as_ref())
            ),
        )
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("TTL", "86400")
        .header("Urgency", "high")
        .body(body)
        .send()
        .context("the push service did not answer")?;
    match response.status().as_u16() {
        200..=299 => Ok(Sent::Delivered),
        404 | 410 => Ok(Sent::Gone),
        code => bail!(
            "the push service refused ({code}): {}",
            response.text().unwrap_or_default().trim()
        ),
    }
}

/// Push `message` to every phone; a phone that is gone is forgotten. How
/// many got it, and why the last one that failed did.
fn to_all(home: &Home, state: &mut State, message: &Value) -> (usize, Option<String>) {
    let mut delivered = 0;
    let mut gone = Vec::new();
    let mut failed = None;
    for sub in &state.subscriptions {
        match send(home, sub, message) {
            Ok(Sent::Delivered) => delivered += 1,
            Ok(Sent::Gone) => gone.push(sub.endpoint.clone()),
            Err(e) => {
                eprintln!("push: {e:#}");
                failed = Some(format!("{e:#}"));
            }
        }
    }
    state.subscriptions.retain(|s| !gone.contains(&s.endpoint));
    (delivered, failed)
}

/// A test notification to every phone: how many got it.
pub fn test(home: &Home) -> Result<usize> {
    let mut state = load(home);
    anyhow::ensure!(
        !state.subscriptions.is_empty(),
        "no phone asked for notifications yet"
    );
    let (n, failed) = to_all(
        home,
        &mut state,
        &json!({"title": "Airth", "body": "Notifications work on this phone.", "url": "/"}),
    );
    save(home, &state)?;
    match (n, failed) {
        (0, Some(why)) => bail!("the test did not go out: {why}"),
        (0, None) => bail!("the phone is gone: turn notifications on again on it"),
        _ => Ok(n),
    }
}

/// What one queue item asks, as Needs you shows it, in a line or two.
fn item_text(item: &crate::business::store::queue::Item) -> String {
    let path = match (item.source["show"].as_str(), item.kind.as_str()) {
        (Some(path), _) => path,
        (None, "incident") => "detail",
        _ => "",
    };
    let text = match item.args.as_object() {
        Some(args) if !path.is_empty() => crate::pipeline::run::field_text(args, path),
        _ => String::new(),
    };
    let text = if text.trim().is_empty() {
        item.title.clone()
    } else {
        text
    };
    text.chars().take(180).collect()
}

/// The notification for what just arrived in `realm`: the item itself when
/// it is alone, else one notification for all of them (a batch of drafts
/// is one tap, not six).
fn items_message(realm: &str, items: &[crate::business::store::queue::Item]) -> Value {
    let what = match items {
        [one] => match one.kind.as_str() {
            "incident" => "Incident".to_string(),
            "deadline" => "Legal deadline".to_string(),
            "prepared" => "To do yourself".to_string(),
            "delayed" => "Goes out unless you cancel".to_string(),
            "report" => "Report".to_string(),
            _ => "Needs you".to_string(),
        },
        many => format!("{} need you", many.len()),
    };
    let body = match items {
        [one] => item_text(one),
        [first, rest @ ..] => format!("{}\nand {} more", item_text(first), rest.len()),
        [] => String::new(),
    };
    json!({
        "title": format!("{realm} · {what}"),
        "body": body,
        "url": format!("/#/b/{realm}/needs"),
        "tag": format!("{realm}/{}", items.first().map_or(0, |i| i.id)),
    })
}

/// Push what is new since the last call: items waiting in each business,
/// then failures in the founder's notifications.
pub fn push_new(home: &Home) -> Result<usize> {
    let mut state = load(home);
    if state.subscriptions.is_empty() {
        return Ok(0);
    }
    let mut out = Vec::new();
    let mut waiting = BTreeSet::new();
    for realm in home.realms()? {
        if realm.config.archived || !realm.dir().join(crate::business::store::FILE).exists() {
            continue;
        }
        let mut new = Vec::new();
        for item in Store::of(&realm)?.queue(&["pending"])? {
            let key = format!("{}/{}", realm.name(), item.id);
            if !state.sent.contains(&key) {
                new.push(item);
            }
            waiting.insert(key);
        }
        if !new.is_empty() {
            let keys = new
                .iter()
                .map(|i| format!("{}/{}", realm.name(), i.id))
                .collect();
            out.push((keys, items_message(realm.name(), &new)));
        }
    }
    // A device asking for access (decision 118): approved in the app.
    for ask in crate::devices::asking(home.dir(), Utc::now()) {
        let key = format!("device/{}", ask.id);
        if !state.sent.contains(&key) {
            out.push((
                vec![key.clone()],
                json!({"title": "Airth · A device asks for access", "body": format!("{}, code {}. Allow it only if this code is on a device you hold.", ask.name, ask.code), "url": "/", "tag": key}),
            ));
        }
        waiting.insert(key);
    }
    let mut seen = state.notices_seen;
    for n in home.inbox().list()? {
        if n.id > state.notices_seen && n.kind == crate::notify::Kind::Failed {
            out.push((
                Vec::new(),
                json!({"title": format!("{} · {}", n.realm, n.title), "body": n.body.chars().take(180).collect::<String>(), "url": "/"}),
            ));
        }
        seen = seen.max(n.id);
    }
    state.notices_seen = seen;
    let mut delivered = 0;
    for (keys, message) in out {
        // Sent or not, an item is pushed once: a push service that is down
        // is not asked again every 15 seconds, and Needs you still shows it.
        delivered += to_all(home, &mut state, &message).0;
        state.sent.extend(keys);
    }
    // An item answered no longer needs remembering; a notice is passed by
    // `notices_seen`.
    state.sent.retain(|k| waiting.contains(k));
    save(home, &state)?;
    Ok(delivered)
}

/// The loop the service runs: every fifteen seconds, what is new goes to
/// the phones.
pub fn run(home: Home) {
    loop {
        if let Err(e) = push_new(&home) {
            eprintln!("push: {e:#}");
        }
        std::thread::sleep(Duration::from_secs(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        decode(s).unwrap()
    }

    /// RFC 8291, section 5: the example message, encrypted with the
    /// example's keys and salt, is the example's body, byte for byte.
    #[test]
    fn a_message_is_encrypted_as_rfc_8291_says() {
        let as_private = hex("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw");
        let as_public = hex(
            "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
        );
        let ua_public = hex(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        );
        let auth = hex("BTBZMqHH6r4Tts7J_aSIgg");
        let salt: [u8; 16] = hex("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let ours =
            agreement::PrivateKey::from_private_key(&agreement::ECDH_P256, &as_private).unwrap();
        let body = encrypt(
            b"When I grow up, I want to be a watermelon",
            (&ours, &as_public),
            &ua_public,
            &auth,
            &salt,
        )
        .unwrap();
        assert_eq!(
            base64url(&body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn the_push_token_is_signed_with_airths_key() {
        use signature::KeyPair;
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        let key = signing_key(&home).unwrap();
        // The key is kept: the same one comes back, and only the founder reads it.
        assert_eq!(
            public_key(&home).unwrap(),
            base64url(key.public_key().as_ref())
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(KEY))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
        let token = vapid_token(&key, "https://web.push.apple.com", 1_000).unwrap();
        let (signed, sig) = token.rsplit_once('.').unwrap();
        signature::UnparsedPublicKey::new(
            &signature::ECDSA_P256_SHA256_FIXED,
            key.public_key().as_ref(),
        )
        .verify(signed.as_bytes(), &decode(sig).unwrap())
        .unwrap();
        let claims: Value =
            serde_json::from_slice(&decode(signed.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://web.push.apple.com");
        assert_eq!(claims["exp"], 1_000 + 12 * 3600);
    }

    #[test]
    fn a_phone_subscribes_once_and_can_leave() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::at(dir.path()).unwrap();
        // RFC 8291's example phone key: a point on the curve.
        let key = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
        let auth = base64url(&[7u8; 16]);
        // 65 bytes, but not a point: refused.
        assert!(
            subscribe(
                &home,
                "https://push.example/1",
                &base64url(&[4u8; 65]),
                &auth
            )
            .is_err()
        );
        assert!(subscribe(&home, "http://push.example/1", key, &auth).is_err());
        assert!(subscribe(&home, "https://push.example/1", "short", &auth).is_err());
        subscribe(&home, "https://push.example/1", key, &auth).unwrap();
        subscribe(&home, "https://push.example/1", key, &auth).unwrap();
        assert_eq!(subscriptions(&home).len(), 1);
        assert!(unsubscribe(&home, "https://push.example/1").unwrap());
        assert!(subscriptions(&home).is_empty());
        // Nothing to push to: nothing is tried.
        assert_eq!(push_new(&home).unwrap(), 0);
    }

    fn item(id: i64, kind: &str, draft: &str) -> crate::business::store::queue::Item {
        crate::business::store::queue::Item {
            id,
            created_at: String::new(),
            kind: kind.into(),
            title: "Reply on X".into(),
            agent: None,
            routine: None,
            run: None,
            action: None,
            args: json!({"answer": {"draft": draft}}),
            original_args: Value::Null,
            source: json!({"show": "answer.draft"}),
            model_call: None,
            execute_at: None,
            status: "pending".into(),
            revision: 0,
            flag: None,
            flag_reason: None,
            note: None,
        }
    }

    #[test]
    fn a_batch_is_one_notification_and_a_lone_item_says_what_it_asks() {
        let one = items_message("hedgen", &[item(4, "approval", "Rates fell.")]);
        assert_eq!(one["title"], "hedgen · Needs you");
        assert_eq!(one["body"], "Rates fell.");
        assert_eq!(one["url"], "/#/b/hedgen/needs");
        let batch = items_message(
            "hedgen",
            &[
                item(5, "approval", "First."),
                item(6, "approval", "Second."),
                item(7, "approval", ""),
            ],
        );
        assert_eq!(batch["title"], "hedgen · 3 need you");
        assert_eq!(batch["body"], "First.\nand 2 more");
        // Nothing to show: the title stands in.
        assert_eq!(item_text(&item(8, "deadline", "")), "Reply on X");
    }
}
