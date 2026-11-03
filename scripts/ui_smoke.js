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
  addEventListener(e) { (this.events = this.events || []).push(e); }
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
  smokeNext();
  if (problems.length) { console.log(problems.join("\n")); process.exit(1); }
  console.log("ui smoke: no leaked null/undefined/false/true/NaN in rendered text");
  process.exit(0); // pages that follow a live run keep polling
})().catch((e) => { console.error("ui smoke crashed:", e); process.exit(2); });

// ---------- the new interface (/next, decision 57), in a context of its own ----------
// Its scripts declare the same names as the current one's, so they run in
// a separate global, with the same fake DOM.
function smokeNext() {
  const vm = require("vm");
  // The current interface's scripts patched FakeNode's append for their
  // own use; the new interface's elements start from the plain DOM's
  // behaviour, which turns a list into text, so its own patch is tested.
  class NextNode extends FakeNode {}
  NextNode.prototype.append = function (...items) { for (const x of items) { const n = x instanceof FakeNode ? x : new FakeText(String(x)); n.parent = this; this.children.push(n); } };
  NextNode.prototype.replaceChildren = function (...items) { this.children = []; this.append(...items); };
  const nbody = new NextNode("body");
  for (const id of ["top", "rail", "panel"]) { const n = new NextNode("div"); n.attrs.id = id; nbody.append(n); }
  const ctx = {
    Node: FakeNode, Element: NextNode, Date, console, storage,
    document: { createElement: (t) => new NextNode(t), createTextNode: (t) => new FakeText(t), addEventListener() {}, querySelector: (s) => nbody.querySelector(s) },
    location: { hash: "" }, window: { addEventListener() {} },
    sessionStorage: storage, localStorage: storage,
  };
  const dir = path.join(__dirname, "..", "src", "ui", "next", "js");
  const code = fs.readdirSync(dir).filter((f) => f.endsWith(".js")).sort()
    .map((f) => fs.readFileSync(path.join(dir, f), "utf8")).join("\n").replace(/\nboot\(\);\s*$/, "\n");
  vm.createContext(ctx);
  vm.runInContext(code + "\n;globalThis.__next = { next, renderAll, PAGES, PINNED };", ctx);
  const N = ctx.__next;
  // The default page takes Airth's key from the link it prints (/#token=…).
  {
    const kept = {};
    const linkCtx = { ...ctx, location: { hash: "#token=abc123", hostname: "127.0.0.1" }, history: { replaceState(_, __, h) { kept.hash = h; } },
      sessionStorage: { getItem: () => null, setItem: (k, v) => { kept[k] = v; } }, localStorage: { getItem: () => null, setItem() {} } };
    vm.createContext(linkCtx);
    vm.runInContext(code + "\n;globalThis.__token = token;", linkCtx);
    if (linkCtx.__token !== "abc123" || kept["airth-token"] !== "abc123" || kept.hash !== "#/hq") problems.push("next: the key in Airth's link is not taken");
  }
  const realms = [{ name: "acme", description: "", archived: false, default: true }, { name: "perso", description: null, archived: false }];
  const show = (label, state) => {
    Object.assign(N.next, { realms, realm: "acme", unauthorized: false, menuOpen: false }, state);
    N.renderAll();
    check(`next/${label}`, nbody);
    if (nbody.textContent.trim().length < 40) problems.push(`next/${label}: rendered (almost) nothing`);
    // A list handed to the DOM as is shows its items' addresses as text.
    if (nbody.textContent.includes("#/")) problems.push(`next/${label}: an address shows as text`);
    // No button that does nothing, no link to nowhere: a button acts or
    // submits its form; a link has an address, or acts.
    const walk = (n, inForm) => {
      const acts = (n.events || []).includes("click");
      const form = inForm || (n.events || []).includes("submit");
      if (n.tag === "button" && !acts && !(form && n.attrs.type === "submit")) problems.push(`next/${label}: a button does nothing: "${n.textContent.trim()}"`);
      if (n.tag === "a" && !acts && (!n.attrs.href || n.attrs.href === "#")) problems.push(`next/${label}: a link goes nowhere: "${n.textContent.trim()}"`);
      n.children.forEach((c) => walk(c, form));
    };
    walk(nbody, false);
  };
  // The HQ: a business to watch with each part, one not started.
  vm.runInContext(`hq.data = { headline: "One business to watch.", needs: 2, businesses: [
    { name: "acme", description: "", needs: 2, health: { state: "watch", sentence: "One item is held by the check. Goal pace and runway unknown: no measured goal, no bank connected." },
      vitals: [{ label: "daily-posts", value: "9", note: "Approved this week", out: false }, { label: "Dropped this week", value: "1", note: "too long", out: true }, { label: "Quality", value: "1 flag", note: "a call to buy", out: true }],
      finance: { source: "Revolut Business · read-only · 07:00", cash: "£12,300", in: "£0", out: "£420", runway: "29 months", runway_months: 29.3 },
      time: "45 min", cost_usd: 0.42, activity: [{ agent: "post-writer", agent_title: "Post writer", sentence: "daily-posts read 58, kept 3, 3 sent to you", at: "07:31" }, { agent: null, agent_title: null, sentence: "weekly-report read 1", at: "Mon" }] },
    { name: "perso", description: null, needs: 0, health: { state: "not started", sentence: "No routine yet." }, vitals: [], finance: null, time: "0 s", cost_usd: 0, activity: [] },
    { name: "byhand", description: null, needs: 0, health: { state: "healthy", sentence: "Nothing is held." }, vitals: [], finance: { source: "Entered by hand · as of 2 Nov", cash: "£8,000", in: null, out: "£1,000", runway: "8 months", runway_months: 8 }, time: "0 s", cost_usd: 0, activity: [] },
    { name: "lapsed", description: null, needs: 0, health: { state: "healthy", sentence: "Nothing is held." }, vitals: [], finance: { unknown: "Revolut access needs re-authorizing" }, time: "0 s", cost_usd: 0, activity: [] }] };`, ctx);
  show("hq", { route: { page: "hq" } });
  for (const text of ["One business to watch.", "2 things need you", "Watch", "Not started", "Dropped this week", "1 flag", "No figures yet", "Entered by hand · as of 2 Nov", "£8,000", "8 months", "£12,300", "29 months", "Revolut Business · read-only · 07:00", "Unknown: Revolut access needs re-authorizing", "Post writer", "daily-posts read 58, kept 3, 3 sent to you", "Airth", "No run yet", "model cost $0.42"]) {
    if (!nbody.textContent.includes(text)) problems.push(`next/hq: "${text}" is missing`);
  }
  // Decision 68: money in is not entered by hand; it shows as a dash.
  const byHand = vm.runInContext(`finance({ source: "Entered by hand · as of 2 Nov", cash: "£8,000", in: null, out: "£1,000", runway: "8 months" }).textContent`, ctx);
  if (!/In, \S+ — Out/.test(byHand)) problems.push(`next/hq: money in by hand is not a dash: ${byHand}`);
  show("hq, menu open", { route: { page: "hq" }, menuOpen: true });
  vm.runInContext(`hq.data = { headline: "No business yet.", needs: 0, businesses: [] };`, ctx);
  show("hq, no business", { route: { page: "hq" }, realms: [], realm: null });
  for (const p of [...N.PAGES, ...N.PINNED]) show(p.id, { route: { page: p.id, realm: "acme" } });
  // The rail ends with the theme toggle and the founder's profile.
  const rail = JSON.stringify(nbody.querySelector("#rail"), (k, v) => (k === "parent" ? undefined : v));
  if (!/Switch to (light|dark) mode/.test(rail) || !rail.includes("Your profile")) problems.push("next/rail: no theme toggle or profile");
  // What is not built yet shows one panel, saying what it will do.
  for (const [page, realm, title] of [["hire", "acme", "Hire an agent"], ["new-business", null, "Create a business"], ["workshop", null, "Workshop"], ["settings", null, "Mailboxes"]]) {
    show(page, { route: { page, realm } });
    if (!nbody.textContent.includes("In development") || !nbody.textContent.includes(title)) problems.push(`next/${page}: no In development panel for ${title}`);
  }
  if (!nbody.textContent.includes("Open the classic interface")) problems.push("next/settings: the classic interface is not reachable");
  show("unauthorized", { route: { page: "hq" }, unauthorized: true });
  // The command bar (decision 64): on business pages, who takes the order
  // and why, a pick when none fits, then where the task went.
  vm.runInContext(`orders.route = { agent: "writer", why: "It writes the long pieces.", agents: ["watcher", "writer"], order: "Outline the week" };`, ctx);
  show("order, routed", { route: { page: "team", realm: "acme" } });
  if (!nbody.textContent.includes("· It writes the long pieces.") || !nbody.textContent.includes("Start")) problems.push("next/order: who takes it and why is missing");
  vm.runInContext(`orders.route = { error: "no agent's job fits this order: pick the agent", agents: ["writer"], order: "File the VAT" };`, ctx);
  show("order, none fits", { route: { page: "team", realm: "acme" } });
  if (!nbody.textContent.includes("pick the agent")) problems.push("next/order: a refused routing is not said");
  vm.runInContext(`orders.route = null; orders.note = "Started: writer works on it.";`, ctx);
  show("order, started", { route: { page: "team", realm: "acme" } });
  if (!nbody.textContent.includes("Started: writer") || !nbody.textContent.includes("See Tasks")) problems.push("next/order: the started task is not said");
  vm.runInContext(`orders.note = null;`, ctx);
  // Tasks (decision 64): the groups, a waiting task sent to Needs you.
  vm.runInContext(`tasks.data.acme = {
    waiting: [{ id: "r1", order: "Check today's rates", agent: "watcher", status: "stopped", line: "", at: "${now}", item: 4 }],
    review: [{ id: "r2", order: "Update the methodology page", agent: "writer", status: "completed", line: "Opened a change", at: "${now}", item: null }],
    running: [{ id: "r3", order: "Outline the week", agent: "writer", status: "running", line: "1 of 3 steps done", at: "${now}", item: null }],
    done: [{ id: "r4", order: "Summarise the RWA news", agent: null, status: "completed", line: "", at: "${now}", item: null }] };`, ctx);
  show("tasks", { route: { page: "tasks", realm: "acme" } });
  for (const text of ["Give the order.", "Waiting for you · 1", "Waits for you in Needs you", "To review · 1", "Running · 1", "1 of 3 steps done", "Done · 1", "completed"]) {
    if (!nbody.textContent.includes(text)) problems.push(`next/tasks: "${text}" is missing`);
  }
  vm.runInContext(`tasks.data.acme = { waiting: [], review: [], running: [], done: [] };`, ctx);
  show("tasks, none yet", { route: { page: "tasks", realm: "acme" } });
  if (!nbody.textContent.includes("No task yet")) problems.push("next/tasks: no empty state");
  show("hq, no order box", { route: { page: "hq" } });
  if (nbody.textContent.includes("Give an order")) problems.push("next/order: the HQ has no order box");
  // Needs you, with every kind of item and fields left empty.
  vm.runInContext(`Object.assign(needs, { realm: "acme", items: ${JSON.stringify([
    { id: 1, kind: "approval", title: "A post", agent: "post-writer", routine: "daily-posts", run: "r1", action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "ETH left.", editable: true, source: { title: null, link: "https://e.com/1" } },
    { id: 2, kind: "incident", title: "watch failed", agent: null, routine: "watch", run: null, action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "It failed.", editable: false, source: { title: null, link: null } },
    { id: 3, kind: "delayed", title: "writer: blog.draft", agent: "writer", routine: null, run: null, action: "blog.draft", created_at: null, execute_at: now, status: "pending", revision: 1, text: "{}", editable: false, source: { title: null, link: null } },
    { id: 4, kind: "prepared", title: "reply", agent: "writer", routine: null, run: null, action: "x.reply", created_at: now, execute_at: null, status: "pending", revision: 1, text: "Hi", editable: false, source: { title: null, link: null } },
    { id: 5, kind: "approval", title: "blog.publish, from a task you started", agent: null, routine: null, run: "r9", action: "blog.publish", created_at: now, execute_at: null, status: "pending", revision: 1, text: "POST blog: posts", editable: false, source: { title: null, link: null }, task: { id: "r9", title: null } },
    { id: 6, kind: "report", title: "Daily watch", agent: "post-writer", routine: "daily-watch", run: "r2", action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "ETH left.\n\nBTC up.", editable: false, source: { title: null, link: null } },
    { id: 7, kind: "approval", title: "BUY ETH", agent: "post-writer", routine: "daily-posts", run: "r3", action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "BUY ETH NOW", editable: true, flag: "blocking", flag_reason: null, note: 'draft: 1 of 3 dropped: "Hack" (too long)', source: { title: null, link: null } },
    { id: 9, kind: "incident", title: "typefully.create_draft failed in daily-posts", agent: "post-writer", routine: "daily-posts", run: null, action: "typefully.create_draft", created_at: now, execute_at: null, status: "pending", revision: 1, text: "HTTP 403", editable: false, retry: "clear", source: { title: null, link: null } },
    { id: 10, kind: "incident", title: "blog.draft failed", agent: "post-writer", routine: "daily-posts", run: null, action: "blog.draft", created_at: now, execute_at: null, status: "pending", revision: 1, text: "cut after sending", editable: false, retry: "ambiguous", source: { title: null, link: null } },
    { id: 11, kind: "deadline", title: "Confirmation statement due Fri 20 Nov 2026", agent: null, routine: null, run: null, action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "Companies House, for the period to 6 Nov 2026.", editable: false, overdue: false, source: { title: null, link: "https://www.gov.uk/guidance/confirmation-statement-guidance" } },
    { id: 12, kind: "deadline", title: "Overdue: Annual accounts was due Sat 7 Aug 2027", agent: null, routine: null, run: null, action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "It stays here until you answer Filed.", editable: false, overdue: true, source: { title: null, link: null } },
    { id: 8, kind: "approval", title: "Long", agent: "post-writer", routine: "daily-posts", run: "r3", action: null, created_at: now, execute_at: null, status: "pending", revision: 1, text: "A long post", editable: false, flag: "note", flag_reason: "long", source: { title: null, link: null } }])},
    agents: [{ name: "post-writer", title: null, department: "marketing" }, { name: "writer", title: "Writer", department: null }] });`, ctx);
  vm.runInContext(`needs.runs = { 1: { routine: "daily-posts", run: "r1", blocks: [
    { block: "news", started_at: "${now}", finished_at: null, outcome: "ok", items_in: 1, items_out: 1, cost_usd: null, error: null },
    { block: "ok", started_at: "${now}", finished_at: "${now}", outcome: "waiting", items_in: 1, items_out: 0, cost_usd: 0.004, error: null },
    { block: "send", started_at: "${now}", finished_at: "${now}", outcome: "failed", items_in: 1, items_out: 1, cost_usd: null, error: "x" }] } };`, ctx);
  vm.runInContext(`company.team.acme = { agents: [
    { name: "watcher", title: null, department: "marketing", job: null, look: "0a1b2c3d", look_saved: true, accessory: "lens", state: "asleep", routines: [{ name: "daily-watch", schedule: null, last: null }], waiting: 0, cost_usd: 0, unpriced: 0, seconds: 0 },
    { name: "writer", title: "Writer", department: null, job: "Write.", look: "ff00aa11", look_saved: false, accessory: null, state: "needs", routines: [{ name: "weekly", schedule: "mon 09:00", last: { at: "${now}", ok: false } }], waiting: 2, cost_usd: 1.5, unpriced: 3, seconds: 900 }] };
    company.profile.acme = { profile: { name: null, legal_name: "Acme Ltd", country: "GB", fiscal_year_end: "11-30", links: ["https://a.com"] }, goal: {}, lines: [{ name: "news", fields: { title: null, kind: null, status: null } }], repos: [{ name: "site", url: null }], missing: ["business/profile.md: country"] };`, ctx);
  show("team", { route: { page: "team", realm: "acme" } });
  // Decision 65: each agent's state in words, "Keep this look" only for
  // a look not saved yet; the character drawn from seed, department, state.
  if (!nbody.textContent.includes("Needs you") || !nbody.textContent.includes("Asleep")) problems.push("next/team: an agent's state is not said");
  if ((nbody.textContent.match(/Keep this look/g) || []).length !== 1) problems.push("next/team: Keep this look is not offered once, to the agent without a saved look");
  const drawn = vm.runInContext(`character({ look: "0a1b2c3d", department: "marketing", accessory: "pen", state: "needs", title: "Writer" }, 72)`, ctx);
  for (const part of ['class="ch st-needs"', "var(--dept-marketing)", 'class="bubble"', 'class="acc"', 'aria-label="Writer, needs you"']) {
    if (!drawn.includes(part)) problems.push(`next/character: ${part} is missing`);
  }
  const plain = vm.runInContext(`character({ look: "bad", department: "spaceships", state: "dancing" }, 72)`, ctx);
  if (!plain.includes("var(--dept-none)") || !plain.includes("st-idle")) problems.push("next/character: an unknown department or state is not drawn plainly");
  const twice = vm.runInContext(`character({ look: "0a1b2c3d", department: "sales" }) === character({ look: "0a1b2c3d", department: "sales" })`, ctx);
  if (!twice) problems.push("next/character: the same seed does not draw the same character");
  // What waits for an agent is said beside its state, not after a routine.
  if (!nbody.textContent.includes("Needs you 2 items wait for you")) problems.push("next/team: what waits is not said beside the agent's state");
  show("business", { route: { page: "business", realm: "acme" } });
  // Decision 66: a UK company's year end reads as its accounting reference date.
  if (!nbody.textContent.includes("Accounting reference date") || nbody.textContent.includes("Fiscal year end")) problems.push("next/business: a UK company's year end is not called its accounting reference date");
  vm.runInContext(`company.team.acme = { agents: [] }; company.profile.acme = { error: "agents/x.md: unknown field 'foo'", repos: [] };`, ctx);
  show("team, no agent", { route: { page: "team", realm: "acme" } });
  show("business, config refused", { route: { page: "business", realm: "acme" } });
  show("needs you", { route: { page: "needs-you", realm: "acme" } });
  // Decision 66: a deadline is answered Filed or Seen; an overdue one, Filed
  // only. They are Airth's own, beside its incidents.
  vm.runInContext(`needs.agent = null;`, ctx);
  show("needs you, Airth's own", { route: { page: "needs-you", realm: "acme" } });
  if (!nbody.textContent.includes("Airth does not file it")) problems.push("next/needs: a legal deadline does not say Airth does not file it");
  const buttonsOf = (title) => {
    const cards = [];
    const walk = (n) => { if (n.tag === "article" && n.textContent.includes(title)) cards.push(n); (n.children || []).forEach(walk); };
    walk(nbody);
    return cards.length ? cards[cards.length - 1].textContent : "";
  };
  if (!/Filed.*Seen/.test(buttonsOf("Confirmation statement due"))) problems.push("next/needs: a deadline is not answered Filed or Seen");
  if (/Seen/.test(buttonsOf("Overdue: Annual accounts"))) problems.push("next/needs: an overdue deadline can be closed without Filed");
  vm.runInContext(`needs.agent = "writer"; needs.rejecting = 3;`, ctx);
  show("needs you, cancelling a delayed action", { route: { page: "needs-you", realm: "acme" } });
  vm.runInContext(`needs.agent = "post-writer"; needs.editing = 1; needs.rejecting = null;`, ctx);
  show("needs you, editing", { route: { page: "needs-you", realm: "acme" } });
  vm.runInContext(`needs.agent = null; needs.editing = null;`, ctx);
  show("needs you, a commit from a task", { route: { page: "needs-you", realm: "acme" } });
  if (!nbody.textContent.includes("From your task")) problems.push("next/needs you: the task a commit came from is missing");
  vm.runInContext(`needs.agent = "post-writer";`, ctx);
  show("needs you, a report", { route: { page: "needs-you", realm: "acme" } });
  if (!nbody.textContent.includes("Report") || !nbody.textContent.includes("Seen")) problems.push("next/needs you: a report shows without its tag or Seen");
  if (!nbody.textContent.includes("Flagged by the check") || !nbody.textContent.includes("Override")) problems.push("next/needs you: a blocked item shows without its flag or Override");
  if (!nbody.textContent.includes("Note from the check") || !nbody.textContent.includes("long A long post")) problems.push("next/needs you: a check's note is missing");
  if (!nbody.textContent.includes("Also in this run") || !nbody.textContent.includes('draft: 1 of 3 dropped: "Hack" (too long)')) problems.push("next/needs you: what the run dropped is missing");
  if (!nbody.textContent.includes("Nothing was done: Retry does it once") || !nbody.textContent.includes("Retry…") || !nbody.textContent.includes("It may have been done")) problems.push("next/needs you: an incident's Retry is missing");
  vm.runInContext(`needs.confirming = 10;`, ctx);
  show("needs you, confirming a retry", { route: { page: "needs-you", realm: "acme" } });
  if (!nbody.textContent.includes("Nothing was created: retry")) problems.push("next/needs you: an ambiguous retry is not confirmed first");
  vm.runInContext(`needs.confirming = null;`, ctx);
  vm.runInContext(`needs.items = []; needs.agent = undefined;`, ctx);
  show("needs you, nothing waits", { route: { page: "needs-you", realm: "acme" } });
  // Routines: every kind of block, a routine refused, then a dry run.
  vm.runInContext(`routines.data.acme = { routines: [
    { name: "broken", runs: [], next_at: null, cost_usd: 0, unpriced: 0, error: "routines/broken.json: the agent 'ghost'" },
    { name: "daily-posts", runs: [{ at: "${now}", due_at: "2026-11-02T05:00:00Z", ok: false, error: "x" }, { at: "${now}", due_at: null, ok: true, error: null }], next_at: "${now}", cost_usd: 0.12, unpriced: 1, agent_title: null,
      graph: { description: "", agent: "post-writer", trigger: { schedule: "daily 07:30" }, blocks: [
        { id: "news", type: "read", source: "rss", feeds: ["https://e.com/f"], max_age_days: 2, dedupe: "link", max: null },
        { id: "week", type: "read", source: "business", records: "week", feeds: [] },
        { id: "pick", type: "ai", prompt: "prompts/watch.md", keep: "relevant", rank: "score", distinct: ["pillar", "story"], limit: 3 },
        { id: "good", type: "condition", if: { field: "answer.score", op: ">=", value: 3 }, then: null, else: "send" },
        { id: "ok", type: "approval", show: "answer.draft", editable: "answer.draft", expires_after: 24 },
        { id: "send", type: "act", action: "typefully.create_draft", fields: { text: "{{answer.draft}}" } }] } },
    { name: "weekly-report", runs: [], next_at: null, cost_usd: 0, unpriced: 0, agent_title: "Chief of staff",
      graph: { description: "The week in figures", agent: "chief", trigger: { schedule: "mon 08:00" }, blocks: [
        { id: "week", type: "read", source: "business", records: "week", feeds: [] }] } }] };`, ctx);
  show("routines, a routine refused", { route: { page: "routines", realm: "acme" } });
  vm.runInContext(`routines.open.acme = "daily-posts";`, ctx);
  show("routines, before a dry run", { route: { page: "routines", realm: "acme" } });
  if (!nbody.textContent.includes("Every day")) problems.push("next/routines: the trigger is missing");
  vm.runInContext(`routines.running["acme/daily-posts"] = true;`, ctx);
  show("routines, a dry run going", { route: { page: "routines", realm: "acme" } });
  vm.runInContext(`routines.running["acme/daily-posts"] = false;
    routines.dry["acme/daily-posts"] = { at: "${now}", title: "Dry run of daily-posts", cost: "cost: $0.0040, 10 tokens in, 5 out", secs: 3.2, blocks: [
      { id: "news", items_in: 1, items_out: 2, secs: 1.1, lines: ["2 read, 0 too old: 2 kept"], notes: [] },
      { id: "send", items_in: 2, items_out: 2, secs: 0, lines: ["would typefully.create_draft [daily-posts/https://e.com/1/send]: POST https://api.typefully.com/v2/drafts"], notes: ["feed down"] }] };
    routines.block["acme/daily-posts"] = "send";`, ctx);
  show("routines, a dry run's block", { route: { page: "routines", realm: "acme" } });
  if (!nbody.textContent.includes("would typefully.create_draft")) problems.push("next/routines: the dry run's act is missing");
  vm.runInContext(`routines.block["acme/daily-posts"] = "pick";`, ctx);
  show("routines, a block not reached", { route: { page: "routines", realm: "acme" } });
  vm.runInContext(`routines.dry["acme/daily-posts"] = { error: "block 'news': a run reads rss and the business only" };`, ctx);
  show("routines, a dry run refused", { route: { page: "routines", realm: "acme" } });
  vm.runInContext(`routines.data.acme = { routines: [] };`, ctx);
  show("routines, none yet", { route: { page: "routines", realm: "acme" } });
  // Calendar: the week from today, each state of a run, the late starts.
  vm.runInContext(`calendar.data.acme = { title: "This week: 5 routine runs.", total: 5, days: [
    { weekday: "Tue", day: "3", today: true, runs: [
      { routine: "daily-watch", agent: "watcher", time: "07:00", state: "done" },
      { routine: "daily-posts", agent: "post-writer", time: "07:30", state: "late" },
      { routine: "noon", agent: null, time: "12:00", state: "failed" },
      { routine: "evening", agent: "watcher", time: "18:00", state: "next" }] },
    { weekday: "Wed", day: "4", today: false, runs: [{ routine: "daily-watch", agent: "watcher", time: "07:00", state: "planned" }] },
    { weekday: "Thu", day: "5", today: false, runs: [] },
    { weekday: "Fri", day: "6", today: false, runs: [], deadlines: ["Corporation Tax payment"] }],
    late: [{ routine: "daily-posts", due: "Mon 2 Nov, 07:30", late_by: 90 }],
    deadlines: { note: "Computed from the business profile.", items: [
      { title: "Confirmation statement", authority: "Companies House", date: "2026-11-20", due: "Fri 20 Nov 2026", days_left: 17, period_end: "6 Nov 2026", source: "https://www.gov.uk/guidance/confirmation-statement-guidance" },
      { title: "Annual accounts", authority: "Companies House", date: "2027-08-07", due: "Sat 7 Aug 2027", days_left: 277, period_end: "30 Nov 2026", source: null, filed: true },
      { title: "Corporation Tax payment", authority: "HMRC", date: "2026-11-02", due: "Mon 2 Nov 2026", days_left: -1, period_end: "1 Feb 2026", source: null }] } };`, ctx);
  show("calendar, the week", { route: { page: "calendar", realm: "acme" } });
  for (const text of ["This week: 5 routine runs.", "Tue · today", "Started late", "Failed", "Next", "Nothing scheduled", "Due Mon 2 Nov, 07:30, started 90 min late", "Deadlines", "Due Fri 20 Nov 2026 · in 17 days", "for the period to 6 Nov 2026", "Source", "Filed · due Sat 7 Aug 2027", "Overdue · was due Mon 2 Nov 2026", "Due Corporation Tax payment"]) {
    if (!nbody.textContent.includes(text)) problems.push(`next/calendar: "${text}" is missing`);
  }
  // A deadline whose date the profile does not give yet says why.
  vm.runInContext(`calendar.data.acme = { title: "No routine runs this week.", total: 0, days: [], late: [],
    deadlines: { note: "Computed from the business profile.", items: [{ title: "Annual accounts", authority: "Companies House", unknown: "Date unknown: incorporation date missing" }] } };`, ctx);
  show("calendar, empty", { route: { page: "calendar", realm: "acme" } });
  if (!nbody.textContent.includes("Date unknown: incorporation date missing")) problems.push("next/calendar: an unknown deadline does not say why");
  if (!nbody.textContent.includes("every scheduled run started on time")) problems.push("next/calendar: no late start is not said");
}
