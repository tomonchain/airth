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
  const ns = northStar();
  if (r && ns) {
    if (ns.milestones.length) add("North Star", "Push the North Star forward now", h("span", { class: "i ns-star" }, "★"), () => startNorthStarRun("push"), { hint: ns.goal });
    add("North Star", ns.milestones.length ? "Rework the milestones with Airth" : "Brainstorm milestones with Airth", h("span", { class: "i ns-star" }, "★"), () => startNorthStarRun("brainstorm"));
    add("North Star", focusOn() ? "Turn focus mode off" : "Turn focus mode on", h("span", { class: "i ns-star" }, "★"), toggleFocus, { hint: "shadow what does not serve the North Star" });
  } else if (r) {
    add("North Star", "Set a North Star for this realm", h("span", { class: "i ns-star" }, "★"), () => { nsUi.open = true; go(`#/r/${r}/northstar`); });
  }
  add("Go to", "Inbox", icon("inbox"), () => go("#/inbox"));
  add("Go to", "Usage", icon("chart"), () => go("#/usage"));
  if (app.info && app.info.benchmarks) add("Go to", "Benchmarks", icon("flask"), () => go("#/bench"));
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
    add("Go to", "Routines", icon("clock"), () => go(`#/r/${r}/routines`));
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

const usageUi = { days: 30 };

/** 1234 → 1.2k, 4567890 → 4.57M: token counts at a glance. */
function tok(n) {
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 1 : 2) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(n >= 1e4 ? 0 : 1) + "k";
  return String(n);
}

const total = (t) => t.input + t.output;
const cachePct = (t) => (t.input ? Math.round((100 * t.cache_read) / t.input) : 0);
const tokDetail = (t) => `${tok(t.input)} in · ${tok(t.output)} out · ${t.calls} call${t.calls === 1 ? "" : "s"}`;

async function renderUsage(main) {
  page(main, "Usage", "Tokens your chats exchanged with the model, across every realm. Input includes what the provider served from its prompt cache, which costs far less than fresh input.");
  const inner = $(".page-inner", main);
  const period = h("div", { class: "seg", role: "radiogroup", "aria-label": "Period" }, [7, 30, 90].map((d) =>
    h("button", { type: "button", role: "radio", "aria-checked": String(usageUi.days === d), class: usageUi.days === d ? "on" : "",
      onclick: () => { usageUi.days = d; renderMain(); } }, `${d} days`)));
  const body = h("div", null, h("div", { class: "working" }, h("span", { class: "orb" }), h("span", { class: "shimmer" }, "Adding up…")));
  inner.append(h("div", { class: "usage-bar" }, period), body);
  let r;
  try { r = await api(`usage?days=${usageUi.days}`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "usage") return;

  const stat = (label, value, detail) => h("div", { class: "stat" },
    h("div", { class: "stat-label" }, label), h("div", { class: "stat-value" }, value), h("div", { class: "stat-detail" }, detail));
  const stats = h("div", { class: "stats" },
    stat(`Last ${r.days} days`, tok(total(r.window)), tokDetail(r.window)),
    stat("Today", tok(total(r.today)), tokDetail(r.today)),
    stat("This month", tok(total(r.month)), tokDetail(r.month)),
    stat("From cache", cachePct(r.window) + "%", `${tok(r.window.cache_read)} of ${tok(r.window.input)} input tokens`));

  if (!r.window.calls) {
    body.replaceChildren(stats, h("div", { class: "card muted small pad" }, "No model calls in this period."));
    return;
  }

  // Daily chart: fresh input, cached input and output, stacked.
  const max = Math.max(1, ...r.by_day.map((d) => total(d)));
  const every = Math.ceil(r.by_day.length / 7);
  const cols = r.by_day.map((d, i) => {
    const seg = (cls, n) => {
      const el = h("span", { class: "seg-" + cls });
      el.style.height = (100 * n) / max + "%";
      return el;
    };
    const date = new Date(d.date + "T12:00:00");
    const label = date.toLocaleDateString([], { day: "numeric", month: "short" });
    const bar = h("div", { class: "bar" }, seg("out", d.output), seg("cache", d.cache_read), seg("in", d.input - d.cache_read));
    bar.style.animationDelay = Math.min(i * 18, 600) + "ms";
    return h("div", { class: "col" + (i === r.by_day.length - 1 ? " today" : ""),
        title: `${label}\n${tok(total(d))} tokens\n${tok(d.input - d.cache_read)} fresh input · ${tok(d.cache_read)} cached · ${tok(d.output)} output\n${d.calls} calls` },
      bar, h("div", { class: "col-label" }, i % every === 0 || i === r.by_day.length - 1 ? label : ""));
  });
  const chart = h("div", { class: "card chart-card" },
    h("div", { class: "chart-head" },
      h("span", { class: "strong" }, "Per day"),
      h("span", { class: "legend" },
        h("span", null, h("i", { class: "seg-in" }), "fresh input"),
        h("span", null, h("i", { class: "seg-cache" }), "cached input"),
        h("span", null, h("i", { class: "seg-out" }), "output"))),
    h("div", { class: "chart" }, cols));

  const shareList = (title, items, linkTo) => h("div", { class: "card share-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, title)),
    items.map((s, i) => {
      const pct = (100 * total(s)) / total(r.window);
      const fill = h("span");
      fill.style.width = Math.max(pct, 1) + "%";
      fill.style.animationDelay = i * 60 + "ms";
      const nameEl = linkTo ? h("a", { href: linkTo(s) }, s.name) : h("span", null, s.name === "unrecorded" ? "before tracking" : s.name);
      return h("div", { class: "share", title: tokDetail(s) + ` · ${cachePct(s)}% cached` },
        h("div", { class: "share-top" }, nameEl, h("span", { class: "muted" }, `${tok(total(s))} · ${Math.round(pct)}%`)),
        h("div", { class: "share-bar" }, fill));
    }));

  const top = h("div", { class: "card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, "Hungriest chats")),
    r.top_runs.map((run) => h("button", { class: "list-row link", type: "button", onclick: () => go(`#/r/${enc(run.realm)}/run/${enc(run.id)}`) },
      h("div", { class: "grow" }, h("div", { class: "clamp" }, run.objective || "(no objective)"), h("div", { class: "muted small" }, `${run.realm} · ${tokDetail(run)} · ${cachePct(run)}% cached`)),
      h("span", { class: "strong" }, tok(total(run))))));

  body.replaceChildren(stats, chart,
    h("div", { class: "shares" },
      shareList("By realm", r.by_realm, (s) => `#/r/${enc(s.name)}`),
      shareList("By model", r.by_model)),
    top);
}

