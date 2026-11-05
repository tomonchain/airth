//! A business's bank, read-only (decision 54): Revolut Business, scope READ.
//!
//! Airth signs in with a client assertion, a JWT signed RS256 by the
//! business's private key, and trades the founder's consent for an access
//! token (40 minutes) and a refresh token. The key and the tokens are files
//! of the realm's Airth home (`skills/mcp-auth/revolut.pk8` and
//! `revolut.json`, mode 600): never in `secrets.env`, never in a command's
//! environment, never readable by an agent, a routine or a model. Airth
//! never moves money: the READ scope cannot.

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Revolut Business's API and consent page.
pub const API: &str = "https://b2b.revolut.com/api/1.0";
pub const CONSENT: &str = "https://business.revolut.com/app-confirm";

/// What Airth keeps to read the business's accounts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Access {
    pub client_id: String,
    pub redirect_uri: String,
    /// Where the API answers: Revolut's, or a test server's.
    pub api: String,
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub refresh_token: String,
}

/// The consent is gone (expired or revoked): only the founder can give it
/// again, with `airth bank connect`.
#[derive(Debug)]
pub struct Reauthorize(pub String);

impl std::fmt::Display for Reauthorize {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Revolut access needs re-authorizing: {}", self.0)
    }
}

impl std::error::Error for Reauthorize {}

/// Revolut refused a request for another reason than the consent: an
/// ordinary incident, with Revolut's own message.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Revolut refused the request: {}", self.0)
    }
}

impl std::error::Error for Refused {}

/// Revolut's own words in a refusal, in one line.
fn said(status: u16, text: &str) -> (Option<String>, String) {
    let body: Value = serde_json::from_str(text).unwrap_or_default();
    let error = body["error"].as_str().map(str::to_string);
    let words = ["error_description", "message", "error"]
        .iter()
        .find_map(|k| body[*k].as_str())
        .map_or_else(|| text.trim().chars().take(200).collect(), str::to_string);
    (error, format!("HTTP {status}: {words}"))
}

/// What a refusal means: the consent gone (401, 403, or `invalid_grant`),
/// or an ordinary refusal.
fn refusal(status: u16, text: &str) -> anyhow::Error {
    let (error, words) = said(status, text);
    match (status, error.as_deref()) {
        (401 | 403, _) | (_, Some("invalid_grant")) => Reauthorize(words).into(),
        _ => Refused(words).into(),
    }
}

pub fn key_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join("mcp-auth").join("revolut.pk8")
}

fn access_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join("mcp-auth").join("revolut.json")
}

/// Whether the business has a bank connected.
pub fn connected(skills_dir: &Path) -> bool {
    access_path(skills_dir).exists()
}

/// The page where the founder consents to read access.
pub fn consent_url(consent: &str, client_id: &str, redirect_uri: &str) -> Result<String> {
    let mut url = Url::parse(consent)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "READ");
    Ok(url.into())
}

/// The code in the address the founder landed on, which must be the
/// redirect address declared to Revolut.
pub fn code_from(pasted: &str, redirect_uri: &str) -> Result<String> {
    let landed = Url::parse(pasted.trim()).context("paste the whole address, starting https://")?;
    let expected = Url::parse(redirect_uri)?;
    if landed.origin() != expected.origin() || landed.path() != expected.path() {
        bail!("this address is not {redirect_uri}: paste the one Revolut sent you to");
    }
    let param = |name: &str| {
        landed
            .query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(error) = param("error") {
        bail!("Revolut refused the consent: {error}");
    }
    param("code").context("no code in this address: consent again and paste the new one")
}

/// The JWT that proves the request comes from the business: signed RS256
/// by its private key, issued by the redirect address's domain.
fn assertion(
    key: &[u8],
    client_id: &str,
    redirect_uri: &str,
    now: DateTime<Utc>,
) -> Result<String> {
    let issuer = Url::parse(redirect_uri)?
        .host_str()
        .context("the redirect address has no domain")?
        .to_string();
    let b64 = |v: &Value| crate::tools::remote::base64url(v.to_string().as_bytes());
    let header = b64(&json!({"alg": "RS256", "typ": "JWT"}));
    let claims = b64(&json!({
        "iss": issuer,
        "sub": client_id,
        "aud": "https://revolut.com",
        "exp": (now + Duration::minutes(5)).timestamp(),
    }));
    let signed = format!("{header}.{claims}");
    let pair = aws_lc_rs::signature::RsaKeyPair::from_pkcs8(key)
        .map_err(|e| anyhow!("the key is not an RSA key in PKCS#8 DER ({e}): convert it with openssl pkcs8 -topk8 -nocrypt -outform DER"))?;
    let mut signature = vec![0; pair.public_modulus_len()];
    pair.sign(
        &aws_lc_rs::signature::RSA_PKCS1_SHA256,
        &aws_lc_rs::rand::SystemRandom::new(),
        signed.as_bytes(),
        &mut signature,
    )
    .map_err(|_| anyhow!("signing the client assertion failed"))?;
    Ok(format!(
        "{signed}.{}",
        crate::tools::remote::base64url(&signature)
    ))
}

/// `form` as an `application/x-www-form-urlencoded` body.
fn urlencoded(form: &[(&str, &str)]) -> String {
    let mut url = Url::parse("http://form").expect("a fixed address");
    url.query_pairs_mut().extend_pairs(form);
    url.query().unwrap_or_default().to_string()
}

/// Ask Revolut's token endpoint; a refused answer is told apart, since it
/// means the consent is gone.
fn token(api: &str, form: &[(&str, &str)]) -> Result<Value> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .post(format!("{api}/auth/token"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(urlencoded(form))
        .send()
        .context("Revolut did not answer")?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    match status.as_u16() {
        200..=299 => Ok(serde_json::from_str(&text).context("Revolut's token answer is not JSON")?),
        400..=499 => Err(refusal(status.as_u16(), &text)),
        _ => bail!("Revolut's token endpoint answered HTTP {status}"),
    }
}

/// Write `bytes` readable by the founder only.
fn write_private_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().context("no folder")?)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)?;
    private(path)
}

/// Readable by the founder only, even when the file was there before:
/// `OpenOptions::mode` only applies to a file it creates.
fn private(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn keep(skills_dir: &Path, access: &Access) -> Result<()> {
    let path = access_path(skills_dir);
    crate::realm::write_private(&path, &serde_json::to_string_pretty(access)?)?;
    private(&path)
}

/// Connect the business's Revolut: keep its key, trade the consent's code
/// for tokens, and keep them, all readable by the founder only.
pub fn connect(
    skills_dir: &Path,
    api: &str,
    client_id: &str,
    redirect_uri: &str,
    key: &[u8],
    pasted: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let code = code_from(pasted, redirect_uri)?;
    let jwt = assertion(key, client_id, redirect_uri, now)?;
    let answer = token(
        api,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", client_id),
            (
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            ),
            ("client_assertion", &jwt),
        ],
    )
    .context("trading the consent for tokens")?;
    let access = Access {
        client_id: client_id.into(),
        redirect_uri: redirect_uri.into(),
        api: api.into(),
        access_token: answer["access_token"]
            .as_str()
            .context("no access token")?
            .into(),
        expires_at: now + Duration::seconds(answer["expires_in"].as_i64().unwrap_or(2400)),
        refresh_token: answer["refresh_token"]
            .as_str()
            .context("no refresh token")?
            .into(),
    };
    write_private_bytes(&key_path(skills_dir), key)?;
    keep(skills_dir, &access)
}

/// A valid access token, refreshed when it has less than a minute left.
/// A refused refresh is a `Reauthorize` error.
pub fn access_token(skills_dir: &Path, now: DateTime<Utc>) -> Result<(String, String)> {
    let text = std::fs::read_to_string(access_path(skills_dir)).context("no bank connected")?;
    let mut access: Access = serde_json::from_str(&text)?;
    if access.expires_at - now > Duration::minutes(1) {
        return Ok((access.api, access.access_token));
    }
    let key =
        std::fs::read(key_path(skills_dir)).context("the bank's key is gone: connect it again")?;
    let jwt = assertion(&key, &access.client_id, &access.redirect_uri, now)?;
    let answer = token(
        &access.api,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &access.refresh_token),
            ("client_id", &access.client_id),
            (
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            ),
            ("client_assertion", &jwt),
        ],
    )?;
    access.access_token = answer["access_token"]
        .as_str()
        .context("no access token")?
        .into();
    access.expires_at = now + Duration::seconds(answer["expires_in"].as_i64().unwrap_or(2400));
    if let Some(refresh) = answer["refresh_token"].as_str() {
        access.refresh_token = refresh.into();
    }
    keep(skills_dir, &access)?;
    Ok((access.api, access.access_token))
}

/// What the HQ shows of a bank (decision 53), in the business's main
/// currency: cash, money in and out this month, and runway in months
/// (None when nothing went out in three months).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finance {
    pub currency: String,
    pub cash: f64,
    pub money_in: f64,
    pub money_out: f64,
    pub runway_months: Option<f64>,
    pub read_at: DateTime<Utc>,
    /// What went out each day of the last three months, in the main
    /// currency, by date (`YYYY-MM-DD`): the Finance page's chart.
    #[serde(default)]
    pub out_by_day: std::collections::BTreeMap<String, f64>,
}

/// Read the business's accounts and the last three months' transactions,
/// read-only. A refused token is a `Reauthorize` error.
pub fn finance(skills_dir: &Path, now: DateTime<Utc>) -> Result<Finance> {
    let (api, token) = access_token(skills_dir, now)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let get = |path: &str| -> Result<Value> {
        let response = client
            .get(format!("{api}{path}"))
            .bearer_auth(&token)
            .send()
            .context("Revolut did not answer")?;
        match response.status().as_u16() {
            200..=299 => Ok(response.json()?),
            code @ 400..=499 => Err(refusal(code, &response.text().unwrap_or_default())),
            code => bail!("Revolut answered HTTP {code} to {path}"),
        }
    };
    let accounts = get("/accounts")?;
    let from = (now - Duration::days(90)).format("%Y-%m-%d");
    let transactions = get(&format!("/transactions?from={from}&count=1000"))?;
    Ok(compute(&accounts, &transactions, now))
}

/// The figures from Revolut's answers. The main currency is the one most
/// cash is in; exchanges and moves between the business's own accounts are
/// neither in nor out.
fn compute(accounts: &Value, transactions: &Value, now: DateTime<Utc>) -> Finance {
    let mut by_currency: std::collections::BTreeMap<String, f64> = Default::default();
    for a in accounts.as_array().into_iter().flatten() {
        if a["state"] == "active" {
            let currency = a["currency"].as_str().unwrap_or_default().to_string();
            *by_currency.entry(currency).or_default() += a["balance"].as_f64().unwrap_or(0.0);
        }
    }
    let (currency, cash) = by_currency
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or_default();
    let month = now.format("%Y-%m").to_string();
    let (mut money_in, mut money_out, mut out_90) = (0.0, 0.0, 0.0);
    let mut out_by_day = std::collections::BTreeMap::new();
    for t in transactions.as_array().into_iter().flatten() {
        if t["state"] != "completed" || t["type"] == "exchange" {
            continue;
        }
        let this_month = t["created_at"]
            .as_str()
            .is_some_and(|c| c.starts_with(&month));
        for leg in t["legs"].as_array().into_iter().flatten() {
            if leg["currency"] != currency.as_str() || leg["counterparty"]["account_type"] == "self"
            {
                continue;
            }
            let amount = leg["amount"].as_f64().unwrap_or(0.0);
            if amount < 0.0 {
                out_90 -= amount;
                if let Some(day) = t["created_at"].as_str().and_then(|c| c.get(..10)) {
                    *out_by_day.entry(day.to_string()).or_insert(0.0) -= amount;
                }
            }
            match (this_month, amount < 0.0) {
                (true, true) => money_out -= amount,
                (true, false) => money_in += amount,
                _ => {}
            }
        }
    }
    let monthly_out = out_90 / 3.0;
    Finance {
        currency,
        cash,
        money_in,
        money_out,
        runway_months: (monthly_out > 0.0).then(|| cash / monthly_out),
        read_at: now,
        out_by_day,
    }
}

/// Forget the business's bank: its key and tokens.
pub fn disconnect(skills_dir: &Path) {
    let _ = std::fs::remove_file(access_path(skills_dir));
    let _ = std::fs::remove_file(key_path(skills_dir));
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::encoding::AsDer;
    use std::sync::{Arc, Mutex};

    fn key() -> (Vec<u8>, Vec<u8>) {
        let pair = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
        let der: aws_lc_rs::encoding::Pkcs8V1Der = pair.as_der().unwrap();
        let public = aws_lc_rs::signature::KeyPair::public_key(&pair)
            .as_ref()
            .to_vec();
        (der.as_ref().to_vec(), public)
    }

    /// A token endpoint that answers each form with the next status and
    /// keeps what it was sent.
    fn revolut(answers: Vec<(u16, Value)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let api = format!("http://{}", server.server_addr().to_ip().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = seen.clone();
        std::thread::spawn(move || {
            let mut answers = answers.into_iter();
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                kept.lock().unwrap().push(format!("{} {body}", req.url()));
                let (code, answer) = answers.next().unwrap_or((500, json!({})));
                req.respond(
                    tiny_http::Response::from_string(answer.to_string()).with_status_code(code),
                )
                .unwrap();
            }
        });
        (api, seen)
    }

    const REDIRECT: &str = "https://hedgen.xyz/airth/revolut";

    /// Base64url without padding, back to bytes.
    fn unbase64url(s: &str) -> Vec<u8> {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut bits = 0u32;
        let mut n = 0;
        let mut out = Vec::new();
        for c in s.bytes() {
            bits = (bits << 6) | T.iter().position(|t| *t == c).unwrap() as u32;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push((bits >> n) as u8);
            }
        }
        out
    }

    /// The consent asks READ only; the code is taken only from the declared
    /// redirect address.
    #[test]
    fn consent_asks_read_only_and_the_code_comes_from_the_redirect() {
        let url = consent_url(CONSENT, "cid", REDIRECT).unwrap();
        assert!(url.starts_with("https://business.revolut.com/app-confirm?"));
        let query: Vec<(String, String)> = Url::parse(&url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        let param = |k: &str| query.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        // Read access only: nothing that can move money.
        assert_eq!(param("scope"), Some("READ"));
        assert_eq!(
            (param("client_id"), param("response_type")),
            (Some("cid"), Some("code"))
        );
        assert!(url.contains("redirect_uri=https%3A%2F%2Fhedgen.xyz%2Fairth%2Frevolut"));
        assert_eq!(
            code_from(&format!("{REDIRECT}?code=oa_abc"), REDIRECT).unwrap(),
            "oa_abc"
        );
        for wrong in [
            "https://evil.com/airth/revolut?code=x",
            "https://hedgen.xyz/other?code=x",
            "hedgen.xyz?code=x",
        ] {
            assert!(code_from(wrong, REDIRECT).is_err(), "{wrong}");
        }
        assert!(
            format!(
                "{:#}",
                code_from(&format!("{REDIRECT}?error=access_denied"), REDIRECT).unwrap_err()
            )
            .contains("access_denied")
        );
    }

    /// The client assertion is a JWT signed RS256 by the business's key,
    /// issued by the redirect address's domain for its client id.
    #[test]
    fn the_client_assertion_is_signed_rs256_by_the_key() {
        let (key, public) = key();
        let jwt = assertion(&key, "cid", REDIRECT, Utc::now()).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        let decode = |s: &str| -> Value { serde_json::from_slice(&unbase64url(s)).unwrap() };
        assert_eq!(decode(parts[0])["alg"], "RS256");
        let claims = decode(parts[1]);
        assert_eq!(
            (
                claims["iss"].as_str(),
                claims["sub"].as_str(),
                claims["aud"].as_str()
            ),
            (Some("hedgen.xyz"), Some("cid"), Some("https://revolut.com"))
        );
        let signature = unbase64url(parts[2]);
        aws_lc_rs::signature::UnparsedPublicKey::new(
            &aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256,
            &public,
        )
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .expect("signed by the business's key");
        assert!(assertion(b"not a key", "cid", REDIRECT, Utc::now()).is_err());
    }

    /// Files already there, readable by others, are made the founder's
    /// only: the mode is set after writing, not only at creation.
    #[test]
    #[cfg(unix)]
    fn existing_files_are_made_private_on_connect() {
        use std::os::unix::fs::PermissionsExt;
        let (key, _) = key();
        let dir = tempfile::tempdir().unwrap();
        let skills = dir.path();
        std::fs::create_dir_all(skills.join("mcp-auth")).unwrap();
        for file in [key_path(skills), access_path(skills)] {
            std::fs::write(&file, "old").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let (api, _) = revolut(vec![(
            200,
            json!({"access_token": "a1", "expires_in": 2400, "refresh_token": "r1"}),
        )]);
        connect(
            skills,
            &api,
            "cid",
            REDIRECT,
            &key,
            &format!("{REDIRECT}?code=oa_1"),
            Utc::now(),
        )
        .unwrap();
        for file in [key_path(skills), access_path(skills)] {
            let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{}", file.display());
        }
    }

    /// Re-authorization only on 401, 403 or `invalid_grant`; any other
    /// refusal carries Revolut's message and is not mistaken for it.
    #[test]
    fn only_a_lost_consent_asks_for_re_authorization() {
        let lost = |status: u16, body: Value| {
            refusal(status, &body.to_string())
                .downcast_ref::<Reauthorize>()
                .is_some()
        };
        assert!(lost(401, json!({})));
        assert!(lost(403, json!({"message": "forbidden"})));
        assert!(lost(400, json!({"error": "invalid_grant"})));
        let other = refusal(
            400,
            &json!({"error": "invalid_request", "error_description": "client_assertion expired"})
                .to_string(),
        );
        assert!(other.downcast_ref::<Reauthorize>().is_none());
        assert_eq!(
            format!("{other}"),
            "Revolut refused the request: HTTP 400: client_assertion expired"
        );
        assert!(refusal(404, "not json").downcast_ref::<Refused>().is_some());
    }

    /// Cash in the main currency; this month's money in and out, without
    /// exchanges, moves between own accounts or pending ones; runway from
    /// the last three months' spending.
    #[test]
    fn the_figures_come_from_accounts_and_this_months_transactions() {
        let now: DateTime<Utc> = "2026-11-20T10:00:00Z".parse().unwrap();
        let accounts = json!([
            {"id": "a", "balance": 12000.0, "currency": "GBP", "state": "active"},
            {"id": "b", "balance": 300.0, "currency": "GBP", "state": "active"},
            {"id": "c", "balance": 500.0, "currency": "EUR", "state": "active"},
            {"id": "d", "balance": 99999.0, "currency": "USD", "state": "inactive"}
        ]);
        let tx = |at: &str, kind: &str, state: &str, amount: f64, currency: &str, own: bool| {
            json!({"created_at": at, "type": kind, "state": state, "legs": [{"amount": amount, "currency": currency,
                "counterparty": {"account_type": if own { "self" } else { "external" }}}]})
        };
        let transactions = json!([
            tx(
                "2026-11-03T09:00:00Z",
                "card_payment",
                "completed",
                -420.0,
                "GBP",
                false
            ),
            tx(
                "2026-11-04T09:00:00Z",
                "transfer",
                "completed",
                1000.0,
                "GBP",
                false
            ),
            tx(
                "2026-11-05T09:00:00Z",
                "transfer",
                "completed",
                -5000.0,
                "GBP",
                true
            ),
            tx(
                "2026-11-06T09:00:00Z",
                "exchange",
                "completed",
                -200.0,
                "GBP",
                false
            ),
            tx(
                "2026-11-07T09:00:00Z",
                "card_payment",
                "pending",
                -50.0,
                "GBP",
                false
            ),
            tx(
                "2026-11-08T09:00:00Z",
                "card_payment",
                "completed",
                -30.0,
                "EUR",
                false
            ),
            tx(
                "2026-10-10T09:00:00Z",
                "card_payment",
                "completed",
                -780.0,
                "GBP",
                false
            ),
            tx(
                "2026-09-10T09:00:00Z",
                "card_payment",
                "completed",
                -300.0,
                "GBP",
                false
            )
        ]);
        let f = compute(&accounts, &transactions, now);
        assert_eq!((f.currency.as_str(), f.cash), ("GBP", 12300.0));
        assert_eq!((f.money_in, f.money_out), (1000.0, 420.0));
        // 1,500 out in three months: 500 a month, 24.6 months of runway.
        assert_eq!(
            f.runway_months.map(|m| (m * 10.0).round() / 10.0),
            Some(24.6)
        );
        // By day: what went out to others, in the main currency, completed.
        let days: Vec<_> = f.out_by_day.iter().map(|(d, a)| (d.as_str(), *a)).collect();
        assert_eq!(
            days,
            [
                ("2026-09-10", 300.0),
                ("2026-10-10", 780.0),
                ("2026-11-03", 420.0)
            ]
        );
        let quiet = compute(&accounts, &json!([]), now);
        assert_eq!(quiet.runway_months, None);
    }

    /// Reading uses the token and asks only for accounts and transactions;
    /// a refused token means the consent must be given again.
    #[test]
    fn reading_the_bank_asks_accounts_and_transactions_with_the_token() {
        let (key, _) = key();
        let dir = tempfile::tempdir().unwrap();
        let (api, seen) = revolut(vec![
            (
                200,
                json!({"access_token": "a1", "expires_in": 2400, "refresh_token": "r1"}),
            ),
            (
                200,
                json!([{"balance": 10.0, "currency": "GBP", "state": "active"}]),
            ),
            (200, json!([])),
            (401, json!({"message": "expired"})),
        ]);
        let now = Utc::now();
        connect(
            dir.path(),
            &api,
            "cid",
            REDIRECT,
            &key,
            &format!("{REDIRECT}?code=oa_1"),
            now,
        )
        .unwrap();
        let f = finance(dir.path(), now).unwrap();
        assert_eq!(f.cash, 10.0);
        let seen = seen.lock().unwrap().clone();
        assert!(seen[1].starts_with("/accounts"), "{seen:?}");
        assert!(seen[2].starts_with("/transactions?from="), "{seen:?}");
        let e = finance(dir.path(), now).unwrap_err();
        assert!(e.downcast_ref::<Reauthorize>().is_some(), "{e:#}");
    }

    /// Connecting keeps the key and tokens readable by the founder only; an
    /// expired access token is refreshed with a new assertion; a refused
    /// refresh means the consent must be given again.
    #[test]
    fn tokens_are_kept_private_refreshed_and_a_refusal_asks_for_consent() {
        let (key, _) = key();
        let dir = tempfile::tempdir().unwrap();
        let skills = dir.path();
        let (api, seen) = revolut(vec![
            (
                200,
                json!({"access_token": "a1", "expires_in": 2400, "refresh_token": "r1"}),
            ),
            (200, json!({"access_token": "a2", "expires_in": 2400})),
            (400, json!({"error": "invalid_grant"})),
        ]);
        let now = Utc::now();
        connect(
            skills,
            &api,
            "cid",
            REDIRECT,
            &key,
            &format!("{REDIRECT}?code=oa_1"),
            now,
        )
        .unwrap();
        #[cfg(unix)]
        for file in [key_path(skills), access_path(skills)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600,
                "{}",
                file.display()
            );
        }
        assert!(
            seen.lock().unwrap()[0].contains("grant_type=authorization_code")
                && seen.lock().unwrap()[0].contains("code=oa_1")
        );
        assert!(seen.lock().unwrap()[0].contains(
            "client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
        ));
        // Still valid: no call.
        assert_eq!(access_token(skills, now).unwrap().1, "a1");
        assert_eq!(seen.lock().unwrap().len(), 1);
        // Expired: refreshed, the refresh token kept.
        let later = now + Duration::minutes(45);
        assert_eq!(access_token(skills, later).unwrap().1, "a2");
        assert!(
            seen.lock().unwrap()[1].contains("grant_type=refresh_token")
                && seen.lock().unwrap()[1].contains("refresh_token=r1")
        );
        // Refused: the consent is gone.
        let e = access_token(skills, later + Duration::minutes(45)).unwrap_err();
        assert!(e.downcast_ref::<Reauthorize>().is_some(), "{e:#}");
        assert!(format!("{e}").starts_with("Revolut access needs re-authorizing"));
        disconnect(skills);
        assert!(!connected(skills) && !key_path(skills).exists());
    }
}
