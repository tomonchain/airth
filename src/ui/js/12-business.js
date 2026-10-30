// ---------- the business: Today and Pipelines (decision 32) ----------

/** What the business pages keep between renders: when each draft was first shown, the one being edited. */
const bizUi = { shown: new Map(), editing: null, data: null };

async function loadBusiness() {
  const realm = app.realm;
  const data = await api(`${realmPath(realm)}/business`);
  if (app.realm === realm) bizUi.data = data;
  return data;
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

function noBusiness(inner) {
  inner.append(h("div", { class: "card pad" },
    h("div", { class: "strong" }, "This realm does not run a business yet"),
    h("p", { class: "muted small" }, "Add a business/ folder to the realm's folder: profile.md says what the business is, archive/ holds its past work, and pipelines/ its recurring work, one JSON file each. The README shows the format; airth pipeline check checks it.")));
}

async function renderToday(main) {
  const realm = app.realm;
  page(main, "Today", null);
  const inner = $(".page-inner", main);
  let d;
  try { d = await loadBusiness(); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "today" || app.realm !== realm) return;
  inner.append(h("p", { class: "sub" },
    `${d.drafts.length ? `${d.drafts.length} draft${d.drafts.length === 1 ? "" : "s"} wait for you` : "Nothing waits for you"} · your time this week: ${duration(d.week.owner_seconds)}`));
  if (!d.has_business) return noBusiness(inner);

  const decide = async (draft, body) => {
    const shown = bizUi.shown.get(draft.id) || Date.now();
    try {
      await api(`${realmPath(realm)}/drafts/${draft.id}`, { ...body, revision: draft.revision, seconds: Math.round((Date.now() - shown) / 1000) });
      bizUi.shown.delete(draft.id);
      bizUi.editing = null;
    } catch (e) { flash(e.message, true); }
    if (app.route.view === "today") { main.replaceChildren(); renderToday(main); }
  };
  const draftCard = (draft) => {
    if (!bizUi.shown.has(draft.id)) bizUi.shown.set(draft.id, Date.now());
    const editing = bizUi.editing === draft.id;
    const src = draft.source || {};
    const ta = editing && h("textarea", { class: "biz-edit", rows: 5 }, draft.text);
    if (ta) ta.value = draft.text;
    return h("div", { class: "biz-draft" },
      h("div", { class: "biz-meta" }, h("span", { class: "biz-tag" }, draft.kind), ` ${draft.pipeline} · ${ago(draft.created_at)}`),
      editing ? ta : h("div", { class: "biz-text" }, draft.text),
      // Feeds are external: only web links are followed.
      /^https?:\/\//i.test(src.link || "") && h("a", { class: "biz-source small", href: src.link, target: "_blank", rel: "noopener" }, src.title || src.link),
      h("div", { class: "biz-actions" }, editing
        ? [h("button", { class: "btn primary", type: "button", onclick: () => decide(draft, { action: "edit", text: ta.value }) }, "Save"),
          h("button", { class: "btn", type: "button", onclick: () => { bizUi.editing = null; main.replaceChildren(); renderToday(main); } }, "Cancel")]
        : [h("button", { class: "btn primary", type: "button", onclick: () => decide(draft, { action: "approve" }) }, "Approve"),
          h("button", { class: "btn", type: "button", onclick: () => { bizUi.editing = draft.id; main.replaceChildren(); renderToday(main); } }, "Edit"),
          h("button", { class: "btn", type: "button", onclick: () => decide(draft, { action: "reject", reason: "" }) }, "Reject")]));
  };

  const review = h("div", { class: "card biz-card" }, h("h2", null, `To review · ${d.drafts.length}`),
    d.drafts.length ? d.drafts.map(draftCard) : h("div", { class: "muted small" }, "No draft waits for you."));
  const pipes = h("div", { class: "card biz-card" }, h("h2", null, "Pipelines"),
    d.pipelines.length
      ? d.pipelines.map((p) => h("a", { class: "biz-row", href: `#/r/${enc(realm)}/pipelines` }, h("span", null, p.name), lastRun(p)))
      : h("div", { class: "muted small" }, "None declared in business/pipelines/."));
  const w = d.week;
  const decided = w.approved + w.rejected;
  const week = h("div", { class: "card biz-card" }, h("h2", null, "Last 7 days"),
    h("div", { class: "biz-row" }, h("span", null, "Your time"), h("span", null, duration(w.owner_seconds))),
    h("div", { class: "biz-row" }, h("span", null, "Drafts sent unedited"),
      h("span", null, w.approved ? `${Math.round((100 * w.unedited) / w.approved)} % of ${w.approved}` : "–")),
    h("div", { class: "biz-row" }, h("span", null, "Drafts rejected"), h("span", null, decided ? `${w.rejected} of ${decided}` : "–")),
    h("div", { class: "biz-row" }, h("span", null, "Model cost"),
      h("span", { title: w.unpriced_calls ? `${w.unpriced_calls} call(s) to a model without a known price are not counted` : "" },
        `${w.model_cost.toFixed(2)} $${w.unpriced_calls ? " +" : ""}`)));
  const recent = d.notifications.length > 0 && h("div", { class: "card biz-card" }, h("h2", null, "Recent"),
    d.notifications.map((n) => h("div", { class: "biz-note" + (n.read ? "" : " unread") },
      h("span", { class: "inbox-kind " + n.kind }, KIND_MARK[n.kind] || "•"),
      h("div", { class: "grow" }, h("div", null, n.title), n.body && h("div", { class: "muted small biz-clip" }, n.body),
        h("div", { class: "muted small" }, ago(n.created_at))))));
  inner.append(h("div", { class: "biz-grid" }, h("div", null, review, recent), h("div", null, pipes, week)));
  const unread = d.notifications.filter((n) => !n.read).map((n) => n.id);
  if (unread.length) api("inbox/read", { ids: unread }).then(() => refreshInfo()).catch(() => {});
}

async function renderPipelines(main) {
  const realm = app.realm;
  page(main, "Pipelines", "The business's recurring work, as declared in business/pipelines/.");
  const inner = $(".page-inner", main);
  let d;
  try { d = await loadBusiness(); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "pipelines" || app.realm !== realm) return;
  if (!d.has_business) return noBusiness(inner);
  if (!d.pipelines.length) inner.append(h("div", { class: "card muted small pad" }, "No pipeline yet: add one in business/pipelines/."));
  for (const p of d.pipelines) {
    const run = h("button", { class: "btn", type: "button", disabled: !!p.error, onclick: async () => {
      try { await api(`${realmPath(realm)}/pipelines/${enc(p.name)}/run`, {}); flash(`${p.name} started: you will find it in Today`); } catch (e) { flash(e.message, true); }
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
