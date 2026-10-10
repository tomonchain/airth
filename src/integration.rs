//! Integrations: services a realm's runs may call over HTTP, with credentials
//! the owner bound to them.
//!
//! An integration is a base URL plus headers added to every request under it,
//! such as `Authorization: Bearer [secret:GITHUB_TOKEN]`. The runtime fills in
//! the secret when it sends the request; the model only names the integration.
//! A credential can therefore only ever go to the base URL the owner chose.

use crate::realm::validate_name;
use anyhow::{Context, Result, bail};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integration {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Requests through this integration must stay under this URL.
    pub base_url: String,
    /// Added to every request; `[secret:NAME]` is replaced by the realm's
    /// credential NAME when the request is sent.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl Integration {
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name).context("invalid integration name")?;
        self.base()?;
        for (name, value) in &self.headers {
            reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .with_context(|| format!("invalid header name '{name}'"))?;
            if value.contains(['\n', '\r']) {
                bail!("header '{name}' must fit on one line");
            }
            for secret in secret_refs(value)? {
                if secret.is_empty() {
                    bail!("header '{name}' has an empty [secret:] reference");
                }
            }
        }
        Ok(())
    }

    pub fn base(&self) -> Result<Url> {
        let url = Url::parse(&self.base_url)
            .with_context(|| format!("invalid base URL '{}'", self.base_url))?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            bail!("the base URL must be http(s)://host/...");
        }
        if url.query().is_some() || url.fragment().is_some() || !url.username().is_empty() {
            bail!("the base URL cannot have a query, fragment or user");
        }
        Ok(url)
    }

    /// The URL for `target`, a path relative to the base (`repos/x/y`, or
    /// `/repos/x/y`) or an absolute URL. It must stay under the base URL.
    pub fn url_for(&self, target: &str) -> Result<Url> {
        let base = self.base()?;
        let mut dir = base.clone();
        if !dir.path().ends_with('/') {
            dir.set_path(&format!("{}/", dir.path()));
        }
        let url = if target.contains("://") {
            Url::parse(target).with_context(|| format!("invalid URL '{target}'"))?
        } else {
            dir.join(target.trim_start_matches('/'))
                .with_context(|| format!("invalid path '{target}'"))?
        };
        let lower = url.path().to_ascii_lowercase();
        let under = url.scheme() == base.scheme()
            && url.host() == base.host()
            && url.port_or_known_default() == base.port_or_known_default()
            && url.username().is_empty()
            && (url.path() == base.path() || url.path().starts_with(dir.path()))
            && !lower.contains("%2e")
            && !lower.contains("%2f");
        if !under {
            bail!(
                "{url} is outside integration '{}' ({})",
                self.name,
                self.base_url
            );
        }
        Ok(url)
    }

    /// The credentials this integration's headers use.
    pub fn secret_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .headers
            .values()
            .flat_map(|v| secret_refs(v).unwrap_or_default())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The headers to send, with credentials filled in by `secret`.
    pub fn resolve_headers(
        &self,
        secret: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Vec<(String, String)>> {
        self.headers
            .iter()
            .map(|(name, template)| {
                let mut value = template.clone();
                for key in secret_refs(template)? {
                    let Some(v) = secret(&key) else {
                        bail!(
                            "integration '{}' needs the credential {key}, which this realm does not have",
                            self.name
                        );
                    };
                    value = value.replace(&format!("[secret:{key}]"), &v);
                }
                Ok((name.clone(), value))
            })
            .collect()
    }
}

/// The NAMEs in every `[secret:NAME]` of `text`.
fn secret_refs(text: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[secret:") {
        let after = &rest[start + 8..];
        let Some(end) = after.find(']') else {
            bail!("unterminated [secret:...] in '{text}'");
        };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github() -> Integration {
        Integration {
            name: "github".into(),
            description: String::new(),
            base_url: "https://api.github.com/repos/me".into(),
            headers: BTreeMap::from([(
                "Authorization".into(),
                "Bearer [secret:GITHUB_TOKEN]".into(),
            )]),
        }
    }

    #[test]
    fn urls_stay_under_the_base() {
        let g = github();
        g.validate().unwrap();
        assert_eq!(
            g.url_for("airth/issues?state=open").unwrap().as_str(),
            "https://api.github.com/repos/me/airth/issues?state=open"
        );
        assert!(g.url_for("/airth").is_ok());
        assert!(g.url_for("https://api.github.com/repos/me/x").is_ok());
        assert!(g.url_for("https://api.github.com/repos/me").is_ok());
        for bad in [
            "../other/x",
            "https://api.github.com/repos/meow",
            "https://api.github.com/user",
            "https://evil.com/repos/me/x",
            "http://api.github.com/repos/me/x",
            "https://api.github.com:8443/repos/me/x",
            "https://x@api.github.com/repos/me/x",
            "%2e%2e/other",
            "a%2F..%2F..%2Fuser",
        ] {
            assert!(g.url_for(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn fills_in_credentials() {
        let g = github();
        assert_eq!(g.secret_names(), vec!["GITHUB_TOKEN"]);
        let headers = g
            .resolve_headers(&|k| (k == "GITHUB_TOKEN").then(|| "ghp_x".to_string()))
            .unwrap();
        assert_eq!(
            headers,
            vec![("Authorization".to_string(), "Bearer ghp_x".to_string())]
        );
        assert!(g.resolve_headers(&|_| None).is_err());
    }

    #[test]
    fn rejects_bad_definitions() {
        let mut g = github();
        g.base_url = "ftp://x".into();
        assert!(g.validate().is_err());
        let mut g = github();
        g.headers.insert("Bad Header".into(), "x".into());
        assert!(g.validate().is_err());
        let mut g = github();
        g.headers.insert("X".into(), "[secret:A".into());
        assert!(g.validate().is_err());
        let mut g = github();
        g.name = "No".into();
        assert!(g.validate().is_err());
    }
}
