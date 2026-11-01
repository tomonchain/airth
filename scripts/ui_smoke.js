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
  replaceWith() {} remove() { if (this.parent) this.parent.children = this.parent.children.filter((c) => c !== this); } focus() {} select() {} scrollIntoView() {} closest() { return null; }
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
for (const id of ["app", "main", "realm-switch", "side-nav", "tabbar", "sidebar-foot", "side-close", "side-open"]) {
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
  location: { hash: "", pathname: "/", hostname: "127.0.0.1" }, history: { replaceState() {} },
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
  { id: "r1", title: null, objective: "First run", status: "completed", live: false, updated_at: now, needs_approval: false, change: null, checks_passed: null, plan_done: 0, plan_total: 0, routine: "weekly-article" },
  { id: "r2", title: "Named", objective: "Second", status: "failed", live: true, updated_at: now, needs_approval: true, change: "open", checks_passed: false, plan_done: 1, plan_total: 3, routine: null },
];
const realmDetail = {
  name: "acme", description: "", root: "/tmp/acme", own_root: true, isolation: "none", policy: { read: "allow", write: "ask", execute: "ask", memory: "allow", network: "ask", skill: "ask" },
  checks: [], share_owner_profile: true, archived: false, default: true, model: null, model_error: null,
  memory: [{ id: "m1", text: "fact", source: "owner", created_at: now }], secrets: [{ key: "K", provider: false }],
  integrations: [{ name: "gh", base_url: "https://api.github.com", description: null, headers: {}, missing_secrets: [] }],
};
const fixtures = {
  "usage?days=30": { days: 30, window: { input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }, today: { input: 0, output: 0, cache_read: 0, cache_write: 0, calls: 0, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }, month: { input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } },
    by_day: [{ date: now.slice(0, 10), input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_realm: [{ name: "acme", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_model: [{ name: "unrecorded", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_purpose: [{ name: "work", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }], by_source: [{ name: "chats", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }, { name: "objectives", input: 4, output: 1, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }],
    top_runs: [{ realm: "acme", id: "r1", objective: "", input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } }] },
  catalog: { source: "https://github.com/anthropics/skills", fetched_at: null, error: null, refreshing: false, skills: [{ name: "pdf", title: "", description: "", files: 2, skipped: 0 }] },
  "realms/acme/catalog": { installed: [], installing: [], errors: {} },
  "realms/acme/mcp": [{ name: "gh", command: "npx x", description: "", enabled: true, connecting: false, server_info: null, connected_at: null, error: null, tools: [] }],
  "realms/acme/connectors": { check: null, connectors: [
    { connected: true, connector: { id: "notion", name: "Notion", category: "Productivity", description: "Pages", docs: "https://d", kind: "http", base_url: "https://api.notion.com", secret: "NOTION_TOKEN", auth: ["Authorization", "Bearer {}"], extra: [], token_url: "https://t", token_help: "Make one." } },
  ] },
  owner: { name: "", about: "", preferences: [], updated_at: null },
  // The business: a pipeline that ran, one refused, one never run.
  "realms/acme/business": {
    pipelines: [
      { name: "watch", description: "Morning shortlist", schedule: "daily 07:00", steps: ["read 2 feed(s)", "report \"Watch\""], next_at: now,
        runs: [{ started_at: now, finished_at: now, ok: true, items_read: 4, items_kept: 2, report: "# Watch\n\n- [a](https://e.com)", notes: "", error: null },
          { started_at: now, finished_at: now, ok: false, items_read: 0, items_kept: 0, report: null, notes: "", error: "no feed could be read" }] },
      { name: "broken", error: "step 2: unknown field", runs: [] },
      { name: "replies", description: "", schedule: null, steps: ["read 1 feed(s)"], next_at: null, runs: [] }] },
  "realms/acme/steering": {
    approvals: [{ run: "r1", objective: "Publish the article", routine: "weekly-article", approval_id: 4, tool: "execute_command", capability: "execute", preview: "$ git push", warnings: [] }],
    drafts: [{ id: 1, created_at: now, pipeline: "production", kind: "post", text: "ETH left the queue.", original: "ETH left the queue.",
      source: { title: "Flows", link: "javascript:alert(1)" }, model_call: 3, status: "pending", revision: 1 }],
    attention: [{ kind: "pipeline", title: "watch failed: no feed could be read", at: now }, { kind: "routine", routine: "digest", title: "digest stopped", run: null, at: null }],
    strategy: "",
    delivered: [{ run: "r1", title: "Write this week's article", routine: "weekly-article", agent: "writer", at: now, report: "Drafted *Liquid staking* on the blog." },
      { run: "r2", title: "Tidy", routine: null, agent: null, at: now, report: "" }],
    team: [{ name: "writer", running: false, routines: 1, last_status: "completed", last_at: now, next_at: now },
      { name: "analyst", running: true, routines: 1, last_status: null, last_at: null, next_at: null },
      { name: "sales", running: false, routines: 0, last_status: null, last_at: null, next_at: null }],
    week: { owner_seconds: 6000, approved: 8, unedited: 5, rejected: 2, model_calls: 40, model_cost: 1.8, unpriced_calls: 0 } },
  "realms/acme/agents": { agents: [
    { name: "writer", job: "Write one article a week.", version: "abcdef123456", month_cost: 1.2, month_calls: 30, unpriced: 0,
      routines: [{ id: "weekly-article", objective: "Write this week's article", trigger: "schedule: mon 08:00", enabled: true, tools: ["blog"], approvals: ["network"], budget: "30 minutes", last_status: null, next_at: null }],
      tasks: [{ id: "r1", objective: "Write this week's article", status: "completed", updated_at: now }] },
    { name: "analyst", job: "Watch the markets.", version: "123456abcdef", month_cost: 0, month_calls: 0, unpriced: 2, routines: [], tasks: [] }] },
  "realms/acme/routines": { agents: ["writer"], extensions: ["blog", "gh", "mac"], routines: [
    { id: "weekly-article", objective: "Write this week's article", every: "mon 08:00", trigger: "schedule: mon 08:00", bot: "writer", tools: ["blog"], allow: ["network", "network:gh"], delegates: ["cleanup"],
      max_minutes: 30, wait: 10, enabled: true, running: false, next_at: now, last_run: "r1", last_at: now, last_status: "completed" },
    { id: "cleanup", objective: "Tidy the files", every: null, trigger: "manual", bot: null, tools: null, allow: [], delegates: [], max_minutes: null, wait: 10, enabled: false, running: false, next_at: null, last_run: null, last_at: null, last_status: null }] },
  "realms/acme/planning": { coming: [{ at: now, name: "weekly-article", kind: "routine", bot: "writer" }, { at: now, name: "watch", kind: "pipeline", bot: null }],
    past: [{ at: now, name: "watch", kind: "pipeline", status: "failed" }, { at: now, name: "weekly-article", kind: "routine", status: "completed", run: "r1" }] },
  phone: { hosts: [{ host: "mac.tail1234.ts.net", link: "https://mac.tail1234.ts.net/#token=x", qr: { width: 3, cells: "101010101" } }] },
  setup: { realms: 1, model_ready: false },
  "inbox/read": {},
};

{
  const share = (name) => ({ name, input: 10, output: 2, cache_read: 0, cache_write: 0, calls: 1, peak_input: 10, unpriced: 0, cost: { input: 0.01, cache_read: 0, cache_write: 0, output: 0.02 } });
  fixtures["usage?days=30&realm=acme"] = { ...fixtures["usage?days=30"], by_routine: [share("weekly-article"), share("cleanup"), share("")],
    agents: { "weekly-article": "writer" }, pipelines: [{ pipeline: "watch", calls: 113, input_tokens: 49000, output_tokens: 54000, cost_usd: 0, unpriced: 113 }] };
}

// ---------- load the app (without booting it) ----------
// The page's app.js: src/ui/js joined in name order, as build.rs does.
const jsDir = path.join(__dirname, "..", "src", "ui", "js");
const src = fs.readdirSync(jsDir).filter((f) => f.endsWith(".js")).sort()
  .map((f) => fs.readFileSync(path.join(jsDir, f), "utf8")).join("").replace(/\nboot\(\);\s*$/, "\n");
const exportsCode = `\n;globalThis.__app = { app, skillUi, routineUi, welcomeUi, WELCOME_STEPS, connUi, transcriptNodes, sideNodes, approvalCard, renderRunList, renderNav, renderTabbar, openMoreSheet, closeMoreSheet, renderFoot, renderRealmSwitch, renderMain, renderRunShell, renderRun,
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
  A.renderNav(); check("nav", body.querySelector("#side-nav"));
  A.renderTabbar(); check("tab bar", body.querySelector("#tabbar"));
  if (body.querySelector("#tabbar").children.length !== 5) problems.push("tab bar: five tabs expected");
  A.openMoreSheet();
  const more = body.querySelector("#more-sheet");
  if (!more || !more.textContent.includes("Routines") || !more.textContent.includes("perso")) problems.push("more sheet: incomplete");
  else check("more sheet", more);
  A.closeMoreSheet();
  if (body.querySelector("#more-sheet")) problems.push("more sheet: not closed");
  A.renderFoot(); check("foot", body.querySelector("#sidebar-foot"));
  A.renderRealmSwitch(); check("realm switch", body.querySelector("#realm-switch"));
  A.app.route = { view: "run", realm: "acme", id: state.id };
  A.app.run = live;
  A.renderMain(); A.renderRun(); check("run view", main);
  check("approval", A.approvalCard({ id: 1, tool: "write_file", capability: "write", preview: "create a.md", warnings: [], grantable: false, grant_label: null }));
  // Tasks: the business's runs, under the box that starts one.
  A.app.route = { view: "tasks", realm: "acme" };
  A.renderMain();
  if (!body.querySelector("#task-list") || body.querySelector("#task-list").children.length === 0) problems.push("tasks: the task list is empty");
  check("tasks", main);
  for (const view of ["steering", "agents", "routines", "planning", "new", "settings", "owner", "skills", "usage", "new-realm"]) {
    A.app.route = { view, realm: "acme" };
    A.renderMain();
    await new Promise((r) => setTimeout(r, 30));
    if (main.textContent.trim().length < 40) problems.push(`${view}: rendered (almost) nothing`);
    check(view, main);
  }
  // A draft's source link from a feed is followed only when it is a web link.
  A.app.route = { view: "steering", realm: "acme" };
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  if (!main.textContent.includes("ETH left the queue.")) problems.push("steering: the draft is missing");
  if (main.textContent.includes("Flows")) problems.push("steering: a javascript: link from a feed was rendered");
  if (!main.textContent.includes("$ git push")) problems.push("steering: the approval is missing");
  if (!main.textContent.includes("watch failed")) problems.push("steering: what needs attention is missing");
  if (!main.textContent.includes("Drafted *Liquid staking*") || !main.textContent.includes("working now")) problems.push("steering: the delivered work or the team is missing");
  // Usage of one business: by agent and routine, and its pipelines.
  A.app.route = { view: "usage" };
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  for (const want of ["By agent", "writer", "Asked by you", "Pipelines", "113 without a known price"])
    if (!main.textContent.includes(want)) problems.push(`usage: "${want}" is missing`);
  // Pairing the phone.
  A.app.route = { view: "owner", tab: "phone" };
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  if (!main.textContent.includes("mac.tail1234.ts.net") || !main.textContent.includes("tailscale serve")) problems.push("phone: the pairing is missing");
  check("phone", main);
  // Onboarding: every step, then an agent being hired and its routine.
  A.app.route = { view: "welcome" };
  for (let i = 0; i < A.WELCOME_STEPS.length; i++) {
    Object.assign(A.welcomeUi, { step: i, realm: i >= 4 ? "acme" : null, setup: null });
    A.renderMain();
    await new Promise((r) => setTimeout(r, 30));
    if (main.textContent.trim().length < 40) problems.push(`welcome/${A.WELCOME_STEPS[i]}: rendered (almost) nothing`);
    check("welcome/" + A.WELCOME_STEPS[i], main);
  }
  Object.assign(A.welcomeUi, { step: A.WELCOME_STEPS.indexOf("agent"), template: { name: "writer", title: "Writer", job: "Write.", task: "Write it.", every: "mon 08:00" } });
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  if (!main.textContent.includes("Hire a writer")) problems.push("welcome: hiring from a template is missing");
  Object.assign(A.welcomeUi, { step: A.WELCOME_STEPS.indexOf("routine"), agent: "writer", routine: { task: "Write it.", every: "mon 08:00" } });
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  if (!main.textContent.includes("When writer works")) problems.push("welcome: the routine step is missing");
  check("welcome/routine+agent", main);
  Object.assign(A.welcomeUi, { step: 0, realm: null, agent: null, template: null, routine: null, setup: null });
  // The routine form, with the business's agents and extensions.
  A.app.route = { view: "routines", realm: "acme" };
  A.routineUi.form = {};
  A.renderMain();
  await new Promise((r) => setTimeout(r, 30));
  if (!main.textContent.includes("New routine") || !main.textContent.includes("Call gh")) problems.push("routines: the form is incomplete");
  if (!main.textContent.includes("It may ask for work") || !main.textContent.includes("may ask: cleanup")) problems.push("routines: delegation is missing");
  if (!main.textContent.includes("Morning shortlist")) problems.push("routines: the pipelines are missing");
  check("routines+form", main);
  A.routineUi.form = null;
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
  // A change that edits the business's config says so before it is adopted.
  A.app.run = { ...live, config: { files: ["agents/writer.md"], agents: ["writer"] },
    state: { ...state, change: { status: "open", base: "a1", head: "b2", base_branch: "main", branch: "airth/x", files: ["M\tagents/writer.md"], checks: null, conflicts: [], merging: null, pull_request: null } } };
  A.renderMain(); A.renderRun();
  if (!main.textContent.includes("edits the business's config") || !main.textContent.includes("Agents: writer")) problems.push("review: a config change is not flagged");
  check("change editing the config", main);
  if (problems.length) { console.log(problems.join("\n")); process.exit(1); }
  console.log("ui smoke: no leaked null/undefined/false/true/NaN in rendered text");
  process.exit(0); // pages that follow a live run keep polling
})().catch((e) => { console.error("ui smoke crashed:", e); process.exit(2); });
