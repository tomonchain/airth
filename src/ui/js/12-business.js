// ---------- the business: Steering and Pipelines (decision 37) ----------

/** What the business pages keep between renders: when each draft was first shown, the one being edited. */
const bizUi = { shown: new Map(), editing: null, strategy: null };

async function loadBusiness() {
  return api(`${realmPath(app.realm)}/business`);
}

/** "1 h 40", "12 min", "42 s" */
function duration(secs) {
  if (secs < 60) return `${secs} s`;
  if (secs < 3600) return `${Math.floor(secs / 60)} min`;
  return `${Math.floor(secs / 3600)} h ${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}`;
}

/** A pipeline's last run, in a few words. */
function lastRun(p) {
  const r = p.runs && p.runs[0];
  if (p.error) return h("span", { class: "biz-bad" }, "refused");
  if (!r) return h("span", { class: "muted" }, p.schedule ? "not run yet" : "runs by hand");
  return r.ok
    ? h("span", { class: "biz-ok" }, `✓ ${ago(r.started_at)}`)
    : h("span", { class: "biz-bad", title: r.error || "" }, `✗ failed ${ago(r.started_at)}`);
}

/** A draft waiting for the owner: approve, edit or reject; `after` redraws the page. */
function draftCard(realm, draft, after) {
  if (!bizUi.shown.has(draft.id)) bizUi.shown.set(draft.id, Date.now());
  const editing = bizUi.editing === draft.id;
  const src = draft.source || {};
  const decide = async (body) => {
    const shown = bizUi.shown.get(draft.id) || Date.now();
    try {
      await api(`${realmPath(realm)}/drafts/${draft.id}`, { ...body, revision: draft.revision, seconds: Math.round((Date.now() - shown) / 1000) });
      bizUi.shown.delete(draft.id);
      bizUi.editing = null;
    } catch (e) { flash(e.message, true); }
    after();
  };
  const ta = editing && h("textarea", { class: "biz-edit", rows: 5 }, draft.text);
  if (ta) ta.value = draft.text;
  return h("div", { class: "biz-draft" },
    h("div", { class: "biz-meta" }, h("span", { class: "biz-tag" }, draft.kind), ` ${draft.pipeline} · ${ago(draft.created_at)}`),
    editing ? ta : h("div", { class: "biz-text" }, draft.text),
    // Feeds are external: only web links are followed.
    /^https?:\/\//i.test(src.link || "") && h("a", { class: "biz-source small", href: src.link, target: "_blank", rel: "noopener" }, src.title || src.link),
    h("div", { class: "biz-actions" }, editing
      ? [h("button", { class: "btn primary", type: "button", onclick: () => decide({ action: "edit", text: ta.value }) }, "Save"),
        h("button", { class: "btn", type: "button", onclick: () => { bizUi.editing = null; after(); } }, "Cancel")]
      : [h("button", { class: "btn primary", type: "button", onclick: () => decide({ action: "approve" }) }, "Approve"),
        h("button", { class: "btn", type: "button", onclick: () => { bizUi.editing = draft.id; after(); } }, "Edit"),
        h("button", { class: "btn", type: "button", onclick: () => decide({ action: "reject", reason: "" }) }, "Reject")]));
}

/** A run waiting for the owner's approval, answered right here. */
function approvalItem(realm, a, after) {
  const answer = async (approve) => {
    try { await api(`${runPath(realm, a.run)}/approval`, { approval_id: a.approval_id, approve }); }
    catch (e) { flash(e.message, true); }
    after();
  };
  return h("div", { class: "biz-draft" },
    h("div", { class: "biz-meta" }, h("span", { class: "biz-tag" }, a.capability), ` ${a.routine || "task"} · ${a.tool}`),
    h("a", { class: "strong", href: `#/r/${enc(realm)}/run/${enc(a.run)}` }, runName({ objective: a.objective })),
    h("pre", { class: "out small biz-clip" }, a.preview),
    a.warnings.map((w) => h("div", { class: "approval-warn" }, "⚠ " + w)),
    h("div", { class: "biz-actions" },
      h("button", { class: "btn primary", type: "button", onclick: () => answer(true) }, "Approve"),
      h("button", { class: "btn", type: "button", onclick: () => answer(false) }, "Deny")));
}

/** The owner's page (decision 37): decide, look at what needs attention, steer. */
async function renderSteering(main) {
  const realm = app.realm;
  page(main, "Steering", null);
  const inner = $(".page-inner", main);
  let d;
  try { d = await api(`${realmPath(realm)}/steering`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "steering" || app.realm !== realm) return;
  const again = () => { if (app.route.view === "steering") { main.replaceChildren(); renderSteering(main); } };
  const waiting = d.approvals.length + d.drafts.length;
  inner.append(h("p", { class: "sub" }, "Decide what waits for you, see what the team did, and set the direction."));

  const w = d.week;
  const stat = (label, value, detail) => h("div", { class: "stat" },
    h("div", { class: "stat-label" }, label), h("div", { class: "stat-value" }, value), h("div", { class: "stat-detail" }, detail));
  const stats = h("div", { class: "stats" },
    stat("Waiting for you", String(waiting), waiting ? "decisions below" : "nothing to decide"),
    stat("Delivered", String(d.delivered.length), "finished this week"),
    stat("Your time", duration(w.owner_seconds), "this week"),
    stat("Model cost", `${w.model_cost.toFixed(2)} $${w.unpriced_calls ? " +" : ""}`,
      w.unpriced_calls ? `${w.unpriced_calls} call(s) without a known price` : "this week"));

  const decide = waiting > 0 && h("div", { class: "card biz-card decide" }, h("h2", null, `Decide · ${waiting}`),
    d.approvals.map((a) => approvalItem(realm, a, again)), d.drafts.map((x) => draftCard(realm, x, again)));
  const attention = h("div", { class: "card biz-card" }, h("h2", null, "Needs attention"),
    d.attention.length
      ? d.attention.map((x) => h(x.run ? "a" : "div", { class: "biz-row", href: x.run ? `#/r/${enc(realm)}/run/${enc(x.run)}` : null },
        h("span", { class: "biz-bad biz-clip" }, x.title), x.at && h("span", { class: "muted small" }, ago(x.at))))
      : h("div", { class: "muted small" }, "All went well this week."));

  // The strategy reads as text; it opens for editing on demand.
  const editing = bizUi.strategy === realm || !d.strategy.trim();
  const text = editing && h("textarea", { class: "biz-edit", rows: 10, "aria-label": "Strategy",
    placeholder: "Your goals, the line every agent follows, and what the team may do alone.\n\nFor example:\n- One article a week on the blog.\n- Explain, never hype. Facts and numbers first.\n- Ask me before publishing or spending money." });
  if (text) text.value = d.strategy;
  const save = async () => {
    try {
      await api(`${realmPath(realm)}/strategy`, { text: text.value }, "PUT");
      bizUi.strategy = null;
      flash("Strategy saved: every agent reads it from its next run");
    } catch (e) { return flash(e.message, true); }
    again();
  };
  const strategy = h("div", { class: "card biz-card" },
    h("div", { class: "biz-head" }, h("h2", null, "Strategy"), h("span", { class: "grow" }),
      !editing && h("button", { class: "btn small", type: "button", onclick: () => { bizUi.strategy = realm; again(); } }, icon("pencil"), "Edit")),
    editing
      ? [h("p", { class: "muted small" }, "Every agent of this business reads it before it works."), text,
        h("div", { class: "biz-actions" }, h("button", { class: "btn primary", type: "button", onclick: save }, "Save strategy"),
          d.strategy.trim() && h("button", { class: "btn", type: "button", onclick: () => { bizUi.strategy = null; again(); } }, "Cancel"))]
      : h("div", { class: "prose strategy-text", html: mdc(d.strategy) }));

  // Drafts' numbers matter once there are drafts.
  const decided = w.approved + w.rejected;
  const week = decided > 0 && h("div", { class: "card biz-card" }, h("h2", null, "Drafts this week"),
    h("div", { class: "biz-row" }, h("span", null, "Sent unedited"),
      h("span", null, w.approved ? `${Math.round((100 * w.unedited) / w.approved)} % of ${w.approved}` : "–")),
    h("div", { class: "biz-row" }, h("span", null, "Rejected"), h("span", null, `${w.rejected} of ${decided}`)));
  const delivered = h("div", { class: "card biz-card" }, h("h2", null, "Delivered this week"),
    d.delivered.length
      ? d.delivered.map((x) => h("a", { class: "biz-delivered", href: `#/r/${enc(realm)}/run/${enc(x.run)}` },
        h("div", { class: "biz-head" }, h("span", { class: "strong biz-clip" }, x.title), h("span", { class: "grow" }),
          h("span", { class: "muted small" }, `${x.agent || x.routine || "you"} · ${ago(x.at)}`)),
        x.report && h("div", { class: "muted small biz-clip" }, x.report)))
      : h("div", { class: "muted small" }, "Nothing finished this week yet."));
  const team = h("div", { class: "card biz-card" }, h("h2", null, "The team"),
    d.team.length
      ? d.team.map((a) => h("a", { class: "biz-row", href: `#/r/${enc(realm)}/agents` },
        h("span", null, h("span", { class: "agent-mark small", "aria-hidden": "true" }, a.name.slice(0, 1).toUpperCase()), " ", a.name),
        h("span", { class: a.running ? "biz-ok small" : "muted small" },
          a.running ? "working now"
            : !a.routines ? "no routine yet"
            : a.next_at ? `next ${when(a.next_at)}`
            : a.last_status ? `${a.last_status} ${ago(a.last_at)}` : "by hand")))
      : h("a", { class: "muted small", href: `#/r/${enc(realm)}/agents` }, "No agent yet: hire one in Agents."));
  inner.append(stats, decide, h("div", { class: "biz-grid" }, h("div", null, delivered, strategy), h("div", null, team, attention, week)));
}

/** The business's pipelines, on the Routines page: fixed sequences declared in business/pipelines/. */
async function drawPipelines(inner, realm) {
  let d;
  try { d = await loadBusiness(); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "routines" || app.realm !== realm || !d.pipelines.length) return;
  inner.append(h("h2", { class: "section-head" }, "Pipelines"),
    h("p", { class: "muted small" }, "Fixed sequences of steps, declared as files in business/pipelines/."));
  for (const p of d.pipelines) {
    const run = h("button", { class: "btn", type: "button", disabled: !!p.error, onclick: async () => {
      try { await api(`${realmPath(realm)}/pipelines/${enc(p.name)}/run`, {}); flash(`${p.name} started: its run shows here when it ends`); } catch (e) { flash(e.message, true); }
    } }, "Run now");
    const last = p.runs[0];
    inner.append(h("div", { class: "card biz-card" },
      h("div", { class: "biz-head" }, h("h2", null, p.name), lastRun(p), h("span", { class: "grow" }), run),
      p.error ? h("div", { class: "biz-bad small" }, p.error) : [
        p.description && h("p", { class: "muted" }, p.description),
        p.schedule && h("div", { class: "muted small" }, `${scheduleText(p.schedule)}${p.next_at ? " · next " + when(p.next_at) : ""}`),
        h("ol", { class: "biz-steps small" }, p.steps.map((s) => h("li", null, s))),
      ],
      p.runs.length > 0 && h("div", { class: "biz-runs" }, p.runs.map((r) => h("div", { class: "biz-row small" },
        h("span", null, new Date(r.started_at).toLocaleString([], { weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })),
        r.ok ? h("span", null, `${r.items_kept} kept of ${r.items_read}`) : h("span", { class: "biz-bad biz-clip", title: r.error || "" }, r.error || "failed")))),
      last && last.ok && last.notes && h("div", { class: "muted small biz-clip" }, last.notes),
      last && last.report && h("details", { class: "biz-report" }, h("summary", null, "Last report"), h("div", { class: "prose", html: mdc(last.report) }))));
  }
}

/** A schedule in words: "weekdays 09:00" → "Weekdays at 09:00". */
function scheduleText(every) {
  const t = (every || "").trim().toLowerCase();
  const m = t.match(/^every (\d+)\s*(m|min|mins|h|hr|hrs|d|day|days)$/);
  if (m) {
    const n = Number(m[1]);
    const unit = m[2][0] === "m" ? "min" : m[2][0] === "h" ? (n === 1 ? "hour" : "hours") : (n === 1 ? "day" : "days");
    return n === 1 && unit !== "min" ? `Every ${unit}` : `Every ${n} ${unit}`;
  }
  const at = t.match(/^(\S+) (\d{1,2}:\d{2})$/);
  if (!at) return every;
  const days = { daily: "Every day", weekdays: "Weekdays", weekends: "Weekends" }[at[1]]
    || at[1].split(",").map((d) => d.slice(0, 1).toUpperCase() + d.slice(1, 3)).join(", ");
  return `${days} at ${at[2]}`;
}
