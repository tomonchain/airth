#!/usr/bin/env node
// UI smoke test: render the app's views in a tiny fake DOM, with data full of
// empty fields, and fail if any visible text says null, undefined, false,
// true, NaN or [object ...]. Run: node scripts/ui_smoke.js
"use strict";
const fs = require("fs");
const path = require("path");

// ---------- a fake DOM, just enough for app.js ----------
class FakeNode {
  constructor(tag) {
    this.tag = tag; this.children = []; this.attrs = {}; this.className = ""; this.dataset = {}; this.style = {};
    this.parent = null; this.value = ""; this.disabled = false; this.open = false;
    const self = this;
    this.classList = {
      add: (...c) => { self.className = [...new Set((self.className + " " + c.join(" ")).trim().split(/\s+/))].join(" "); },
      remove: (...c) => { self.className = self.className.split(/\s+/).filter((x) => !c.includes(x)).join(" "); },
      toggle: (c, on) => { const has = self.classList.contains(c); if (on ?? !has) self.classList.add(c); else self.classList.remove(c); return !has; },
      contains: (c) => self.className.split(/\s+/).includes(c),
    };
  }
  // Like the real DOM: anything that is not a node becomes text (null → "null").
  append(...items) { for (const x of items) { const n = x instanceof FakeNode ? x : new FakeText(String(x)); n.parent = this; this.children.push(n); } }
  replaceChildren(...items) { this.children = []; this.append(...items); }
  insertBefore(n) { n.parent = this; this.children.unshift(n); }
  setAttribute(k, v) { this.attrs[k] = String(v); }
  getAttribute(k) { return this.attrs[k]; }
  addEventListener() {}
  set innerHTML(v) { this.children = [new FakeText(String(v).replace(/<[^>]*>/g, " "))]; }
  get textContent() { return this.children.map((c) => c.textContent).join(" "); }
  set textContent(v) { this.children = [new FakeText(String(v))]; }
  querySelector(sel) { return find(this, sel, true)[0] || null; }
  querySelectorAll(sel) { return find(this, sel, false); }
  replaceWith() {} remove() {} focus() {} select() {} scrollIntoView() {} closest() { return null; }
  contains() { return false; }
  get offsetWidth() { return 0; }
}
class FakeText extends FakeNode {
  constructor(t) { super("#text"); this.text = t; }
  get textContent() { return this.text; }
}
function find(root, sel, first) {
  const out = [];
  const match = (n) => (sel.startsWith(".") ? n.classList.contains(sel.slice(1)) : sel.startsWith("#") ? n.attrs.id === sel.slice(1) : n.tag === sel);
  const walk = (n) => { for (const c of n.children || []) { if (!(c instanceof FakeText) && match(c)) { out.push(c); if (first) return true; } if (walk(c)) return true; } return false; };
  walk(root);
  return out;
}
const body = new FakeNode("body");
for (const id of ["app", "main", "realm-switch", "side-nav", "run-list", "sidebar-foot", "side-close", "side-open"]) {
  const n = new FakeNode("div"); n.attrs.id = id; body.append(n);
}
const storage = { getItem: () => null, setItem() {} };
Object.assign(globalThis, {
  Node: FakeNode,
  Element: FakeNode,
  window: globalThis,
  document: {
    createElement: (t) => new FakeNode(t), createTextNode: (t) => new FakeText(t), addEventListener() {},
    querySelector: (s) => body.querySelector(s), body, activeElement: null, contains: () => false, title: "",
    documentElement: new FakeNode("html"),
  },
  location: { hash: "", pathname: "/" }, history: { replaceState() {} },
  sessionStorage: storage, localStorage: storage,
  matchMedia: () => ({ matches: false, addEventListener() {} }),
  fetch: async () => ({ status: 401, json: async () => ({}) }),
});
Object.defineProperty(globalThis, "navigator", { value: { platform: "MacIntel", language: "fr-FR" } });

// ---------- fixtures: realistic shapes, with empty fields everywhere ----------
const now = new Date().toISOString();
const state = {
  id: "20261012T120000.000Z", objective: "Ship the kit\nwith details", status: "completed", iterations: 3,
  input_tokens: 1200, output_tokens: 300, updated_at: now, origin: null, skill: null, change: null, report: null,
  plan: [{ title: "Read", status: "done", note: null }, { title: "Write", status: "in_progress", note: "half" }, { title: "Test", status: "pending" }],
  usage: [{ at: now, model: "m", input: 1000, output: 100, cache_read: 800, cache_write: 0 }],
  events: [{ tool_use_id: "t1", status: "ok", result: "file text" }, { tool_use_id: "t2", status: "error", result: null }],
  messages: [
    { role: "user", content: [{ type: "text", text: "Ship the kit" }] },
    { role: "assistant", content: [
      { type: "reasoning", summary: "Thinking it over" },
      { type: "tool_use", id: "t1", name: "read_file", input: { path: "a.md" } },
      { type: "tool_use", id: "t2", name: "execute_command", input: { command: null } },
      { type: "tool_use", id: "t3", name: "mcp__gh__issue", input: {} },
      { type: "tool_use", id: "t4", name: "update_plan", input: {} },
    ] },
    { role: "user", content: [{ type: "tool_result", tool_use_id: "t3", content: null, is_error: true }] },
    { role: "assistant", content: [{ type: "text", text: "Done. | a | b |\n|---|---|\n| 1 | 2 |" }] },
  ],
};
const runs = [
  { id: "r1", title: null, objective: "First run", status: "completed", live: false, updated_at: now, needs_approval: false, change: null, checks_passed: null, plan_done: 0, plan_total: 0, focus: false },
  { id: "r2", title: "Named", objective: "Second", status: "failed", live: true, updated_at: now, needs_approval: true, change: "open", checks_passed: false, plan_done: 1, plan_total: 3, focus: null },
];
const realmDetail = {
  name: "acme", description: "", root: "/tmp/acme", own_root: true, isolation: "none", policy: { read: "allow", write: "ask", execute: "ask", memory: "allow", network: "ask", skill: "ask", apps: "deny" },
  checks: [], share_owner_profile: true, archived: false, default: true, model: null, model_error: null,
  memory: [{ id: "m1", text: "fact", source: "owner", created_at: now }], secrets: [{ key: "K", provider: false }],
  integrations: [{ name: "gh", base_url: "https://api.github.com", description: null, headers: {}, missing_secrets: [] }],
};
const fixtures = {
  "realms/acme/objectives": [{ id: "o1", objective: "Daily", enabled: true, trigger_label: "daily 08:00", next_at: null, running: false, last_status: null, last_run: null, last_at: null, approvals: [], approval_wait_minutes: 10, budget: {}, webhook_url: null }],
  "realms/acme/skills": [{ name: "wc", status: "proposed", description: "Count", reason: null, proposed_by: "owner", dev_run: null, evaluation: null, installed: null, developing_now: false, summary: null }],
  inbox: [{ id: 1, kind: "completed", realm: "acme", run: null, title: "Done", body: null, read: false, created_at: now }],
  "summary?hours=24": [{ realm: "acme", runs: 1, completed: 1, failed: 0, stopped: 0, to_review: [], unfinished: [], recent: [{ id: "r1", objective: "x", origin: null, report: null }], objectives: [] }],
  "usage?days=30": { days: 30, window: { input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1 }, today: { input: 0, output: 0, cache_read: 0, cache_write: 0, calls: 0 }, month: { input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1 },
    by_day: [{ date: now.slice(0, 10), input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1 }], by_realm: [{ name: "acme", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1 }], by_model: [{ name: "unrecorded", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1 }],
    top_runs: [{ realm: "acme", id: "r1", objective: "", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1 }] },
  catalog: { source: "https://github.com/anthropics/skills", fetched_at: null, error: null, refreshing: false, skills: [{ name: "pdf", title: "", description: "", files: 2, skipped: 0 }] },
  "realms/acme/catalog": { installed: [], installing: [], errors: {} },
  "realms/acme/mcp": [{ name: "gh", command: "npx x", description: "", enabled: true, connecting: false, server_info: null, connected_at: null, error: null, tools: [] }],
  "realms/acme/connectors": { check: null, apps_rule: "ask", connectors: [
    { connected: true, connector: { id: "notion", name: "Notion", category: "Productivity", description: "Pages", docs: "https://d", kind: "http", base_url: "https://api.notion.com", secret: "NOTION_TOKEN", auth: ["Authorization", "Bearer {}"], extra: [], token_url: "https://t", token_help: "Make one." } },
    { connected: false, connector: { id: "google", name: "Google", category: "Productivity", description: "Via the Mac", docs: "https://d", kind: "mac", setup_url: "x-apple.systempreferences:x" } },
  ] },
  owner: { name: "", about: "", preferences: [], goals: [], commitments: [], boundaries: [], updated_at: null },
};

// ---------- load the app (without booting it) ----------
const src = fs.readFileSync(path.join(__dirname, "..", "src", "ui", "app.js"), "utf8").replace(/\nboot\(\);\s*$/, "\n");
const exportsCode = `\n;globalThis.__app = { app, skillUi, connUi, transcriptNodes, sideNodes, approvalCard, renderRunList, renderNav, renderFoot, renderRealmSwitch, renderMain, renderRunShell, renderRun,
  setApi: (f) => { api = f; } };`;
(0, eval)(src + exportsCode);
const A = globalThis.__app;
A.setApi(async (p) => { if (p in fixtures) return fixtures[p]; throw new Error("no fixture for " + p); });
Object.assign(A.app, { realm: "acme", realms: [{ name: "acme", description: "", archived: false, default: true, live: 1, needs_approval: 0 }, { name: "perso", description: null, archived: false, live: 0, needs_approval: 1 }],
  realmDetail, runs, unauthorized: false, info: { version: "0.1.0", unread: 0, sandbox: true, tools: [{ name: "read_file", capability: "read", description: "Read a file. More." }], update: null } });

// ---------- render, then look for leaked values ----------
const BAD = /\b(null|undefined|false|true|NaN)\b|\[object /;
const problems = [];
function check(label, nodes) {
  for (const n of [].concat(nodes)) {
    const walk = (x, trail) => {
      if (x instanceof FakeText) { if (BAD.test(x.text) && !/^\s*<html>/.test(x.text)) problems.push(`${label}: "${x.text.trim().slice(0, 80)}" in ${trail}`); return; }
      if (x.tag === "pre" || x.tag === "code") return; // raw tool output may legitimately say null
      for (const c of x.children) walk(c, trail + ">" + x.tag + (x.className ? "." + x.className.split(" ")[0] : ""));
    };
    walk(n, "");
  }
}
const main = body.querySelector("#main");
(async () => {
  const live = { state, live: false, pending: null, checks_required: false, title: null, version: 0 };
  A.renderRunList(); check("sidebar runs", body.querySelector("#run-list"));
  A.renderNav(); check("nav", body.querySelector("#side-nav"));
  A.renderFoot(); check("foot", body.querySelector("#sidebar-foot"));
  A.renderRealmSwitch(); check("realm switch", body.querySelector("#realm-switch"));
  A.app.route = { view: "run", realm: "acme", id: state.id };
  A.app.run = live;
  A.renderMain(); A.renderRun(); check("run view", main);
  check("approval", A.approvalCard({ id: 1, tool: "write_file", capability: "write", preview: "create a.md", warnings: [], grantable: false, grant_label: null }));
  for (const view of ["new", "settings", "owner", "objectives", "skills", "inbox", "usage", "new-realm"]) {
    A.app.route = { view, realm: "acme" };
    A.renderMain();
    await new Promise((r) => setTimeout(r, 30));
    if (main.textContent.trim().length < 40) problems.push(`${view}: rendered (almost) nothing`);
    check(view, main);
  }
  A.app.route = { view: "skills", realm: "acme" };
  for (const tab of ["catalog", "mcp", "connectors"]) {
    A.skillUi.tab = tab;
    A.renderMain();
    await new Promise((r) => setTimeout(r, 30));
    if (main.textContent.trim().length < 40) problems.push(`skills/${tab}: rendered (almost) nothing`);
    check("skills/" + tab, main);
  }
  // Registry results, one with an address that is not a URL (entries are not vetted).
  A.connUi.query = "lin";
  A.connUi.registry = [
    { name: "app.linear/linear", title: "Linear", url: "https://mcp.linear.app/mcp", description: null, website: null },
    { name: "x/broken", title: null, url: "not a url", description: "Broken", website: null },
  ];
  A.skillUi.tab = "connectors";
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  if (!main.textContent.includes("From the MCP Registry")) problems.push("connectors: registry results not shown");
  check("skills/connectors+registry", main);
  // A live run waiting for approval, with a model picked for it.
  A.app.route = { view: "run", realm: "acme", id: state.id };
  A.app.run = { ...live, live: true, state: { ...state, status: "running", model_choice: "deepseek/deepseek-chat" },
    pending: { id: 2, tool: "execute_command", capability: "execute", preview: "$ cargo test", warnings: [], grantable: true, grant_label: "commands" } };
  A.renderMain(); A.renderRun();
  if (!main.textContent.includes("deepseek-chat")) problems.push("run: the picked model is not shown in the model menu");
  if (!main.textContent.includes("Run this command?")) problems.push("run: the approval card is missing");
  check("live run + approval", main);
  if (problems.length) { console.log(problems.join("\n")); process.exit(1); }
  console.log("ui smoke: no leaked null/undefined/false/true/NaN in rendered text");
})().catch((e) => { console.error("ui smoke crashed:", e); process.exit(2); });
