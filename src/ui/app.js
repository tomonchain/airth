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
    history.replaceState(null, "", "#/");
  }
}

class AuthError extends Error {}

async function api(path, body, method) {
  method = method || (body === undefined ? "GET" : "POST");
  const res = await fetch("/api/" + path, {
    method,
    headers: { "X-Airth-Token": token || "", "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 401) throw new AuthError("unauthorized");
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

const enc = encodeURIComponent;
const realmPath = (r) => `realms/${enc(r)}`;
const runPath = (r, id) => `realms/${enc(r)}/runs/${enc(id)}`;

// ---------- app state ----------

const app = {
  info: null,
  realms: [],
  realm: null, // current realm name
  realmDetail: null, // settings of the current realm
  runs: [],
  route: { view: "home" },
  run: null, // last full response for the current run
  version: null,
  diff: null, // { runId, text } once loaded
  openKeys: new Set(),
  reason: "",
  error: null,
  changeError: null,
  busy: false,
  unauthorized: !token,
};

function parseRoute() {
  const h = location.hash;
  let m;
  if ((m = h.match(/^#\/r\/([^/]+)\/run\/([^/?#]+)/))) return { view: "run", realm: decodeURIComponent(m[1]), id: decodeURIComponent(m[2]) };
  if ((m = h.match(/^#\/r\/([^/]+)\/settings/))) return { view: "settings", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)\/objectives/))) return { view: "objectives", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)\/skills/))) return { view: "skills", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)/))) return { view: "new", realm: decodeURIComponent(m[1]) };
  if (h.startsWith("#/owner")) return { view: "owner" };
  if (h.startsWith("#/inbox")) return { view: "inbox" };
  if (h.startsWith("#/realms/new")) return { view: "new-realm" };
  return { view: "home" };
}

function go(hash) {
  if (location.hash === hash) onRoute();
  else location.hash = hash;
}

/** The realm to show when the URL does not name one. */
function preferredRealm() {
  const live = app.realms.filter((r) => !r.archived);
  const pick = (name) => live.find((r) => r.name === name);
  let saved = null;
  try { saved = localStorage.getItem("airth-realm"); } catch {}
  return (pick(app.realm) || pick(saved) || pick(app.info && app.info.realm) || live.find((r) => r.default) || live[0] || {}).name || null;
}

async function onRoute() {
  app.route = parseRoute();
  app.run = null;
  app.version = null;
  app.diff = null;
  app.error = null;
  app.changeError = null;
  app.reason = "";
  confirmDiscard = false;
  if (app.route.view === "home") {
    const r = preferredRealm();
    return go(r ? `#/r/${enc(r)}` : "#/realms/new");
  }
  if (app.route.realm && app.route.realm !== app.realm) await selectRealm(app.route.realm);
  renderMain();
  renderSidebar();
  if (app.route.view === "run") pollRun();
}

async function selectRealm(name) {
  app.realm = name;
  app.runs = [];
  app.realmDetail = null;
  try { localStorage.setItem("airth-realm", name); } catch {}
  await Promise.all([refreshRuns(), refreshRealmDetail()]);
}

// ---------- sidebar ----------

let realmMenuOpen = false;

function renderSidebar() {
  renderRealmSwitch();

  const list = $("#run-list");
  list.replaceChildren();
  if (app.realm && !app.runs.length) list.append(h("div", { class: "run-empty" }, "No runs in this realm yet."));
  for (const r of app.runs) {
    const status = r.live ? "running" : r.status;
    const active = app.route.view === "run" && app.route.id === r.id;
    list.append(
      h("button", { class: "run-item" + (active ? " active" : ""), type: "button", title: r.objective, onclick: () => go(`#/r/${enc(app.realm)}/run/${enc(r.id)}`) },
        h("div", { class: "obj" }, r.objective),
        h("div", { class: "meta" },
          h("span", { class: "dot " + status }),
          h("span", null, r.live ? "running" : r.status),
          h("span", null, "·"),
          h("span", null, ago(r.updated_at)),
          r.plan_total > 0 && h("span", { class: "progress-mini", title: `${r.plan_done} of ${r.plan_total} steps done` }, `${r.plan_done}/${r.plan_total}`),
          r.needs_approval ? h("span", { class: "needs" }, "needs approval") : changeBadge(r))));
  }
  const newBtn = $("#new-run-btn");
  newBtn.disabled = !app.realm;
  newBtn.classList.toggle("ghost", app.route.view === "new");

  const foot = $("#sidebar-foot");
  foot.replaceChildren();
  if (app.info) $("#version").textContent = "v" + app.info.version;
  const d = app.realmDetail;
  const unread = app.info ? app.info.unread : 0;
  foot.append(
    navLink("#/inbox", h("span", null, "Inbox", unread > 0 && h("span", { class: "count" }, String(unread))), app.route.view === "inbox"),
    app.realm && navLink(`#/r/${enc(app.realm)}/objectives`, "Objectives", app.route.view === "objectives"),
    app.realm && navLink(`#/r/${enc(app.realm)}/skills`, "Skills", app.route.view === "skills"),
    navLink("#/owner", "Your profile", app.route.view === "owner"),
    app.realm && navLink(`#/r/${enc(app.realm)}/settings`, "Realm settings", app.route.view === "settings"),
    d && h("div", { class: "row", title: d.model_error || d.model || "" }, h("span", { class: "k" }, "model"),
      d.model ? h("span", { class: "v" }, d.model) : h("span", { class: "warn" }, "not configured")),
    app.info && !app.info.sandbox && h("div", { class: "row warn", title: "Commands are not confined to the realm on this platform." }, "⚠ no command sandbox"));
  updateTitle();
}

function navLink(hash, label, active) {
  return h("button", { class: "nav-link" + (active ? " active" : ""), type: "button", onclick: () => go(hash) }, label);
}

function changeBadge(r) {
  if (r.change === "adopted") return h("span", { class: "badge ok" }, "adopted");
  if (r.change === "discarded") return h("span", { class: "badge" }, "discarded");
  if (r.checks_passed === true) return h("span", { class: "badge ok" }, "✓ checks");
  if (r.checks_passed === false) return h("span", { class: "badge err" }, "✗ checks");
  return null;
}

function renderRealmSwitch() {
  const el = $("#realm-switch");
  el.replaceChildren();
  const elsewhere = app.realms.filter((r) => r.name !== app.realm && r.needs_approval > 0);
  const current = app.realms.find((r) => r.name === app.realm);
  const button = h("button", { class: "realm-button", type: "button", "aria-haspopup": "true", "aria-expanded": String(realmMenuOpen),
      onclick: (e) => { e.stopPropagation(); realmMenuOpen = !realmMenuOpen; renderRealmSwitch(); } },
    h("span", { class: "realm-mark" }, (app.realm || "?").slice(0, 1).toUpperCase()),
    h("span", { class: "realm-name" },
      h("span", { class: "label" }, "Realm"),
      h("span", null, app.realm || "none")),
    elsewhere.length > 0 && h("span", { class: "needs-dot", title: "Approvals waiting in other realms" }),
    h("span", { class: "chev" }, "▾"));
  el.append(button);
  if (current && current.description) el.append(h("div", { class: "realm-desc" }, current.description));
  if (elsewhere.length)
    el.append(h("button", { class: "realm-alert", type: "button", onclick: () => go(`#/r/${enc(elsewhere[0].name)}`) },
      `⚠ ${elsewhere.map((r) => r.name).join(", ")} waiting for approval`));
  if (!realmMenuOpen) return;
  const menu = h("div", { class: "realm-menu", role: "menu" });
  for (const r of app.realms) {
    menu.append(h("button", { class: "realm-option" + (r.name === app.realm ? " active" : "") + (r.archived ? " archived" : ""), type: "button", role: "menuitem",
        onclick: () => { realmMenuOpen = false; go(`#/r/${enc(r.name)}`); } },
      h("span", { class: "realm-mark small" }, r.name.slice(0, 1).toUpperCase()),
      h("span", { class: "grow" }, r.name, r.archived && h("span", { class: "muted" }, " · archived")),
      r.needs_approval > 0 ? h("span", { class: "needs" }, r.needs_approval) : r.live > 0 ? h("span", { class: "dot running" }) : null));
  }
  menu.append(h("button", { class: "realm-option add", type: "button", role: "menuitem", onclick: () => { realmMenuOpen = false; go("#/realms/new"); } }, "+ New realm"));
  el.append(menu);
}

document.addEventListener("click", () => {
  if (realmMenuOpen) { realmMenuOpen = false; renderRealmSwitch(); }
});

function updateTitle() {
  const n = app.realms.reduce((acc, r) => acc + (r.needs_approval || 0), 0);
  document.title = (n ? `(${n}) ` : "") + "Airth";
}

// ---------- main ----------

function renderMain() {
  const main = $("#main");
  main.replaceChildren();
  runEls = null;
  if (app.unauthorized) {
    main.append(h("div", { class: "center-note" },
      h("h1", null, "Open Airth from your terminal"),
      h("p", null, "Run ", h("code", null, "airth ui"), " and use the link it prints. The link carries a private key that lets this page control the agent.")));
    return;
  }
  switch (app.route.view) {
    case "new": return renderNew(main);
    case "run": return renderRunShell(main);
    case "settings": return renderSettings(main);
    case "owner": return renderOwner(main);
    case "objectives": return renderObjectives(main);
    case "skills": return renderSkills(main);
    case "inbox": return renderInbox(main);
    case "new-realm": return renderNewRealm(main);
  }
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
      errorEl.hidden = true;
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

function page(main, title, sub, ...children) {
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner" },
    h("h1", null, title), sub && h("p", { class: "sub" }, sub), ...children)));
}

// ---------- new run ----------

const SUGGESTIONS = {
  code: ["Identify and implement one small improvement", "Find and fix a bug", "Add tests for an untested module", "Review error messages and make them clearer"],
  other: ["Organize the files in this realm", "Summarize what's in this realm", "Draft a plan for this week", "Write a checklist for an upcoming task"],
};

const POLICY_WORDS = { allow: "automatic", ask: "needs approval", deny: "disabled" };

function renderNew(main) {
  const d = app.realmDetail;
  const realm = app.realms.find((r) => r.name === app.realm);
  const modelError = d && d.model_error;
  const c = composer({
    placeholder: `Describe an objective for ${app.realm}…`,
    buttonLabel: "Start",
    rows: 3,
    disabled: !!modelError || (realm && realm.archived),
    note: d && d.isolation === "worktree"
      ? "Airth works in its own copy of the repository. You review the change, and it reaches your checkout only when you adopt it."
      : "Every step that needs approval waits for you.",
    onSubmit: async (objective) => {
      const { id } = await api(`${realmPath(app.realm)}/runs`, { objective });
      await refreshRuns();
      go(`#/r/${enc(app.realm)}/run/${enc(id)}`);
    },
  });
  if (modelError) {
    c.errorEl.textContent = "No model for this realm: " + modelError + " Add an ANTHROPIC_API_KEY credential in the realm settings, or set it in Airth's environment.";
    c.errorEl.hidden = false;
  }
  const isCode = d && d.own_root && d.checks.length > 0;
  const tools = app.info && d
    ? h("div", { class: "tools-card" },
        h("h2", null, `What Airth can do in ${app.realm}`),
        app.info.tools.map((t) => {
          const rule = d.policy[t.capability];
          return h("div", { class: "tool-row" },
            h("code", null, t.name),
            h("span", { class: "chip rule-" + rule }, POLICY_WORDS[rule]),
            h("span", { class: "d" }, t.description.split(". ")[0] + "."));
        }))
    : null;
  main.append(
    h("div", { class: "new-view" },
      h("div", { class: "new-inner" },
        h("div", { class: "eyebrow" }, "Realm · " + (app.realm || "")),
        h("h1", null, "What should Airth work on?"),
        h("p", { class: "sub" }, d ? `Workspace: ${d.root}` : ""),
        c.el,
        h("div", { class: "suggestions" },
          (isCode ? SUGGESTIONS.code : SUGGESTIONS.other).map((s) => h("button", { class: "suggestion", type: "button", onclick: () => { c.ta.value = s; c.ta.focus(); } }, s))),
        tools)));
  c.ta.focus();
}

// ---------- run view ----------

let runEls = null;

function renderRunShell(main) {
  const header = h("header", { class: "run-header" });
  const transcript = h("div", { class: "transcript" }, h("div", { class: "working" }, h("span", { class: "spinner" }), "Loading…"));
  const scroller = h("div", { class: "scroller" }, transcript);
  const c = composer({
    placeholder: "Send a follow-up…",
    buttonLabel: "Send",
    disabled: true,
    onSubmit: (message) => sendFollowUp(message),
  });
  main.append(header, scroller, h("div", { class: "composer-wrap" }, c.el));
  runEls = { header, transcript, scroller, composer: c };
}

async function sendFollowUp(message) {
  await api(`${runPath(app.realm, app.route.id)}/messages`, { message });
  app.version = null;
  app.diff = null;
  await Promise.all([pollRun(), refreshRuns()]);
}

function liveStatus(d) {
  if (!d.live) return d.interrupted ? "interrupted" : d.state.status;
  if (d.stopping) return "stopping";
  if (d.pending) return "awaiting";
  if (d.state.change && d.state.change.checks && !d.state.change.checks.finished) return "checking";
  return "running";
}

const STATUS_LABEL = { awaiting: "awaiting approval", stopping: "stopping…", checking: "running checks" };

function renderRun() {
  const d = app.run;
  if (!d || !runEls) return;
  const s = d.state;
  const status = liveStatus(d);

  const canStop = d.live && !d.elsewhere && !d.stopping;
  runEls.header.replaceChildren(
    h("div", { class: "run-title" },
      h("h1", { title: s.objective }, s.objective),
      h("div", { class: "run-meta" },
        h("span", { class: "chip " + (status === "stopping" ? "stopped" : status === "checking" ? "running" : status) }, STATUS_LABEL[status] || status),
        s.origin && h("span", { class: "chip origin", title: s.origin.trigger }, "◷ " + s.origin.objective),
        s.skill && h("a", { class: "chip origin", href: `#/r/${enc(app.realm)}/skills` }, "⚙ developing " + s.skill),
        h("span", null, `${s.iterations} iteration${s.iterations === 1 ? "" : "s"}`),
        h("span", null, `${s.events.length} tool call${s.events.length === 1 ? "" : "s"}`),
        (s.input_tokens || s.output_tokens) && h("span", { title: `${s.input_tokens} in / ${s.output_tokens} out` }, `${kfmt(s.input_tokens)} in · ${kfmt(s.output_tokens)} out tokens`),
        h("span", null, "updated " + ago(s.updated_at)))),
    canStop && h("button", { class: "btn danger", type: "button", onclick: stopRun }, "■ Stop"),
    s.plan && s.plan.length ? planPanel(s.plan, d.live) : null);

  const sc = runEls.scroller;
  const nearBottom = sc.scrollHeight - sc.scrollTop - sc.clientHeight < 80;
  const focusedId = document.activeElement && document.activeElement.id;
  runEls.transcript.replaceChildren(...transcriptNodes(d));
  if (focusedId === "deny-reason") $("#deny-reason")?.focus();
  if (nearBottom || app.version === null) sc.scrollTop = sc.scrollHeight;

  const c = runEls.composer;
  const closed = s.change && s.change.status !== "open";
  const disabled = d.live || closed || !!(app.realmDetail && app.realmDetail.model_error);
  c.ta.disabled = disabled;
  c.btn.disabled = disabled;
  c.ta.placeholder = d.live
    ? (d.elsewhere ? "This run is in progress in another Airth process…" : "Airth is working… you can send a follow-up when it stops.")
    : closed ? `This run's change was ${s.change.status}. Start a new run to keep going.`
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
      for (const b of texts) {
        if (b.text.startsWith(RUNTIME_PREFIX)) {
          nodes.push(h("div", { class: "msg runtime" }, h("div", { class: "who" }, "Airth"),
            h("div", { class: "prose", html: md(b.text.slice(RUNTIME_PREFIX.length).trim()) })));
        } else {
          nodes.push(h("div", { class: "msg user" },
            h("div", { class: "who" }, mi === 0 ? "Objective" : "You"),
            h("div", { class: "prose", html: md(b.text) })));
        }
      }
      return;
    }
    m.content.forEach((b, bi) => {
      if (b.type === "reasoning" && b.summary && b.summary.trim()) {
        nodes.push(keyedDetails(`think:${mi}:${bi}`, { class: "thinking" },
          h("summary", null, "Thinking"),
          h("div", { class: "prose", html: md(b.summary) })));
      } else if (b.type === "text" && b.text.trim()) {
        nodes.push(h("div", { class: "msg assistant prose", html: md(b.text) }));
      } else if (b.type === "tool_use") {
        nodes.push(toolCard(b, events.get(b.id), results.get(b.id), d.live, b.id === awaitingId));
      }
    });
  });

  const checking = s.change && s.change.checks && !s.change.checks.finished && d.live;
  if (d.pending) nodes.push(approvalCard(d.pending));
  else if (d.live && !checking) {
    const lastMsg = s.messages[s.messages.length - 1];
    const label = d.stopping ? "Stopping after the current step…" : lastMsg && lastMsg.role === "assistant" ? "Running…" : "Thinking…";
    nodes.push(h("div", { class: "working" }, h("span", { class: "spinner" }), label));
  } else if (!d.live) {
    const status = d.interrupted ? "interrupted" : s.status;
    const text = {
      completed: `Completed after ${s.iterations} iteration${s.iterations === 1 ? "" : "s"}.`,
      failed: "The run failed.",
      stopped: "The run stopped.",
      interrupted: "The run was interrupted: the process exited before it finished.",
    }[status];
    const showReport = status !== "completed" && s.report;
    const closed = s.change && s.change.status !== "open";
    const canResume = status !== "completed" && !closed && !d.elsewhere;
    nodes.push(h("div", { class: "end " + status },
      h("div", { class: "grow" }, text, showReport && h("pre", null, s.report),
        canResume && h("div", { class: "muted small" }, "Resume to let Airth check where it stopped and carry on with its plan, or send a message to redirect it.")),
      canResume && h("button", { class: "btn primary", type: "button", disabled: !!(app.realmDetail && app.realmDetail.model_error), onclick: resumeRun }, "Resume")));
  }
  if (s.change) nodes.push(changePanel(d));
  return nodes;
}

const RUNTIME_PREFIX = "[Airth runtime]";

const STEP_MARK = { pending: "○", in_progress: "◐", done: "✓", blocked: "!", skipped: "–" };

function planPanel(plan, live) {
  const closed = plan.filter((p) => p.status === "done" || p.status === "skipped").length;
  const current = plan.find((p) => p.status === "in_progress") || plan.find((p) => p.status === "blocked");
  const el = keyedDetails("plan", { class: "plan" },
    h("summary", null,
      h("span", { class: "strong" }, "Plan"),
      h("span", { class: "plan-bar", "aria-hidden": "true" }, h("span", { style: `width:${Math.round((100 * closed) / plan.length)}%` })),
      h("span", { class: "muted small" }, `${closed}/${plan.length}`),
      current && h("span", { class: "plan-current" }, current.title)),
    h("ol", { class: "plan-steps" }, plan.map((p) =>
      h("li", { class: "step " + p.status },
        h("span", { class: "mark", title: p.status.replace("_", " ") }, STEP_MARK[p.status] || "•"),
        h("div", null, h("div", null, p.title), p.note && h("div", { class: "muted small" }, p.note))))));
  if (!app.openKeys.has("plan:seen")) { app.openKeys.add("plan:seen"); app.openKeys.add("plan"); el.open = true; }
  return el;
}

async function resumeRun() {
  try {
    await api(`${runPath(app.realm, app.route.id)}/resume`, {});
    app.version = null;
    await Promise.all([pollRun(), refreshRuns()]);
  } catch (e) {
    flash(e.message, true);
  }
}

function keyedDetails(key, attrs, ...children) {
  const el = h("details", attrs, ...children);
  el.open = app.openKeys.has(key);
  el.addEventListener("toggle", () => (el.open ? app.openKeys.add(key) : app.openKeys.delete(key)));
  return el;
}

const ICONS = { read_file: "≡", list_directory: "▤", write_file: "✎", edit_file: "✎", execute_command: "❯", remember: "✦", http_request: "⇄", update_plan: "☰" };

function toolLabel(name, input) {
  switch (name) {
    case "read_file": return input.path;
    case "list_directory": return (input.path || ".") + (input.recursive ? "  (recursive)" : "");
    case "write_file": return `${input.path}  · ${(input.content || "").split("\n").length} lines`;
    case "execute_command": return "$ " + String(input.command || "").split("\n")[0];
    case "remember": return input.text;
    case "edit_file": return input.path;
    case "http_request": return `${input.method || "GET"} ${input.integration ? input.integration + ": " : ""}${input.url || ""}`;
    case "update_plan": {
      const steps = input.steps || [];
      return `${steps.filter((p) => p.status === "done" || p.status === "skipped").length}/${steps.length} steps done`;
    }
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
  else if (b.name === "edit_file") body.push(section("Edit · " + input.path, diffView((input.old_text || "").split("\n").map((l) => "-" + l).concat((input.new_text || "").split("\n").map((l) => "+" + l)).join("\n"), "diff")));
  else if (b.name === "http_request") body.push(section("Request", h("pre", { class: "out" }, toolLabel(b.name, input) + (input.body ? "\n\n" + input.body : ""))));
  else if (!["read_file", "list_directory", "remember", "update_plan"].includes(b.name)) body.push(section("Input", h("pre", { class: "out" }, JSON.stringify(input, null, 2))));
  if (output != null) body.push(section("Result", h("pre", { class: "out" }, output || "(empty)")));

  return keyedDetails("tool:" + b.id, { class: "tool" },
    h("summary", null,
      h("span", { class: "icon" }, ICONS[b.name] || "•"),
      h("span", { class: "label" }, h("span", { class: "name" }, b.name), toolLabel(b.name, input)),
      h("span", { class: "chip " + status }, label)),
    h("div", { class: "body" }, body));
}

const section = (title, ...children) => h("div", null, h("h4", null, title), ...children);

function diffView(text, kind) {
  const el = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    let cls = "";
    if (kind === "command") cls = line.startsWith("$ ") ? "cmd" : "hunk";
    else if (kind === "plain") cls = "cmd";
    else if (/^(\+\+\+|---)\s/.test(line) || /^(create|modify)\s/.test(line) || line.startsWith("diff --git")) cls = "file";
    else if (line.startsWith("@@")) cls = "hunk";
    else if (/^(index |new file|deleted file|similarity|rename )/.test(line)) cls = "meta";
    else if (line.startsWith("+")) cls = "add";
    else if (line.startsWith("-")) cls = "del";
    el.append(h("div", { class: cls }, line || " "));
  }
  return el;
}

function approvalCard(p) {
  const title = { write_file: "Write this file?", edit_file: "Apply this edit?", execute_command: "Run this command?", remember: "Remember this?", http_request: "Send this request?" }[p.tool] || `Allow ${p.tool}?`;
  const kind = p.tool === "execute_command" ? "command" : p.tool === "remember" || p.tool === "http_request" ? "plain" : "diff";
  const reason = h("input", {
    id: "deny-reason", type: "text", placeholder: "Reason for denying (optional; Airth sees it)",
    value: app.reason, autocomplete: "off",
    oninput: (e) => (app.reason = e.target.value),
    onkeydown: (e) => { if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) { e.preventDefault(); answer(false); } },
  });
  return h("div", { class: "approval", role: "region", "aria-label": "Approval required" },
    h("div", { class: "approval-head" },
      h("h3", null, title),
      h("span", { class: "chip rule-ask" }, p.capability)),
    p.warnings.map((w) => h("div", { class: "approval-warn" }, "⚠ " + w)),
    diffView(p.preview, kind),
    h("div", { class: "approval-actions" },
      h("button", { class: "btn primary", type: "button", onclick: () => answer(true) }, "Approve ", h("kbd", null, modKey + "↵")),
      p.grantable && h("button", { class: "btn", type: "button", title: "Later requests like this one in this run are approved without asking", onclick: () => answer(true, true) }, `Allow ${p.grant_label} for this run`),
      h("button", { class: "btn danger", type: "button", onclick: () => answer(false) }, "Deny"),
      reason));
}

// ---------- change review ----------

let confirmDiscard = false;

function changePanel(d) {
  const s = d.state;
  const c = s.change;
  const checks = c.checks;
  const hasChanges = c.base !== c.head;
  const checksCurrent = checks && checks.commit === c.head;
  const checksRunning = checks && !checks.finished && d.live;
  const passed = checksCurrent && checks.finished && checks.results.every((r) => r.ok);
  const failed = checks && checks.results.find((r) => !r.ok);
  const open = c.status === "open";
  const canAdopt = open && hasChanges && !d.live && (passed || !d.checks_required);

  const head = h("div", { class: "change-head" },
    h("div", { class: "grow" },
      h("h3", null, "Change"),
      h("div", { class: "muted small" }, `branch ${c.branch} · from ${c.base.slice(0, 8)}`)),
    h("span", { class: "chip " + (c.status === "adopted" ? "completed" : c.status === "discarded" ? "stopped" : "running") }, c.status));

  const files = hasChanges
    ? h("div", { class: "files" }, c.files.map((f) => {
        const [st, ...rest] = f.split("\t");
        return h("div", { class: "file-row" }, h("span", { class: "st st-" + st[0] }, st[0]), h("span", null, rest.join(" → ")));
      }))
    : h("div", { class: "muted small pad" }, d.live ? "Changes appear here when the run finishes." : "No files changed.");

  let checkList = null;
  if (checks) {
    checkList = h("div", { class: "checks" },
      h("div", { class: "checks-title" }, "Checks", !checksCurrent && h("span", { class: "muted small" }, " · ran on an older commit")),
      checks.results.map((r, i) =>
        keyedDetails(`check:${checks.started_at}:${i}`, { class: "check " + (r.ok ? "ok" : "err") },
          h("summary", null, h("span", { class: "mark" }, r.ok ? "✓" : "✗"), h("code", null, r.command), h("span", { class: "muted small" }, (r.duration_ms / 1000).toFixed(1) + "s")),
          h("pre", { class: "out" }, r.output || "(no output)"))),
      checksRunning && h("div", { class: "working small" }, h("span", { class: "spinner" }), "Running checks…"),
      !checksRunning && !checks.finished && h("div", { class: "muted small" }, "The checks did not finish."));
  } else if (open && hasChanges && d.checks_required && !d.live) {
    checkList = h("div", { class: "muted small pad" }, "Checks have not run on this change.");
  }

  const actions = h("div", { class: "change-actions" });
  if (open && hasChanges && !d.live) {
    actions.append(
      h("button", { class: "btn primary", type: "button", disabled: !canAdopt, title: canAdopt ? "Merge into your checkout" : "The checks must pass first",
        onclick: () => changeAction("adopt") }, "Adopt into checkout"),
      h("button", { class: "btn", type: "button", onclick: () => changeAction("checks") }, checks ? "Re-run checks" : "Run checks"),
      failed && h("button", { class: "btn", type: "button", onclick: () => askToFix(failed) }, "Ask Airth to fix"),
      confirmDiscard
        ? h("span", { class: "confirm" }, "Discard this change? ",
            h("button", { class: "btn danger", type: "button", onclick: () => { confirmDiscard = false; changeAction("discard"); } }, "Discard"),
            h("button", { class: "btn ghost", type: "button", onclick: () => { confirmDiscard = false; renderRun(); } }, "Keep"))
        : h("button", { class: "btn danger ghost", type: "button", onclick: () => { confirmDiscard = true; renderRun(); } }, "Discard…"));
  }
  if (hasChanges) {
    const shown = app.diff && app.diff.runId === s.id;
    actions.append(h("button", { class: "btn ghost", type: "button", onclick: toggleDiff }, shown ? "Hide diff" : "View diff"));
  }
  const err = app.changeError ? h("div", { class: "error-banner" }, app.changeError) : null;
  const diff = app.diff && app.diff.runId === s.id ? diffView(app.diff.text || "(empty diff)", "diff") : null;

  return h("div", { class: "change" }, head, files, checkList, err, actions.childNodes.length ? actions : null, diff);
}

async function toggleDiff() {
  if (app.diff && app.diff.runId === app.route.id) app.diff = null;
  else {
    try {
      const { diff } = await api(`${runPath(app.realm, app.route.id)}/diff`);
      app.diff = { runId: app.route.id, text: diff };
    } catch (e) { app.changeError = e.message; }
  }
  renderRun();
}

async function changeAction(action) {
  app.changeError = null;
  try {
    await api(`${runPath(app.realm, app.route.id)}/${action}`, {});
    app.version = null;
    if (action !== "checks") app.diff = null;
    await Promise.all([pollRun(), refreshRuns()]);
  } catch (e) {
    app.changeError = e.message;
    renderRun();
  }
}

function askToFix(failed) {
  const lines = failed.output.split("\n");
  const tail = lines.slice(-60).join("\n");
  sendFollowUp(`The check \`${failed.command}\` failed:\n\n\`\`\`\n${tail}\n\`\`\`\n\nFix the problem, then summarize what you changed.`)
    .catch((e) => { app.changeError = e.message; renderRun(); });
}

async function answer(approve, forRun) {
  const d = app.run;
  if (!d || !d.pending || app.busy) return;
  app.busy = true;
  try {
    await api(`${runPath(app.realm, app.route.id)}/approval`, {
      approval_id: d.pending.id, approve, for_run: !!forRun, reason: approve ? "" : app.reason,
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
    await api(`${runPath(app.realm, app.route.id)}/stop`, {});
    await pollRun();
  } catch (e) {
    app.error = e.message;
  }
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && app.route.view === "run" && app.run && app.run.pending) {
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag !== "TEXTAREA") { e.preventDefault(); answer(true); }
  }
});

// ---------- realm settings ----------

const CAP_HELP = {
  read: "Read files and list directories",
  write: "Create and change files",
  execute: "Run shell commands",
  memory: "Save facts to this realm's memory",
  network: "Call integrations and websites",
  skill: "Use skills you installed",
};

function renderSettings(main) {
  const d = app.realmDetail;
  if (!d) return page(main, "Realm settings", "Loading…");
  const save = async (patch, msg) => {
    try {
      app.realmDetail = await api(realmPath(d.name), patch, "PATCH");
      flash(msg || "Saved");
      await refreshRealms();
      renderMain();
      renderSidebar();
    } catch (e) { flash(e.message, true); }
  };

  const desc = h("input", { type: "text", value: d.description, placeholder: "What this realm is about (the agent sees this)" });
  desc.addEventListener("change", () => save({ description: desc.value }));

  const policy = h("div", { class: "policy" }, Object.keys(CAP_HELP).map((cap) =>
    h("div", { class: "policy-row" },
      h("div", null, h("div", { class: "strong" }, cap), h("div", { class: "muted small" }, CAP_HELP[cap])),
      h("div", { class: "seg", role: "radiogroup", "aria-label": cap },
        ["allow", "ask", "deny"].map((rule) =>
          h("button", { type: "button", role: "radio", "aria-checked": String(d.policy[cap] === rule), class: d.policy[cap] === rule ? "on rule-" + rule : "",
            onclick: () => save({ policy: { [cap]: rule } }) }, rule))))));

  const checks = h("textarea", { rows: 3, placeholder: "One command per line, e.g. cargo test" });
  checks.value = d.checks.join("\n");

  const memInput = h("input", { type: "text", placeholder: "Something Airth should remember in this realm…" });
  const addMemory = async () => {
    if (!memInput.value.trim()) return;
    try {
      app.realmDetail = await api(`${realmPath(d.name)}/memory`, { text: memInput.value });
      renderMain();
    } catch (e) { flash(e.message, true); }
  };
  memInput.addEventListener("keydown", (e) => { if (e.key === "Enter") addMemory(); });

  const secKey = h("input", { type: "text", placeholder: "NAME, e.g. GITHUB_TOKEN", autocomplete: "off", spellcheck: "false" });
  const secVal = h("input", { type: "password", placeholder: "value", autocomplete: "new-password" });
  const addSecret = async () => {
    try {
      app.realmDetail = await api(`${realmPath(d.name)}/secrets`, { key: secKey.value.trim().toUpperCase(), value: secVal.value });
      renderMain();
      flash("Credential saved");
    } catch (e) { flash(e.message, true); }
  };

  const integ = {
    name: h("input", { type: "text", placeholder: "name, e.g. github", autocomplete: "off", spellcheck: "false" }),
    url: h("input", { type: "text", placeholder: "base URL, e.g. https://api.github.com", autocomplete: "off", spellcheck: "false" }),
    header: h("input", { type: "text", placeholder: "header (optional), e.g. Authorization: Bearer [secret:GITHUB_TOKEN]", autocomplete: "off", spellcheck: "false" }),
    desc: h("input", { type: "text", placeholder: "what it is for (the agent sees this)" }),
  };
  const addIntegration = async () => {
    const headers = {};
    const hv = integ.header.value.trim();
    if (hv) {
      const i = hv.indexOf(":");
      if (i < 1) return flash("Headers look like “Name: value”", true);
      headers[hv.slice(0, i).trim()] = hv.slice(i + 1).trim();
    }
    try {
      app.realmDetail = await api(`${realmPath(d.name)}/integrations`, {
        name: integ.name.value.trim(), base_url: integ.url.value.trim(), description: integ.desc.value.trim(), headers,
      });
      renderMain();
      flash("Integration saved");
    } catch (e) { flash(e.message, true); }
  };

  page(main, d.name, null,
    h("div", { class: "card" },
      field("Description", desc),
      field("Workspace", h("code", { class: "path" }, d.root), d.own_root ? null : "A private folder in Airth's home."),
      field("Isolation", h("span", null, d.isolation === "worktree" ? "Each run works in its own git worktree; you adopt or discard its change." : "Runs work directly in the workspace.")),
      field("Model", d.model ? h("span", null, d.model) : h("span", { class: "warn" }, d.model_error || "not configured"))),

    h("h2", null, "Permissions"),
    h("p", { class: "muted" }, "What Airth may do in this realm without asking. Only you can change this."),
    h("div", { class: "card" }, policy),

    h("h2", null, "Checks"),
    h("p", { class: "muted" }, "Commands Airth's runtime runs on every change before you can adopt it. They run in the run's sandbox."),
    h("div", { class: "card" }, checks,
      h("div", { class: "row-end" }, h("button", { class: "btn", type: "button", onclick: () => save({ checks: checks.value.split("\n") }, "Checks saved") }, "Save checks"))),

    h("h2", null, "Memory"),
    h("p", { class: "muted" }, "What Airth remembers in this realm. Runs in other realms never see it."),
    h("div", { class: "card" },
      d.memory.length ? d.memory.map((m) =>
        h("div", { class: "list-row" },
          h("div", { class: "grow" }, m.text, h("div", { class: "muted small" }, (m.source === "owner" ? "added by you" : "learned in " + m.source.replace("run:", "run ")) + " · " + ago(m.created_at))),
          h("button", { class: "icon-btn", type: "button", title: "Forget", "aria-label": "Forget",
            onclick: async () => { app.realmDetail = await api(`${realmPath(d.name)}/memory/${enc(m.id)}`, undefined, "DELETE"); renderMain(); } }, "×")))
        : h("div", { class: "muted small pad" }, "Nothing yet."),
      h("div", { class: "inline-form" }, memInput, h("button", { class: "btn", type: "button", onclick: addMemory }, "Add"))),

    h("h2", null, "Credentials"),
    h("p", { class: "muted" }, "Given to this realm's commands as environment variables, and hidden from the model. Model keys (ANTHROPIC_API_KEY…) are used for this realm's model only."),
    h("div", { class: "card" },
      d.secrets.length ? d.secrets.map((s) =>
        h("div", { class: "list-row" },
          h("code", { class: "grow" }, s.key), s.provider && h("span", { class: "chip" }, "model"),
          h("button", { class: "icon-btn", type: "button", title: "Remove", "aria-label": "Remove " + s.key,
            onclick: async () => { app.realmDetail = await api(`${realmPath(d.name)}/secrets/${enc(s.key)}`, undefined, "DELETE"); renderMain(); } }, "×")))
        : h("div", { class: "muted small pad" }, "No credentials."),
      h("div", { class: "inline-form" }, secKey, secVal, h("button", { class: "btn", type: "button", onclick: addSecret }, "Save"))),

    h("h2", null, "Integrations"),
    h("p", { class: "muted" }, "Services this realm's runs may call. Credentials go in headers as [secret:NAME]: the runtime fills them in, only for requests under the base URL, and the model never sees them. Without an integration, runs can only reach public websites, with no credentials."),
    h("div", { class: "card" },
      d.integrations.length ? d.integrations.map((i) =>
        h("div", { class: "list-row" },
          h("div", { class: "grow" },
            h("div", null, h("span", { class: "strong" }, i.name), " ", h("code", { class: "path" }, i.base_url)),
            i.description && h("div", { class: "muted small" }, i.description),
            Object.entries(i.headers).map(([k, v]) => h("div", { class: "muted small" }, h("code", null, `${k}: ${v}`))),
            i.missing_secrets.length > 0 && h("div", { class: "warn small" }, "Missing credential: " + i.missing_secrets.join(", "))),
          h("button", { class: "icon-btn", type: "button", title: "Remove", "aria-label": "Remove " + i.name,
            onclick: async () => { app.realmDetail = await api(`${realmPath(d.name)}/integrations/${enc(i.name)}`, undefined, "DELETE"); renderMain(); } }, "×")))
        : h("div", { class: "muted small pad" }, "No integrations."),
      h("div", { class: "stack-form" }, h("div", { class: "inline-form" }, integ.name, integ.url), integ.header, h("div", { class: "inline-form" }, integ.desc, h("button", { class: "btn", type: "button", onclick: addIntegration }, "Add")))),

    h("h2", null, "Privacy"),
    h("div", { class: "card" },
      h("label", { class: "toggle" },
        h("input", { type: "checkbox", checked: d.share_owner_profile, onchange: (e) => save({ share_owner_profile: e.target.checked }) }),
        h("span", null, "Share your profile with this realm's runs"))),

    h("div", { class: "row-end spaced" },
      !d.default && h("button", { class: "btn", type: "button", onclick: () => save({ default: true }, "Default realm set") }, "Make default"),
      h("button", { class: "btn danger", type: "button", onclick: () => save({ archived: !d.archived }, d.archived ? "Restored" : "Archived") }, d.archived ? "Restore realm" : "Archive realm")));
}

function field(label, control, help) {
  return h("div", { class: "field" }, h("label", null, label), h("div", null, control, help && h("div", { class: "muted small" }, help)));
}

let flashTimer = null;
function flash(text, isError) {
  let el = $("#flash");
  if (!el) { el = h("div", { id: "flash", role: "status" }); document.body.append(el); }
  el.textContent = text;
  el.className = isError ? "err show" : "show";
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => (el.className = ""), isError ? 5000 : 1800);
}

// ---------- owner profile ----------

const OWNER_LISTS = [
  ["preferences", "Preferences", "How you like things done"],
  ["goals", "Goals", "Short- and long-term"],
  ["commitments", "Commitments", "What you are on the hook for"],
  ["boundaries", "Boundaries", "Lines Airth must never cross"],
];

async function renderOwner(main) {
  page(main, "Your profile", "What Airth knows about you. Realms that share your profile give it to every run; you can turn that off per realm.");
  let owner;
  try { owner = await api("owner"); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "owner") return;
  const name = h("input", { type: "text", value: owner.name, placeholder: "Your name" });
  const about = h("textarea", { rows: 3, placeholder: "Who you are, what you do" });
  about.value = owner.about;
  const lists = OWNER_LISTS.map(([key, label, help]) => {
    const ta = h("textarea", { rows: 3, placeholder: "One per line" });
    ta.value = owner[key].join("\n");
    return { key, ta, el: field(label, ta, help) };
  });
  const saveBtn = h("button", { class: "btn primary", type: "button", onclick: async () => {
    const body = { name: name.value.trim(), about: about.value.trim() };
    for (const l of lists) body[l.key] = l.ta.value.split("\n").map((x) => x.trim()).filter(Boolean);
    try { await api("owner", body, "PUT"); flash("Profile saved"); } catch (e) { flash(e.message, true); }
  } }, "Save profile");
  $(".page-inner", main).append(
    h("div", { class: "card" }, field("Name", name), field("About", about), lists.map((l) => l.el)),
    h("div", { class: "row-end spaced" }, owner.updated_at && h("span", { class: "muted small" }, "updated " + ago(owner.updated_at)), saveBtn));
}

// ---------- objectives ----------

function when(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  const mins = Math.round((d - Date.now()) / 60000);
  if (mins <= 0) return "due now";
  if (mins < 60) return `in ${mins} min`;
  if (mins < 24 * 60) return `in ${Math.round(mins / 60)} h · ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  return d.toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

function grantText(g) {
  return g.scope ? `${g.capability}: ${g.scope.replace(/^integration /, "")}` : `all ${g.capability}`;
}

let confirmDelete = null;

async function renderObjectives(main) {
  const realm = app.realm;
  page(main, "Objectives", "Work Airth starts on its own: on a schedule, when files change, or from a local webhook. Each run may do what this realm allows plus what you approve here; anything else waits for you, then is denied. You get a notification when a run ends.");
  const inner = $(".page-inner", main);
  const listEl = h("div", null, h("div", { class: "muted small" }, "Loading…"));
  inner.append(listEl);
  let list;
  try { list = await api(`${realmPath(realm)}/objectives`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "objectives" || app.realm !== realm) return;
  const d = app.realmDetail;

  const refresh = (l) => { list = l; draw(); };
  const draw = () => listEl.replaceChildren(
    list.length ? h("div", { class: "objectives" }, list.map((o) => h("div", { class: "card objective" + (o.enabled ? "" : " off") },
      h("div", { class: "objective-head" },
        h("div", { class: "grow" },
          h("div", { class: "strong" }, o.id),
          h("div", null, o.objective)),
        h("label", { class: "toggle small", title: o.enabled ? "Turn off" : "Turn on" },
          h("input", { type: "checkbox", checked: o.enabled, onchange: async (e) => {
            try { refresh(await api(`${realmPath(realm)}/objectives/${enc(o.id)}`, { enabled: e.target.checked }, "PATCH")); } catch (err) { flash(err.message, true); }
          } }), h("span", null, o.enabled ? "on" : "off"))),
      h("div", { class: "objective-meta" },
        h("span", { class: "chip" }, "◷ " + o.trigger_label),
        o.enabled && o.next_at && h("span", null, "next " + when(o.next_at)),
        o.running ? h("span", { class: "chip running" }, "running")
          : o.last_status && h("span", null, "last: ", o.last_run
            ? h("a", { href: `#/r/${enc(realm)}/run/${enc(o.last_run)}` }, o.last_status)
            : o.last_status, o.last_at && " · " + ago(o.last_at))),
      h("div", { class: "objective-meta muted small" },
        "Approved in advance: ", o.approvals.length ? o.approvals.map(grantText).join(", ") : "nothing beyond the realm's rules",
        " · waits ", String(o.approval_wait_minutes), " min for you",
        Object.keys(o.budget || {}).length ? " · budget: " + Object.entries(o.budget).map(([k, v]) => `${v} ${k.replace("max_", "")}`).join(", ") : ""),
      o.webhook_url && h("div", { class: "objective-meta" }, h("code", { class: "path small" }, "curl -X POST --data @payload.json " + o.webhook_url)),
      h("div", { class: "row-end" },
        h("button", { class: "btn", type: "button", disabled: o.running || !!(d && d.model_error), onclick: async () => {
          try { const r = await api(`${realmPath(realm)}/objectives/${enc(o.id)}/run`, {}); go(`#/r/${enc(realm)}/run/${enc(r.id)}`); } catch (e) { flash(e.message, true); }
        } }, "Run now"),
        h("button", { class: "btn danger", type: "button", onclick: async () => {
          if (confirmDelete !== o.id) { confirmDelete = o.id; return draw(); }
          confirmDelete = null;
          try { refresh(await api(`${realmPath(realm)}/objectives/${enc(o.id)}`, undefined, "DELETE")); } catch (e) { flash(e.message, true); }
        } }, confirmDelete === o.id ? "Really delete?" : "Delete")))))
      : h("div", { class: "card muted small pad" }, "No objectives yet."));
  draw();

  // New objective
  const f = {
    id: h("input", { type: "text", placeholder: "name, e.g. morning-triage", autocomplete: "off", spellcheck: "false" }),
    objective: h("textarea", { rows: 3, placeholder: "What each run should achieve, e.g. “Triage new issues in the tracker and write today's summary to triage.md”" }),
    kind: h("select", null, ["schedule", "watch", "webhook", "manual"].map((k) => h("option", { value: k }, { schedule: "On a schedule", watch: "When files change", webhook: "From a webhook", manual: "Only when I start it" }[k]))),
    every: h("input", { type: "text", placeholder: "every 30m · daily 08:30 · weekdays 09:00 · mon,thu 18:00", value: "daily 08:30" }),
    path: h("input", { type: "text", placeholder: "folder or file in the realm, e.g. inbox" }),
    wait: h("input", { type: "number", min: 0, value: 10 }),
    minutes: h("input", { type: "number", min: 1, placeholder: "minutes" }),
    tokens: h("input", { type: "number", min: 1000, step: 1000, placeholder: "tokens" }),
    iterations: h("input", { type: "number", min: 1, placeholder: "model calls" }),
  };
  const grantOptions = [
    { capability: "write", label: "Change files" },
    { capability: "execute", label: "Run commands" },
    { capability: "memory", label: "Save to memory" },
    { capability: "network", label: "Any web request" },
    ...((d && d.integrations) || []).map((i) => ({ capability: "network", scope: "integration " + i.name, label: `Call ${i.name}` })),
  ].map((g) => ({ ...g, box: h("input", { type: "checkbox" }) }));
  const triggerField = h("div");
  const syncTrigger = () => triggerField.replaceChildren(
    f.kind.value === "schedule" ? f.every : f.kind.value === "watch" ? f.path
      : h("div", { class: "muted small" }, f.kind.value === "webhook" ? "You get a private URL to POST to from this machine; the request body is given to the run as data." : "Start it from this page or with airth objective run."));
  f.kind.addEventListener("change", syncTrigger);
  syncTrigger();
  const num = (el) => (el.value.trim() ? Number(el.value) : undefined);
  const save = async () => {
    const body = {
      id: f.id.value.trim(), objective: f.objective.value.trim(),
      trigger: { kind: f.kind.value, every: f.every.value, path: f.path.value },
      approvals: grantOptions.filter((g) => g.box.checked).map((g) => ({ capability: g.capability, scope: g.scope || null })),
      approval_wait_minutes: num(f.wait) ?? 10,
      budget: { max_minutes: num(f.minutes), max_tokens: num(f.tokens), max_iterations: num(f.iterations) },
    };
    try {
      refresh(await api(`${realmPath(realm)}/objectives`, body));
      f.id.value = ""; f.objective.value = "";
      flash("Objective saved");
    } catch (e) { flash(e.message, true); }
  };
  inner.append(
    h("h2", null, "New objective"),
    h("div", { class: "card" },
      field("Name", f.id),
      field("Objective", f.objective),
      field("Starts", h("div", { class: "stack" }, f.kind, triggerField)),
      field("Approved in advance", h("div", { class: "checks" }, grantOptions.map((g) => h("label", { class: "toggle small" }, g.box, h("span", null, g.label)))),
        "Commands still run in the realm's sandbox. Edits to Airth's security code are never approved in advance."),
      field("Otherwise", h("div", { class: "inline" }, "wait ", f.wait, " minutes for me, then deny")),
      field("Budget per run", h("div", { class: "inline" }, f.minutes, f.tokens, f.iterations), "Leave empty for the defaults."),
      h("div", { class: "row-end" }, h("button", { class: "btn primary", type: "button", onclick: save }, "Save objective"))));
}

// ---------- skills ----------

const SKILL_HELP = {
  proposed: "Proposed by a run. Nothing is built yet.",
  developing: "Airth is building it in its own folder, with no access to the rest of the realm.",
  ready: "Built, and every test passed. Review it, then install it to enable it.",
  failed: "Built, but the evaluation failed.",
  installed: "Enabled: runs in this realm can use it.",
  rejected: "You declined it.",
};

const skillUi = { open: new Set(), files: {}, feedback: {} };

async function renderSkills(main) {
  const realm = app.realm;
  page(main, "Skills", "Capabilities Airth builds for itself: small programs with tests that become tools. Runs propose them when they hit a limit; Airth develops each one in isolation and tests it; nothing is enabled until you install it.");
  const inner = $(".page-inner", main);
  const listEl = h("div", null, h("div", { class: "muted small" }, "Loading…"));
  inner.append(listEl);
  let list;
  try { list = await api(`${realmPath(realm)}/skills`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "skills" || app.realm !== realm) return;
  const d = app.realmDetail;
  const noModel = !!(d && d.model_error);

  const act = async (name, action) => {
    try { list = await api(`${realmPath(realm)}/skills/${enc(name)}/${action}`, {}); draw(); flash({ install: "Installed", uninstall: "Uninstalled", evaluate: "Evaluated", reject: "Rejected" }[action]); }
    catch (e) { flash(e.message, true); }
  };
  const develop = async (name, feedback) => {
    try {
      const r = await api(`${realmPath(realm)}/skills/${enc(name)}/develop`, feedback ? { feedback } : {});
      go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
    } catch (e) { flash(e.message, true); }
  };
  const toggleFiles = async (name) => {
    if (skillUi.open.has(name)) { skillUi.open.delete(name); return draw(); }
    skillUi.open.add(name);
    try { skillUi.files[name] = await api(`${realmPath(realm)}/skills/${enc(name)}/files`); } catch (e) { flash(e.message, true); }
    draw();
  };

  const card = (sk) => {
    const ev = sk.evaluation;
    const fb = h("input", { type: "text", placeholder: "What to change (optional)…", value: skillUi.feedback[sk.name] || "", oninput: (e) => (skillUi.feedback[sk.name] = e.target.value) });
    const failedReport = ev ? [...ev.problems.map((p) => "problem: " + p), ...ev.results.filter((r) => !r.ok).map((r) => `${r.name}: ${r.detail}`)].join("\n") : "";
    return h("div", { class: "card objective" },
      h("div", { class: "objective-head" },
        h("div", { class: "grow" },
          h("div", null, h("span", { class: "strong mono" }, sk.name), " ", h("span", { class: "chip skill-" + sk.status }, sk.developing_now ? "developing…" : sk.status)),
          h("div", null, sk.description))),
      h("div", { class: "muted small" }, SKILL_HELP[sk.status]),
      sk.reason && h("div", { class: "objective-meta" }, "Why: ", sk.reason),
      h("div", { class: "objective-meta" },
        h("span", null, sk.proposed_by === "owner" ? "asked for by you" : "proposed by ", sk.proposed_by !== "owner" && h("a", { href: `#/r/${enc(realm)}/run/${enc(sk.proposed_by.replace("run:", ""))}` }, "a run")),
        sk.dev_run && h("a", { href: `#/r/${enc(realm)}/run/${enc(sk.dev_run)}` }, "development run"),
        sk.installed && h("span", null, "installed " + ago(sk.installed.at))),
      ev && h("div", { class: "eval" },
        h("div", { class: ev.problems.length || ev.results.some((r) => !r.ok) ? "warn small strong" : "ok small strong" }, "Evaluation: " + sk.summary + " · " + ago(ev.at)),
        ev.problems.map((p) => h("div", { class: "eval-row bad" }, "✗ " + p)),
        ev.results.map((r) => h("div", { class: "eval-row " + (r.ok ? "good" : "bad") }, (r.ok ? "✓ " : "✗ ") + r.name + ` (${(r.duration_ms / 1000).toFixed(1)}s)`,
          !r.ok && h("pre", { class: "out" }, r.detail)))),
      skillUi.open.has(sk.name) && h("div", { class: "files" }, (skillUi.files[sk.name] || []).map((f) =>
        h("details", { class: "tool", open: true }, h("summary", null, h("span", { class: "label" }, f.path)),
          h("div", { class: "body" }, h("pre", { class: "out" }, f.content == null ? "(binary file)" : f.content))))),
      ["ready", "failed", "installed"].includes(sk.status) && !sk.developing_now && h("div", { class: "inline-form" }, fb,
        h("button", { class: "btn", type: "button", disabled: noModel, onclick: () => develop(sk.name, fb.value.trim() || (sk.status === "failed" ? "The evaluation failed:\n" + failedReport + "\nFix it and make every test pass." : "")) }, sk.status === "failed" ? "Ask Airth to fix" : "Ask for changes")),
      h("div", { class: "row-end" },
        sk.dev_run || sk.evaluation ? h("button", { class: "btn", type: "button", onclick: () => toggleFiles(sk.name) }, skillUi.open.has(sk.name) ? "Hide files" : "Review files") : null,
        sk.status === "proposed" && h("button", { class: "btn primary", type: "button", disabled: noModel, onclick: () => develop(sk.name) }, "Develop it"),
        ["ready", "failed", "installed"].includes(sk.status) && !sk.developing_now && h("button", { class: "btn", type: "button", onclick: () => act(sk.name, "evaluate") }, "Re-run tests"),
        sk.status === "ready" && h("button", { class: "btn primary", type: "button", onclick: () => act(sk.name, "install") }, "Install"),
        sk.installed && h("button", { class: "btn", type: "button", onclick: () => act(sk.name, "uninstall") }, "Uninstall"),
        ["proposed", "ready", "failed"].includes(sk.status) && h("button", { class: "btn", type: "button", onclick: () => act(sk.name, "reject") }, "Reject"),
        h("button", { class: "btn danger", type: "button", onclick: async () => {
          if (confirmDelete !== "skill:" + sk.name) { confirmDelete = "skill:" + sk.name; return draw(); }
          confirmDelete = null;
          try { list = await api(`${realmPath(realm)}/skills/${enc(sk.name)}`, undefined, "DELETE"); draw(); } catch (e) { flash(e.message, true); }
        } }, confirmDelete === "skill:" + sk.name ? "Really delete?" : "Delete")));
  };
  const draw = () => listEl.replaceChildren(list.length ? h("div", { class: "objectives" }, list.map(card)) : h("div", { class: "card muted small pad" }, "No skills yet. Runs propose them when they need a capability; you can also ask for one below."));
  draw();

  const name = h("input", { type: "text", placeholder: "name, e.g. ical_events", autocomplete: "off", spellcheck: "false" });
  const desc = h("textarea", { rows: 3, placeholder: "What it should do: inputs, outputs, edge cases" });
  inner.append(h("h2", null, "Ask for a skill"),
    h("div", { class: "card" }, field("Name", name), field("What it does", desc),
      h("div", { class: "row-end" }, h("button", { class: "btn primary", type: "button", disabled: noModel, onclick: async () => {
        try {
          const r = await api(`${realmPath(realm)}/skills`, { name: name.value.trim(), description: desc.value.trim() });
          go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
        } catch (e) { flash(e.message, true); }
      } }, "Develop it"))));
}

// ---------- inbox ----------

const KIND_MARK = { completed: "✓", failed: "✗", stopped: "■", approval: "?", review: "⧗" };

async function renderInbox(main) {
  page(main, "Inbox", "What Airth did on its own, and what waits for you.");
  const inner = $(".page-inner", main);
  let notes, summary;
  try { [notes, summary] = await Promise.all([api("inbox"), api("summary?hours=24")]); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "inbox") return;
  const open = (realm, run) => go(`#/r/${enc(realm)}${run ? "/run/" + enc(run) : ""}`);

  inner.append(h("h2", null, "Last 24 hours"));
  if (!summary.length) inner.append(h("div", { class: "card muted small pad" }, "Nothing happened."));
  for (const r of summary) {
    const line = (mark, l, extra) => h("button", { class: "list-row link", type: "button", onclick: () => open(r.realm, l.id) },
      h("span", { class: "mark" }, mark),
      h("div", { class: "grow" }, h("div", null, l.objective, l.origin && h("span", { class: "muted small" }, " · " + l.origin)),
        (extra || l.report) && h("div", { class: "muted small" }, extra || l.report)));
    inner.append(h("div", { class: "card" },
      h("div", { class: "summary-head" }, h("span", { class: "strong" }, r.realm),
        h("span", { class: "muted small" }, `${r.runs} run${r.runs === 1 ? "" : "s"} · ${r.completed} completed · ${r.failed} failed · ${r.stopped} stopped`)),
      r.to_review.map((l) => line("⧗", l, "Change waiting for your review")),
      r.unfinished.map((l) => line("↻", l, `${l.status}: can be resumed`)),
      r.recent.map((l) => line("✓", l)),
      r.objectives.filter((o) => o.enabled).length > 0 && h("div", { class: "muted small pad-top" },
        "Objectives: ", r.objectives.filter((o) => o.enabled).map((o) => o.id + (o.next_at ? ` (${when(o.next_at)})` : "")).join(" · "))));
  }

  inner.append(h("h2", null, "Notifications"));
  inner.append(notes.length ? h("div", { class: "card" }, notes.slice(0, 100).map((n) =>
    h("button", { class: "list-row link" + (n.read ? "" : " unread"), type: "button", onclick: () => open(n.realm, n.run) },
      h("span", { class: "mark " + n.kind }, KIND_MARK[n.kind] || "•"),
      h("div", { class: "grow" },
        h("div", null, n.title),
        n.body && h("div", { class: "muted small clamp" }, n.body),
        h("div", { class: "muted small" }, n.realm + " · " + ago(n.created_at))))))
    : h("div", { class: "card muted small pad" }, "No notifications."));
  if (notes.some((n) => !n.read)) {
    api("inbox/read", {}).then(() => { if (app.info) app.info.unread = 0; renderSidebar(); }).catch(() => {});
  }
}

// ---------- new realm ----------

function renderNewRealm(main) {
  const first = app.realms.length === 0;
  const name = h("input", { type: "text", placeholder: "personal, acme, freelance…", autocomplete: "off", spellcheck: "false" });
  const desc = h("input", { type: "text", placeholder: "e.g. My job at Acme Corp" });
  const root = h("input", { type: "text", placeholder: "Leave empty for a private folder, or e.g. ~/code/project", spellcheck: "false" });
  const iso = h("select", null,
    h("option", { value: "" }, "Automatic (a worktree per run for git repositories)"),
    h("option", { value: "none" }, "None: work directly in the folder"),
    h("option", { value: "worktree" }, "Worktree per run"));
  const err = h("div", { class: "error-banner", hidden: true });
  const create = async () => {
    try {
      const r = await api("realms", { name: name.value.trim(), description: desc.value, root: root.value, isolation: iso.value || null });
      await refreshRealms();
      go(`#/r/${enc(r.name)}`);
    } catch (e) { err.textContent = e.message; err.hidden = false; }
  };
  name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
  page(main, first ? "Welcome to Airth" : "New realm",
    first
      ? "Airth keeps each area of your life in its own realm, with its own files, memory, credentials and permissions. Nothing crosses between realms. Create your first one."
      : "A realm is an isolated area of your life: its own files, memory, credentials and permissions.",
    h("div", { class: "card" },
      field("Name", name, "Lowercase letters, digits and dashes."),
      field("Description", desc, "Airth sees this, so it knows what the realm is for."),
      field("Folder", root, "The directory Airth works in. Other realms can never read it."),
      field("Isolation", iso)),
    err,
    h("div", { class: "row-end spaced" }, h("button", { class: "btn primary", type: "button", onclick: create }, "Create realm")));
  name.focus();
}

// ---------- polling ----------

let runTimer = null;

async function pollRun() {
  clearTimeout(runTimer);
  if (app.route.view !== "run" || app.unauthorized) return;
  const { realm, id } = app.route;
  let next = 3000;
  try {
    const q = app.version != null && app.run && app.run.live ? `?since=${app.version}` : "";
    const d = await api(runPath(realm, id) + q);
    if (app.route.view !== "run" || app.route.id !== id) return;
    const same = !d.unchanged && app.run && !d.live && !app.run.live &&
      d.state.updated_at === app.run.state.updated_at;
    if (!d.unchanged && !same) {
      const wasLive = app.run && app.run.live;
      app.run = d;
      if (wasLive && !d.live) app.diff = null;
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
  if (!app.realm) return;
  try {
    app.runs = await api(`${realmPath(app.realm)}/runs`);
    renderSidebar();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

async function refreshInfo() {
  try {
    const before = app.info && app.info.unread;
    app.info = await api("info");
    if (app.info.unread !== before) renderSidebar();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

async function refreshRealms() {
  try {
    app.realms = await api("realms");
    renderRealmSwitch();
    updateTitle();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

async function refreshRealmDetail() {
  if (!app.realm) return;
  try {
    app.realmDetail = await api(realmPath(app.realm));
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
    app.realmDetail = null;
  }
}

function unauthorized() {
  app.unauthorized = true;
  renderMain();
}

async function boot() {
  $("#new-run-btn").addEventListener("click", () => app.realm && go(`#/r/${enc(app.realm)}`));
  window.addEventListener("hashchange", onRoute);
  if (app.unauthorized) return renderMain();
  try {
    app.info = await api("info");
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
  }
  await refreshRealms();
  await onRoute();
  setInterval(() => { refreshRealms(); refreshRuns(); }, 2000);
  setInterval(refreshInfo, 5000);
}

boot();
