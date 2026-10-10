"use strict";

// ---------- small helpers ----------

const $ = (sel, root = document) => root.querySelector(sel);

/** Build a DOM element. Strings become text nodes, so content is never parsed as HTML. */
function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === false || v == null) continue;
    if (k === "class") el.className = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "html") el.innerHTML = v; // only ever used with md(), which escapes
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) {
    if (c == null || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

const esc = (s) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function inline(s) {
  return s
    .split(/(`[^`\n]+`)/g)
    .map((part) => {
      if (/^`[^`\n]+`$/.test(part)) return "<code>" + esc(part.slice(1, -1)) + "</code>";
      return esc(part)
        .replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
        .replace(/(^|[\s(])\*([^*\n]+)\*(?=[\s).,:;!?]|$)/g, "$1<em>$2</em>")
        .replace(
          /\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g,
          '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>'
        );
    })
    .join("");
}

/** Minimal Markdown → HTML. All text is escaped before any markup is added. */
function md(src) {
  const lines = String(src || "").replace(/\r\n/g, "\n").split("\n");
  const out = [];
  const isBullet = (l) => /^\s*[-*]\s+/.test(l);
  const isNum = (l) => /^\s*\d+[.)]\s+/.test(l);
  const isFence = (l) => /^\s*```/.test(l);
  const isHeading = (l) => /^#{1,6}\s+/.test(l);
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (isFence(line)) {
      const buf = [];
      i++;
      while (i < lines.length && !isFence(lines[i])) buf.push(lines[i++]);
      i++;
      out.push("<pre><code>" + esc(buf.join("\n")) + "</code></pre>");
    } else if (isHeading(line)) {
      const m = line.match(/^(#{1,6})\s+(.*)$/);
      const level = Math.min(m[1].length + 1, 4);
      out.push(`<h${level}>${inline(m[2])}</h${level}>`);
      i++;
    } else if (isBullet(line) || isNum(line)) {
      const ordered = isNum(line);
      const items = [];
      while (i < lines.length && (isBullet(lines[i]) || isNum(lines[i]) || /^\s{2,}\S/.test(lines[i]))) {
        const l = lines[i];
        if (isBullet(l) || isNum(l)) items.push(l.replace(/^\s*(?:[-*]|\d+[.)])\s+/, ""));
        else items[items.length - 1] += " " + l.trim();
        i++;
      }
      const tag = ordered ? "ol" : "ul";
      out.push(`<${tag}>` + items.map((t) => "<li>" + inline(t) + "</li>").join("") + `</${tag}>`);
    } else if (!line.trim()) {
      i++;
    } else {
      const buf = [];
      while (
        i < lines.length && lines[i].trim() &&
        !isFence(lines[i]) && !isHeading(lines[i]) && !isBullet(lines[i]) && !isNum(lines[i])
      ) buf.push(lines[i++]);
      out.push("<p>" + buf.map(inline).join("<br>") + "</p>");
    }
  }
  return out.join("");
}

function ago(iso) {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 45) return "just now";
  if (s < 3600) return Math.round(s / 60) + "m ago";
  if (s < 86400) return Math.round(s / 3600) + "h ago";
  return Math.round(s / 86400) + "d ago";
}

const kfmt = (n) => (n >= 1e6 ? (n / 1e6).toFixed(1) + "M" : n >= 1e3 ? (n / 1e3).toFixed(1) + "k" : String(n));
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
const modKey = isMac ? "⌘" : "Ctrl";

// ---------- API ----------

let token = null;
try { token = sessionStorage.getItem("airth-token"); } catch {}
{
  const m = location.hash.match(/token=([0-9a-f]+)/);
  if (m) {
    token = m[1];
    try { sessionStorage.setItem("airth-token", token); } catch {}
    history.replaceState(null, "", "#/new");
  }
}

class AuthError extends Error {}

async function api(path, body) {
  const res = await fetch("/api/" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "X-Airth-Token": token || "", "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 401) throw new AuthError("unauthorized");
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

// ---------- app state ----------

const app = {
  info: null,
  runs: [],
  route: { view: "new" },
  run: null, // last full response for the current run
  version: null,
  openKeys: new Set(),
  reason: "",
  error: null,
  busy: false,
  unauthorized: !token,
};

function parseRoute() {
  const m = location.hash.match(/^#\/run\/([^/?#]+)/);
  return m ? { view: "run", id: decodeURIComponent(m[1]) } : { view: "new" };
}

function go(hash) {
  if (location.hash === hash) onRoute();
  else location.hash = hash;
}

function onRoute() {
  app.route = parseRoute();
  app.run = null;
  app.version = null;
  app.error = null;
  app.reason = "";
  renderMain();
  renderSidebar();
  if (app.route.view === "run") pollRun();
}

// ---------- sidebar ----------

function renderSidebar() {
  const list = $("#run-list");
  list.replaceChildren();
  if (!app.runs.length) list.append(h("div", { class: "run-empty" }, "No runs yet."));
  for (const r of app.runs) {
    const status = r.live ? "running" : r.status;
    const active = app.route.view === "run" && app.route.id === r.id;
    list.append(
      h("button", { class: "run-item" + (active ? " active" : ""), type: "button", title: r.objective, onclick: () => go("#/run/" + encodeURIComponent(r.id)) },
        h("div", { class: "obj" }, r.objective),
        h("div", { class: "meta" },
          h("span", { class: "dot " + status }),
          h("span", null, r.live ? "running" : r.status),
          h("span", null, "·"),
          h("span", null, ago(r.updated_at)),
          r.needs_approval && h("span", { class: "needs" }, "needs approval"))));
  }
  $("#new-run-btn").classList.toggle("ghost", app.route.view === "new");

  const foot = $("#sidebar-foot");
  foot.replaceChildren();
  if (app.info) {
    $("#version").textContent = "v" + app.info.version;
    foot.append(
      h("div", { class: "row" }, h("span", { class: "k" }, "model"),
        app.info.model ? h("span", { class: "v" }, app.info.model) : h("span", { class: "warn" }, "not configured")),
      h("div", { class: "row", title: app.info.workspace }, h("span", { class: "k" }, "workspace"), h("span", { class: "v" }, app.info.workspace)));
  }
  updateTitle();
}

function updateTitle() {
  const n = app.runs.filter((r) => r.needs_approval).length;
  document.title = (n ? `(${n}) ` : "") + "Airth";
}

// ---------- main: new run ----------

const SUGGESTIONS = [
  "Improve yourself",
  "Find and fix a bug",
  "Add tests for an untested module",
  "Review the code for unclear error messages and improve them",
];

function renderMain() {
  const main = $("#main");
  main.replaceChildren();
  if (app.unauthorized) {
    main.append(h("div", { class: "center-note" },
      h("h1", null, "Open Airth from your terminal"),
      h("p", null, "Run ", h("code", null, "airth ui"), " and use the link it prints. The link carries a private key that lets this page control the agent.")));
    return;
  }
  if (app.route.view === "new") renderNew(main);
  else renderRunShell(main);
}

function composer({ placeholder, buttonLabel, onSubmit, rows = 1, disabled = false, note }) {
  const ta = h("textarea", { rows, placeholder, disabled, "aria-label": placeholder });
  const btn = h("button", { class: "btn primary", type: "button", disabled }, buttonLabel, " ", h("kbd", null, modKey + "↵"));
  const errorEl = h("div", { class: "error-banner", hidden: !app.error }, app.error || "");
  const submit = async () => {
    const text = ta.value.trim();
    if (!text || btn.disabled) return;
    btn.disabled = true;
    try {
      await onSubmit(text);
      ta.value = "";
      app.error = null;
    } catch (e) {
      app.error = e.message;
      errorEl.textContent = e.message;
      errorEl.hidden = false;
    } finally {
      btn.disabled = ta.disabled;
    }
  };
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submit(); }
  });
  ta.addEventListener("input", () => {
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, 200) + "px";
  });
  btn.addEventListener("click", submit);
  const el = h("div", { class: "composer" }, errorEl, h("div", { class: "composer-box" }, ta, btn), note && h("div", { class: "note" }, note));
  return { el, ta, btn, errorEl };
}

function renderNew(main) {
  const info = app.info;
  const c = composer({
    placeholder: "Describe an objective…",
    buttonLabel: "Start",
    rows: 3,
    disabled: !!(info && info.model_error),
    note: "Reads happen automatically. Every file write and command waits for your approval.",
    onSubmit: async (objective) => {
      const { id } = await api("runs", { objective });
      await refreshRuns();
      go("#/run/" + encodeURIComponent(id));
    },
  });
  if (info && info.model_error) {
    c.errorEl.textContent = "No model available: " + info.model_error;
    c.errorEl.hidden = false;
  }
  const tools = info
    ? h("div", { class: "tools-card" },
        h("h2", null, "What Airth can do"),
        info.tools.map((t) =>
          h("div", { class: "tool-row" },
            h("code", null, t.name),
            h("span", { class: "chip cap-" + t.capability }, t.capability === "read" ? "automatic" : "needs approval"),
            h("span", { class: "d" }, t.description.split(". ")[0] + "."))))
    : null;
  main.append(
    h("div", { class: "new-view" },
      h("div", { class: "new-inner" },
        h("h1", null, "What should Airth work on?"),
        h("p", { class: "sub" }, "Airth inspects the workspace, proposes a change, and asks before it writes or runs anything."),
        c.el,
        h("div", { class: "suggestions" },
          SUGGESTIONS.map((s) => h("button", { class: "suggestion", type: "button", onclick: () => { c.ta.value = s; c.ta.focus(); } }, s))),
        tools)));
  c.ta.focus();
}

// ---------- main: run view ----------

let runEls = null;

function renderRunShell(main) {
  const header = h("header", { class: "run-header" });
  const transcript = h("div", { class: "transcript" }, h("div", { class: "working" }, h("span", { class: "spinner" }), "Loading…"));
  const scroller = h("div", { class: "scroller" }, transcript);
  const c = composer({
    placeholder: "Send a follow-up…",
    buttonLabel: "Send",
    disabled: true,
    onSubmit: async (message) => {
      await api(`runs/${encodeURIComponent(app.route.id)}/messages`, { message });
      app.version = null;
      await Promise.all([pollRun(), refreshRuns()]);
    },
  });
  main.append(header, scroller, h("div", { class: "composer-wrap" }, c.el));
  runEls = { header, transcript, scroller, composer: c };
}

function liveStatus(d) {
  if (!d.live) return d.interrupted ? "interrupted" : d.state.status;
  if (d.stopping) return "stopping";
  if (d.pending) return "awaiting";
  return "running";
}

const STATUS_LABEL = { awaiting: "awaiting approval", stopping: "stopping…" };

function renderRun() {
  const d = app.run;
  if (!d || !runEls) return;
  const s = d.state;
  const status = liveStatus(d);

  // Header
  const canStop = d.live && !d.elsewhere && !d.stopping;
  runEls.header.replaceChildren(
    h("div", { class: "run-title" },
      h("h1", { title: s.objective }, s.objective),
      h("div", { class: "run-meta" },
        h("span", { class: "chip " + (status === "stopping" ? "stopped" : status) }, STATUS_LABEL[status] || status),
        h("span", null, `${s.iterations} iteration${s.iterations === 1 ? "" : "s"}`),
        h("span", null, `${s.events.length} tool call${s.events.length === 1 ? "" : "s"}`),
        (s.input_tokens || s.output_tokens) && h("span", { title: `${s.input_tokens} in / ${s.output_tokens} out` }, `${kfmt(s.input_tokens)} in · ${kfmt(s.output_tokens)} out tokens`),
        h("span", null, "updated " + ago(s.updated_at)))),
    canStop && h("button", { class: "btn danger", type: "button", onclick: stopRun }, "■ Stop"));

  // Transcript — keep scroll pinned to the bottom if it was there, and keep the reason input.
  const sc = runEls.scroller;
  const nearBottom = sc.scrollHeight - sc.scrollTop - sc.clientHeight < 80;
  const reasonFocused = document.activeElement && document.activeElement.id === "deny-reason";
  runEls.transcript.replaceChildren(...transcriptNodes(d));
  if (reasonFocused) $("#deny-reason")?.focus();
  if (nearBottom || app.version === null) sc.scrollTop = sc.scrollHeight;

  // Composer
  const c = runEls.composer;
  const disabled = d.live || (app.info && !!app.info.model_error);
  c.ta.disabled = disabled;
  c.btn.disabled = disabled;
  c.ta.placeholder = d.live
    ? (d.elsewhere ? "This run is in progress in another Airth process…" : "Airth is working… you can send a follow-up when it stops.")
    : status === "interrupted" ? "This run was interrupted. Send a message to resume it…" : "Send a follow-up…";
}

function transcriptNodes(d) {
  const s = d.state;
  const events = new Map();
  for (const e of s.events) if (e.tool_use_id) events.set(e.tool_use_id, e);
  const results = new Map();
  for (const m of s.messages)
    if (m.role === "user") for (const b of m.content) if (b.type === "tool_result") results.set(b.tool_use_id, b);

  // The call waiting for approval is the first unanswered call in the last turn.
  let awaitingId = null;
  if (d.pending) {
    const last = [...s.messages].reverse().find((m) => m.role === "assistant");
    const call = last && last.content.find((b) => b.type === "tool_use" && !events.has(b.id) && !results.has(b.id));
    awaitingId = call && call.id;
  }

  const nodes = [];
  s.messages.forEach((m, mi) => {
    if (m.role === "user") {
      const texts = m.content.filter((b) => b.type === "text");
      if (texts.length)
        nodes.push(h("div", { class: "msg user" },
          h("div", { class: "who" }, mi === 0 ? "Objective" : "You"),
          ...texts.map((b) => h("div", { class: "prose", html: md(b.text) }))));
      return;
    }
    m.content.forEach((b, bi) => {
      if (b.type === "reasoning" && b.summary && b.summary.trim()) {
        const key = `think:${mi}:${bi}`;
        nodes.push(keyedDetails(key, { class: "thinking" },
          h("summary", null, "Thinking"),
          h("div", { class: "prose", html: md(b.summary) })));
      } else if (b.type === "text" && b.text.trim()) {
        nodes.push(h("div", { class: "msg assistant prose", html: md(b.text) }));
      } else if (b.type === "tool_use") {
        nodes.push(toolCard(b, events.get(b.id), results.get(b.id), d.live, b.id === awaitingId));
      }
    });
  });

  if (d.pending) nodes.push(approvalCard(d.pending));
  else if (d.live) {
    const lastMsg = s.messages[s.messages.length - 1];
    const label = d.stopping ? "Stopping after the current step…" : lastMsg && lastMsg.role === "assistant" ? "Running…" : "Thinking…";
    nodes.push(h("div", { class: "working" }, h("span", { class: "spinner" }), label));
  } else {
    const status = d.interrupted ? "interrupted" : s.status;
    const text = {
      completed: `Completed after ${s.iterations} iteration${s.iterations === 1 ? "" : "s"}.`,
      failed: "The run failed.",
      stopped: "The run stopped.",
      interrupted: "The run was interrupted (the process exited before it finished). Send a message to resume.",
    }[status];
    const showReport = status !== "completed" && s.report;
    nodes.push(h("div", { class: "end " + status }, h("div", null, text, showReport && h("pre", null, s.report))));
  }
  return nodes;
}

function keyedDetails(key, attrs, ...children) {
  const el = h("details", attrs, ...children);
  el.open = app.openKeys.has(key);
  el.addEventListener("toggle", () => (el.open ? app.openKeys.add(key) : app.openKeys.delete(key)));
  return el;
}

const ICONS = { read_file: "≡", list_directory: "▤", write_file: "✎", execute_command: "❯" };

function toolLabel(name, input) {
  switch (name) {
    case "read_file": return input.path;
    case "list_directory": return (input.path || ".") + (input.recursive ? "  (recursive)" : "");
    case "write_file": return `${input.path}  · ${(input.content || "").split("\n").length} lines`;
    case "execute_command": return "$ " + String(input.command || "").split("\n")[0];
    default: return JSON.stringify(input);
  }
}

function toolCard(b, event, result, live, awaiting) {
  let status, label;
  if (event) { status = event.status; label = event.status; }
  else if (result) { status = result.is_error ? "denied" : "ok"; label = result.is_error ? "not run" : "ok"; }
  else if (awaiting) { status = "awaiting"; label = "awaiting approval"; }
  else if (live) { status = "running"; label = "running"; }
  else { status = "denied"; label = "not run"; }
  const output = event ? event.result : result ? result.content : null;
  const input = b.input || {};

  const body = [];
  if (b.name === "execute_command") body.push(section("Command", h("pre", { class: "out" }, input.command)));
  else if (b.name === "write_file") body.push(section("New content · " + input.path, h("pre", { class: "out" }, input.content)));
  else if (!["read_file", "list_directory"].includes(b.name)) body.push(section("Input", h("pre", { class: "out" }, JSON.stringify(input, null, 2))));
  if (output != null) body.push(section("Result", h("pre", { class: "out" }, output || "(empty)")));

  return keyedDetails("tool:" + b.id, { class: "tool" },
    h("summary", null,
      h("span", { class: "icon" }, ICONS[b.name] || "•"),
      h("span", { class: "label" }, h("span", { class: "name" }, b.name), toolLabel(b.name, input)),
      h("span", { class: "chip " + status }, label)),
    h("div", { class: "body" }, body));
}

const section = (title, ...children) => h("div", null, h("h4", null, title), ...children);

function approvalCard(p) {
  const title = { write_file: "Write this file?", execute_command: "Run this command?" }[p.tool] || `Allow ${p.tool}?`;
  const preview = h("div", { class: "diff" });
  for (const line of p.preview.split("\n")) {
    let cls = "";
    if (p.tool === "execute_command") cls = line.startsWith("$ ") ? "cmd" : "hunk";
    else if (/^(\+\+\+|---)\s/.test(line) || /^(create|modify)\s/.test(line)) cls = "file";
    else if (line.startsWith("@@")) cls = "hunk";
    else if (line.startsWith("+")) cls = "add";
    else if (line.startsWith("-")) cls = "del";
    preview.append(h("div", { class: cls }, line || " "));
  }
  const reason = h("input", {
    id: "deny-reason", type: "text", placeholder: "Reason for denying (optional; Airth sees it)",
    value: app.reason, autocomplete: "off",
    oninput: (e) => (app.reason = e.target.value),
    onkeydown: (e) => { if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) { e.preventDefault(); answer(false); } },
  });
  return h("div", { class: "approval", role: "region", "aria-label": "Approval required" },
    h("div", { class: "approval-head" },
      h("h3", null, title),
      h("span", { class: "chip cap-" + p.capability }, p.capability)),
    p.warnings.map((w) => h("div", { class: "approval-warn" }, "⚠ " + w)),
    preview,
    h("div", { class: "approval-actions" },
      h("button", { class: "btn primary", type: "button", onclick: () => answer(true) }, "Approve ", h("kbd", null, modKey + "↵")),
      h("button", { class: "btn danger", type: "button", onclick: () => answer(false) }, "Deny"),
      reason));
}

async function answer(approve) {
  const d = app.run;
  if (!d || !d.pending || app.busy) return;
  app.busy = true;
  try {
    await api(`runs/${encodeURIComponent(app.route.id)}/approval`, {
      approval_id: d.pending.id, approve, reason: approve ? "" : app.reason,
    });
    app.reason = "";
    app.version = null;
    await pollRun();
  } catch (e) {
    app.error = e.message;
  } finally {
    app.busy = false;
  }
}

async function stopRun() {
  try {
    await api(`runs/${encodeURIComponent(app.route.id)}/stop`, {});
    await pollRun();
  } catch (e) {
    app.error = e.message;
  }
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && app.run && app.run.pending) {
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag !== "TEXTAREA") { e.preventDefault(); answer(true); }
  }
});

// ---------- polling ----------

let runTimer = null;

async function pollRun() {
  clearTimeout(runTimer);
  if (app.route.view !== "run" || app.unauthorized) return;
  const id = app.route.id;
  let next = 3000;
  try {
    const q = app.version != null && app.run && app.run.live ? `?since=${app.version}` : "";
    const d = await api(`runs/${encodeURIComponent(id)}${q}`);
    if (app.route.view !== "run" || app.route.id !== id) return;
    const same = !d.unchanged && app.run && !d.live && !app.run.live &&
      d.state.updated_at === app.run.state.updated_at;
    if (!d.unchanged && !same) {
      app.run = d;
      renderRun();
    }
    app.version = d.version;
    if (d.live || (app.run && app.run.live)) next = 600;
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
    if (runEls) runEls.transcript.replaceChildren(h("div", { class: "end failed" }, e.message));
  }
  runTimer = setTimeout(pollRun, next);
}

async function refreshRuns() {
  try {
    app.runs = await api("runs");
    renderSidebar();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

function unauthorized() {
  app.unauthorized = true;
  renderMain();
}

async function boot() {
  $("#new-run-btn").addEventListener("click", () => go("#/new"));
  window.addEventListener("hashchange", onRoute);
  if (app.unauthorized) return renderMain();
  try {
    app.info = await api("info");
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
  }
  await refreshRuns();
  onRoute();
  setInterval(refreshRuns, 2000);
}

boot();
