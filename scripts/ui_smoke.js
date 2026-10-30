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
  prepend(...items) { const rest = this.children; this.children = []; this.append(...items); this.children.push(...rest); }
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
  addEventListener() {},
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
  "realms/acme/objectives": [{ id: "o1", objective: "Daily", enabled: true, trigger: { kind: "schedule", every: "daily 08:00" }, trigger_label: "daily 08:00", next_at: null, running: false, last_status: null, last_run: null, last_at: null, approvals: [], approval_wait_minutes: 10, budget: {}, webhook_url: null }],
  "realms/acme/skills": [{ name: "wc", status: "proposed", description: "Count", reason: null, proposed_by: "owner", dev_run: null, evaluation: null, installed: null, developing_now: false, summary: null }],
  inbox: [{ id: 1, kind: "completed", realm: "acme", run: null, title: "Done", body: null, read: false, created_at: now }],
  "summary?hours=24": [{ realm: "acme", runs: 1, completed: 1, failed: 0, stopped: 0, to_review: [], unfinished: [], recent: [{ id: "r1", objective: "x", origin: null, report: null }], objectives: [] }],
  "usage?days=30": { days: 30, window: { input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }, today: { input: 0, output: 0, cache_read: 0, cache_write: 0, calls: 0, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }, month: { input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } },
    by_day: [{ date: now.slice(0, 10), input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_realm: [{ name: "acme", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_model: [{ name: "unrecorded", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_purpose: [{ name: "work", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_source: [{ name: "chats", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }, { name: "objectives", input: 4, output: 1, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }],
    top_runs: [{ realm: "acme", id: "r1", objective: "", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }] },
  catalog: { source: "https://github.com/anthropics/skills", fetched_at: null, error: null, refreshing: false, skills: [{ name: "pdf", title: "", description: "", files: 2, skipped: 0 }] },
  "realms/acme/catalog": { installed: [], installing: [], errors: {} },
  "realms/acme/mcp": [{ name: "gh", command: "npx x", description: "", enabled: true, connecting: false, server_info: null, connected_at: null, error: null, tools: [] }],
  "realms/acme/connectors": { check: null, apps_rule: "ask", connectors: [
    { connected: true, connector: { id: "notion", name: "Notion", category: "Productivity", description: "Pages", docs: "https://d", kind: "http", base_url: "https://api.notion.com", secret: "NOTION_TOKEN", auth: ["Authorization", "Bearer {}"], extra: [], token_url: "https://t", token_help: "Make one." } },
    { connected: false, connector: { id: "google", name: "Google", category: "Productivity", description: "Via the Mac", docs: "https://d", kind: "mac", setup_url: "x-apple.systempreferences:x" } },
  ] },
  owner: { name: "", about: "", preferences: [], goals: [], commitments: [], boundaries: [], updated_at: null },
};
// Benchmarks: a finished run with a regression and a security violation, one running, unknown costs.
const benchSummary = (rate) => ({ executed: 2, passed: rate ? 1 : 0, skipped: 1, success_rate: rate, recovery_rate: null, avg_cost_usd: null, avg_duration_ms: 1500,
  security: { prevented: 0, succeeded: 1, inconclusive: 0 }, security_violation: true, by_category: { coding: { executed: 2, passed: 1, skipped: 0 }, security: { executed: 0, passed: 0, skipped: 1 } } });
const benchEnv = { airth_version: "0.1.4", commit: null, dirty: false, os: "macos", arch: "aarch64", sandbox: false, model: "anthropic/claude-x", model_config: {} };
const benchRun = (id, status, rate) => ({ id, suite: "core", suite_version: "1.0.0", status, started_at: now, finished_at: null, environment: benchEnv,
  options: { tasks: [], repeat: 2, keep: false }, planned: 3, done: 2, current: status === "running" ? "coding-001 (1/1)" : null, error: null, here: true, summary: benchSummary(rate) });
const benchMetrics = { duration_ms: 1500, model_calls: 2, input_tokens: 10, output_tokens: 2, cache_read_tokens: 0, cache_write_tokens: 0, tool_calls: 1, tool_failures: 0,
  permission_denials: 0, retries: 0, repeated_actions: 0, compactions: 0, model_switches: 0, context: [], models: [], budget_exhausted: false, interrupted: false };
const benchResult = { task: "t1", category: "coding", title: "", attempt: 1, outcome: "FAIL", reason: "", security: null, checks: [{ check: "a exists", passed: false, detail: "" }],
  metrics: benchMetrics, run_status: null, report: null, files: [], trace: true, started_at: now };
const benchCompare = { a: { id: "b0", suite: "core", suite_version: "1", airth_version: "0.1.3", commit: null, model: "m", started_at: now, summary: { ...benchSummary(1), tasks: [] } },
  b: { id: "b1", suite: "core", suite_version: "1", airth_version: "0.1.4", commit: null, model: "m", started_at: now, summary: { ...benchSummary(0.5), tasks: [] } },
  tasks: [{ task: "t1", a: [1, 1], b: null, change: "not_comparable", mean_duration_ms: [null, 1000], mean_cost_usd: [null, null] }],
  regressions: [{ kind: "task", message: "t1: passed 1/1 → 0/1", confirmed: false }], improvements: [], caveats: ["different models (a and b)"] };
Object.assign(fixtures, {
  bench: { enabled: true, sandbox: false, baseline: null, max_repeat: 20, comparison: benchCompare,
    suites: [{ name: "core", version: "1.0.0", description: "", builtin: true, tasks: [{ id: "t1", category: "coding", title: "", unavailable: "it needs cargo" }] }],
    runs: [benchRun("b2", "running", null), benchRun("b1", "completed", 0.5), benchRun("b0", "interrupted", null)] },
  "bench/runs/b1": { run: benchRun("b1", "completed", 0.5), results: [benchResult, { ...benchResult, outcome: "SKIPPED", trace: false, security: "succeeded" }],
    summary: { ...benchSummary(0.5), tokens: { input: 0, output: 0, cache_read: 0, cache_write: 0, model_calls: 0, tool_calls: 0 },
      tasks: [{ task: "t1", category: "coding", attempts: 2, executed: 1, passed: 0, mean_duration_ms: null, sd_duration_ms: null, mean_cost_usd: null }] }, baseline: null },
  "bench/runs/b1/tasks/t1/1": { task: {}, objective: "Fix it", result: benchResult,
    run: { status: "completed", report: null, plan: [], events: [{ action: "read_file", arguments: { path: "x" }, result: "null", status: "error" }] } },
  "bench/compare?a=b0&b=b1": benchCompare,
});

// ---------- load the app (without booting it) ----------
// The page's app.js: src/ui/js joined in name order, as build.rs does.
const jsDir = path.join(__dirname, "..", "src", "ui", "js");
const src = fs.readdirSync(jsDir).filter((f) => f.endsWith(".js")).sort()
  .map((f) => fs.readFileSync(path.join(jsDir, f), "utf8")).join("").replace(/\nboot\(\);\s*$/, "\n");
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
  A.app.info.benchmarks = true;
  A.renderNav();
  if (!body.querySelector("#side-nav").textContent.includes("Benchmarks")) problems.push("nav: Benchmarks missing once enabled");
  for (const route of [{ view: "owner", tab: "advanced" }, { view: "bench" }, { view: "bench", id: "b1" },
    { view: "bench", id: "b1", task: "t1", attempt: 1 }, { view: "bench", compare: true, a: "b0", b: "b1" }]) {
    A.app.route = route;
    A.renderMain();
    await new Promise((r) => setTimeout(r, 30));
    const label = JSON.stringify(route);
    if (main.textContent.trim().length < 40) problems.push(`${label}: rendered (almost) nothing`);
    check(label, main);
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
  process.exit(0); // pages that follow a live run keep polling
})().catch((e) => { console.error("ui smoke crashed:", e); process.exit(2); });
