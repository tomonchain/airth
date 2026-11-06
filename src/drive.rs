//! Google Drive, read only, one folder (decision 85). The founder connects
//! it with their own Google OAuth client: Airth signs in with PKCE (the
//! remote MCP sign-in, `tools::remote`), keeps the tokens in the realm's
//! private files, and reads only inside the folder the founder named: every
//! file asked for is checked to be the folder or under it.

use crate::tools::remote::{self, Auth};
use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The name its tokens are kept under, and what an agent's file names to
/// be given it (`integrations: google-drive`).
pub const NAME: &str = "google-drive";
const API: &str = "https://www.googleapis.com/drive/v3";
const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
/// The most of a document handed to the agent at once.
const MAX_TEXT: usize = 100_000;
/// How far up a file's parents are followed to find the folder.
const MAX_LOOKUPS: usize = 25;

/// The folder the founder shares with the business's agents.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

fn folder_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join("drive.json")
}

pub fn folder(skills_dir: &Path) -> Option<Folder> {
    let text = std::fs::read_to_string(folder_path(skills_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_folder(skills_dir: &Path, folder: &Folder) -> Result<()> {
    std::fs::create_dir_all(skills_dir)?;
    crate::realm::write_private(
        &folder_path(skills_dir),
        &serde_json::to_string_pretty(folder)?,
    )
}

/// A folder named and signed in to.
pub fn connected(skills_dir: &Path) -> bool {
    folder(skills_dir).is_some()
        && remote::load_auth(skills_dir, NAME)
            .is_some_and(|a| a.refresh_token.is_some() || a.access_token.is_some())
}

/// The folder's id in a link the founder pastes (`…/folders/<id>`,
/// `…?id=<id>`), or the id itself.
pub fn folder_id(link: &str) -> Result<String> {
    let link = link.trim();
    let id = match (link.split_once("/folders/"), link.split_once("id=")) {
        (Some((_, rest)), _) | (None, Some((_, rest))) => rest,
        (None, None) => link,
    };
    let id: String = id
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if id.len() < 10 {
        bail!(
            "this is not a Google Drive folder link: open the folder in Drive and copy its address"
        );
    }
    Ok(id)
}

/// Name the folder and begin signing in: the page to open. The client is
/// the founder's own (Google Cloud, a "Desktop app" OAuth client); Google
/// sends the browser back to Airth's callback.
pub fn start(
    skills_dir: &Path,
    client_id: &str,
    client_secret: &str,
    folder_link: &str,
    redirect_uri: &str,
) -> Result<String> {
    let (client_id, client_secret) = (client_id.trim(), client_secret.trim());
    if client_id.is_empty() || client_secret.is_empty() {
        bail!("the OAuth client's ID and secret are both needed");
    }
    let id = folder_id(folder_link)?;
    save_folder(
        skills_dir,
        &Folder {
            id,
            name: String::new(),
        },
    )?;
    let auth = Auth {
        client_id: client_id.to_string(),
        client_secret: Some(client_secret.to_string()),
        redirect_uri: redirect_uri.to_string(),
        authorization_endpoint: AUTHORIZE.to_string(),
        token_endpoint: TOKEN.to_string(),
        scope: Some(SCOPE.to_string()),
        ..Default::default()
    };
    // Offline, and asked each time: Google gives a refresh token only then.
    remote::start_known(
        skills_dir,
        NAME,
        auth,
        &[("access_type", "offline"), ("prompt", "consent")],
    )
}

/// Forget the sign-in and the folder.
pub fn disconnect(skills_dir: &Path) {
    remote::forget_auth(skills_dir, NAME);
    let _ = std::fs::remove_file(folder_path(skills_dir));
}

/// The folder's name, from Drive, kept once known.
pub fn folder_name(skills_dir: &Path) -> Result<String> {
    let mut f = folder(skills_dir).context("no Google Drive folder")?;
    if f.name.is_empty() {
        f.name = Drive::open(skills_dir)?.meta(&f.id)?["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        save_folder(skills_dir, &f)?;
    }
    Ok(f.name)
}

/// Drive, as the connected account sees it, held to one folder.
pub struct Drive {
    api: String,
    token: String,
    root: String,
    http: Client,
}

impl Drive {
    pub fn open(skills_dir: &Path) -> Result<Self> {
        let folder = folder(skills_dir)
            .context("Google Drive is not connected: connect it in Settings → Connections")?;
        let token = remote::access_token(skills_dir, NAME, false)?
            .context("Google Drive is not connected: connect it in Settings → Connections")?;
        Self::at(API, &token, &folder.id)
    }

    pub(crate) fn at(api: &str, token: &str, root: &str) -> Result<Self> {
        Ok(Self {
            api: api.trim_end_matches('/').to_string(),
            token: token.to_string(),
            root: root.to_string(),
            http: Client::builder().timeout(Duration::from_secs(60)).build()?,
        })
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<reqwest::blocking::Response> {
        let mut url =
            url::Url::parse(&format!("{}{path}", self.api)).context("invalid Drive URL")?;
        url.query_pairs_mut().extend_pairs(query);
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .context("Google Drive did not answer")?;
        let status = resp.status();
        if !status.is_success() {
            let body: Value = resp.json().unwrap_or(Value::Null);
            bail!(
                "Google Drive answered {status}: {}",
                body["error"]["message"]
                    .as_str()
                    .unwrap_or("no reason given")
            );
        }
        Ok(resp)
    }

    fn meta(&self, id: &str) -> Result<Value> {
        Ok(self
            .get(
                &format!("/files/{id}"),
                &[
                    ("fields", "id,name,mimeType,parents,modifiedTime,size"),
                    ("supportsAllDrives", "true"),
                ],
            )?
            .json()?)
    }

    /// The file `id`, if it is the folder or under it: followed up its
    /// parents. Anything else is refused, whatever the account may read.
    pub fn within(&self, id: &str) -> Result<Value> {
        let file = self.meta(id)?;
        if id == self.root {
            return Ok(file);
        }
        let mut next: Vec<String> = parents(&file);
        let mut seen = 0;
        while let Some(parent) = next.pop() {
            if parent == self.root {
                return Ok(file);
            }
            seen += 1;
            if seen > MAX_LOOKUPS {
                break;
            }
            next.extend(parents(&self.meta(&parent)?));
        }
        bail!(
            "{} is not in the folder shared with the business",
            file["name"].as_str().unwrap_or(id)
        )
    }

    /// The files of `folder` (the shared folder by default), folders first.
    pub fn list(&self, folder: Option<&str>) -> Result<Vec<Value>> {
        let folder = folder
            .filter(|f| !f.is_empty())
            .unwrap_or(&self.root)
            .to_string();
        if folder != self.root {
            self.within(&folder)?;
        }
        let q = format!("'{}' in parents and trashed = false", quoted(&folder));
        self.files(&q, 200, true)
    }

    /// Files whose name or contents hold `words`, in the shared folder.
    pub fn search(&self, words: &str) -> Result<Vec<Value>> {
        let q = format!(
            "fullText contains '{}' and trashed = false",
            quoted(words.trim())
        );
        Ok(self
            // Drive refuses to sort a full-text search: by relevance only.
            .files(&q, 30, false)?
            .into_iter()
            .filter(|f| f["id"].as_str().is_some_and(|id| self.within(id).is_ok()))
            .take(20)
            .collect())
    }

    fn files(&self, q: &str, size: usize, sorted: bool) -> Result<Vec<Value>> {
        let size = size.to_string();
        let mut query = vec![
            ("q", q),
            ("fields", "files(id,name,mimeType,modifiedTime,size)"),
            ("pageSize", size.as_str()),
            ("supportsAllDrives", "true"),
            ("includeItemsFromAllDrives", "true"),
        ];
        if sorted {
            query.push(("orderBy", "folder,name"));
        }
        let body: Value = self.get("/files", &query)?.json()?;
        Ok(body["files"].as_array().cloned().unwrap_or_default())
    }

    /// A file's name and its text: a Doc as Markdown, a Sheet as CSV (its
    /// first sheet), Slides as text, a text file as it is.
    pub fn read(&self, id: &str) -> Result<(String, String)> {
        let file = self.within(id)?;
        let name = file["name"].as_str().unwrap_or(id).to_string();
        let mime = file["mimeType"].as_str().unwrap_or_default();
        let export = |as_: &str| {
            self.get(&format!("/files/{id}/export"), &[("mimeType", as_)])
                .and_then(|r| Ok(r.text()?))
        };
        let text = match mime {
            "application/vnd.google-apps.document" => {
                export("text/markdown").or_else(|_| export("text/plain"))?
            }
            "application/vnd.google-apps.spreadsheet" => export("text/csv")?,
            "application/vnd.google-apps.presentation" => export("text/plain")?,
            "application/vnd.google-apps.folder" => bail!("{name} is a folder: list it"),
            m if m.starts_with("text/") || m == "application/json" => self
                .get(
                    &format!("/files/{id}"),
                    &[("alt", "media"), ("supportsAllDrives", "true")],
                )?
                .text()?,
            // A Word document or a PDF: its text, read here (decision 86).
            "application/pdf"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                let bytes = self
                    .get(
                        &format!("/files/{id}"),
                        &[("alt", "media"), ("supportsAllDrives", "true")],
                    )?
                    .bytes()?;
                crate::extract::text(&bytes, mime).with_context(|| format!("reading {name}"))?
            }
            other => bail!("{name} is a {other} file, which Airth cannot read as text yet"),
        };
        Ok((name, cut(text)))
    }
}

fn parents(file: &Value) -> Vec<String> {
    file["parents"]
        .as_array()
        .map(|p| {
            p.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// A value inside a Drive query's quotes.
fn quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\'', "\\'")
}

fn cut(text: String) -> String {
    match text.char_indices().nth(MAX_TEXT) {
        Some((at, _)) => format!(
            "{}\n\n[cut: the document goes on past {MAX_TEXT} characters]",
            &text[..at]
        ),
        None => text,
    }
}

/// A file's kind, as the founder says it.
pub fn kind(mime: &str) -> &str {
    match mime {
        "application/vnd.google-apps.folder" => "folder",
        "application/vnd.google-apps.document" => "Doc",
        "application/vnd.google-apps.spreadsheet" => "Sheet",
        "application/vnd.google-apps.presentation" => "Slides",
        "application/pdf" => "PDF",
        m if m.starts_with("text/") => "text",
        m => m.rsplit('/').next().unwrap_or(m),
    }
}

#[cfg(test)]
mod tests;
