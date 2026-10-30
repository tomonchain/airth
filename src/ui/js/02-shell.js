// ---------- sidebar ----------

let realmMenuOpen = false;

function renderSidebar() {
  renderRealmSwitch();
  renderNav();
  renderRunList();
  renderFoot();
  renderStatus();
  updateTitle();
}

// Past this many input tokens a chat is summarized (see AIRTH_COMPACT_AT): the context "full" mark.
const CONTEXT_LIMIT = 150000;

/** The thin footer: how full the open chat's context is, its tokens, and today's tokens. */
function renderStatus() {
  const el = $("#statusbar");
  if (!el) return;
  const sep = () => h("span", { class: "sb-sep", "aria-hidden": "true" }, "·");
  const tokens = (label, input, output, title) =>
    h("span", { class: "sb-item", title }, label, `${kfmt(input)} in`, sep(), `${kfmt(output)} out`);
  const items = [];
  const s = app.route.view === "run" && app.run ? app.run.state : null;
  if (s) {
    const last = s.usage && s.usage.length ? s.usage[s.usage.length - 1].input : 0;
    const pct = Math.min(100, Math.round((100 * last) / CONTEXT_LIMIT));
    const fill = h("span");
    fill.style.width = pct + "%";
    items.push(
      h("span", { class: "sb-item", title: `${last} input tokens in the last call; the chat is summarized past ${CONTEXT_LIMIT}` },
        "Context", h("span", { class: "sb-meter" + (pct >= 90 ? " hot" : pct >= 70 ? " warm" : ""), "aria-hidden": "true" }, fill), `${pct}%`),
      tokens("This chat ", s.input_tokens || 0, s.output_tokens || 0, `${s.input_tokens || 0} in / ${s.output_tokens || 0} out`));
  }
  const t = app.today;
  items.push(h("span", { class: "sb-spacer" }));
  if (t) items.push(tokens("Today ", t.input, t.output, `${t.input} in / ${t.output} out, all realms`));
  el.replaceChildren(...items);
}

async function refreshToday() {
  if (app.unauthorized) return;
  try { app.today = (await api("usage?days=1")).today; } catch { return; }
  renderStatus();
}

function renderNav() {
  const unread = app.info ? app.info.unread : 0;
  const v = app.route.view;
  const r = app.realm && enc(app.realm);
  $("#side-nav").replaceChildren(
    h("button", { class: "nav-link", type: "button", onclick: openPalette, title: "Search chats, pages, realms and actions" },
      icon("search"), h("span", { class: "grow" }, "Search"), h("kbd", { class: "nav-kbd" }, modKey + "K")),
    navLink(r ? `#/r/${r}/today` : null, "inbox", "Today", v === "today", unread > 0 && h("span", { class: "count" }, String(unread))),
    navLink(r ? `#/r/${r}/pipelines` : null, "clock", "Pipelines", v === "pipelines"),
    navLink(r ? `#/r/${r}` : null, "pencil", "Ask Airth", v === "new"),
    navLink(r ? `#/r/${r}/documents` : null, "folder", "Documents", v === "documents"),
    navLink(r ? `#/r/${r}/skills` : null, "spark", "Skills", v === "skills"),
    navLink("#/usage", "chart", "Costs", v === "usage"),
  );
}

/** Which date group a run falls in, ChatGPT style. */
function dateGroup(iso) {
  const d = new Date(iso);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  if (d >= today) return "Today";
  const days = (today - d) / 86400000;
  if (days <= 1) return "Yesterday";
  if (days <= 7) return "Previous 7 days";
  if (days <= 30) return "Previous 30 days";
  return "Older";
}

/** One discreet mark per run: only what deserves attention. */
function runMark(r) {
  if (r.needs_approval) return h("span", { class: "dot awaiting", title: "Needs your approval" });
  if (r.live) return h("span", { class: "run-progress", title: "Running" }, r.plan_total > 0 && `${r.plan_done}/${r.plan_total}`, h("span", { class: "dot running" }));
  if (r.status === "failed") return h("span", { class: "dot failed", title: "Failed" });
  if (r.status === "stopped" || r.status === "interrupted") return h("span", { class: "dot stopped", title: r.status });
  return changeBadge(r);
}

function renderRunList() {
  if (app.renaming) return; // a run's name is being edited in place
  const list = $("#run-list");
  list.replaceChildren();
  if (app.realm && !app.runs.length) list.append(h("div", { class: "run-empty" }, "No chats yet."));
  const all = [...app.runs].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at));
  const archivedCount = all.filter((r) => r.archived).length;
  const runs = all.filter((r) => app.showArchived ? r.archived : !r.archived);
  if (app.realm && app.runs.length && !runs.length) list.append(h("div", { class: "run-empty" }, app.showArchived ? "No archived chats." : "All chats are archived."));
  let group = null;
  for (const r of runs) {
    const g = dateGroup(r.updated_at);
    if (g !== group) { group = g; list.append(h("div", { class: "section-label" }, g)); }
    const active = app.route.view === "run" && app.route.id === r.id;
    const status = r.needs_approval ? "needs approval" : r.live ? "running" : r.status;
    list.append(
      h("button", { class: "run-item" + (active ? " active" : ""),
          type: "button", title: `${runName(r)}\n${status} · ${ago(r.updated_at)}\nDouble-click to rename`,
          onclick: () => go(`#/r/${enc(app.realm)}/run/${enc(r.id)}`),
          ondblclick: (e) => { e.preventDefault(); editInPlace(e.currentTarget, r.id, runName(r)); } },
        h("span", { class: "obj" }, runName(r)),
        !r.live && runActions(r),
        runMark(r)));
  }
  if (archivedCount || app.showArchived) {
    list.append(h("button", { class: "run-archive-toggle", type: "button",
      onclick: () => { app.showArchived = !app.showArchived; renderRunList(); } },
      app.showArchived ? "← Back to chats" : `Archived (${archivedCount})`));
  }
}

/** Hover buttons of a sidebar chat: archive (or restore) and delete (asks twice). */
function runActions(r) {
  const stop = (fn) => (e) => { e.preventDefault(); e.stopPropagation(); fn(e.currentTarget); };
  const del = h("span", { class: "run-act danger", role: "button", tabindex: "0", title: "Delete this chat for good",
    onclick: stop((el) => {
      if (el.dataset.sure) { deleteRun(r.id); return; }
      el.dataset.sure = "1";
      el.classList.add("sure");
      el.textContent = "Delete?";
      setTimeout(() => { if (el.isConnected) { delete el.dataset.sure; el.classList.remove("sure"); el.replaceChildren(icon("trash")); } }, 3000);
    }) }, icon("trash"));
  const arch = h("span", { class: "run-act", role: "button", tabindex: "0", title: r.archived ? "Restore this chat" : "Archive this chat",
    onclick: stop(() => archiveRun(r.id, !r.archived)) }, icon("archive"));
  for (const el of [arch, del]) el.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") el.click(); });
  return h("span", { class: "run-acts" }, arch, del);
}

async function archiveRun(id, archived) {
  try {
    await api(`${runPath(app.realm, id)}/archive`, { archived });
    const run = app.runs.find((r) => r.id === id);
    if (run) run.archived = archived;
    if (app.route.view === "run" && app.route.id === id && archived) go(`#/r/${enc(app.realm)}`);
    renderRunList();
    flash(archived ? "Chat archived" : "Chat restored");
  } catch (e) {
    flash(e.message, true);
  }
}

async function deleteRun(id) {
  try {
    await api(runPath(app.realm, id), undefined, "DELETE");
    app.runs = app.runs.filter((r) => r.id !== id);
    if (app.route.view === "run" && app.route.id === id) go(`#/r/${enc(app.realm)}`);
    renderRunList();
    flash("Chat deleted");
  } catch (e) {
    flash(e.message, true);
  }
}

/** A run's display name: the name the owner gave it, else the first line of its objective. */
function runName(r) {
  return (r.title || "").trim() || String(r.objective || "").split("\n")[0].trim() || "(untitled chat)";
}

async function renameRun(id, title) {
  try {
    const res = await api(`${runPath(app.realm, id)}`, { title }, "PATCH");
    const run = app.runs.find((r) => r.id === id);
    if (run) run.title = res.title;
    if (app.run && app.route.id === id) app.run.title = res.title;
    flash(res.title ? "Renamed" : "Name cleared");
  } catch (e) {
    flash(e.message, true);
  }
}

/** Turn a run's name into a text box, in the sidebar or the run header. Enter saves, Esc cancels. */
function editInPlace(el, id, current) {
  if (!el || app.renaming) return;
  app.renaming = id;
  const input = h("input", { type: "text", class: "title-input", value: current, "aria-label": "Chat name", maxlength: "120" });
  el.replaceWith(input);
  input.focus();
  input.select();
  let done = false;
  const finish = async (save) => {
    if (done) return;
    done = true;
    const value = input.value.trim();
    if (save && value !== current) await renameRun(id, value);
    app.renaming = null;
    renderRunList();
    renderRun();
    updateTitle();
  };
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { e.preventDefault(); finish(true); }
    else if (e.key === "Escape") { e.preventDefault(); finish(false); }
  });
  // Clicks in the box must not reach the run item (which would navigate).
  input.addEventListener("click", (e) => e.stopPropagation());
  input.addEventListener("blur", () => finish(true));
}

function renderFoot() {
  const foot = $("#sidebar-foot");
  const d = app.realmDetail;
  const name = (app.ownerName || "").trim();
  foot.replaceChildren(
    updateWidget(),
    d && !d.model && h("button", { class: "side-warn", type: "button", title: d.model_error || "", onclick: () => go(`#/r/${enc(app.realm)}/settings/models`) }, "⚠ No model configured"),
    app.info && !app.info.sandbox && h("div", { class: "side-warn", title: "Commands are not confined to the realm on this platform." }, "⚠ No command sandbox"),
    h("div", { class: "me" },
      h("button", { class: "me-btn" + (app.route.view === "owner" ? " active" : ""), type: "button", onclick: () => go("#/owner") },
        pictureMark("avatar", "owner/avatar", app.ownerAvatar, name),
        h("span", { class: "me-name" }, name || "Your profile")),
      themeToggle(),
      app.realm && h("button", { class: "icon-action" + (app.route.view === "settings" ? " active" : ""), type: "button", title: "Realm settings", "aria-label": "Realm settings",
        onclick: () => go(`#/r/${enc(app.realm)}/settings`) }, icon("gear"))));
}

function navLink(hash, iconName, label, active, extra) {
  return h("button", { class: "nav-link" + (active ? " active" : ""), type: "button", disabled: !hash, onclick: () => hash && go(hash) },
    icon(iconName), h("span", { class: "grow" }, label), extra);
}

function changeBadge(r) {
  const badge = (cls, ic, title, text) => h("span", { class: "badge " + cls, title, "aria-label": title }, icon(ic), text);
  if (r.change === "adopted") return badge("ok", "gitMerge", "Adopted");
  if (r.change === "discarded") return badge("", "x", "Discarded");
  if (r.conflicts) return badge("warn", "alert", "Conflicts with its base branch");
  if (r.pull_request) return badge("ok", "gitPull", `Pull request #${r.pull_request.number} open on GitHub`, `#${r.pull_request.number}`);
  if (r.checks_passed === true) return badge("ok", "check", "Checks passed: ready to adopt");
  if (r.checks_passed === false) return badge("err", "x", "Checks failed");
  return null;
}

function renderRealmSwitch() {
  const el = $("#realm-switch");
  el.replaceChildren();
  const elsewhere = app.realms.filter((r) => r.name !== app.realm && r.needs_approval > 0);
  const current = app.realms.find((r) => r.name === app.realm);
  const button = h("button", { class: "realm-button", type: "button", "aria-haspopup": "true", "aria-expanded": String(realmMenuOpen),
      title: current && current.description ? current.description : "",
      onclick: (e) => { e.stopPropagation(); realmMenuOpen = !realmMenuOpen; renderRealmSwitch(); } },
    pictureMark("realm-mark", `${realmPath(app.realm || "")}/image`, current && current.image, app.realm),
    h("span", { class: "realm-name" }, app.realm || "No realm"),
    elsewhere.length > 0 && h("span", { class: "needs-dot", title: "Approvals waiting in other realms" }),
    h("span", { class: "chev" }, icon("chevrons")));
  el.append(button);
  if (elsewhere.length)
    el.append(h("button", { class: "realm-alert", type: "button", onclick: () => go(`#/r/${enc(elsewhere[0].name)}`) },
      `${elsewhere.map((r) => r.name).join(", ")} waiting for approval →`));
  if (!realmMenuOpen) return;
  const close = () => { realmMenuOpen = false; };
  const d = app.realmDetail;
  const menu = h("div", { class: "realm-menu", role: "menu" }, h("div", { class: "menu-label" }, "Realms"));
  for (const r of app.realms) {
    menu.append(h("button", { class: "realm-option" + (r.name === app.realm ? " active" : "") + (r.archived ? " archived" : ""), type: "button", role: "menuitem",
        onclick: () => { close(); go(`#/r/${enc(r.name)}`); } },
      pictureMark("realm-mark small", `${realmPath(r.name)}/image`, r.image, r.name),
      h("span", { class: "grow" }, r.name, r.archived && h("span", { class: "muted" }, " · archived")),
      r.needs_approval > 0 ? h("span", { class: "needs" }, r.needs_approval) : r.live > 0 ? h("span", { class: "dot running" }) : r.name === app.realm ? h("span", { class: "check" }, "✓") : null));
  }
  menu.append(
    h("div", { class: "menu-sep" }),
    app.realm && h("button", { class: "realm-option", type: "button", role: "menuitem", onclick: () => { close(); go(`#/r/${enc(app.realm)}/settings`); } }, icon("gear"), h("span", { class: "grow" }, "Realm settings")),
    h("button", { class: "realm-option", type: "button", role: "menuitem", onclick: () => { close(); go("#/realms/new"); } }, icon("plus"), h("span", { class: "grow" }, "New realm")),
    h("div", { class: "menu-sep" }),
    h("div", { class: "menu-foot" },
      h("span", { class: "logo", "aria-hidden": "true" }),
      h("span", { class: "grow" }, "airth", app.info && h("span", { class: "muted" }, " v" + app.info.version)),
      d && h("span", { class: "muted mono", title: "Model for this realm" }, d.model || "no model")));
  el.append(menu);
}

document.addEventListener("click", () => {
  if (realmMenuOpen) { realmMenuOpen = false; renderRealmSwitch(); }
});

function updateTitle() {
  const n = app.realms.reduce((acc, r) => acc + (r.needs_approval || 0), 0);
  const run = app.route.view === "run" && app.run && runName({ title: app.run.title, objective: app.run.state.objective });
  const page = run ? (run.length > 48 ? run.slice(0, 47) + "…" : run) + " · Airth" : "Airth";
  document.title = (n ? `(${n}) ` : "") + page;
}

// ---------- main ----------

function renderMain() {
  const main = $("#main");
  ideStash();
  main.replaceChildren();
  runEls = null;
  if (app.unauthorized) {
    main.append(h("div", { class: "center-note" },
      h("h1", null, "Open Airth from your terminal"),
      h("p", null, "Run ", h("code", null, "airth ui"), " and use the link it prints. The link carries a private key that lets this page control the agent.")));
    return;
  }
  switch (app.route.view) {
    case "today": return renderToday(main);
    case "pipelines": return renderPipelines(main);
    case "new": return renderNew(main);
    case "run": return renderRunShell(main);
    case "settings": return renderSettings(main);
    case "owner": return renderOwner(main);
    case "documents": return renderDocuments(main);
    case "objectives": return renderObjectives(main);
    case "skills": return renderSkills(main);
    case "inbox": return renderInbox(main);
    case "usage": return renderUsage(main);
    case "new-realm": return renderNewRealm(main);
    case "welcome": return renderWelcome(main);
  }
}

const SpeechRec = window.SpeechRecognition || window.webkitSpeechRecognition;

/** Dictate into a text box: speech becomes text you can edit before sending. */
function micButton(ta) {
  const label = "Dictate";
  if (!SpeechRec) {
    return h("button", { class: "mic", type: "button", title: label, "aria-label": label, onclick: () => {
      ta.focus();
      flash(isMac ? "Press the 🎤 key (or Fn twice) to dictate with macOS." : "Use your system's dictation to speak.");
    } }, icon("mic"));
  }
  let rec = null;
  const btn = h("button", { class: "mic", type: "button", title: label, "aria-label": label, onclick: () => {
    if (rec) { rec.stop(); return; }
    rec = new SpeechRec();
    rec.lang = navigator.language || "fr-FR";
    rec.interimResults = true;
    rec.continuous = true;
    const base = ta.value.trim() ? ta.value.replace(/\s*$/, " ") : "";
    rec.onresult = (e) => {
      let text = "";
      for (const r of e.results) text += r[0].transcript;
      ta.value = base + text;
      ta.dispatchEvent(new Event("input"));
    };
    rec.onerror = (e) => { if (e.error !== "aborted" && e.error !== "no-speech") flash("Dictation: " + e.error, true); };
    rec.onend = () => { rec = null; btn.classList.remove("on"); ta.focus(); };
    try { rec.start(); btn.classList.add("on"); } catch (e) { rec = null; flash("Dictation unavailable: " + e.message, true); }
  } }, icon("mic"));
  return btn;
}

// ---------- model picker ----------

const modelUi = { realm: null, options: null, loading: null, open: null };

function loadModels(force) {
  if (!app.realm) return Promise.resolve(null);
  if (!force && modelUi.realm === app.realm && modelUi.options) return Promise.resolve(modelUi.options);
  if (!force && modelUi.loading && modelUi.realm === app.realm) return modelUi.loading;
  modelUi.realm = app.realm;
  modelUi.loading = api(`${realmPath(app.realm)}/models${force ? "?refresh=1" : ""}`)
    .then((o) => (modelUi.options = o))
    .catch((e) => { flash(e.message, true); return null; })
    .finally(() => { modelUi.loading = null; });
  return modelUi.loading;
}

const shortModel = (choice) => {
  if (!choice || choice === "auto") return "Auto";
  const m = choice.split("/").slice(1).join("/");
  return m.length > 24 ? m.slice(0, 23) + "…" : m;
};

document.addEventListener("mousedown", (e) => {
  if (modelUi.open && !modelUi.open.contains(e.target)) { modelUi.open.remove(); modelUi.open = null; }
});

/** "Auto ▾": which model answers this request. get/set read and store the choice.
 *  `auto` renames the first entry ({label, sub}), e.g. "None" for an optional model. */
/** The realm's repositories a new chat can work in ("" is the realm's folder, when it is not one). */
function repoChoices() {
  const repos = (app.realmDetail && app.realmDetail.repos) || [];
  const choices = repos.map((r) => ({ value: r.name, label: r.name, sub: r.main ? "this realm's folder" : tildePath(r.path) }));
  if (!repos.some((r) => r.main)) choices.unshift({ value: "", label: "Folder", sub: "the realm's files, no repository" });
  return choices;
}

/** The repository a new chat in this realm works in: the last one picked, else the first. */
function chosenRepo() {
  const choices = repoChoices();
  let saved = null;
  try { saved = localStorage.getItem("airth-repo:" + app.realm); } catch {}
  return (choices.find((c) => c.value === saved) || choices[0] || { value: "" }).value;
}

/** Which repository a new chat works in, next to the model pill (only with a choice to make). */
function repoPicker({ onChange }) {
  const choices = repoChoices();
  if (choices.length < 2) return null;
  const label = h("span", { class: "mp-label" });
  const btn = h("button", { class: "model-pill", type: "button", title: "Which repository the chat works in", "aria-haspopup": "true" }, icon("code"), label, h("span", { class: "mp-chev" }, "▾"));
  const refresh = () => { const c = choices.find((x) => x.value === chosenRepo()); label.textContent = c ? c.label : "Folder"; };
  const wrap = h("span", { class: "mp" }, btn);
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    if (modelUi.open) { modelUi.open.remove(); modelUi.open = null; return; }
    const menu = h("div", { class: "mp-menu", role: "menu" },
      h("div", { class: "mp-list" }, choices.map((c) => h("button", { type: "button", class: "mp-item" + (c.value === chosenRepo() ? " on" : ""), role: "menuitem",
        onclick: () => {
          try { localStorage.setItem("airth-repo:" + app.realm, c.value); } catch { /* per window only */ }
          refresh(); menu.remove(); modelUi.open = null;
          if (onChange) onChange(c.value);
        } }, h("span", { class: "grow" }, c.label, h("span", { class: "mp-sub" }, c.sub)), c.value === chosenRepo() && icon("check")))));
    wrap.append(menu);
    modelUi.open = menu;
  });
  refresh();
  return wrap;
}

function modelPicker({ get, set, auto }) {
  const label = h("span", { class: "mp-label" });
  const btn = h("button", { class: "model-pill", type: "button", title: "Which model answers", "aria-haspopup": "true" }, icon("brain"), label, h("span", { class: "mp-chev" }, "▾"));
  const refresh = () => {
    label.textContent = get() === "auto" && auto ? auto.label : shortModel(get());
    btn.title = get() === "auto" ? "Which model answers" : get();
    btn.classList.toggle("chosen", get() !== "auto");
  };
  const wrap = h("span", { class: "mp" }, btn);
  btn.addEventListener("click", async (e) => {
    e.stopPropagation();
    if (modelUi.open) { modelUi.open.remove(); modelUi.open = null; return; }
    const menu = h("div", { class: "mp-menu", role: "menu" }, working("Loading models…"));
    wrap.append(menu);
    modelUi.open = menu;
    const opts = await loadModels(false);
    if (modelUi.open !== menu) return;
    const q = h("input", { type: "search", class: "mp-search", placeholder: "Search, or type provider/model", "aria-label": "Search models" });
    const list = h("div", { class: "mp-list" });
    const pick = (choice) => { set(choice); refresh(); menu.remove(); modelUi.open = null; };
    const item = (choice, text, sub, on) => h("button", { type: "button", class: "mp-item" + (on ? " on" : ""), role: "menuitem", onclick: () => pick(choice) },
      h("span", { class: "grow" }, text, sub && h("span", { class: "mp-sub" }, sub)), on && icon("check"));
    const draw = () => {
      const s = q.value.trim().toLowerCase();
      const cur = get();
      const groups = [];
      groups.push(auto
        ? item("auto", auto.label, auto.sub, cur === "auto")
        : item("auto", "Auto", opts && opts.default ? `realm default · ${opts.default}` : "the realm's model and routing", cur === "auto"));
      for (const p of (opts && opts.providers) || []) {
        if (!p.configured) {
          if (!s || p.name.toLowerCase().includes(s)) groups.push(h("div", { class: "mp-group off" }, p.name,
            h("a", { href: `#/r/${enc(app.realm)}/settings`, onclick: () => { menu.remove(); modelUi.open = null; } }, p.key ? `add ${p.key}` : "set up")));
          continue;
        }
        let models = p.models.filter((m) => !s || `${p.id}/${m}`.toLowerCase().includes(s) || p.name.toLowerCase().includes(s));
        if (p.default_model && !models.includes(p.default_model) && (!s || p.default_model.includes(s))) models.unshift(p.default_model);
        if (!models.length) continue;
        const shown = s ? models.slice(0, 30) : models.slice(-8).reverse();
        groups.push(h("div", { class: "mp-group" }, p.name, p.error && h("span", { class: "mp-sub warn", title: p.error }, "list unavailable")),
          shown.map((m) => item(`${p.id}/${m}`, m, m === p.default_model ? "default" : null, cur === `${p.id}/${m}`)));
      }
      if (s.includes("/") && !groups.some((g) => g.textContent === s)) groups.push(item(q.value.trim(), `Use ${q.value.trim()}`, "custom", false));
      list.replaceChildren(...groups.flat());
    };
    q.addEventListener("input", draw);
    q.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape") { menu.remove(); modelUi.open = null; }
      if (ev.key === "Enter" && q.value.includes("/")) pick(q.value.trim());
    });
    menu.replaceChildren(q, list,
      h("button", { type: "button", class: "mp-refresh", onclick: async () => { list.replaceChildren(working("Refreshing…")); await loadModels(true); if (modelUi.open === menu) { menu.remove(); modelUi.open = null; btn.click(); } } }, icon("refresh"), "Refresh lists"));
    draw();
    setTimeout(() => q.focus(), 0);
  });
  refresh();
  return { el: wrap, refresh };
}

const modelChoices = cappedStore("airth-models", 300);
// One-time cleanup of the per-key format used before the capped store.
try { for (const k of Object.keys(localStorage)) if (k.startsWith("airth-model:")) localStorage.removeItem(k); } catch {}
function storedModel(key) { return modelChoices.get(key) || null; }
function storeModel(key, v) { modelChoices.set(key, v); }

/** A host name for display; never throws on a bad address (registry entries are not vetted). */
const hostOf = (u) => { try { return new URL(u).host; } catch { return String(u || ""); } };

/** A small map kept in this browser, capped: the least recently written entries go first. */
function cappedStore(name, cap) {
  let map = {};
  try { map = JSON.parse(localStorage.getItem(name) || "{}") || {}; } catch { map = {}; }
  return {
    get: (k) => map[k],
    set(k, v) {
      delete map[k];
      if (v != null && v !== "") map[k] = v;
      const keys = Object.keys(map);
      for (const old of keys.slice(0, Math.max(0, keys.length - cap))) delete map[old];
      try { localStorage.setItem(name, JSON.stringify(map)); } catch {}
    },
  };
}

/** Unsent text, per message box (a run, or a realm's new-run screen), kept in this browser. */
const drafts = cappedStore("airth-drafts", 300);
function saveDraft(key, text) {
  if (!key) return;
  drafts.set(key, text.trim() ? text : "");
}

function composer({ placeholder, buttonLabel, onSubmit, rows = 1, disabled = false, note, draftKey, model, repo }) {
  const ta = h("textarea", { rows, placeholder, disabled, "aria-label": placeholder });
  if (draftKey && drafts.get(draftKey)) {
    ta.value = drafts.get(draftKey);
    // Fit the box to the restored text once it is on screen.
    setTimeout(() => { if (ta.scrollHeight) { ta.style.height = "auto"; ta.style.height = Math.min(ta.scrollHeight, 200) + "px"; } }, 0);
  }
  const enterSends = () => window.airthPrefs && window.airthPrefs.get("send") === "enter";
  // Blocked: you may write (a draft) but not send yet, e.g. while the run works.
  let blocked = false;
  const sync = () => { btn.disabled = ta.disabled || blocked || !ta.value.trim(); };
  const btn = h("button", { class: "send", type: "button", disabled, "aria-label": buttonLabel, title: `${buttonLabel} (${enterSends() ? "↵" : modKey + "↵"})` }, icon("send"));
  const errorEl = h("div", { class: "error-banner", hidden: !app.error }, app.error || "");
  const submit = async () => {
    const text = ta.value.trim();
    if (!text || btn.disabled) return;
    btn.disabled = true;
    try {
      await onSubmit(text);
      ta.value = "";
      saveDraft(draftKey, "");
      app.error = null;
      errorEl.hidden = true;
    } catch (e) {
      app.error = e.message;
      errorEl.textContent = e.message;
      errorEl.hidden = false;
    } finally {
      sync();
    }
  };
  ta.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || e.isComposing) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod || (enterSends() && !e.shiftKey && !e.altKey)) { e.preventDefault(); submit(); }
  });
  ta.addEventListener("input", () => {
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, 200) + "px";
    sync();
    saveDraft(draftKey, ta.value);
  });
  btn.disabled = disabled || !ta.value.trim();
  btn.addEventListener("click", submit);
  const picker = model ? modelPicker(model) : null;
  const repoEl = repo ? repoPicker(repo) : null;
  const el = h("div", { class: "composer" }, errorEl, h("div", { class: "composer-box" }, ta, repoEl, picker && picker.el, micButton(ta), btn), note && h("div", { class: "note" }, note));
  return { el, ta, btn, errorEl, picker, block: (b) => { blocked = b; sync(); } };
}

function page(main, title, sub, ...children) {
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner" },
    h("h1", null, title), sub && h("p", { class: "sub" }, sub), ...children)));
}

