//! Pull requests on GitHub, for realms that deliver changes that way: the
//! runtime pushes a chat's branch, opens its pull request, and follows it
//! until the owner merges or closes it.
//!
//! Like the rest of the git work (see isolation.rs), this runs in the
//! runtime, outside the agent's sandbox: the token never reaches the model or
//! the chat's commands.

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// The realm secret holding the token: a fine-grained token limited to the
/// repository, with Contents and Pull requests read and write.
pub const TOKEN: &str = "GITHUB_TOKEN";

/// A repository on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub owner: String,
    pub name: String,
}

impl Remote {
    /// `https://github.com/o/r(.git)`, `git@github.com:o/r(.git)` or
    /// `ssh://git@github.com/o/r(.git)`.
    pub fn parse(url: &str) -> Option<Remote> {
        let url = url.trim();
        let path = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("git@github.com:"))
            .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, name) = path.split_once('/')?;
        let ok = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        (ok(owner) && ok(name)).then(|| Remote {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    /// The GitHub repository behind `origin` in `repo`.
    pub fn of(repo: &Path) -> Result<Remote> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["remote", "get-url", "origin"])
            .output()
            .context("cannot run git")?;
        let url = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            bail!("the repository has no remote named origin");
        }
        Remote::parse(&url).context("the repository's origin is not on GitHub")
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

/// The checks (CI) on a pull request's latest commit.
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Checks {
    /// No check ran on it.
    None,
    Pending,
    Passing,
    Failing,
}

/// Where a pull request stands, for the app.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrStatus {
    pub state: PrState,
    pub title: String,
    pub url: String,
    /// Its latest commit.
    pub head: String,
    /// GitHub's word on merging it: clean, behind, dirty (conflicts),
    /// blocked (a rule not met yet), unstable, unknown (still computing).
    pub merge_state: String,
    pub checks: Checks,
    /// The checks that failed.
    pub failed: Vec<String>,
}

pub struct GitHub {
    token: String,
    api: String,
    git: String,
}

impl GitHub {
    pub fn new(token: &str) -> GitHub {
        GitHub {
            token: token.to_string(),
            api: "https://api.github.com".into(),
            git: "https://github.com".into(),
        }
    }

    /// A GitHub at other addresses (tests: a local API, a folder of bare repos).
    #[cfg(test)]
    pub fn at(token: &str, api: &str, git: &str) -> GitHub {
        GitHub {
            token: token.to_string(),
            api: api.to_string(),
            git: git.to_string(),
        }
    }

    fn url(&self, r: &Remote) -> String {
        format!("{}/{}/{}.git", self.git, r.owner, r.name)
    }

    /// Run git in `dir` with the token as an HTTP header (given in the
    /// environment, so it shows in no command line), and no prompt.
    fn git(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let header = format!(
            "AUTHORIZATION: basic {}",
            base64(format!("x-access-token:{}", self.token).as_bytes())
        );
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "core.hooksPath=/dev/null", "-c", "credential.helper="])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_COUNT", "1")
            .env(
                "GIT_CONFIG_KEY_0",
                format!("http.{}/.extraheader", self.git),
            )
            .env("GIT_CONFIG_VALUE_0", header)
            .output()
            .context("cannot run git")?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).replace(&self.token, "[token]");
            bail!("git {} failed: {}", args[0], err.trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }

    /// Fetch `branch` of the repository into `refs/remotes/origin/<branch>`
    /// of the checkout at `dir`; returns that ref.
    pub fn fetch(&self, dir: &Path, r: &Remote, branch: &str) -> Result<String> {
        let target = format!("refs/remotes/origin/{branch}");
        self.git(
            dir,
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                &self.url(r),
                &format!("+refs/heads/{branch}:{target}"),
            ],
        )
        .with_context(|| format!("cannot fetch '{branch}' from {}", r.slug()))?;
        Ok(target)
    }

    /// Push the worktree's HEAD to `branch` on GitHub.
    pub fn push(&self, worktree: &Path, r: &Remote, branch: &str) -> Result<()> {
        self.git(
            worktree,
            &[
                "push",
                "--quiet",
                &self.url(r),
                &format!("HEAD:refs/heads/{branch}"),
            ],
        )
        .with_context(|| format!("cannot push to {}", r.slug()))?;
        Ok(())
    }

    fn client(&self) -> Result<Client> {
        Ok(Client::builder()
            .user_agent(concat!("airth/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()?)
    }

    /// Call the API; a GitHub error comes back with its message.
    fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self
            .client()?
            .request(method, format!("{}{path}", self.api))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(body) = body {
            req = req.json(&body);
        }
        let res = req.send().context("cannot reach GitHub")?;
        let status = res.status();
        let text = res.text().unwrap_or_default();
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            let mut message = value["message"].as_str().unwrap_or("").to_string();
            if let Some(first) = value["errors"][0]["message"].as_str() {
                message = format!("{message}: {first}");
            }
            bail!("GitHub answered {}: {message}", status.as_u16());
        }
        Ok(value)
    }

    /// Open a pull request from `head` into `base`, or find the open one
    /// that already exists. Returns its number and web address.
    pub fn open(
        &self,
        r: &Remote,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<(u64, String)> {
        let pulls = format!("/repos/{}/pulls", r.slug());
        let created = self.call(
            reqwest::Method::POST,
            &pulls,
            Some(json!({"title": title, "head": head, "base": base, "body": body})),
        );
        let pr = match created {
            Ok(pr) => pr,
            Err(e) if e.to_string().contains("already exists") => {
                let found = self.call(
                    reqwest::Method::GET,
                    &format!("{pulls}?state=open&head={}:{head}", r.owner),
                    None,
                )?;
                found[0].clone()
            }
            Err(e) => return Err(e),
        };
        let number = pr["number"].as_u64().context("GitHub gave no number")?;
        let url = pr["html_url"].as_str().unwrap_or_default().to_string();
        Ok((number, url))
    }

    pub fn state(&self, r: &Remote, number: u64) -> Result<PrState> {
        let pr = self.call(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls/{number}", r.slug()),
            None,
        )?;
        Ok(if pr["merged"].as_bool() == Some(true) {
            PrState::Merged
        } else if pr["state"] == "closed" {
            PrState::Closed
        } else {
            PrState::Open
        })
    }

    /// The pull request and the checks on its latest commit.
    pub fn status(&self, r: &Remote, number: u64) -> Result<PrStatus> {
        let pr = self.call(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls/{number}", r.slug()),
            None,
        )?;
        let head = pr["head"]["sha"].as_str().unwrap_or_default().to_string();
        let runs = self.call(
            reqwest::Method::GET,
            &format!("/repos/{}/commits/{head}/check-runs?per_page=100", r.slug()),
            None,
        )?;
        let runs = runs["check_runs"].as_array().cloned().unwrap_or_default();
        let failed: Vec<String> = runs
            .iter()
            .filter(|c| {
                c["status"] == "completed"
                    && !matches!(
                        c["conclusion"].as_str(),
                        Some("success" | "neutral" | "skipped")
                    )
            })
            .filter_map(|c| c["name"].as_str().map(String::from))
            .collect();
        let checks = if runs.is_empty() {
            Checks::None
        } else if !failed.is_empty() {
            Checks::Failing
        } else if runs.iter().any(|c| c["status"] != "completed") {
            Checks::Pending
        } else {
            Checks::Passing
        };
        Ok(PrStatus {
            state: if pr["merged"].as_bool() == Some(true) {
                PrState::Merged
            } else if pr["state"] == "closed" {
                PrState::Closed
            } else {
                PrState::Open
            },
            title: pr["title"].as_str().unwrap_or_default().to_string(),
            url: pr["html_url"].as_str().unwrap_or_default().to_string(),
            head,
            merge_state: pr["mergeable_state"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            checks,
            failed,
        })
    }

    /// Squash-merge the pull request, if its head is still `sha`. The
    /// commit is titled like the pull request, so the release notes read well.
    pub fn merge(&self, r: &Remote, number: u64, title: &str, sha: &str) -> Result<()> {
        self.call(
            reqwest::Method::PUT,
            &format!("/repos/{}/pulls/{number}/merge", r.slug()),
            Some(json!({
                "merge_method": "squash",
                "commit_title": format!("{title} (#{number})"),
                "sha": sha,
            })),
        )?;
        Ok(())
    }

    pub fn close(&self, r: &Remote, number: u64) -> Result<()> {
        self.call(
            reqwest::Method::PATCH,
            &format!("/repos/{}/pulls/{number}", r.slug()),
            Some(json!({"state": "closed"})),
        )?;
        Ok(())
    }

    pub fn delete_branch(&self, r: &Remote, branch: &str) -> Result<()> {
        self.call(
            reqwest::Method::DELETE,
            &format!("/repos/{}/git/refs/heads/{branch}", r.slug()),
            None,
        )?;
        Ok(())
    }
}

/// Standard base64, with padding (HTTP basic authentication).
fn base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn github_remotes_are_recognized() {
        let r = Remote {
            owner: "tomonchain".into(),
            name: "airth".into(),
        };
        for url in [
            "https://github.com/tomonchain/airth.git",
            "https://github.com/tomonchain/airth",
            "git@github.com:tomonchain/airth.git",
            "ssh://git@github.com/tomonchain/airth.git\n",
        ] {
            assert_eq!(Remote::parse(url), Some(r.clone()), "{url}");
        }
        for url in [
            "https://gitlab.com/a/b.git",
            "https://github.com/a",
            "https://github.com/a/b/c",
            "git@github.com:a/b;rm",
        ] {
            assert_eq!(Remote::parse(url), None, "{url}");
        }
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"x-access-token:t"), "eC1hY2Nlc3MtdG9rZW46dA==");
    }
}
