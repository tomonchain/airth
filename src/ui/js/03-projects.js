// ---------- North Star ----------

const NS_MARK = { todo: "", doing: "", done: "✓", dropped: "–" };
const NS_NEXT = { todo: "doing", doing: "done", done: "todo", dropped: "todo" };
const nsUi = { open: false, editing: false, confirmClear: false };

/** The current realm's North Star, if the owner set one. */
const northStar = () => (app.realmDetail && app.realmDetail.north_star) || null;

/** Focus mode (per realm, on by default once there is a North Star): what does not serve it is shadowed. */
function focusOn() {
  if (!northStar()) return false;
  let saved = null;
  try { saved = localStorage.getItem("airth-focus:" + app.realm); } catch {}
  return saved !== "0";
}

function toggleFocus() {
  const on = !focusOn();
  try { localStorage.setItem("airth-focus:" + app.realm, on ? "1" : "0"); } catch {}
  renderSidebar();
  if (app.route.view === "new") renderMain();
  flash(on ? "Focus on: what does not serve the North Star is shadowed" : "Focus off");
}

function nsBar(ns) {
  const fill = h("span");
  // Set through the CSSOM: the page's CSP blocks inline style attributes.
  fill.style.width = (ns.total ? Math.round((100 * ns.done) / ns.total) : 0) + "%";
  return h("div", { class: "plan-bar ns-bar", "aria-hidden": "true" }, fill);
}

/** The goal, always in sight at the top of the sidebar. */
function nsSide() {
  const ns = northStar();
  if (!ns) return null;
  return h("div", { class: "ns-side" },
    h("button", { class: "ns-side-goal", type: "button", title: ns.goal + (ns.total ? `\n${ns.done} of ${ns.total} milestones done` : ""), onclick: () => go(`#/r/${enc(app.realm)}/northstar`) },
      h("span", { class: "ns-star", "aria-hidden": "true" }, "★"),
      h("span", { class: "grow ns-side-text" }, ns.goal),
      ns.total > 0 && h("span", { class: "progress-mini" }, `${ns.done}/${ns.total}`)),
    ns.total > 0 && nsBar(ns),
    h("label", { class: "ns-focus", title: "Shadow the chats that do not serve the North Star" },
      h("input", { type: "checkbox", checked: focusOn(), onchange: toggleFocus }), h("span", null, "Focus")));
}

async function startNorthStarRun(what, realm = app.realm) {
  try {
    const r = await api(`${realmPath(realm)}/northstar/${what}`, {});
    await refreshRuns();
    go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
  } catch (e) { flash(e.message, true); }
}

/** The North Star page: the focus project's page, or where to set one. */
function renderNorthStar(main) {
  const ns = northStar();
  if (ns && ns.project) return renderProject(main, ns.project);
  page(main, "North Star", "The one goal of this realm. Every chat sees it and prefers the work that moves it forward.", northStarCard());
}

// ---------- projects ----------
//
// A realm's projects: a goal, milestones (the stages) and tasks on a board.
// Tasks Airth works on move by themselves with their chat. The focus project
// is the North Star (its card above handles the goal, milestones and rhythm).

const projectUi = { creating: false, editing: false, confirm: null, task: null, adding: "", poll: null };
const TASK_COLS = [["todo", "To do"], ["doing", "In progress"], ["review", "Review"], ["done", "Done"]];

/** The realm's projects, kept for the pickers and headers (loaded once per realm). */
async function loadProjects(force) {
  const realm = app.realm;
  if (!realm) return [];
  if (!force && app.projects && app.projects.realm === realm) return app.projects.list;
  try {
    const { projects } = await api(`${realmPath(realm)}/projects`);
    app.projects = { realm, list: projects };
  } catch { app.projects = { realm, list: [] }; }
  return app.projects.list;
}
const projectsNow = () => (app.projects && app.projects.realm === app.realm ? app.projects.list : []);

async function projectCall(path, body, method) {
  const r = await api(`${realmPath(app.realm)}/projects${path}`, body, method);
  if (r && r.projects) app.projects = { realm: app.realm, list: r.projects };
  if (path === "" || /^\/[^/]+$/.test(path)) await refreshRealmDetail();
  renderSidebar();
  return r;
}

async function renderProjects(main) {
  const realm = app.realm;
  const head = h("div", { class: "page-head" }, h("h1", null, "Projects"),
    !projectUi.creating && h("button", { class: "btn primary", type: "button", onclick: () => { projectUi.creating = true; renderMain(); } }, icon("plus"), "New project"));
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner" }, head,
    h("p", { class: "sub" }, "What you pursue in this realm. Each project has a goal, milestones and a board of tasks; Airth can take a task and the board follows its chat. The North Star is the project every chat keeps in sight."))));
  const inner = $(".page-inner", main);
  const list = await loadProjects(true);
  if (app.route.view !== "projects" || app.realm !== realm) return;

  if (projectUi.creating) {
    const name = h("input", { type: "text", placeholder: "Name, e.g. Launch the site", autocomplete: "off" });
    const goal = h("input", { type: "text", placeholder: "The goal: what done looks like, e.g. “The site is live and takes orders”" });
    const why = h("input", { type: "text", placeholder: "Why it matters (optional)" });
    const focus = h("input", { type: "checkbox", checked: !northStar() });
    const create = async () => {
      if (!name.value.trim()) return name.focus();
      try {
        const r = await projectCall("", { name: name.value, goal: goal.value, why: why.value, focus: focus.checked }, "POST");
        projectUi.creating = false;
        go(`#/r/${enc(realm)}/projects/${enc(r.id)}`);
      } catch (e) { flash(e.message, true); }
    };
    for (const i of [name, goal, why]) i.addEventListener("keydown", (e) => { if (e.key === "Enter") create(); if (e.key === "Escape") { projectUi.creating = false; renderMain(); } });
    inner.append(h("div", { class: "card project-new" },
      h("div", { class: "strong" }, "New project"), name, goal, why,
      h("label", { class: "toggle small" }, focus, h("span", null, "Make it the North Star", northStar() ? h("span", { class: "muted" }, ` (instead of “${northStar().goal}”)`) : null)),
      h("div", { class: "row-end" },
        h("button", { class: "btn ghost", type: "button", onclick: () => { projectUi.creating = false; renderMain(); } }, "Cancel"),
        h("button", { class: "btn primary", type: "button", onclick: create }, "Create"))));
    setTimeout(() => name.focus(), 30);
  }

  if (!list.length && !projectUi.creating) {
    inner.append(h("div", { class: "routine-empty" },
      h("div", { class: "routine-icon big", "aria-hidden": "true" }, icon("listCheck")),
      h("div", { class: "strong" }, "No projects yet"),
      h("p", { class: "muted small" }, "Launch a site, prepare a trip, ship a release… Give it a goal and tasks, and hand tasks to Airth."),
      h("button", { class: "btn", type: "button", onclick: () => { projectUi.creating = true; renderMain(); } }, icon("plus"), "New project")));
    return;
  }
  const order = (p) => (p.focus ? 0 : p.status === "active" ? 1 : p.status === "paused" ? 2 : 3);
  inner.append(h("div", { class: "projects" }, [...list].sort((a, b) => order(a) - order(b)).map((p) => {
    const count = (st) => p.tasks.filter((t) => t.status === st).length;
    const working = p.tasks.some((t) => t.live);
    const fill = h("span");
    fill.style.width = p.total ? `${Math.round((100 * p.done) / p.total)}%` : "0";
    return h("a", { class: "card project-card" + (p.status !== "active" ? " off" : ""), href: `#/r/${enc(realm)}/projects/${enc(p.id)}` },
      h("div", { class: "project-top" },
        p.focus && h("span", { class: "ns-star", title: "The North Star" }, "★"),
        h("span", { class: "project-name" }, p.name),
        p.status !== "active" && h("span", { class: "chip" }, p.status),
        working && h("span", { class: "chip running" }, "Airth is working")),
      p.goal && h("div", { class: "project-goal" }, p.goal),
      h("div", { class: "project-meta" },
        p.total > 0 && h("span", { class: "project-progress", title: `${p.done} of ${p.total} milestones done` }, h("span", { class: "bar" }, fill), `${p.done}/${p.total}`),
        p.tasks.length > 0 && h("span", null, `${count("todo")} to do · ${count("doing")} in progress · ${count("review")} to review · ${count("done")} done`),
        !p.tasks.length && h("span", { class: "r-faint" }, "No tasks yet")));
  })));
}

async function renderProject(main, id) {
  const realm = app.realm;
  const page_ = h("div", { class: "page" }, h("div", { class: "page-inner wide" }, working("Loading…")));
  main.append(page_);
  const list = await loadProjects(true);
  if (app.realm !== realm || !["project", "northstar"].includes(app.route.view)) return;
  drawProject(page_.firstChild, id);
}

function drawProject(inner, id) {
  const realm = app.realm;
  const p = projectsNow().find((x) => x.id === id);
  clearTimeout(projectUi.poll);
  if (!p) return inner.replaceChildren(h("p", { class: "muted" }, "This project does not exist (anymore). ", h("a", { href: `#/r/${enc(realm)}/projects` }, "All projects")));
  const redraw = () => { if (inner.isConnected) drawProject(inner, id); };
  const call = async (path, body, method, msg) => {
    try { await projectCall(`/${enc(id)}${path}`, body, method); if (msg) flash(msg); } catch (e) { flash(e.message, true); }
    redraw();
  };
  // While Airth works on a task, follow it.
  if (p.tasks.some((t) => t.live)) projectUi.poll = setTimeout(async () => { if (inner.isConnected) { await loadProjects(true); redraw(); } }, 4000);

  const act = (ic, label, onclick, cls = "") => h("button", { class: "c-act " + cls, type: "button", title: label, "aria-label": label, onclick }, icon(ic));
  const del = projectUi.confirm === "project"
    ? h("button", { class: "c-act sure", type: "button", onclick: async () => { projectUi.confirm = null; await call("", undefined, "DELETE", "Project deleted"); go(`#/r/${enc(realm)}/projects`); } }, "Delete it and its tasks?")
    : act("trash", "Delete this project", () => { projectUi.confirm = "project"; redraw(); setTimeout(() => { if (projectUi.confirm === "project") { projectUi.confirm = null; redraw(); } }, 3000); }, "danger");
  const head = h("div", { class: "project-page-head" },
    h("a", { class: "back", href: `#/r/${enc(realm)}/projects` }, "← Projects"),
    h("div", { class: "page-head" },
      h("h1", null, p.focus && h("span", { class: "ns-star", title: "The North Star" }, "★ "), p.name),
      h("div", { class: "project-tools" },
        h("div", { class: "seg small" }, [["active", "Active"], ["paused", "Paused"], ["done", "Done"]].map(([v, label]) =>
          h("button", { type: "button", class: p.status === v ? "on" : "", onclick: () => p.status !== v && call("", { status: v }, "PATCH") }, label))),
        !p.focus && act("star", "Make it the North Star", () => call("", { focus: true }, "PATCH", "It is the North Star now")),
        act("pencil", "Rename, change the goal", () => { projectUi.editing = !projectUi.editing; redraw(); }),
        del)));

  // Goal and milestones: the North Star card for the focus project.
  let about;
  if (projectUi.editing) {
    const name = h("input", { type: "text", value: p.name });
    const goal = h("input", { type: "text", value: p.goal, placeholder: "The goal" });
    const why = h("input", { type: "text", value: p.why, placeholder: "Why it matters (optional)" });
    about = h("div", { class: "card project-new" }, name, goal, why,
      h("div", { class: "row-end" },
        h("button", { class: "btn ghost", type: "button", onclick: () => { projectUi.editing = false; redraw(); } }, "Cancel"),
        h("button", { class: "btn primary", type: "button", onclick: () => { projectUi.editing = false; call("", { name: name.value, goal: goal.value, why: why.value }, "PATCH", "Saved"); } }, "Save")));
  } else if (p.focus) {
    about = northStarCard();
  } else {
    about = h("div", { class: "project-about" },
      p.goal ? h("div", { class: "ns-goal" }, p.goal) : h("p", { class: "muted small" }, "No goal written yet."),
      p.why && h("div", { class: "ns-why" }, p.why),
      projectMilestones(p, call));
  }

  inner.replaceChildren(head, about, h("h2", null, "Tasks"), taskBoard(p, call, redraw));
}

/** A project's milestones (the North Star has its own card): click a mark to move it, add one at the end. */
function projectMilestones(p, call) {
  const add = h("input", { type: "text", placeholder: "Add a milestone…", class: "ms-add" });
  add.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || !add.value.trim()) return;
    const items = [...p.milestones.map((m) => ({ title: m.title, done_when: m.done_when })), { title: add.value, done_when: "" }];
    call("/milestones", { milestones: items }, "PUT");
  });
  return h("div", { class: "project-ms" },
    p.milestones.length > 0 && h("ol", { class: "ns-milestones" }, p.milestones.map((m) => h("li", { class: "ns-m " + m.status },
      h("button", { class: "mark", type: "button", title: `${m.status}: click to mark ${NS_NEXT[m.status]}`, onclick: () => call(`/milestones/${enc(m.id)}`, { status: NS_NEXT[m.status] }, "PATCH") }, NS_MARK[m.status]),
      h("div", { class: "grow" }, h("div", { class: "ns-m-title" }, m.title), m.note && h("div", { class: "muted small" }, m.note))))),
    add);
}

/** The board: tasks by status. Drag a card to move it; Airth's tasks move with their chat. */
function taskBoard(p, call, redraw) {
  const realm = app.realm;
  const repos = (app.realmDetail && app.realmDetail.repos) || [];
  const act = (ic, label, onclick, cls = "") => h("button", { class: "c-act " + cls, type: "button", title: label, "aria-label": label,
    onclick: (e) => { e.stopPropagation(); onclick(); } }, icon(ic));
  const run = async (t) => {
    try {
      const model = storedModel(`new:${realm}`) || "auto";
      const r = await api(`${realmPath(realm)}/projects/${enc(p.id)}/tasks/${enc(t.id)}/run`, { model });
      await Promise.all([refreshRuns(), loadProjects(true)]);
      go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
    } catch (e) { flash(e.message, true); }
  };

  const editor = (t) => {
    const title = h("input", { type: "text", value: t.title });
    const notes = h("textarea", { rows: 4, placeholder: "Details for Airth: what to do, what done means…" });
    notes.value = t.notes || "";
    const ms = h("select", null, h("option", { value: "" }, "No milestone"), p.milestones.map((m) => h("option", { value: m.id, selected: m.id === t.milestone }, `${m.id} · ${m.title}`)));
    const repo = repos.length > 0 && h("select", null, h("option", { value: "" }, repos.some((r) => r.main) ? "The realm's folder" : "No repository"),
      repos.filter((r) => !r.main).map((r) => h("option", { value: r.name, selected: r.name === t.repo }, r.name)));
    const save = () => { projectUi.task = null; call(`/tasks/${enc(t.id)}`, { title: title.value, notes: notes.value, milestone: ms.value, repo: repo ? repo.value : undefined }, "PATCH"); };
    return h("div", { class: "task-card editing" }, title, notes,
      h("div", { class: "inline" }, p.milestones.length > 0 && ms, repo),
      h("div", { class: "row-end" },
        h("button", { class: "btn ghost small", type: "button", onclick: () => { projectUi.task = null; redraw(); } }, "Cancel"),
        h("button", { class: "btn primary small", type: "button", onclick: save }, "Save")));
  };

  const card = (t) => {
    if (projectUi.task === t.id) return editor(t);
    const sure = projectUi.confirm === "task:" + t.id;
    const el = h("div", { class: "task-card" + (t.live ? " live" : ""), draggable: "true", tabindex: "0",
        title: t.notes || "", onclick: () => { projectUi.task = t.id; redraw(); } },
      h("div", { class: "task-title" }, t.title),
      h("div", { class: "task-meta" },
        t.milestone && h("span", { class: "task-chip", title: (p.milestones.find((m) => m.id === t.milestone) || {}).title || "" }, t.milestone),
        t.repo && h("span", { class: "task-chip" }, icon("code"), t.repo),
        t.live && h("span", { class: "task-live" }, h("span", { class: "spinner" }), "Airth is on it"),
        t.run && h("a", { class: "task-chip link", href: `#/r/${enc(realm)}/run/${enc(t.run)}`, onclick: (e) => e.stopPropagation(), title: "The chat" }, icon("pencil"), "chat")),
      t.note && t.status !== "done" && h("div", { class: "task-note" }, t.note),
      h("div", { class: "task-acts" },
        (t.status === "todo" || (t.status === "doing" && !t.live)) && act("play", t.run ? "Run again with Airth" : "Run with Airth", () => run(t)),
        sure ? h("button", { class: "c-act sure", type: "button", onclick: (e) => { e.stopPropagation(); projectUi.confirm = null; call(`/tasks/${enc(t.id)}`, undefined, "DELETE"); } }, "Delete?")
          : act("trash", "Delete this task", () => { projectUi.confirm = "task:" + t.id; redraw(); setTimeout(() => { if (projectUi.confirm === "task:" + t.id) { projectUi.confirm = null; redraw(); } }, 3000); }, "danger")));
    el.addEventListener("dragstart", (e) => { e.dataTransfer.setData("text/plain", t.id); e.dataTransfer.effectAllowed = "move"; el.classList.add("dragging"); });
    el.addEventListener("dragend", () => el.classList.remove("dragging"));
    el.addEventListener("keydown", (e) => { if (e.key === "Enter") { projectUi.task = t.id; redraw(); } });
    return el;
  };

  const add = h("input", { type: "text", class: "task-add", placeholder: "+ Add a task", value: projectUi.adding });
  add.addEventListener("input", () => { projectUi.adding = add.value; });
  add.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter" || !add.value.trim()) return;
    projectUi.adding = "";
    await call("/tasks", { title: add.value }, "POST");
    const box = document.querySelector(".task-add");
    if (box) box.focus();
  });

  return h("div", { class: "board" }, TASK_COLS.map(([st, label]) => {
    const tasks = p.tasks.filter((t) => t.status === st);
    const col = h("div", { class: "board-col", "data-status": st },
      h("div", { class: "board-head" }, h("span", null, label), h("span", { class: "count" }, String(tasks.length))),
      tasks.map(card),
      st === "todo" && add);
    col.addEventListener("dragover", (e) => { e.preventDefault(); col.classList.add("over"); });
    col.addEventListener("dragleave", () => col.classList.remove("over"));
    col.addEventListener("drop", (e) => {
      e.preventDefault();
      col.classList.remove("over");
      const t = e.dataTransfer.getData("text/plain");
      const task = p.tasks.find((x) => x.id === t);
      if (task && task.status !== st) call(`/tasks/${enc(t)}`, { status: st }, "PATCH");
    });
    return col;
  }));
}

/** The North Star card: goal, progress, milestones, and how Airth pushes it. */
function northStarCard() {
  const el = h("div", { id: "ns-wrap", class: "ns-wrap" });
  drawNorthStar(el);
  return el;
}

function drawNorthStar(el) {
  const d = app.realmDetail;
  if (!d) return el.replaceChildren();
  const ns = d.north_star;
  const realm = app.realm;
  const noModel = !!d.model_error;
  const change = async (path, body, method, msg) => {
    try {
      app.realmDetail = await api(`${realmPath(realm)}/northstar${path}`, body, method);
      drawNorthStar(el);
      renderSidebar();
      if (msg) flash(msg);
    } catch (e) { flash(e.message, true); }
  };

  if (!ns && !nsUi.open) {
    return el.replaceChildren(h("button", { class: "ns-empty", type: "button", onclick: () => { nsUi.open = true; drawNorthStar(el); } },
      h("span", { class: "ns-star", "aria-hidden": "true" }, "★"),
      h("span", null, h("span", { class: "strong" }, "Set a North Star"), " · write the one goal for this realm. Airth then asks you a few questions, helps you sharpen it and define the milestones, and pushes it forward every day.")));
  }
  if (!ns || nsUi.editing) {
    const goal = h("input", { type: "text", value: ns ? ns.goal : "", placeholder: "The one goal this realm works toward, e.g. “100 kit sales by December”" });
    const why = h("input", { type: "text", value: ns ? ns.why : "", placeholder: "Why it matters, or what success looks like (optional)" });
    const save = async () => {
      if (!goal.value.trim()) return goal.focus();
      nsUi.editing = false;
      nsUi.open = false;
      const isNew = !ns;
      await change("", { goal: goal.value, why: why.value }, "PUT", ns ? "North Star saved" : null);
      // A new North Star: Airth starts the conversation that defines it.
      if (isNew && northStar()) {
        if (noModel) flash("North Star set. Add a model in the realm settings to define it with Airth.");
        else { flash("North Star set. Airth has a few questions for you."); startNorthStarRun("brainstorm"); }
      }
    };
    const cancel = () => { nsUi.editing = false; nsUi.open = false; nsUi.confirmClear = false; drawNorthStar(el); };
    for (const input of [goal, why]) input.addEventListener("keydown", (e) => { if (e.key === "Enter") save(); else if (e.key === "Escape") cancel(); });
    el.replaceChildren(h("div", { class: "ns-card editing" },
      h("div", { class: "ns-head" }, h("span", { class: "ns-star", "aria-hidden": "true" }, "★"), h("span", { class: "ns-label grow" }, "North Star")),
      goal, why,
      h("div", { class: "row-end" },
        ns && (nsUi.confirmClear
          ? h("button", { class: "btn danger", type: "button", onclick: () => { nsUi.editing = false; nsUi.confirmClear = false; change("", undefined, "DELETE", "North Star removed"); } }, "Really remove it?")
          : h("button", { class: "btn danger ghost", type: "button", onclick: () => { nsUi.confirmClear = true; drawNorthStar(el); } }, "Remove…")),
        h("span", { class: "grow" }),
        h("button", { class: "btn ghost", type: "button", onclick: cancel }, "Cancel"),
        h("button", { class: "btn primary", type: "button", onclick: save }, ns ? "Save" : "Set it and define it with Airth"))));
    goal.focus();
    return;
  }

  const s = ns.schedule;
  const has = ns.milestones.length > 0;
  const milestones = has
    ? h("ol", { class: "ns-milestones" }, ns.milestones.map((m) => h("li", { class: "ns-m " + m.status + (m.id === ns.next ? " next" : "") },
        h("button", { class: "mark", type: "button", title: `${m.status}: click to mark ${NS_NEXT[m.status]}`, "aria-label": `${m.title}: ${m.status}`,
          onclick: () => change(`/milestones/${enc(m.id)}`, { status: NS_NEXT[m.status] }, "PATCH") }, NS_MARK[m.status]),
        h("div", { class: "grow" },
          h("div", { class: "ns-m-title" }, m.title, m.id === ns.next && h("span", { class: "chip running" }, "next")),
          m.done_when && h("div", { class: "muted small" }, "Done when: " + m.done_when),
          m.note && h("div", { class: "muted small" }, m.note)),
        m.runs.length > 0 && h("a", { class: "muted small ns-runs", href: `#/r/${enc(realm)}/run/${enc(m.runs[m.runs.length - 1])}`, title: "The latest chat that moved it" },
          `${m.runs.length} chat${m.runs.length === 1 ? "" : "s"}`))))
    : h("p", { class: "muted small ns-hint" }, "No milestones yet. Define them with Airth: it asks you a few questions, proposes a sharper goal and the steps to it, and records them once you agree.");

  const every = h("input", { type: "text", class: "ns-every", value: s ? s.every : "weekdays 09:00", placeholder: "weekdays 09:00 · every 3h", "aria-label": "When Airth advances it", spellcheck: "false" });
  every.addEventListener("change", () => { if (s) change("/schedule", { every: every.value }, undefined, "Schedule saved"); });
  const daily = has && h("div", { class: "ns-daily" },
    h("label", { class: "toggle small", title: "A routine: the chats it starts may record North Star progress without asking; anything else follows this realm's rules" },
      h("input", { type: "checkbox", checked: !!s, onchange: (e) => change("/schedule", { every: e.target.checked ? every.value : null }, undefined, e.target.checked ? "Airth will push it forward on its own" : "Stopped") }),
      h("span", null, "Airth pushes it forward on its own")),
    every,
    s && s.running ? h("a", { class: "chip running", href: `#/r/${enc(realm)}/run/${enc(s.last_run)}` }, "working on it")
      : s && s.next_at && h("span", { class: "muted small" }, "next " + when(s.next_at)),
    s && !s.running && s.last_run && h("a", { class: "muted small", href: `#/r/${enc(realm)}/run/${enc(s.last_run)}` }, "last: " + (s.last_status || "run")));

  el.replaceChildren(h("div", { class: "ns-card" },
    h("div", { class: "ns-head" },
      h("span", { class: "ns-star", "aria-hidden": "true" }, "★"),
      h("span", { class: "ns-label grow" }, "North Star", ns.total > 0 && h("span", { class: "muted" }, ` · ${ns.done} of ${ns.total} milestones`)),
      h("button", { class: "icon-action", type: "button", title: "Edit the North Star", "aria-label": "Edit the North Star", onclick: () => { nsUi.editing = true; drawNorthStar(el); } }, icon("pencil"))),
    h("div", { class: "ns-goal" }, ns.goal),
    ns.why && h("div", { class: "ns-why" }, ns.why),
    ns.total > 0 && nsBar(ns),
    milestones,
    h("div", { class: "ns-actions" },
      has
        ? h("button", { class: "btn primary", type: "button", disabled: noModel, title: "A chat on the next milestone: the most valuable step Airth can complete and verify now", onclick: () => startNorthStarRun("push") }, "Push it forward now")
        : h("button", { class: "btn primary", type: "button", disabled: noModel, onclick: () => startNorthStarRun("brainstorm") }, "Define it with Airth"),
      has && h("button", { class: "btn", type: "button", disabled: noModel, title: "Airth asks what changed and reworks the goal and milestones with you", onclick: () => startNorthStarRun("brainstorm") }, "Refine with Airth")),
    daily));
}

/** Keep the progress live on the realm's home page (runs record it as they go). */
async function refreshNorthStar() {
  if (!app.realm || app.unauthorized) return;
  const before = JSON.stringify(northStar());
  try { await refreshRealmDetail(); } catch { return; }
  if (JSON.stringify(northStar()) === before) return;
  renderSidebar();
  const el = $("#ns-wrap");
  if (el && !nsUi.editing && !nsUi.open) drawNorthStar(el);
}

// ---------- what Airth can do ----------

/** What Airth can take on, by area, in words; examples fill the message box. */
const CAN_DO = [
  { caps: ["read", "write"], icon: "folder", title: "Your files",
    text: "Reads, sorts, writes and edits files in this realm's folder, and never outside it.",
    examples: ["Sort the files in this folder by topic", "Summarize every document in this folder"] },
  { caps: ["apps"], icon: "calendar", title: "Your Mac apps",
    text: "Calendar, Reminders, Notes, Mail (it reads and prepares drafts; it never sends), Safari tabs and the clipboard.",
    examples: ["What's on my calendar tomorrow?", "Summarize today's unread emails", "Add a reminder to call the bank on Friday"] },
  { caps: ["network"], icon: "world", title: "The web and your services",
    text: "Looks things up on public websites, and calls the services you connect (Notion, Linear, Stripe…) without ever seeing their keys.",
    examples: ["Compare the prices of three competitors", "Find the best-rated plumbers near me"], services: true },
  { caps: ["execute"], icon: "terminal", title: "Commands and code",
    text: "Runs commands in a sandbox: scripts, file conversions, tests, code changes you review before they count.",
    examples: ["Convert the images in this folder to WebP", "Run the tests and fix what fails"] },
  { caps: ["memory"], icon: "brain", title: "Memory and goals",
    text: "Remembers what matters in this realm, and keeps the North Star's milestones up to date.",
    examples: ["Remember that I prefer meetings in the morning", "What should I do next for the North Star?"] },
  { caps: [], icon: "clock", title: "Work on its own",
    text: "Routines start work for you: on a schedule, when files change, or from a webhook. A brief every morning, a weekly review…",
    link: ["Set up a routine", (r) => `#/r/${enc(r)}/routines`] },
  { caps: ["skill"], icon: "spark", title: "New skills",
    text: "When it hits a limit, it can build and test a small tool; nothing is installed until you say so. Or add ready-made skills.",
    link: ["Browse skills", (r) => `#/r/${enc(r)}/skills`] },
];
const CAN_DO_RULE = { allow: ["Automatic", "rule-allow"], ask: ["Asks you first", "rule-ask"], deny: ["Off", "rule-deny"] };

let canDo = null;

/** The presentation of what Airth can do in the current realm. */
async function openCanDo(onExample) {
  if (canDo) return;
  const d = app.realmDetail;
  const realm = app.realm;
  if (!d || !realm) return;
  // The strictest rule of an area says what to expect.
  const ruleOf = (caps) => {
    const rules = caps.map((c) => d.policy[c]).filter(Boolean);
    if (!rules.length) return null;
    return rules.includes("deny") ? (rules.every((r) => r === "deny") ? "deny" : "ask") : rules.includes("ask") ? "ask" : "allow";
  };
  const close = () => {
    if (!canDo) return;
    const el = canDo;
    canDo = null;
    el.classList.add("closing");
    setTimeout(() => el.remove(), 140);
    document.removeEventListener("keydown", onKey);
  };
  const onKey = (e) => { if (e.key === "Escape") close(); };
  const services = h("div", { class: "cando-services muted small" });
  const cards = CAN_DO.map((a) => {
    const rule = ruleOf(a.caps);
    const [word, cls] = rule ? CAN_DO_RULE[rule] : [];
    return h("div", { class: "cando-card" + (rule === "deny" ? " off" : "") },
      h("div", { class: "cando-top" },
        h("span", { class: "welcome-icon" }, icon(a.icon)),
        h("div", { class: "grow strong" }, a.title),
        rule && h("a", { class: "chip " + cls, href: `#/r/${enc(realm)}/settings/permissions`, title: "Change in Permissions", onclick: close }, word)),
      h("p", { class: "muted small" }, a.text),
      a.services && services,
      a.examples && h("div", { class: "cando-examples" }, a.examples.map((x) =>
        h("button", { class: "suggestion", type: "button", disabled: rule === "deny", onclick: () => { close(); onExample && onExample(x); } }, x))),
      a.link && h("a", { class: "cando-more", href: a.link[1](realm), onclick: close }, a.link[0], " ›"));
  });
  const tools = app.info && h("details", { class: "cando-tools" },
    h("summary", null, "Technical details: every tool"),
    h("div", null, app.info.tools.map((t) => {
      const rule = d.policy[t.capability];
      return h("div", { class: "tool-row" },
        h("code", null, t.name),
        rule ? h("span", { class: "chip rule-" + rule }, POLICY_WORDS[rule]) : h("span"),
        h("span", { class: "d" }, t.description.split(". ")[0] + "."));
    })));
  canDo = h("div", { class: "pal-overlay cando-overlay", onmousedown: (e) => { if (e.target === canDo) close(); } },
    h("div", { class: "pal cando", role: "dialog", "aria-modal": "true", "aria-label": "What Airth can do" },
      h("div", { class: "cando-head" },
        h("div", { class: "grow" },
          h("div", { class: "eyebrow" }, realm),
          h("h2", null, "What can Airth do for you?"),
          h("p", { class: "muted" }, "Describe an outcome in plain words. Airth makes a plan, uses the tools below, shows you each step, and asks before anything this realm does not allow on its own.")),
        h("button", { class: "icon-btn", type: "button", "aria-label": "Close", title: "Close (Esc)", onclick: close }, "×")),
      h("div", { class: "cando-body" },
        h("div", { class: "cando-grid" }, cards),
        h("div", { class: "cando-promise" },
          h("div", { class: "strong" }, "Always"),
          h("ul", null,
            h("li", null, "It works only in this realm: other realms' files, memory and keys stay out of reach."),
            h("li", null, "It never sees your keys and passwords; the runtime adds them for it."),
            h("li", null, "You can stop Airth at any time, and review a change before it counts."))),
        tools)));
  document.body.append(canDo);
  document.addEventListener("keydown", onKey);
  // Connected services, for the web card.
  try {
    const c = await api(`${realmPath(realm)}/connectors`);
    const on = (c.connectors || []).filter((x) => x.connected).map((x) => x.connector.name);
    services.replaceChildren(on.length
      ? h("span", null, "Connected here: ", h("b", null, on.join(", ")))
      : h("span", null, "No service connected yet. ", h("a", { href: `#/r/${enc(realm)}/skills`, onclick: () => { skillUi.tab = "connectors"; close(); } }, "Connect one"), "."));
  } catch { services.remove(); }
}

