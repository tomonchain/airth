// ---------- routines and planning (decision 37) ----------

const routineUi = { form: null, removing: null };

/** What a routine may do alone, as the form offers it: capabilities, and each integration. */
function allowChoices(extensions) {
  const base = [["network", "Reach the web"], ["write", "Write files"], ["execute", "Run commands"], ["memory", "Remember"]];
  const integrations = ((app.realmDetail && app.realmDetail.integrations) || []).map((i) => [`network:${i.name}`, `Call ${i.name}`]);
  return [...base, ...integrations].filter(([v]) => !v.startsWith("network:") || extensions.includes(v.slice(8)));
}

async function renderRoutines(main) {
  const realm = app.realm;
  const add = h("button", { class: "btn primary", type: "button", onclick: () => { routineUi.form = {}; again(); } }, icon("plus"), "New routine");
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner" },
    h("div", { class: "page-head" }, h("h1", null, "Routines"), add),
    h("p", { class: "sub" }, "The business's recurring work: which agent does what, and when. Each time, it may do what the business allows plus what you approve here; anything else waits for you."))));
  const inner = $(".page-inner", main);
  const again = () => { if (app.route.view === "routines") { main.replaceChildren(); renderRoutines(main); } };
  let d;
  try { d = await api(`${realmPath(realm)}/routines`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "routines" || app.realm !== realm) return;

  if (routineUi.form) inner.append(routineForm(realm, d, routineUi.form, again));
  if (!d.routines.length && !routineUi.form) inner.append(h("div", { class: "card muted pad" }, d.agents.length
    ? "No routine yet. A routine says which agent works, on what, and when."
    : h("span", null, "No routine yet. Start by hiring an agent in ", h("a", { href: `#/r/${enc(realm)}/agents` }, "Agents"), ", then give it a routine.")));
  const act = (path, body, method, done) => async () => {
    try { await api(`${realmPath(realm)}/routines/${path}`, body, method); if (done) flash(done); } catch (e) { flash(e.message, true); }
    again();
  };
  for (const o of d.routines) {
    const remove = h("button", { class: "btn ghost", type: "button", onclick: () => {
      if (routineUi.removing !== o.id) { routineUi.removing = o.id; remove.textContent = "Remove? Click again"; return; }
      routineUi.removing = null;
      act(enc(o.id), undefined, "DELETE", `${o.id} removed`)();
    } }, "Remove");
    const run = h("button", { class: "btn", type: "button", disabled: o.running, onclick: async () => {
      try {
        const { id } = await api(`${realmPath(realm)}/routines/${enc(o.id)}/run`, {});
        go(`#/r/${enc(realm)}/run/${enc(id)}`);
      } catch (e) { flash(e.message, true); }
    } }, o.running ? "Running…" : "Run now");
    inner.append(h("div", { class: "card biz-card" + (o.enabled ? "" : " paused") },
      h("div", { class: "biz-head" },
        h("h2", null, o.id),
        o.bot && h("a", { class: "biz-tag", href: `#/r/${enc(realm)}/agents` }, o.bot),
        !o.enabled && h("span", { class: "muted small" }, "paused"),
        h("span", { class: "grow" }),
        o.last_status && h("span", { class: o.last_status === "completed" ? "biz-ok small" : "biz-bad small" }, `${o.last_status} ${o.last_at ? ago(o.last_at) : ""}`)),
      h("div", { class: "muted small" }, o.every ? scheduleText(o.every) : "By hand", o.enabled && o.next_at ? ` · next ${when(o.next_at)}` : ""),
      h("p", { class: "routine-task" }, o.objective),
      h("div", { class: "muted small" },
        `Tools: ${o.tools ? (o.tools.length ? o.tools.join(", ") : "none of the business's") : "all of the business's"}`,
        ` · alone: ${o.allow.length ? o.allow.join(", ") : "only what the business allows"}`,
        o.max_minutes ? ` · at most ${o.max_minutes} min` : ""),
      h("div", { class: "biz-actions" }, run,
        h("button", { class: "btn", type: "button", onclick: act(enc(o.id), { enabled: !o.enabled }, "PATCH", o.enabled ? `${o.id} paused` : `${o.id} resumed`) }, o.enabled ? "Pause" : "Resume"),
        h("button", { class: "btn", type: "button", onclick: () => { routineUi.form = o; again(); } }, "Edit"),
        o.last_run && h("a", { class: "btn ghost", href: `#/r/${enc(realm)}/run/${enc(o.last_run)}` }, "Last task"),
        remove)));
  }
  drawPipelines(inner, realm);
}

/** Create or change a routine. `o` is the routine being changed, or {} for a new one. */
function routineForm(realm, d, o, again) {
  const editing = !!o.id;
  const name = h("input", { type: "text", value: o.id || "", disabled: editing, autocomplete: "off", spellcheck: "false", placeholder: "weekly-article" });
  name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
  const bot = h("select", null, h("option", { value: "" }, "No agent: the general assistant"),
    d.agents.map((a) => h("option", { value: a, selected: o.bot === a }, a)));
  const task = h("textarea", { rows: 3, placeholder: "Write and prepare this week's article." });
  task.value = o.objective || "";
  const every = h("input", { type: "text", value: o.every || "", spellcheck: "false", placeholder: "mon 08:00 · daily 07:00 · weekdays 09:00 · every 2h" });
  const only = h("input", { type: "checkbox", checked: !!o.tools });
  const tools = d.extensions.map((x) => h("label", { class: "check-row" }, h("input", { type: "checkbox", value: x, checked: !!(o.tools && o.tools.includes(x)) }), x));
  const allow = allowChoices(d.extensions).map(([v, label]) => h("label", { class: "check-row" }, h("input", { type: "checkbox", value: v, checked: (o.allow || []).includes(v) }), label));
  const minutes = h("input", { type: "number", min: "1", value: o.max_minutes || "", placeholder: "30" });
  const err = h("div", { class: "error-banner", hidden: true });
  const checked = (rows) => rows.map((r) => $("input", r)).filter((i) => i.checked).map((i) => i.value);
  const save = async () => {
    try {
      await api(`${realmPath(realm)}/routines`, {
        id: name.value.trim(), objective: task.value, bot: bot.value || null, every: every.value.trim() || null,
        tools: only.checked ? checked(tools) : null, allow: checked(allow),
        max_minutes: Number(minutes.value) || null, wait: o.wait != null ? o.wait : 10,
      });
      routineUi.form = null;
      flash(editing ? "Routine saved" : "Routine created");
      again();
    } catch (e) { err.textContent = e.message; err.hidden = false; }
  };
  return h("div", { class: "card biz-card" },
    h("h2", null, editing ? `Change ${o.id}` : "New routine"),
    field("Name", name, "Lowercase letters, digits and dashes."),
    field("Agent", bot, "Who does it: its job and its tools come with it."),
    field("Task", task, "What each run should achieve."),
    field("When", every, "Leave empty to run it by hand."),
    h("div", { class: "field" }, h("label", { class: "check-row strong" }, only, "Only some of the business's tools"),
      h("div", { class: "check-grid" }, tools.length ? tools : h("span", { class: "muted small" }, "The business has no integrations, MCP servers or skills yet."))),
    h("div", { class: "field" }, h("div", { class: "strong small" }, "It may, without asking you"), h("div", { class: "check-grid" }, allow)),
    field("Time limit (minutes)", minutes, "A run stops past it."),
    err,
    h("div", { class: "biz-actions" },
      h("button", { class: "btn primary", type: "button", onclick: save }, editing ? "Save" : "Create"),
      h("button", { class: "btn", type: "button", onclick: () => { routineUi.form = null; again(); } }, "Cancel")));
}

async function renderPlanning(main) {
  const realm = app.realm;
  page(main, "Planning", "What runs when: the coming week of every scheduled routine and pipeline, and what ran in the last one.");
  const inner = $(".page-inner", main);
  let d;
  try { d = await api(`${realmPath(realm)}/planning`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "planning" || app.realm !== realm) return;
  const day = (iso) => new Date(iso).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
  const time = (iso) => new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const byDay = (items) => {
    const groups = [];
    for (const x of items) {
      const k = day(x.at);
      if (!groups.length || groups[groups.length - 1].k !== k) groups.push({ k, items: [] });
      groups[groups.length - 1].items.push(x);
    }
    return groups;
  };
  const row = (x, past) => h(x.run ? "a" : "div", { class: "biz-row", href: x.run ? `#/r/${enc(realm)}/run/${enc(x.run)}` : null },
    h("span", null, h("span", { class: "plan-time" }, time(x.at)), x.name, x.bot && h("span", { class: "muted" }, ` · ${x.bot}`)),
    past ? h("span", { class: x.status === "completed" ? "biz-ok small" : x.status === "failed" ? "biz-bad small" : "muted small" }, x.status)
      : h("span", { class: "muted small" }, x.kind));
  const column = (title, items, past, empty) => h("div", { class: "card biz-card" }, h("h2", null, title),
    items.length ? byDay(items).map((g) => [h("div", { class: "plan-day" }, g.k), g.items.map((x) => row(x, past))]) : h("div", { class: "muted small" }, empty));
  // The coming week at a glance: one column per day, today first.
  const start = new Date();
  start.setHours(0, 0, 0, 0);
  const days = Array.from({ length: 7 }, (_, i) => new Date(start.getTime() + i * 86400000));
  const calendar = h("section", { class: "card plan-calendar", "aria-label": "The coming 7 days" },
    days.map((day, i) => {
      const next = new Date(day.getTime() + 86400000);
      const items = d.coming.filter((x) => { const t = new Date(x.at); return t >= day && t < next; });
      return h("div", { class: "plan-col" + (i === 0 ? " today" : "") },
        h("div", { class: "plan-col-head" },
          h("span", { class: "plan-weekday" }, i === 0 ? "Today" : i === 1 ? "Tomorrow" : day.toLocaleDateString([], { weekday: "long" })),
          h("span", { class: "plan-date" }, day.toLocaleDateString([], { day: "numeric", month: "short" }))),
        items.length
          ? items.map((x) => h("div", { class: "plan-item " + x.kind },
            h("div", { class: "plan-item-time" }, time(x.at)),
            h("div", { class: "plan-item-name" }, x.name),
            h("div", { class: "plan-item-who" }, x.bot || x.kind)))
          : h("div", { class: "plan-empty" }, "Nothing planned"));
    }));
  inner.append(calendar, h("div", { class: "biz-grid even" },
    column("Coming", d.coming, false, "Nothing scheduled this week. Give a routine a time in Routines."),
    column("Last 7 days", d.past, true, "Nothing ran this week.")));
}
