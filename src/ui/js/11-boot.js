// ---------- notifications ----------


// ---------- new realm ----------

function renderNewRealm(main) {
  const first = app.realms.length === 0;
  const name = h("input", { type: "text", placeholder: "hedgen, my-shop, studio…", autocomplete: "off", spellcheck: "false" });
  const desc = h("input", { type: "text", placeholder: "e.g. My job at Acme Corp" });
  const root = h("input", { type: "text", placeholder: "Leave empty for a private folder, or e.g. ~/code/project", spellcheck: "false" });
  const iso = h("select", null,
    h("option", { value: "" }, "Automatic (a worktree per task for git repositories)"),
    h("option", { value: "none" }, "None: work directly in the folder"),
    h("option", { value: "worktree" }, "Worktree per task"));
  const err = h("div", { class: "error-banner", hidden: true });
  const create = async () => {
    try {
      const r = await api("realms", { name: name.value.trim(), description: desc.value, root: root.value, isolation: iso.value || null });
      await refreshRealms();
      go(`#/r/${enc(r.name)}`);
    } catch (e) { err.textContent = e.message; err.hidden = false; }
  };
  name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
  page(main, first ? "Welcome to Airth" : "New business",
    first
      ? "Each business gets its own space: its files, code, agents, connectors and rules. Nothing crosses between businesses. Create your first one."
      : "One business: its own files, code, agents, connectors and rules, isolated from the others.",
    h("div", { class: "card" },
      field("Name", name, "Lowercase letters, digits and dashes."),
      field("Description", desc, "Airth sees this, so it knows what the business is for."),
      field("Folder", root, "The directory Airth works in. Other businesses can never read it."),
      field("Isolation", iso)),
    err,
    h("div", { class: "row-end spaced" }, h("button", { class: "btn primary", type: "button", onclick: create }, "Create business")));
  name.focus();
}

// ---------- onboarding ----------

/** The first launch, one step at a time: what Airth is, the owner, a model,
 *  then a business with its strategy, a first agent and its first routine. */
const WELCOME_STEPS = ["hello", "you", "model", "business", "strategy", "agent", "routine", "done"];
const WELCOME_TITLES = { hello: "Welcome", you: "You", model: "Model", business: "Business", strategy: "Strategy", agent: "Agent", routine: "Routine", done: "Done" };
const welcomeUi = { step: 0, setup: null, realm: null, agent: null, template: null, routine: null };

/** When a routine runs, in words the owner picks from. */
const WHEN_CHOICES = [["mon 08:00", "Every Monday at 08:00"], ["weekdays 09:00", "Every weekday at 09:00"], ["daily 07:00", "Every day at 07:00"], ["fri 17:00", "Every Friday at 17:00"], ["", "Only when I start it"]];

async function renderWelcome(main) {
  if (!welcomeUi.setup) {
    try { welcomeUi.setup = await api("setup"); } catch { welcomeUi.setup = { realms: app.realms.length, model_ready: false }; }
    if (app.route.view !== "welcome") return;
  }
  const s = welcomeUi.setup;
  const step = WELCOME_STEPS[welcomeUi.step] || "hello";
  const next = () => { welcomeUi.step = Math.min(welcomeUi.step + 1, WELCOME_STEPS.length - 1); renderMain(); };
  const back = () => { welcomeUi.step = Math.max(welcomeUi.step - 1, 0); renderMain(); };
  const skipTo = (name) => { welcomeUi.step = WELCOME_STEPS.indexOf(name); renderMain(); };
  const err = h("div", { class: "error-banner", hidden: true });
  const fail = (e) => { err.textContent = e.message; err.hidden = false; };
  const busy = async (btn, work) => {
    btn.disabled = true;
    try { await work(); } catch (e) { fail(e); } finally { btn.disabled = false; }
  };
  const actions = (...items) => h("div", { class: "welcome-actions" },
    welcomeUi.step > 0 ? h("button", { class: "btn ghost", type: "button", onclick: back }, "Back") : h("span"),
    h("div", { class: "row-start" }, ...items));
  const skip = (label, to) => h("button", { class: "btn ghost", type: "button", onclick: to ? () => skipTo(to) : next }, label || "Skip");
  const primary = (label) => h("button", { class: "btn primary", type: "button" }, label);
  const realm = welcomeUi.realm;
  const finish = () => {
    try { localStorage.setItem("airth-welcomed", "1"); } catch {}
    const r = welcomeUi.realm || preferredRealm();
    Object.assign(welcomeUi, { step: 0, setup: null, realm: null, agent: null, template: null, routine: null });
    go(r ? `#/r/${enc(r)}/steering` : "#/realms/new");
  };

  let body;
  if (step === "hello") {
    const point = (ic, title, text) => h("div", { class: "welcome-point" }, h("span", { class: "welcome-icon" }, icon(ic)), h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, text)));
    body = [
      h("h1", null, "Run your business with a team of agents"),
      h("p", { class: "lead" }, "Airth works on your Mac. Agents with a job do the work; you set the direction and decide what matters."),
      h("div", { class: "welcome-points" },
        point("compass", "You steer", "A strategy every agent follows, and the decisions that wait for you."),
        point("brain", "Agents work", "Each has a job, its own tools and limits, and works on a schedule."),
        point("chart", "You see the cost", "What each agent did, and what running the business costs.")),
      h("p", { class: "muted small" }, "About five minutes. Every step can be changed later."),
      actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Get started")),
    ];
  } else if (step === "you") {
    const name = h("input", { type: "text", placeholder: "Your first name", value: app.ownerName || "" });
    const about = h("textarea", { rows: 2, placeholder: "e.g. Founder of a small research media. I write in French." });
    const go_on = primary("Continue");
    go_on.onclick = () => busy(go_on, async () => {
      if (name.value.trim() || about.value.trim()) {
        const owner = await api("owner");
        await api("owner", { ...owner, name: name.value.trim() || owner.name, about: about.value.trim() || owner.about }, "PUT");
        app.ownerName = name.value.trim() || owner.name;
        renderFoot();
      }
      next();
    });
    api("owner").then((o) => { if (!name.value) name.value = o.name || ""; if (!about.value) about.value = o.about || ""; }).catch(() => {});
    body = [
      h("h1", null, "A little about you"),
      h("p", { class: "lead" }, "Agents write and decide the way you would. This stays on this Mac."),
      h("label", { class: "welcome-label" }, "Name", name),
      h("label", { class: "welcome-label" }, "Who you are", about),
      err,
      actions(skip(), go_on),
    ];
    setTimeout(() => name.focus());
  } else if (step === "model") {
    if (s.model_ready) {
      body = [
        h("h1", null, "A model is ready"),
        h("p", { class: "lead" }, h("span", { class: "ok" }, "✓ "), "Agents can think. You can change the model, or add a cheaper fast one, in each business's settings."),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else {
      const key = h("input", { type: "password", placeholder: "sk-ant-…", autocomplete: "off", spellcheck: "false" });
      const save = primary("Check and save");
      save.onclick = () => busy(save, async () => {
        welcomeUi.setup = await api("setup/key", { key: key.value });
        flash("Key saved");
        next();
      });
      key.addEventListener("keydown", (e) => { if (e.key === "Enter") save.click(); });
      body = [
        h("h1", null, "Connect a model"),
        h("p", { class: "lead" }, "Agents think with a model you pay for directly. Paste an Anthropic API key, from ",
          h("a", { href: "https://console.anthropic.com/settings/keys", target: "_blank", rel: "noopener" }, "console.anthropic.com"), "."),
        h("label", { class: "welcome-label" }, "Anthropic API key", key),
        h("p", { class: "muted small" }, "It stays on this Mac, readable by you alone; the model never sees it. Prefer DeepSeek, OpenAI, Grok, Mistral, OpenRouter or a local model? Skip, and pick one in the business's settings."),
        err,
        actions(skip("Later"), save),
      ];
      setTimeout(() => key.focus());
    }
  } else if (step === "business") {
    if (realm) {
      body = [
        h("h1", null, `${realm} is ready`),
        h("p", { class: "lead" }, "Its own folder, rules, agents and connectors, isolated from any other business."),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else {
      const name = h("input", { type: "text", placeholder: "my-shop", autocomplete: "off", spellcheck: "false" });
      name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
      const what = h("textarea", { rows: 4, placeholder: "What the business does, for whom, and how it sounds.\ne.g. A finance research media on X, for informed retail investors. Calm, sourced, never hype." });
      const root = h("input", { type: "text", placeholder: "Optional: ~/code/my-shop", spellcheck: "false" });
      const create = primary("Create the business");
      create.onclick = () => busy(create, async () => {
        const r = await api("realms", { name: name.value.trim(), description: what.value.trim().split("\n")[0].slice(0, 200), root: root.value.trim() });
        if (what.value.trim()) await api(`${realmPath(r.name)}/profile`, { text: what.value }, "PUT");
        welcomeUi.realm = r.name;
        await refreshRealms();
        renderSidebar();
        next();
      });
      const existing = app.realms.filter((r) => !r.archived);
      body = [
        h("h1", null, "Your business"),
        h("p", { class: "lead" }, "One business, one space: its files, code, agents and connectors. Add others later."),
        h("label", { class: "welcome-label" }, "Name", name, h("span", { class: "muted small" }, "Lowercase letters, digits and dashes.")),
        h("label", { class: "welcome-label" }, "What it does", what, h("span", { class: "muted small" }, "Every agent reads it. You can change it in its business/profile.md.")),
        h("label", { class: "welcome-label" }, "Its folder", root, h("span", { class: "muted small" }, "Where its files and code are. Empty: a private folder Airth creates.")),
        err,
        actions(existing.length ? h("button", { class: "btn ghost", type: "button", onclick: () => { welcomeUi.realm = preferredRealm(); next(); } }, `Use ${preferredRealm()}`) : null, create),
      ];
      setTimeout(() => name.focus());
    }
  } else if (step === "strategy") {
    const text = h("textarea", { rows: 7, placeholder: "Your goals, the line every agent follows, and what the team may do alone.\n\n- One article a week on the blog.\n- Explain, never hype. Facts and numbers first.\n- Ask me before publishing or spending money." });
    const save = primary("Save the strategy");
    save.onclick = () => busy(save, async () => {
      if (text.value.trim()) await api(`${realmPath(realm)}/strategy`, { text: text.value }, "PUT");
      next();
    });
    if (realm) api(`${realmPath(realm)}/steering`).then((d) => { if (!text.value) text.value = d.strategy || ""; }).catch(() => {});
    body = [
      h("h1", null, "Set the direction"),
      h("p", { class: "lead" }, "This is where you steer. Every agent reads it before it works; you change it any time on Steering."),
      text,
      err,
      actions(skip(), save),
    ];
    setTimeout(() => text.focus());
  } else if (step === "agent") {
    const t = welcomeUi.template;
    if (welcomeUi.agent) {
      body = [
        h("h1", null, `${welcomeUi.agent} is on the team`),
        h("p", { class: "lead" }, "It works only when a routine says so. Give it one now."),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else if (t) {
      const name = h("input", { type: "text", value: t.name, autocomplete: "off", spellcheck: "false" });
      name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
      const job = h("textarea", { rows: 10 });
      job.value = t.job;
      const hire = primary("Hire");
      hire.onclick = () => busy(hire, async () => {
        await api(`${realmPath(realm)}/agents/${enc(name.value.trim())}`, { job: job.value }, "PUT");
        welcomeUi.agent = name.value.trim();
        welcomeUi.routine = { task: t.task || "", every: t.every || "mon 08:00" };
        renderMain();
      });
      body = [
        h("h1", null, `Hire ${t.title ? "a " + t.title.toLowerCase() : "an agent"}`),
        h("p", { class: "lead" }, "Its job, in your words. Adjust it to your business."),
        h("label", { class: "welcome-label" }, "Name", name),
        h("label", { class: "welcome-label" }, "Job", job),
        err,
        actions(h("button", { class: "btn ghost", type: "button", onclick: () => { welcomeUi.template = null; renderMain(); } }, "Another one"), hire),
      ];
    } else {
      body = [
        h("h1", null, "Your first agent"),
        h("p", { class: "lead" }, "Pick a job to start from. You can hire more later, in Agents."),
        h("div", { class: "agent-templates" },
          AGENT_TEMPLATES.map((x) => h("button", { class: "agent-template", type: "button", onclick: () => { welcomeUi.template = x; renderMain(); } },
            h("span", { class: "strong" }, x.title), h("span", { class: "muted small" }, x.desc)))),
        actions(skip("Later", "done")),
      ];
    }
  } else if (step === "routine") {
    if (!welcomeUi.agent) { welcomeUi.step = WELCOME_STEPS.indexOf("done"); return renderWelcome(main); }
    const r = welcomeUi.routine || { task: "", every: "mon 08:00" };
    const task = h("textarea", { rows: 2 });
    task.value = r.task;
    const when = h("select", null, WHEN_CHOICES.map(([v, label]) => h("option", { value: v, selected: v === r.every }, label)));
    if (!WHEN_CHOICES.some(([v]) => v === r.every)) when.prepend(h("option", { value: r.every, selected: true }, scheduleText(r.every)));
    const create = primary("Create the routine");
    create.onclick = () => busy(create, async () => {
      await api(`${realmPath(realm)}/routines`, { id: `${welcomeUi.agent}-routine`, objective: task.value, bot: welcomeUi.agent, every: when.value || null, tools: null, allow: [] });
      next();
    });
    body = [
      h("h1", null, `When ${welcomeUi.agent} works`),
      h("p", { class: "lead" }, "What it does each time, and when. Anything it may not do alone waits for you on Steering."),
      h("label", { class: "welcome-label" }, "Each time", task),
      h("label", { class: "welcome-label" }, "When", when),
      err,
      actions(skip("Later"), create),
    ];
  } else {
    const done = h("button", { class: "btn primary", type: "button", onclick: finish }, "Open Steering");
    const tip = (ic, title, text) => h("div", { class: "welcome-point" }, h("span", { class: "welcome-icon" }, icon(ic)), h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, text)));
    body = [
      h("h1", null, "You're set"),
      h("p", { class: "lead" }, realm ? `${realm} is ready to run.` : "Airth is ready."),
      h("div", { class: "welcome-points" },
        tip("compass", "Steering", "What waits for your decision, what needs attention, and your strategy."),
        tip("refresh", "Routines and Planning", "Who works when. Run one now to see it work."),
        tip("plug", "Connectors", "Connect the business's services (Stripe, Notion, GitHub…) and give each agent only what it needs."),
        tip("send", "Your phone", "Approve from anywhere through Tailscale: your profile → Phone.")),
      actions(done),
    ];
  }

  main.append(h("div", { class: "page welcome" },
    h("div", { class: "welcome-inner" },
      h("ol", { class: "welcome-progress", "aria-label": `Step ${welcomeUi.step + 1} of ${WELCOME_STEPS.length}` },
        WELCOME_STEPS.map((k, i) => h("li", { class: i === welcomeUi.step ? "on" : i < welcomeUi.step ? "done" : "" }, WELCOME_TITLES[k]))),
      h("div", { class: "welcome-card" }, ...body),
      step !== "done" && h("button", { class: "welcome-skip", type: "button", onclick: finish }, "Skip the guide"))));
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
    flash(`${live} task${live === 1 ? " is" : "s are"} in progress: they will stop (and can be resumed). Click again to restart.`);
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
  // Background refreshes pause while the window is hidden, and catch up when it shows.
  setInterval(() => { if (!document.hidden) { refreshRealms(); refreshRuns(); } }, 2000);
  setInterval(() => { if (!document.hidden) refreshInfo(); }, 5000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) { refreshRealms(); refreshRuns(); refreshInfo(); } });
}

boot();
