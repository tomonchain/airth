// Routines (docs/design/mockups/routine-editor.html), read-only until the
// editor of milestone 5: each routine's graph, block by block, its last
// runs, and a dry run on demand (decision 49). A routine is changed in the
// business folder.

const routines = {
  data: {}, // by business, once loaded
  open: {}, // the routine open, by business
  block: {}, // the block whose details are open, by routine
  dry: {}, // the last dry run's report or error, by routine
  running: {}, // routines whose dry run is going
};

const BLOCK_TYPES = { read: "Read", ai: "AI", condition: "Condition", approval: "Approval", act: "Act" };
const DAYS = { mon: "Mondays", tue: "Tuesdays", wed: "Wednesdays", thu: "Thursdays", fri: "Fridays", sat: "Saturdays", sun: "Sundays" };

/** A schedule as the founder reads it: "Every day 07:30", "Mondays 08:00". */
function scheduleText(s) {
  const [when, at] = String(s).split(/\s+/);
  if (when === "daily") return ["Every day", at];
  if (when === "weekdays") return ["Weekdays", at];
  if (when === "every") return [`Every ${at}`, ""];
  const days = when.split(",").map((d) => DAYS[d]).filter(Boolean);
  return days.length ? [days.join(", "), at] : [s, ""];
}

/** A block's title and one line on what it does, from its file. */
function blockText(b) {
  switch (b.type) {
    case "read":
      if (b.source === "business") return ["Business records", b.records === "week" ? "The last 7 days" : b.records];
      if (b.source === "rss") return [`${b.feeds.length} news feed${b.feeds.length === 1 ? "" : "s"}`,
        [b.max_age_days && `last ${b.max_age_days} days`, b.dedupe && "no repeats", b.max && `at most ${b.max}`].filter(Boolean).join(", ")];
      return [b.source, b.action || b.url || ""];
    case "ai": {
      const keep = [b.over === "list" && "all at once", b.model === "fast" && "fast model", b.keep && `keeps ${b.keep}`, b.limit && `best ${b.limit}`, b.distinct && b.distinct.length && `one per ${b.distinct.join(" and ")}`];
      return [b.id, [b.prompt.replace(/^prompts\//, ""), ...keep].filter(Boolean).join(" · ")];
    }
    case "condition": return [b.id, `if ${b.if.field} ${b.if.op}${b.if.value === undefined ? "" : ` ${JSON.stringify(b.if.value)}`}`];
    case "approval": return ["You, in Needs you", [b.editable ? "editable" : "approve or reject", b.expires_after && `${b.expires_after} h`].filter(Boolean).join(", ")];
    case "act": return [b.action, b.id];
    default: return [b.id, ""];
  }
}

/** Routines, in the Workspace layout (decision 72): the routines, the open
    one's graph, its details. */
function RoutinesPage() {
  const realm = next.realm;
  const data = loaded(routines.data, "graphs");
  const header = { label: realm, title: "Routines" };
  const only = (main) => html`<${Workspace} header=${header} main=${main} />`;
  if (!data) return only(html`<p class="lede">Loading…</p>`);
  if (data.error) return only(html`<p class="note bad">${data.error}</p>`);
  const list = data.routines || [];
  if (!list.length) return only(html`<div class="empty"><strong>No routine yet</strong>
    <span>A routine is a file in routines/ in the business folder: a trigger, then its blocks.</span></div>`);
  if (!list.some((r) => r.name === routines.open[realm])) routines.open[realm] = list[0].name;
  const r = list.find((x) => x.name === routines.open[realm]);
  const key = `${realm}/${r.name}`;
  const g = r.graph;
  const summary = g && html`${g.description ? g.description.replace(/([^.!?])$/, "$1.") : "No description."} Owned by <strong>${r.agent_title || g.agent}</strong>.`;
  const actions = g && html`
    <button type="button" class="button-plain" aria-pressed=${routines.editing ? "true" : "false"} onClick=${() => { routines.editing = !routines.editing; renderPanel(); }}>Edit</button>
    <button type="button" class="primary" disabled=${!!routines.running[key]} onClick=${() => dryRun(r)}>${routines.running[key] ? "Running…" : "Dry run"}</button>`;
  return html`<${Workspace} header=${{ ...header, summary, actions }}
    list=${html`<${RoutineList} list=${list} open=${r.name} />`}
    main=${html`<${Graph} r=${r} />`}
    details=${html`<${RoutineDetails} r=${r} />`} />`;
}

function RoutineList({ list, open }) {
  return html`<div class="pane-stack" role="tablist" aria-label="Routines">${list.map((x) => html`
    <button type="button" role="tab" class="routine-pick" aria-selected=${x.name === open ? "true" : "false"}
        onClick=${() => { routines.open[next.realm] = x.name; renderPanel(); }}>
      <span class=${`dot ${x.error ? "bad" : x.runs[0] && !x.runs[0].ok ? "warn" : "ok"}`} aria-hidden="true"></span>
      <span class="mono">${x.name}</span>
    </button>`)}</div>`;
}

function Graph({ r }) {
  const key = `${next.realm}/${r.name}`;
  const report = routines.dry[key];
  const g = r.graph;
  if (!g) return html`<p class="note bad">${r.error}</p>`;
  return html`<section class="graph" aria-label="Routine graph">
    <ol class="flow">
      <li class="node"><${NodeBody} type="Trigger" title=${triggerText(g.trigger, r.next_at)[0]} sub=${triggerText(g.trigger, r.next_at)[1]} /></li>
      ${g.blocks.map((b) => {
        const [title, sub] = blockText(b);
        const ran = report && report.blocks && report.blocks.find((x) => x.id === b.id);
        const current = routines.block[key] === b.id;
        return html`<li class="node"><button type="button" class="node-button" aria-pressed=${current ? "true" : "false"}
            onClick=${() => { routines.block[key] = current ? null : b.id; renderPanel(); }}>
          <${NodeBody} type=${BLOCK_TYPES[b.type] || b.type} title=${title} sub=${sub} />
          ${report && report.blocks && html`<span class=${`node-out ${!ran ? "none" : ran.notes.length ? "warn" : "ok"}`}>
            ${!ran ? "not reached" : `${ran.notes.length ? "!" : "✓"} ${ran.items_in} → ${ran.items_out} · ${ran.secs.toFixed(1)} s`}</span>`}
        </button></li>`;
      })}
    </ol>
    <${BlockReport} report=${report} open=${routines.block[key]} />
    <p class="graph-foot">A dry run reads and asks the model for real, takes every approval as given, and only shows what each act would do. Nothing is sent.</p>
  </section>`;
}

function NodeBody({ type, title, sub }) {
  return html`<span class="node-type">${type}</span><span class="node-title">${title}</span>${sub && html`<span class="node-sub">${sub}</span>`}`;
}

function triggerText(t, nextAt) {
  if (t.schedule) {
    const [when, at] = scheduleText(t.schedule);
    return [when, [at, nextAt && `next ${clock(nextAt)}`].filter(Boolean).join(" · ")];
  }
  if (t.webhook) return ["Webhook", t.webhook];
  // Decision 63: a post link the founder sends the bot.
  if (t.telegram) return ["From Telegram", t.telegram === "x_post" ? "a post link you send" : t.telegram];
  return ["By hand", ""];
}

async function dryRun(r) {
  const key = `${next.realm}/${r.name}`;
  routines.running[key] = true;
  routines.dry[key] = null;
  renderPanel();
  try {
    routines.dry[key] = await api(`realms/${enc(next.realm)}/graphs/${enc(r.name)}/dry-run`, {});
    routines.dry[key].at = new Date().toISOString();
  } catch (e) {
    routines.dry[key] = { error: e.message };
  }
  routines.running[key] = false;
  renderPanel();
}

function RoutineDetails({ r }) {
  const key = `${next.realm}/${r.name}`;
  const report = routines.dry[key];
  return html`<div class="details" aria-labelledby="routine-details">
    <h2 id="routine-details">Routine details</h2>
    ${routines.editing && html`<${InDevelopment} part="routine-edit" />`}
    ${routines.running[key] ? html`<p class="note">Dry run going: the feeds are read and the model asked. It can take a minute.</p>`
      : report && report.error ? html`<p class="note bad">${report.error}</p>`
      : report ? html`<${DryReport} report=${report} />`
      : html`<${LastRuns} r=${r} />`}
  </div>`;
}

function DryReport({ report }) {
  const notes = report.blocks.reduce((n, b) => n + b.notes.length, 0);
  return html`<div class="details-head"><span class=${`state ${notes ? "warn" : "ok"}`}>${notes ? `${notes} note${notes === 1 ? "" : "s"}` : "Done"}</span><span>Dry run · ${clock(report.at)}</span></div>
    <div class="steps"><span class="steps-label">Steps</span>${report.blocks.map((b, n) => html`<div class="step">
      <span>${n + 1} · ${b.id}</span><span class=${`step-out ${b.notes.length ? "waiting" : "ok"}`}>${b.items_in} → ${b.items_out} · ${b.secs.toFixed(1)} s</span></div>`)}</div>
    <div class="tile"><span class="tile-label">Model cost</span><span class="tile-value">${report.cost.replace(/^cost: /, "")}</span></div>`;
}

/** What the open block did in the last dry run, under the graph. */
function BlockReport({ report, open }) {
  if (!report || !report.blocks) return null;
  if (!open) return html`<p class="note">Choose a block to see what it did.</p>`;
  const ran = report.blocks.find((b) => b.id === open);
  if (!ran) return html`<p class="note">${open} was not reached in this dry run.</p>`;
  return html`<section class="block-report" aria-label=${`What ${ran.id} did`}>
    <h2>What ${ran.id} did</h2>
    ${ran.lines.map((l) => html`<pre class="line">${l}</pre>`)}
    ${ran.notes.map((l) => html`<pre class="line warn">${l}</pre>`)}
    ${!ran.lines.length && !ran.notes.length && html`<p class="note">Nothing to say: its items went on.</p>`}
  </section>`;
}

/** How late a scheduled run started, past the 15 minutes of grace. */
function late(run) {
  if (!run.due_at) return null;
  const secs = (new Date(run.at) - new Date(run.due_at)) / 1000;
  return secs > 15 * 60 ? minutes(Math.round(secs)) : null;
}

function LastRuns({ r }) {
  return html`<div class="steps"><span class="steps-label">Last runs</span>
      ${!r.runs.length && html`<p class="note">No run yet. Each scheduled run shows here once it ends.</p>`}
      ${r.runs.map((x) => html`<div class="step">
        <span>${clock(x.at)}${late(x) && html`<span class="late"> · ${late(x)} late</span>`}</span>
        <span class=${`step-out ${x.ok ? "ok" : "failed"}`} title=${x.error || ""}>${x.ok ? "Done" : "Failed"}</span></div>`)}
    </div>
    <div class="tile"><span class="tile-label">Model cost, last 7 days</span><span class="tile-value">${money(r.cost_usd, r.unpriced)}</span></div>
    <p class="note">Run it dry to see each block's items, what it would ask you and what it would do.</p>`;
}
