// ---------- command palette & shortcuts ----------

let palette = null;

/** Everything the palette can do, grouped; runs and realms come from what the app already loaded. */
function paletteItems() {
  const r = app.realm && enc(app.realm);
  const dark = window.airthTheme && window.airthTheme.current() === "dark";
  const items = [];
  const add = (group, label, lead, action, extra = {}) => items.push({ group, label, lead, action, ...extra });
  if (r) add("Actions", "New chat", icon("pencil"), () => go(`#/r/${r}`), { keys: modKey + "⇧O" });
  add("Actions", dark ? "Switch to light mode" : "Switch to dark mode", icon(dark ? "sun" : "moon"),
    () => { window.airthTheme && window.airthTheme.set(dark ? "light" : "dark"); renderFoot(); });
  if (app.route.view === "run" && runEls) add("Actions", "Show or hide plan & details", icon("panel"), toggleSide);
  if (app.route.view === "run" && runEls && app.run) add("Actions", "Rename this chat", icon("rename"),
    () => setTimeout(() => editInPlace(runEls && runEls.header.querySelector("h1"), app.route.id, runName({ title: app.run.title, objective: app.run.state.objective })), 160));
  if (r) add("Go to", "Steering", icon("compass"), () => go(`#/r/${r}/steering`));
  if (r) add("Go to", "Routines", icon("refresh"), () => go(`#/r/${r}/routines`));
  add("Go to", "Usage", icon("chart"), () => go("#/usage"));
  if (app.realm) add("Go to", "What can Airth do for you?", icon("compass"), () => {
    const show = () => openCanDo((text) => {
      const ta = $(".new-view textarea");
      if (ta) { ta.value = text; ta.dispatchEvent(new Event("input")); ta.focus(); }
    });
    if (app.route.view === "new") show(); else { go(`#/r/${enc(app.realm)}`); setTimeout(show, 300); }
  });
  add("Go to", "Welcome guide", icon("compass"), () => { welcomeUi.step = 0; welcomeUi.setup = null; go("#/welcome"); });
  if (r) {
    add("Go to", "Documents", icon("folder"), () => go(`#/r/${r}/documents`));
    add("Go to", "Skills", icon("spark"), () => go(`#/r/${r}/skills`));
    add("Go to", "Connectors (Notion, Linear, GitHub…)", icon("world"), () => { skillUi.tab = "connectors"; go(`#/r/${r}/skills`); });
    add("Go to", "Realm settings", icon("gear"), () => go(`#/r/${r}/settings`));
  }
  add("Go to", "Your profile", icon("notes"), () => go("#/owner"));
  add("Go to", "Appearance", icon("sun"), () => go("#/owner/appearance"));
  const runs = [...app.runs].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at));
  for (const run of runs) {
    const status = run.needs_approval ? "awaiting" : run.live ? "running" : run.status;
    add(`Chats in ${app.realm}`, runName(run), h("span", { class: "i" }, h("span", { class: "dot " + status })),
      () => go(`#/r/${enc(app.realm)}/run/${enc(run.id)}`), { hint: `${run.live ? "running" : run.status} · ${ago(run.updated_at)}`, run: true });
  }
  for (const realm of app.realms) {
    if (realm.name === app.realm || realm.archived) continue;
    add("Switch realm", realm.name, h("span", { class: "realm-mark small" }, realm.name.slice(0, 1).toUpperCase()),
      () => go(`#/r/${enc(realm.name)}`), { hint: realm.description });
  }
  return items;
}

function openPalette() {
  if (palette || app.unauthorized) return;
  const input = h("input", { type: "text", class: "pal-input", placeholder: "Search chats, pages, realms, actions…", "aria-label": "Search", autocomplete: "off", spellcheck: "false" });
  const list = h("div", { class: "pal-list", role: "listbox" });
  const overlay = h("div", { class: "pal-overlay", onmousedown: (e) => { if (e.target === overlay) closePalette(); } },
    h("div", { class: "pal", role: "dialog", "aria-modal": "true", "aria-label": "Command palette" },
      h("div", { class: "pal-search" }, icon("search"), input, h("kbd", null, "esc")), list));
  const items = paletteItems();
  let shown = [];
  let sel = 0;
  const run = (it) => { closePalette(); it.action(); };
  const draw = () => {
    const q = input.value.trim().toLowerCase();
    if (q) {
      const words = q.split(/\s+/);
      shown = items.filter((it) => { const text = `${it.label} ${it.hint || ""} ${it.group}`.toLowerCase(); return words.every((w) => text.includes(w)); });
    } else {
      // Without a query: actions, pages, the 8 latest runs, other realms.
      let runs = 0;
      shown = items.filter((it) => !it.run || runs++ < 8);
    }
    shown = shown.slice(0, 80);
    sel = Math.max(0, Math.min(sel, shown.length - 1));
    list.replaceChildren();
    if (!shown.length) list.append(h("div", { class: "pal-empty" }, "Nothing matches."));
    let group = null;
    shown.forEach((it, i) => {
      if (it.group !== group) { group = it.group; list.append(h("div", { class: "pal-group" }, group)); }
      list.append(h("button", { type: "button", class: "pal-item" + (i === sel ? " sel" : ""), role: "option", "aria-selected": String(i === sel),
          onmousemove: () => { if (sel !== i) { sel = i; draw(); } }, onclick: () => run(it) },
        it.lead, h("span", { class: "pal-label" }, it.label), it.hint && h("span", { class: "pal-hint" }, it.hint), it.keys && h("kbd", null, it.keys)));
    });
    const cur = list.querySelector(".pal-item.sel");
    if (cur) cur.scrollIntoView({ block: "nearest" });
  };
  input.addEventListener("input", () => { sel = 0; draw(); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); sel = Math.min(sel + 1, shown.length - 1); draw(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); sel = Math.max(sel - 1, 0); draw(); }
    else if (e.key === "Enter") { e.preventDefault(); if (shown[sel]) run(shown[sel]); }
    else if (e.key === "Escape") { e.preventDefault(); closePalette(); }
  });
  palette = { overlay, prevFocus: document.activeElement };
  document.body.append(overlay);
  draw();
  input.focus();
}

function closePalette() {
  if (!palette) return;
  const { overlay, prevFocus } = palette;
  palette = null;
  overlay.classList.add("closing");
  setTimeout(() => overlay.remove(), 140);
  if (prevFocus && prevFocus.focus && document.contains(prevFocus)) prevFocus.focus();
}

/** Phone-sized windows: the sidebar opens over the page, on demand. */
const narrow = () => window.matchMedia("(max-width: 760px)").matches;

/** The sidebar hides only on a phone, where it opens over the page. */
function sidebarHidden() {
  return narrow();
}

// Crossing the phone-size boundary: closed on a phone, the owner's choice otherwise.
window.matchMedia("(max-width: 760px)").addEventListener("change", () => {
  const root = $("#app");
  if (root) root.classList.toggle("side-hidden", sidebarHidden());
});

// On a phone, a tap outside the open sidebar closes it.
document.addEventListener("click", (e) => {
  const root = $("#app");
  if (!narrow() || !root || root.classList.contains("side-hidden")) return;
  if (e.target.closest && (e.target.closest(".sidebar") || e.target.closest(".side-open"))) return;
  root.classList.add("side-hidden");
});

// In the Mac app the page is the title bar: an empty spot of the top strip
// moves the window, and a double click zooms it, as in other Mac apps.
const TITLE_BAR = 44;
const INTERACTIVE = "button, a, input, textarea, select, label, summary, [contenteditable], [role=button], .renamable";
function onTitleBar(e) {
  if (document.documentElement.dataset.shell !== "mac" || !window.ipc) return false;
  if (e.button !== 0 || e.clientY > TITLE_BAR) return false;
  return !(e.target.closest && e.target.closest(INTERACTIVE));
}
document.addEventListener("mousedown", (e) => {
  if (!onTitleBar(e)) return;
  e.preventDefault(); // no text selection from a title bar click
  if (e.detail === 1) window.ipc.postMessage("drag");
});
document.addEventListener("dblclick", (e) => {
  if (onTitleBar(e)) window.ipc.postMessage("zoom");
});

function toggleSidebar() {
  if (narrow()) $("#app").classList.toggle("side-hidden");
}

document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  const key = e.key.toLowerCase();
  if (mod && !e.shiftKey && !e.altKey && key === "k") { e.preventDefault(); palette ? closePalette() : openPalette(); }
  else if (mod && e.shiftKey && key === "o") { e.preventDefault(); if (app.realm) go(`#/r/${enc(app.realm)}`); }
  else if (e.key === "Escape" && realmMenuOpen) { realmMenuOpen = false; renderRealmSwitch(); }
});

// ---------- usage ----------

const usageUi = { days: 30, show: "cost", scope: "business" };

/** 1234 → 1.2k, 4567890 → 4.57M: token counts at a glance. */
function tok(n) {
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 1 : 2) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(n >= 1e4 ? 0 : 1) + "k";
  return String(n);
}

/** Estimated dollars: cents below $100, whole dollars above. */
function usd(x) {
  if (!x) return "$0";
  if (x < 0.01) return "<$0.01";
  return "$" + (x >= 100 ? Math.round(x).toLocaleString() : x.toFixed(2));
}

const total = (t) => t.input + t.output;
const costOf = (t) => t.cost.input + t.cost.cache_read + t.cost.cache_write + t.cost.output;
const fresh = (t) => Math.max(0, t.input - t.cache_read - t.cache_write);
const cachePct = (t) => (t.input ? Math.round((100 * t.cache_read) / t.input) : 0);
const tokDetail = (t) => `${tok(t.input)} in · ${tok(t.output)} out · ${t.calls} call${t.calls === 1 ? "" : "s"}`;

/** What was billed, in the order the chart stacks it (bottom first). */
const KINDS = [
  { key: "input", label: "fresh input", tokens: fresh },
  { key: "cache_write", label: "cache writes", tokens: (t) => t.cache_write },
  { key: "cache_read", label: "cache reads", tokens: (t) => t.cache_read },
  { key: "output", label: "output", tokens: (t) => t.output },
];

const PURPOSES = {
  work: ["Run steps", "The model working on the request: each step re-reads the whole conversation."],
  compaction: ["Compaction", "Summarizing a conversation that grew too long."],
  warm: ["Keeping the cache warm", "Refreshing the prompt cache while tools wait for your approval, so the next step reads it instead of rewriting it."],
  triage: ["Triage", "Deciding whether a request fits the fast model."],
  title: ["Change titles", "Naming a change before it is delivered."],
};
const USAGE_SOURCES = { chats: "Asked by you", objectives: "Routines" };

/** Shares summed by a key of each: a routine's agent, say. */
function mergeShares(items, keyOf) {
  const out = new Map();
  for (const s of items) {
    const k = keyOf(s);
    const m = out.get(k) || { name: k, input: 0, output: 0, cache_read: 0, cache_write: 0, calls: 0, unpriced: 0, cost: { input: 0, cache_read: 0, cache_write: 0, output: 0 } };
    for (const f of ["input", "output", "cache_read", "cache_write", "calls", "unpriced"]) m[f] += s[f];
    for (const f of Object.keys(m.cost)) m.cost[f] += s.cost[f];
    out.set(k, m);
  }
  return [...out.values()].sort((a, b) => costOf(b) - costOf(a) || total(b) - total(a));
}

async function renderUsage(main) {
  const one = usageUi.scope === "business" && app.realm;
  page(main, "Usage", `What running ${one ? app.realm : "your businesses"} costs: what its tasks and pipelines exchanged with the model, and who asked. Costs are estimates from public list prices; input read from the prompt cache costs a tenth of fresh input.`);
  const inner = $(".page-inner", main);
  const seg = (label, options, current, set) => h("div", { class: "seg", role: "radiogroup", "aria-label": label }, options.map(([value, text]) =>
    h("button", { type: "button", role: "radio", "aria-checked": String(current === value), class: current === value ? "on" : "",
      onclick: () => { set(value); renderMain(); } }, text)));
  const bar = h("div", { class: "usage-bar" });
  const body = h("div", null, h("div", { class: "working" }, h("span", { class: "orb" }), h("span", { class: "shimmer" }, "Adding up…")));
  inner.append(bar, body);
  let r;
  try { r = await api(`usage?days=${usageUi.days}${one ? "&realm=" + enc(app.realm) : ""}`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "usage") return;

  const priced = r.window.unpriced < r.window.calls;
  const cost = priced && usageUi.show === "cost";
  const weight = cost ? costOf : total;
  const fmt = cost ? usd : tok;
  bar.replaceChildren(
    priced ? seg("Show", [["cost", "Cost"], ["tokens", "Tokens"]], usageUi.show, (v) => (usageUi.show = v)) : null,
    seg("Period", [7, 30, 90].map((d) => [d, `${d} days`]), usageUi.days, (v) => (usageUi.days = v)),
    app.realm && seg("Scope", [["business", app.realm], ["all", "All businesses"]], usageUi.scope, (v) => (usageUi.scope = v)));

  const stat = (label, value, detail) => h("div", { class: "stat" },
    h("div", { class: "stat-label" }, label), h("div", { class: "stat-value" }, value), h("div", { class: "stat-detail" }, detail));
  const amount = (t) => (priced ? usd(costOf(t)) : tok(total(t)));
  const amountDetail = (t) => priced ? `${tok(total(t))} tokens · ${t.calls} call${t.calls === 1 ? "" : "s"}` : tokDetail(t);
  const stats = h("div", { class: "stats" },
    stat(`Last ${r.days} days`, amount(r.window), amountDetail(r.window)),
    stat("Today", amount(r.today), amountDetail(r.today)),
    stat("This month", amount(r.month), amountDetail(r.month)),
    stat("From cache", cachePct(r.window) + "%", `${tok(r.window.cache_read)} of ${tok(r.window.input)} input tokens`));

  // Pipelines' model calls are recorded apart, in the business's database.
  const pipelines = (r.pipelines || []).length > 0 && h("div", { class: "card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, "Pipelines")),
    r.pipelines.map((p) => h("div", { class: "list-row" },
      h("div", { class: "grow" }, h("div", null, p.pipeline), h("div", { class: "muted small" }, `${p.calls} model call${p.calls === 1 ? "" : "s"} · ${tok(p.input_tokens + p.output_tokens)} tokens${p.unpriced ? ` · ${p.unpriced} without a known price` : ""}`)),
      h("div", { class: "run-amount" }, h("span", { class: "strong" }, usd(p.cost_usd))))));
  if (!r.window.calls) {
    body.replaceChildren(stats, pipelines, h("div", { class: "card muted small pad" }, "No task called a model in this period."));
    return;
  }

  const part = (t, k) => (cost ? t.cost[k.key] : k.tokens(t));

  // Daily chart: what was billed, stacked.
  const max = Math.max(Number.MIN_VALUE, ...r.by_day.map(weight));
  const every = Math.ceil(r.by_day.length / 7);
  const cols = r.by_day.map((d, i) => {
    const date = new Date(d.date + "T12:00:00");
    const label = date.toLocaleDateString([], { day: "numeric", month: "short" });
    const segs = KINDS.slice().reverse().map((k) => {
      const el = h("span", { class: "seg-" + k.key });
      el.style.height = (100 * part(d, k)) / max + "%";
      return el;
    });
    const b = h("div", { class: "bar" }, segs);
    b.style.animationDelay = Math.min(i * 18, 600) + "ms";
    const lines = KINDS.map((k) => `${k.label}: ${tok(k.tokens(d))}${priced ? " · " + usd(d.cost[k.key]) : ""}`);
    return h("div", { class: "col" + (i === r.by_day.length - 1 ? " today" : ""),
        title: `${label}\n${priced ? usd(costOf(d)) + " · " : ""}${tok(total(d))} tokens · ${d.calls} calls\n${lines.join("\n")}` },
      b, h("div", { class: "col-label" }, i % every === 0 || i === r.by_day.length - 1 ? label : ""));
  });
  const chart = h("div", { class: "card chart-card" },
    h("div", { class: "chart-head" },
      h("span", { class: "strong" }, "Per day"),
      h("span", { class: "legend" }, KINDS.map((k) => h("span", null, h("i", { class: "seg-" + k.key }), k.label)))),
    h("div", { class: "chart" }, cols));

  const row = (i, name, value, pct, title) => {
    const fill = h("span");
    fill.style.width = Math.max(pct, 1) + "%";
    fill.style.animationDelay = i * 60 + "ms";
    return h("div", { class: "share", title },
      h("div", { class: "share-top" }, name, h("span", { class: "muted" }, `${value} · ${Math.round(pct)}%`)),
      h("div", { class: "share-bar" }, fill));
  };
  const all = weight(r.window) || 1;
  const card = (title, rows) => h("div", { class: "card share-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, title)), rows);
  const shareList = (title, items, name) => card(title, items.map((s, i) => row(i, name(s), fmt(weight(s)), (100 * weight(s)) / all,
    `${tokDetail(s)} · ${cachePct(s)}% cached${priced ? " · " + usd(costOf(s)) : ""}`)));

  const kindTotal = KINDS.reduce((a, k) => a + part(r.window, k), 0) || 1;
  const kinds = card(cost ? "Where the cost goes" : "Where the tokens go", KINDS.map((k, i) =>
    row(i, h("span", null, h("i", { class: "dot seg-" + k.key }), k.label), fmt(part(r.window, k)), (100 * part(r.window, k)) / kindTotal,
      `${tok(k.tokens(r.window))} tokens${priced ? " · " + usd(r.window.cost[k.key]) : ""}`)));
  const purposes = shareList("Why the model was called", r.by_purpose, (s) => {
    const [label, help] = PURPOSES[s.name] || [s.name, ""];
    return h("span", { title: help }, label);
  });

  const runDetail = (run) => [one ? null : run.realm, run.scheduled ? "routine" : null, `${run.calls} calls`,
    `${tok(Math.round(run.input / run.calls))} avg context`, `${tok(run.peak_input)} peak`, `${cachePct(run)}% cached`,
    run.compactions ? `${run.compactions} compaction${run.compactions === 1 ? "" : "s"}` : null].filter(Boolean).join(" · ");
  const top = h("div", { class: "card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, priced ? "Costliest tasks" : "Hungriest tasks")),
    r.top_runs.map((run) => h("button", { class: "list-row link", type: "button", onclick: () => go(`#/r/${enc(run.realm)}/run/${enc(run.id)}`) },
      h("div", { class: "grow" }, h("div", { class: "clamp" }, run.objective || "(no objective)"), h("div", { class: "muted small" }, runDetail(run))),
      h("div", { class: "run-amount" }, h("span", { class: "strong" }, fmt(weight(run))),
        h("span", { class: "muted small" }, cost ? tok(total(run)) + " tokens" : priced ? usd(costOf(run)) : "")))));

  const unpriced = r.window.unpriced
    ? h("div", { class: "card muted small pad" }, `${r.window.unpriced} of ${r.window.calls} calls used a model without a known price and are left out of costs. Add its prices, in USD per million tokens, to prices.json in Airth's home.`)
    : null;

  const agentOf = (s) => (!s.name ? "Asked by you" : (r.agents || {})[s.name] || "No agent");
  const byModel = shareList("By model", r.by_model, (s) => h("span", null, s.name === "unrecorded" ? "before tracking" : s.name));
  body.replaceChildren(stats, chart,
    one
      ? h("div", { class: "shares" },
        shareList("By agent", mergeShares(r.by_routine, agentOf), (s) => h("a", { href: `#/r/${enc(app.realm)}/agents` }, s.name)),
        shareList("By routine", r.by_routine, (s) => h("span", null, s.name || "Asked by you")))
      : h("div", { class: "shares" },
        shareList("By business", r.by_realm, (s) => h("a", { href: `#/r/${enc(s.name)}` }, s.name)), byModel),
    h("div", { class: "shares" }, kinds, one ? byModel : purposes),
    one && h("div", { class: "shares" }, purposes, r.by_source.length > 1 ? shareList("Asked by you, and routines", r.by_source, (s) => h("span", null, USAGE_SOURCES[s.name] || s.name)) : null),
    pipelines, top, unpriced);
}
