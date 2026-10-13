//! MCP (Model Context Protocol): stdio servers the owner adds to a realm.
//!
//! A server is a shell command that speaks JSON-RPC 2.0 over stdin and stdout,
//! one message per line. The owner connects it from the app: Airth starts it
//! in the realm's sandbox, with the realm's credentials and nothing else, asks
//! for its tools and keeps the list. In a run, each of those tools is a tool
//! of the agent (`mcp__<server>__<tool>`) under the realm's `skill` rule.
//! Every call starts the server, initializes it, calls the tool and stops the
//! whole process group: no server outlives a call or holds state between
//! calls, and none can see another realm.

use super::shell::{kill_process_group, sandboxed_command};
use super::{Tool, Workspace, truncate};
use crate::cancel::CancelToken;
use crate::models::ToolSpec;
use crate::permissions::Capability;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_OUTPUT: usize = 50_000;
/// Generous: `npx` and `uvx` may download the server the first time.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(90);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// A tool a server offers, as it described it when last connected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolSpec {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "empty_schema")]
    pub input_schema: Value,
}

fn empty_schema() -> Value {
    json!({"type": "object", "properties": {}})
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    /// Shell command that starts the server, e.g. `npx -y @modelcontextprotocol/server-memory`.
    /// Empty for a remote server.
    #[serde(default)]
    pub command: String,
    /// A remote (Streamable HTTP) server's URL, connected with OAuth (see `remote`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The tools from the last successful connection.
    #[serde(default)]
    pub tools: Vec<McpToolSpec>,
    #[serde(default)]
    pub server_info: Option<String>,
    #[serde(default)]
    pub connected_at: Option<DateTime<Utc>>,
    /// Why the last connection failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Set while a connection is in progress (cleared when it ends).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connecting_since: Option<DateTime<Utc>>,
}

impl McpServer {
    /// A connection started recently enough to still be running.
    pub fn connecting(&self) -> bool {
        self.connecting_since.is_some_and(|t| {
            Utc::now() - t < chrono::Duration::seconds(CONNECT_TIMEOUT.as_secs() as i64 * 2 + 15)
        })
    }
}

/// A realm's MCP servers, in `skills/mcp.json`.
pub struct McpStore {
    path: PathBuf,
}

impl McpStore {
    /// `skills_dir` is the realm's `skills/` folder.
    pub fn new(skills_dir: &Path) -> Self {
        Self {
            path: skills_dir.join("mcp.json"),
        }
    }

    pub fn list(&self) -> Result<Vec<McpServer>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    fn write(&self, servers: &[McpServer]) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(servers)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<McpServer> {
        self.list()?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("no MCP server named {name}"))
    }

    /// Add a server, or change an existing one's command and description. A
    /// changed command forgets the tools until the server connects again.
    pub fn save(&self, name: &str, command: &str, description: &str) -> Result<()> {
        validate_name(name)?;
        let command = command.trim();
        if command.is_empty() {
            bail!("give the command that starts the server");
        }
        let mut servers = self.list()?;
        match servers.iter_mut().find(|s| s.name == name) {
            Some(s) => {
                if s.command != command {
                    s.command = command.to_string();
                    s.tools.clear();
                    s.server_info = None;
                    s.connected_at = None;
                    s.error = None;
                }
                s.description = description.trim().to_string();
            }
            None => servers.push(McpServer {
                name: name.to_string(),
                command: command.to_string(),
                url: None,
                description: description.trim().to_string(),
                enabled: true,
                tools: Vec::new(),
                server_info: None,
                connected_at: None,
                error: None,
                connecting_since: None,
            }),
        }
        self.write(&servers)
    }

    /// Add a remote server (or change its URL, which forgets its tools).
    pub fn save_remote(&self, name: &str, url: &str, description: &str) -> Result<()> {
        validate_name(name)?;
        let url = url.trim();
        let parsed = reqwest::Url::parse(url)
            .context("give the server's URL, e.g. https://mcp.example.com/mcp")?;
        if !matches!(parsed.scheme(), "https" | "http") {
            bail!("the server's URL must start with https://");
        }
        let mut servers = self.list()?;
        match servers.iter_mut().find(|s| s.name == name) {
            Some(s) => {
                if s.url.as_deref() != Some(url) {
                    s.url = Some(url.to_string());
                    s.command.clear();
                    s.tools.clear();
                    s.server_info = None;
                    s.connected_at = None;
                }
                s.error = None;
                if !description.trim().is_empty() {
                    s.description = description.trim().to_string();
                }
            }
            None => servers.push(McpServer {
                name: name.to_string(),
                command: String::new(),
                url: Some(url.to_string()),
                description: description.trim().to_string(),
                enabled: true,
                tools: Vec::new(),
                server_info: None,
                connected_at: None,
                error: None,
                connecting_since: None,
            }),
        }
        self.write(&servers)
    }

    pub fn update(&self, name: &str, f: impl FnOnce(&mut McpServer)) -> Result<()> {
        let mut servers = self.list()?;
        let server = servers
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("no MCP server named {name}"))?;
        f(server);
        self.write(&servers)
    }

    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut servers = self.list()?;
        let before = servers.len();
        servers.retain(|s| s.name != name);
        if servers.len() == before {
            return Ok(false);
        }
        self.write(&servers)?;
        Ok(true)
    }
}

/// Server names become part of tool names: short, lowercase, no spaces.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = (1..=32).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !ok {
        bail!("a server name is 1 to 32 lowercase letters, digits, '-' or '_', e.g. github");
    }
    Ok(())
}

/// The agent's name for a server's tool: `mcp__<server>__<tool>`, within what
/// model APIs accept (64 characters of letters, digits, '_' and '-').
pub fn tool_name(server: &str, tool: &str) -> String {
    let clean: String = tool
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut name = format!("mcp__{server}__{clean}");
    name.truncate(64);
    name
}

/// A running server, stopped (with everything it spawned) when dropped.
struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    stderr: Arc<Mutex<String>>,
    next_id: u64,
}

impl Session {
    fn start(command: &str, ws: &Workspace) -> Result<Self> {
        let mut cmd = sandboxed_command(command, ws);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().context("cannot start the MCP server")?;
        let stdout = child.stdout.take().context("no stdout")?;
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(mut err) = child.stderr.take() {
            let sink = stderr.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut s = sink.lock().unwrap();
                    if s.len() < 16_000 {
                        s.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
            });
        }
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            lines,
            stderr,
            next_id: 1,
        })
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("the MCP server's input is closed")?;
        writeln!(stdin, "{message}")
            .and_then(|_| stdin.flush())
            .map_err(|e| anyhow!("the MCP server stopped reading: {e}{}", self.stderr_tail()))
    }

    fn stderr_tail(&self) -> String {
        let s = self.stderr.lock().unwrap();
        let s = s.trim();
        if s.is_empty() {
            String::new()
        } else {
            let tail: String = s
                .chars()
                .rev()
                .take(1500)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("\n--- server stderr ---\n{tail}")
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: &CancelToken,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.is_cancelled() {
                bail!("stopped");
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!(
                    "the MCP server did not answer {method} within {}s{}",
                    timeout.as_secs(),
                    self.stderr_tail()
                );
            }
            let line = match self
                .lines
                .recv_timeout(left.min(Duration::from_millis(200)))
            {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    // Give the stderr reader a moment to collect the last words.
                    std::thread::sleep(Duration::from_millis(100));
                    bail!("the MCP server exited{}", self.stderr_tail());
                }
            };
            // Anything that is not JSON-RPC (stray logging) is skipped.
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message.get("method").is_some() {
                // The server asks us something (roots, sampling, elicitation):
                // decline, so it does not wait forever. Notifications are ignored.
                if let Some(their_id) = message.get("id") {
                    self.send(&json!({"jsonrpc": "2.0", "id": their_id,
                        "error": {"code": -32601, "message": "not supported by Airth"}}))?;
                }
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                bail!(
                    "{method} failed: {}",
                    error["message"].as_str().unwrap_or("unknown error")
                );
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// The MCP handshake; returns the server's name and version.
    fn initialize(&mut self, cancel: &CancelToken) -> Result<String> {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "airth", "version": env!("CARGO_PKG_VERSION")},
            }),
            CONNECT_TIMEOUT,
            cancel,
        )?;
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        let info = &result["serverInfo"];
        Ok(format!(
            "{} {}",
            info["name"].as_str().unwrap_or("unnamed server"),
            info["version"].as_str().unwrap_or("")
        )
        .trim()
        .to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stdin.take();
        kill_process_group(&mut self.child);
        let _ = self.child.wait();
    }
}

/// Start the server, shake hands, and list its tools.
pub fn connect(
    command: &str,
    ws: &Workspace,
    cancel: &CancelToken,
) -> Result<(String, Vec<McpToolSpec>)> {
    let mut session = Session::start(command, ws)?;
    let info = session.initialize(cancel)?;
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let params = match &cursor {
            Some(c) => json!({"cursor": c}),
            None => json!({}),
        };
        let page = session.request("tools/list", params, CONNECT_TIMEOUT, cancel)?;
        tools.extend(parse_tools(&page));
        cursor = page["nextCursor"].as_str().map(String::from);
        if cursor.is_none() {
            break;
        }
    }
    Ok((info, tools))
}

/// Start the server, call one tool, and return what it answered as text.
pub fn call(
    command: &str,
    tool: &str,
    arguments: &Value,
    ws: &Workspace,
    cancel: &CancelToken,
) -> Result<String> {
    let mut session = Session::start(command, ws)?;
    session.initialize(cancel)?;
    let result = session.request(
        "tools/call",
        json!({"name": tool, "arguments": arguments}),
        CALL_TIMEOUT,
        cancel,
    )?;
    result_text(&result)
}

/// The tools of one `tools/list` page (stdio and remote servers alike).
pub fn parse_tools(page: &Value) -> Vec<McpToolSpec> {
    let mut tools = Vec::new();
    for t in page["tools"].as_array().into_iter().flatten() {
        let Some(name) = t["name"].as_str() else {
            continue;
        };
        let mut schema = t
            .get("inputSchema")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(empty_schema);
        if schema.get("type").is_none() {
            schema["type"] = json!("object");
        }
        tools.push(McpToolSpec {
            name: name.to_string(),
            description: t["description"].as_str().unwrap_or_default().to_string(),
            input_schema: schema,
        });
    }
    tools
}

/// A `tools/call` result as text for the model; a tool error is an error.
pub fn result_text(result: &Value) -> Result<String> {
    let text = render(result);
    if result["isError"].as_bool() == Some(true) {
        bail!("{}", truncate(text, MAX_OUTPUT));
    }
    Ok(truncate(text, MAX_OUTPUT))
}

/// Connect a server on a background thread (the app's requests are served one
/// at a time, and a first `npx` start can take a minute): mark it connecting,
/// then save its tools, or why it failed.
pub fn connect_in_background(skills_dir: &Path, name: &str, ws: Workspace) -> Result<()> {
    let store = McpStore::new(skills_dir);
    let server = store.get(name)?;
    if server.connecting() {
        bail!("{name} is already connecting");
    }
    store.update(name, |s| s.connecting_since = Some(Utc::now()))?;
    let dir = skills_dir.to_path_buf();
    let name = name.to_string();
    std::thread::spawn(move || {
        // A remote server lists its tools with its saved sign-in (if any).
        if server.url.is_some() {
            if let Err(e) = super::remote::refresh_tools(&dir, &name) {
                let _ = McpStore::new(&dir).update(&name, |s| {
                    s.connecting_since = None;
                    s.error = Some(format!("{e:#}"));
                });
            }
            return;
        }
        let result = connect(&server.command, &ws, &CancelToken::default());
        let store = McpStore::new(&dir);
        let _ = store.update(&name, |s| {
            s.connecting_since = None;
            match result {
                Ok((info, tools)) => {
                    s.server_info = Some(info);
                    s.tools = tools;
                    s.connected_at = Some(Utc::now());
                    s.error = None;
                }
                Err(e) => s.error = Some(ws.redact(&format!("{e:#}"))),
            }
        });
    });
    Ok(())
}

/// A tool result's content as text for the model.
fn render(result: &Value) -> String {
    let mut parts = Vec::new();
    for item in result["content"].as_array().into_iter().flatten() {
        let part = match item["type"].as_str() {
            Some("text") => item["text"].as_str().unwrap_or_default().to_string(),
            Some("image") | Some("audio") => format!(
                "[{} {}]",
                item["type"].as_str().unwrap_or_default(),
                item["mimeType"].as_str().unwrap_or("")
            ),
            Some("resource") => match item["resource"]["text"].as_str() {
                Some(text) => text.to_string(),
                None => format!(
                    "[resource {}]",
                    item["resource"]["uri"].as_str().unwrap_or("")
                ),
            },
            Some("resource_link") => format!(
                "[link {} {}]",
                item["name"].as_str().unwrap_or(""),
                item["uri"].as_str().unwrap_or("")
            ),
            _ => item.to_string(),
        };
        parts.push(part);
    }
    if parts.is_empty()
        && let Some(structured) = result.get("structuredContent")
    {
        return serde_json::to_string_pretty(structured).unwrap_or_default();
    }
    if parts.is_empty() {
        "(no content)".into()
    } else {
        parts.join("\n")
    }
}

/// One of a connected server's tools, as a tool of the agent.
pub struct McpTool {
    server: String,
    command: String,
    spec: McpToolSpec,
    /// For a remote server: the realm's `skills/` folder (where its tokens
    /// are) and its URL.
    remote: Option<(PathBuf, String)>,
}

impl Tool for McpTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: tool_name(&self.server, &self.spec.name),
            description: format!(
                "{} (Tool `{}` of the MCP server `{}`, added by the owner.)",
                self.spec.description.trim(),
                self.spec.name,
                self.server
            ),
            input_schema: self.spec.input_schema.clone(),
        }
    }

    fn capability(&self) -> Capability {
        Capability::Skill
    }

    fn scope(&self, _: &Value) -> Option<String> {
        Some(tool_name(&self.server, &self.spec.name))
    }

    fn preview(&self, input: &Value, _ws: &Workspace) -> Result<String> {
        let target = match &self.remote {
            Some((_, url)) => url.clone(),
            None => format!("$ {}", self.command),
        };
        Ok(format!(
            "MCP server {} · tool {}\n{}\n{}",
            self.server,
            self.spec.name,
            target,
            serde_json::to_string_pretty(input)?
        ))
    }

    fn execute(&self, input: &Value, ws: &Workspace, cancel: &CancelToken) -> Result<String> {
        let arguments = if input.is_object() {
            input.clone()
        } else {
            json!({})
        };
        if let Some((dir, url)) = &self.remote {
            return super::remote::call(
                dir,
                &self.server,
                url,
                &self.spec.name,
                &arguments,
                cancel,
            );
        }
        call(&self.command, &self.spec.name, &arguments, ws, cancel)
    }
}

/// The tools of a realm's enabled, connected servers.
pub fn tools_for(skills_dir: &Path) -> Vec<Box<dyn Tool>> {
    let servers = McpStore::new(skills_dir).list().unwrap_or_default();
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    for server in servers.into_iter().filter(|s| s.enabled) {
        for spec in server.tools {
            tools.push(Box::new(McpTool {
                server: server.name.clone(),
                command: server.command.clone(),
                spec,
                remote: server.url.clone().map(|u| (skills_dir.to_path_buf(), u)),
            }));
        }
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny MCP server in sh: it answers initialize, tools/list and
    /// tools/call (echoing `text`, failing on `fail`, and asking the client
    /// for its roots first, as real servers may).
    const FAKE: &str = r#"
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      echo "starting" >&2
      echo 'not json, just logging'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.2"}}}\n' "$id" ;;
    *'"method":"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echo text","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}},{"name":"no.schema"}]}}\n' "$id" ;;
    *'"method":"tools/call"'*'"fail"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"isError":true,"content":[{"type":"text","text":"it broke"}]}}\n' "$id" ;;
    *'"method":"tools/call"'*)
      text=$(printf '%s' "$line" | sed -n 's/.*"text":"\([^"]*\)".*/\1/p')
      printf '{"jsonrpc":"2.0","id":900,"method":"roots/list"}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"echo: %s"},{"type":"image","mimeType":"image/png","data":"x"}]}}\n' "$id" "$text" ;;
  esac
done
"#;

    fn setup() -> (tempfile::TempDir, Workspace, String) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("server.sh"), FAKE).unwrap();
        let ws = Workspace::new(dir.path()).unwrap().with_sandbox(false);
        (dir, ws, "sh server.sh".to_string())
    }

    #[test]
    fn connects_and_lists_tools() {
        let (_dir, ws, cmd) = setup();
        let (info, tools) = connect(&cmd, &ws, &CancelToken::default()).unwrap();
        assert_eq!(info, "fake 1.2");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[1].input_schema["type"], "object");
    }

    #[test]
    fn calls_a_tool_and_declines_server_requests() {
        let (_dir, ws, cmd) = setup();
        let out = call(
            &cmd,
            "echo",
            &json!({"text": "hi"}),
            &ws,
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(out, "echo: hi\n[image image/png]");
    }

    #[test]
    fn tool_errors_and_dead_servers_are_errors() {
        let (_dir, ws, cmd) = setup();
        let err = call(
            &cmd,
            "x",
            &json!({"fail": true}),
            &ws,
            &CancelToken::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("it broke"), "{err}");
        let err = connect(
            "echo 'missing token' >&2; exit 3",
            &ws,
            &CancelToken::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("exited") && err.contains("missing token"),
            "{err}"
        );
    }

    #[test]
    fn store_names_and_tools() {
        let dir = tempfile::tempdir().unwrap();
        let store = McpStore::new(dir.path());
        assert!(store.save("Bad Name", "x", "").is_err());
        assert!(store.save("gh", "  ", "").is_err());
        store.save("gh", "npx server", "GitHub").unwrap();
        store
            .update("gh", |s| {
                s.tools = vec![McpToolSpec {
                    name: "create issue".into(),
                    description: "d".into(),
                    input_schema: empty_schema(),
                }]
            })
            .unwrap();
        let tools = tools_for(dir.path());
        assert_eq!(tools[0].spec().name, "mcp__gh__create_issue");
        assert_eq!(tools[0].capability(), Capability::Skill);
        // A new command forgets the old tools; disabling hides them.
        store.save("gh", "npx other", "GitHub").unwrap();
        assert!(store.get("gh").unwrap().tools.is_empty());
        store.update("gh", |s| s.enabled = false).unwrap();
        assert!(tools_for(dir.path()).is_empty());
        assert!(store.remove("gh").unwrap());
        assert!(!store.remove("gh").unwrap());
        assert_eq!(tool_name("s", &"x".repeat(100)).len(), 64);
    }
}
