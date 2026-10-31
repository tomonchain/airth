// ---------- notifications ----------

const KIND_MARK = { completed: "✓", failed: "✗", stopped: "■", approval: "?", review: "⧗" };

// ---------- new realm ----------

function renderNewRealm(main) {
  const first = app.realms.length === 0;
  const name = h("input", { type: "text", placeholder: "personal, acme, freelance…", autocomplete: "off", spellcheck: "false" });
  const desc = h("input", { type: "text", placeholder: "e.g. My job at Acme Corp" });
  const root = h("input", { type: "text", placeholder: "Leave empty for a private folder, or e.g. ~/code/project", spellcheck: "false" });
  const iso = h("select", null,
    h("option", { value: "" }, "Automatic (a worktree per chat for git repositories)"),
    h("option", { value: "none" }, "None: work directly in the folder"),
    h("option", { value: "worktree" }, "Worktree per chat"));
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

// ---------- welcome guide ----------

/** The first launch: what Airth is, then the few things it needs, one at a time. */
const WELCOME_STEPS = ["hello", "you", "model", "realm", "tips"];
const welcomeUi = { step: 0, setup: null, realm: null };

async function renderWelcome(main) {
  if (!welcomeUi.setup) {
    try { welcomeUi.setup = await api("setup"); } catch { welcomeUi.setup = { realms: app.realms.length, model_ready: false }; }
    if (app.route.view !== "welcome") return;
  }
  const s = welcomeUi.setup;
  const step = WELCOME_STEPS[welcomeUi.step] || "hello";
  const next = () => { welcomeUi.step = Math.min(welcomeUi.step + 1, WELCOME_STEPS.length - 1); renderMain(); };
  const back = () => { welcomeUi.step = Math.max(welcomeUi.step - 1, 0); renderMain(); };
  const err = h("div", { class: "error-banner", hidden: true });
  const fail = (e) => { err.textContent = e.message; err.hidden = false; };
  const busy = async (btn, work) => {
    btn.disabled = true;
    try { await work(); } catch (e) { fail(e); } finally { btn.disabled = false; }
  };
  const actions = (...items) => h("div", { class: "welcome-actions" },
    welcomeUi.step > 0 ? h("button", { class: "btn ghost", type: "button", onclick: back }, "Back") : h("span"),
    h("div", { class: "row-start" }, ...items));
  const skip = (label) => h("button", { class: "btn ghost", type: "button", onclick: next }, label || "Skip");

  let body;
  if (step === "hello") {
    const point = (ic, title, text) => h("div", { class: "welcome-point" }, h("span", { class: "welcome-icon" }, icon(ic)), h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, text)));
    body = [
      h("div", { class: "logo welcome-logo", "aria-hidden": "true" }),
      h("h1", null, "Welcome to Airth"),
      h("p", { class: "lead" }, "Your personal agent. It works on your Mac with your files, apps and services, and asks you before anything that matters."),
      h("div", { class: "welcome-points" },
        point("folder", "Realms", "Each area of your life (work, a project, home) gets its own files, memory, keys and rules. Nothing crosses between them."),
        point("send", "Chats", "Ask for something; Airth plans it, does it, and shows you each step. You approve what it may not do on its own.")),
      h("p", { class: "muted small" }, "Setting up takes about two minutes."),
      actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Get started")),
    ];
  } else if (step === "you") {
    const name = h("input", { type: "text", placeholder: "Your first name", value: app.ownerName || "" });
    const about = h("textarea", { rows: 3, placeholder: "e.g. Freelance designer in Lyon. I sell kits online on the side." });
    const go_on = h("button", { class: "btn primary", type: "button" }, "Continue");
    go_on.onclick = () => busy(go_on, async () => {
      if (name.value.trim() || about.value.trim()) {
        const owner = await api("owner");
        const keep = Object.fromEntries(OWNER_LISTS.map(([k]) => [k, owner[k]]));
        await api("owner", { ...keep, name: name.value.trim() || owner.name, about: about.value.trim() || owner.about }, "PUT");
        app.ownerName = name.value.trim() || owner.name;
        renderFoot();
      }
      next();
    });
    api("owner").then((o) => { if (!name.value) name.value = o.name || ""; if (!about.value) about.value = o.about || ""; }).catch(() => {});
    body = [
      h("h1", null, "A little about you"),
      h("p", { class: "lead" }, "Airth uses this to write and decide the way you would. It stays on this Mac; you choose which realms see it."),
      h("label", { class: "welcome-label" }, "Name", name),
      h("label", { class: "welcome-label" }, "What you do", about),
      err,
      actions(skip(), go_on),
    ];
    setTimeout(() => name.focus());
  } else if (step === "model") {
    if (s.model_ready) {
      body = [
        h("h1", null, "Airth can think"),
        h("p", { class: "lead" }, h("span", { class: "ok" }, "✓ "), "A model is set up. You can change it, or add a cheaper fast model, in each realm's settings under Models."),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else {
      const key = h("input", { type: "password", placeholder: "sk-ant-…", autocomplete: "off", spellcheck: "false" });
      const save = h("button", { class: "btn primary", type: "button" }, "Check and save");
      save.onclick = () => busy(save, async () => {
        welcomeUi.setup = await api("setup/key", { key: key.value });
        flash("Key saved");
        next();
      });
      key.addEventListener("keydown", (e) => { if (e.key === "Enter") save.click(); });
      body = [
        h("h1", null, "Connect a model"),
        h("p", { class: "lead" }, "Airth thinks with Claude, through your own Anthropic account: you pay Anthropic directly for what you use."),
        h("ol", { class: "welcome-steps" },
          h("li", null, "Open ", h("a", { href: "https://console.anthropic.com/settings/keys", target: "_blank", rel: "noopener" }, "console.anthropic.com/settings/keys"), " and sign in."),
          h("li", null, "Create a key, copy it, and paste it here.")),
        h("label", { class: "welcome-label" }, "Anthropic API key", key),
        h("p", { class: "muted small" }, "Airth checks the key, then keeps it on this Mac only (in ~/.airth, readable by you alone) for all your realms. The model never sees it. Prefer OpenAI, Grok, DeepSeek, Mistral, OpenRouter or a local Ollama model? Skip this and pick one in a realm's settings."),
        err,
        actions(skip("Later"), save),
      ];
      setTimeout(() => key.focus());
    }
  } else if (step === "realm") {
    if (welcomeUi.realm || s.realms > 0) {
      const n = welcomeUi.realm ? 1 : s.realms;
      body = [
        h("h1", null, welcomeUi.realm ? `“${welcomeUi.realm}” is ready` : "Your realms are ready"),
        h("p", { class: "lead" }, welcomeUi.realm ? "Your first realm exists, with a private folder of its own." : `You already have ${n} realm${n > 1 ? "s" : ""}. Add more anytime from the realm menu, top left.`),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else {
      const name = h("input", { type: "text", value: "personal", autocomplete: "off", spellcheck: "false" });
      name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
      const desc = h("input", { type: "text", placeholder: "e.g. My personal life: admin, health, side projects" });
      const create = h("button", { class: "btn primary", type: "button" }, "Create realm");
      create.onclick = () => busy(create, async () => {
        const r = await api("realms", { name: name.value.trim(), description: desc.value.trim() });
        welcomeUi.realm = r.name;
        await refreshRealms();
        renderSidebar();
        next();
      });
      body = [
        h("h1", null, "Your first realm"),
        h("p", { class: "lead" }, "A realm is one area of your life. Start with one; add others (work, a client, a project) whenever you like."),
        h("label", { class: "welcome-label" }, "Name", name, h("span", { class: "muted small" }, "Lowercase letters, digits and dashes.")),
        h("label", { class: "welcome-label" }, "What it is for", desc, h("span", { class: "muted small" }, "Airth reads this in every chat of the realm.")),
        err,
        actions(create),
      ];
      setTimeout(() => desc.focus());
    }
  } else {
    const tip = (ic, title, text) => h("div", { class: "welcome-point" }, h("span", { class: "welcome-icon" }, icon(ic)), h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, text)));
    const mac = document.documentElement.dataset.shell === "mac";
    const done = h("button", { class: "btn primary", type: "button" }, "Start using Airth");
    done.onclick = () => busy(done, async () => {
      try { localStorage.setItem("airth-welcomed", "1"); } catch {}
      const realm = welcomeUi.realm || preferredRealm();
      Object.assign(welcomeUi, { step: 0, setup: null, realm: null });
      if (!realm) return go("#/realms/new");
      go(`#/r/${enc(realm)}`);
    });
    body = [
      h("h1", null, "You're set"),
      h("p", { class: "lead" }, "A few things worth knowing:"),
      h("div", { class: "welcome-points" },
        tip("pencil", "New chat", "Ask for anything in plain words: “sort my downloads”, “draft a reply to Marc”, “plan my week”."),
        tip("compass", "Steering", "What waits for your decision, what needs attention, and your strategy: the line every agent follows."),
        tip("refresh", "Routines", "The business's recurring work, each with the agent that does it and when."),
        tip("world", "Skills & connectors", "Connect Notion, Linear, Stripe… or install skills, per realm."),
        tip("search", modKey + "K", "Find a chat, a page or an action from anywhere in the app."),
        mac && tip("window", "⌥⇧A", "Shows or hides Airth from any app. Closing the window keeps Airth in the menu bar.")),
      err,
      actions(done),
    ];
  }

  main.append(h("div", { class: "page welcome" },
    h("div", { class: "welcome-inner" },
      h("div", { class: "welcome-dots", "aria-label": `Step ${welcomeUi.step + 1} of ${WELCOME_STEPS.length}` },
        WELCOME_STEPS.map((_, i) => h("span", { class: i === welcomeUi.step ? "on" : i < welcomeUi.step ? "done" : "" }))),
      h("div", { class: "welcome-card" }, ...body),
      welcomeUi.step < WELCOME_STEPS.length - 1 && h("button", { class: "welcome-skip", type: "button", onclick: () => {
        try { localStorage.setItem("airth-welcomed", "1"); } catch {}
        Object.assign(welcomeUi, { step: 0, setup: null, realm: null });
        const r = preferredRealm();
        go(r ? `#/r/${enc(r)}` : "#/realms/new");
      } }, "Skip the guide"))));
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
    if (app.pollFailures) { app.pollFailures = 0; flash("Back in touch with Airth"); }
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
    // A run already on screen stays there through a short outage (a restart,
    // an update): say it once, then retry, more slowly each time.
    app.pollFailures = (app.pollFailures || 0) + 1;
    if (app.run && runEls) {
      if (app.pollFailures === 1) flash("Lost touch with Airth; retrying…", true);
      next = Math.min(1000 * 2 ** app.pollFailures, 15000);
    } else if (runEls) {
      runEls.transcript.replaceChildren(h("div", { class: "end failed" }, e.message));
    }
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

function updateKey(info) {
  const u = info && info.update;
  return u ? [u.phase, u.step, u.available && u.available.commit, u.error].join("|") : "";
}

function updateWidget() {
  const u = app.info && app.info.update;
  if (app.restarting) return h("div", { class: "update restarting" }, h("span", { class: "spinner" }), "Restarting Airth…");
  if (!u || !u.available) return null;
  const a = u.available;
  const what = `${a.count} change${a.count === 1 ? "" : "s"}`;
  const changes = h("ul", { class: "update-changes" }, a.changes.slice(0, 8).map((c) => h("li", null, c)));
  if (u.phase === "ready")
    return h("div", { class: "update ready" },
      h("div", { class: "strong" }, "New update available"),
      h("details", null, h("summary", { class: "muted small" }, `${a.short} · ${what} · ${a.release ? "signature verified" : "tests passed"}`), changes),
      h("button", { class: "btn primary", type: "button", onclick: restartToUpdate }, "Restart to update"));
  if (u.phase === "preparing" || u.phase === "checking")
    return h("div", { class: "update" }, h("span", { class: "spinner" }),
      h("div", null, h("div", null, "Preparing update ", h("code", null, a.short)), h("div", { class: "muted small" }, (u.step || "starting") + "…")));
  if (u.phase === "failed")
    return h("div", { class: "update failed" },
      h("div", { class: "strong" }, `Update ${a.short} not installed`),
      h("details", null, h("summary", { class: "muted small" }, a.release ? "It could not be downloaded or verified" : "Its build or tests failed"), h("pre", { class: "out small" }, u.error || "")),
      h("button", { class: "btn", type: "button", onclick: () => api("update/check", {}).then(refreshInfo) }, "Try again"));
  return null;
}

async function restartToUpdate() {
  const live = app.runs.filter((r) => r.live).length;
  if (live && !app.confirmRestart) {
    app.confirmRestart = true;
    flash(`${live} chat${live === 1 ? " is" : "s are"} in progress: they will stop (and can be resumed). Click again to restart.`);
    return;
  }
  app.confirmRestart = false;
  const before = app.info.version;
  try { await api("update/restart", {}); } catch (e) { return flash(e.message, true); }
  app.restarting = true;
  renderSidebar();
  for (let i = 0; i < 300; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    try {
      const info = await api("info");
      if (info.version !== before) { location.reload(); return; }
    } catch {}
  }
  app.restarting = false;
  flash("Airth did not come back; check the terminal or `airth service status`.", true);
  renderSidebar();
}

async function refreshInfo() {
  try {
    const before = app.info && app.info.unread;
    const beforeUpdate = updateKey(app.info);
    app.info = await api("info");
    if (app.info.unread !== before || updateKey(app.info) !== beforeUpdate) renderSidebar();
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
  $("#side-close").append(icon("sidebar"));
  $("#side-open").append(icon("sidebar"));
  $("#side-close").addEventListener("click", toggleSidebar);
  $("#side-open").addEventListener("click", toggleSidebar);
  if (sidebarHidden()) $("#app").classList.add("side-hidden");
  window.addEventListener("hashchange", onRoute);
  if (app.unauthorized) return renderMain();
  try {
    app.info = await api("info");
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
  }
  try { const o = await api("owner"); app.ownerName = o.name; app.ownerAvatar = o.avatar; } catch {}
  await refreshRealms();
  await onRoute();
  refreshToday();
  setInterval(() => { if (!document.hidden) refreshToday(); }, 30000);
  // Background refreshes pause while the window is hidden, and catch up when it shows.
  setInterval(() => { if (!document.hidden) { refreshRealms(); refreshRuns(); } }, 2000);
  setInterval(() => { if (!document.hidden) refreshInfo(); }, 5000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) { refreshRealms(); refreshRuns(); refreshInfo(); } });
}

boot();
