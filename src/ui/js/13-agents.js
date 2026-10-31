// ---------- agents: the business's bots (decisions 36 and 37) ----------

/** Jobs to start from, by business function, with a first routine. The owner edits them before hiring. */
const AGENT_TEMPLATES = [
  { name: "writer", task: "Write and prepare this week's article.", every: "mon 08:00", title: "Writer", desc: "Articles, posts and threads in the business's voice.", job:
`Write the business's articles and posts, in its voice (see the profile).

Each time:
1. Pick one subject that serves the strategy: news worth explaining, or a lasting notion your readers need.
2. Research it from reliable sources. Note every source you use.
3. Write it: facts and numbers first, plain words, no hype.
4. Prepare it where it is published, as a draft. Never publish without the owner's approval.

In your report: the subject and why, the sources, and what the owner must decide.` },
  { name: "analyst", task: "Watch what changed and report what matters.", every: "weekdays 08:00", title: "Analyst", desc: "Watches markets, competitors or metrics, and reports.", job:
`Watch what matters to the business (markets, competitors, its own numbers) and report what changed.

Each time:
1. Read the sources you were given, and nothing you were not.
2. Keep what changes the picture for the business; leave out the noise.
3. Say what it means and what you would do about it.

In your report: the few things that matter, with their source, and one recommendation.` },
  { name: "accountant", task: "Sum up the week's money in and out.", every: "fri 17:00", title: "Accountant", desc: "Reads the bank and the invoices. Never pays.", job:
`Keep the business's books readable for the owner.

Each time:
1. Read what came in and what went out since the last time.
2. Match it to invoices and subscriptions; flag what you cannot match.
3. Never move money, pay, or change anything at the bank.

In your report: money in, money out, what the team cost, and what looks wrong.` },
  { name: "support", task: "Answer the new customer messages.", every: "every 2h", title: "Support", desc: "Answers customers within the rules you set.", job:
`Answer the business's customers, within the rules in the strategy.

Each time:
1. Read the new messages.
2. Draft an answer for each: clear, polite, short.
3. Anything about money (refunds, prices, discounts) or a complaint waits for the owner.

In your report: what came in, what you answered, what waits for the owner.` },
  { name: "sales", task: "Find five new leads and draft a first message to each.", every: "weekdays 10:00", title: "Sales", desc: "Finds leads and drafts outreach for you to send.", job:
`Find people the business could serve, and prepare the first message to them.

Each time:
1. Look for leads that fit the strategy's target.
2. For each, note why they fit and where you found them.
3. Draft a short, personal first message. Never send it: the owner does.

In your report: the leads, why each one, and the drafts.` },
];

const agentUi = { editing: null, hiring: null, letGo: null };

async function renderAgents(main) {
  const realm = app.realm;
  page(main, "Agents", "Each agent has a job, written in plain words, and only the tools and limits you give it. Its job is in business/bots/; its routines say when it works.");
  const inner = $(".page-inner", main);
  let d;
  try { d = await api(`${realmPath(realm)}/agents`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "agents" || app.realm !== realm) return;
  const again = () => { if (app.route.view === "agents") { main.replaceChildren(); renderAgents(main); } };
  const title = (name) => name.slice(0, 1).toUpperCase() + name.slice(1);
  const save = async (name, job) => {
    try {
      await api(`${realmPath(realm)}/agents/${enc(name)}`, { job }, "PUT");
      agentUi.editing = null;
      agentUi.hiring = null;
      flash(`${title(name)}'s job is saved: it follows it from its next task`);
    } catch (e) { return flash(e.message, true); }
    again();
  };

  const card = (a) => {
    const editing = agentUi.editing === a.name;
    const ta = editing && h("textarea", { class: "biz-edit", rows: 12, "aria-label": `${a.name}'s job` });
    if (ta) ta.value = a.job;
    const letGo = h("button", { class: "btn ghost", type: "button", onclick: async () => {
      if (agentUi.letGo !== a.name) { agentUi.letGo = a.name; letGo.textContent = "Let go? Click again"; return; }
      agentUi.letGo = null;
      try { await api(`${realmPath(realm)}/agents/${enc(a.name)}`, undefined, "DELETE"); flash(`${title(a.name)} left the team`); }
      catch (e) { flash(e.message, true); }
      again();
    } }, "Let go");
    return h("div", { class: "card biz-card agent" },
      h("div", { class: "biz-head" },
        h("span", { class: "agent-mark", "aria-hidden": "true" }, a.name.slice(0, 1).toUpperCase()),
        h("h2", null, title(a.name)),
        h("span", { class: "grow" }),
        h("span", { class: "muted small", title: a.unpriced ? `${a.unpriced} call(s) to a model without a known price are not counted` : "" },
          `${a.month_cost.toFixed(2)} $${a.unpriced ? " +" : ""} this month`)),
      editing
        ? [ta, h("div", { class: "biz-actions" },
          h("button", { class: "btn primary", type: "button", onclick: () => save(a.name, ta.value) }, "Save job"),
          h("button", { class: "btn", type: "button", onclick: () => { agentUi.editing = null; again(); } }, "Cancel"))]
        : h("div", { class: "prose agent-job", html: mdc(a.job) }),
      h("div", { class: "agent-section" }, h("div", { class: "strong small" }, "Routines"),
        a.routines.map((o) => h("div", { class: "biz-row" },
          h("a", { href: `#/r/${enc(realm)}/routines` }, o.id, !o.enabled && h("span", { class: "muted" }, " · paused")),
          h("span", { class: "row-start" }, h("span", { class: "muted small" }, o.trigger),
            h("button", { class: "btn small", type: "button", onclick: async () => {
              try {
                const { id } = await api(`${realmPath(realm)}/routines/${enc(o.id)}/run`, {});
                go(`#/r/${enc(realm)}/run/${enc(id)}`);
              } catch (e) { flash(e.message, true); }
            } }, "Run now")))),
        !a.routines.length && h("div", { class: "muted small" }, "No routine yet: it only works when a routine names it."),
        h("button", { class: "btn ghost small agent-add-routine", type: "button", onclick: () => {
          routineUi.form = { bot: a.name };
          go(`#/r/${enc(realm)}/routines`);
        } }, icon("plus"), a.routines.length ? "Another routine" : "Give it a routine")),
      a.routines.map((o) => h("div", { class: "muted small agent-limits" },
        `${o.id}: tools ${o.tools ? (o.tools.length ? o.tools.join(", ") : "none of the business's") : "all of the business's"}`,
        ` · alone: ${o.approvals.length ? o.approvals.join(", ") : "only what the business allows"} · budget: ${o.budget}`)),
      a.tasks.length > 0 && h("div", { class: "agent-section" }, h("div", { class: "strong small" }, "Last tasks"),
        a.tasks.map((t) => h("a", { class: "biz-row", href: `#/r/${enc(realm)}/run/${enc(t.id)}` },
          h("span", { class: "biz-clip" }, runName({ objective: t.objective })), h("span", { class: "muted small" }, `${t.status} · ${ago(t.updated_at)}`)))),
      !editing && h("div", { class: "biz-actions" },
        h("button", { class: "btn", type: "button", onclick: () => { agentUi.editing = a.name; again(); } }, "Edit job"),
        letGo),
      h("div", { class: "muted small" }, `Job version ${a.version}`));
  };

  const hire = () => {
    const t = agentUi.hiring;
    if (t) {
      const name = h("input", { type: "text", value: t.name, "aria-label": "Name", autocomplete: "off", spellcheck: "false" });
      name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
      const job = h("textarea", { class: "biz-edit", rows: 14, "aria-label": "Job" });
      job.value = t.job;
      return h("div", { class: "card biz-card" }, h("h2", null, `Hire ${t.title ? "a " + t.title.toLowerCase() : "an agent"}`),
        field("Name", name, "Lowercase letters, digits and dashes."),
        field("Job", job, "What it is for and how it works, in your words. It reads the strategy and the profile too."),
        h("div", { class: "biz-actions" },
          h("button", { class: "btn primary", type: "button", onclick: () => save(name.value.trim(), job.value) }, "Hire"),
          h("button", { class: "btn", type: "button", onclick: () => { agentUi.hiring = null; again(); } }, "Cancel")));
    }
    return h("div", { class: "card biz-card" }, h("h2", null, "Hire an agent"),
      h("div", { class: "agent-templates" },
        AGENT_TEMPLATES.map((x) => h("button", { class: "agent-template", type: "button", onclick: () => { agentUi.hiring = x; again(); } },
          h("span", { class: "strong" }, x.title), h("span", { class: "muted small" }, x.desc))),
        h("button", { class: "agent-template", type: "button", onclick: () => { agentUi.hiring = { name: "", title: "", job: "" }; again(); } },
          h("span", { class: "strong" }, "Your own"), h("span", { class: "muted small" }, "Write its job from scratch."))));
  };

  if (!d.agents.length && !agentUi.hiring) inner.append(h("p", { class: "muted" }, "No agent yet. Hire one to start: give it a job, then a routine that says when it works."));
  inner.append(h("div", { class: "agent-grid" }, d.agents.map(card)), hire());
}
