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
      const keep = [b.over === "list" && "all at once", b.keep && `keeps ${b.keep}`, b.limit && `best ${b.limit}`, b.distinct && b.distinct.length && `one per ${b.distinct.join(" and ")}`];
      return [b.id, [b.prompt.replace(/^prompts\//, ""), ...keep].filter(Boolean).join(" · ")];
    }
    case "condition": return [b.id, `if ${b.if.field} ${b.if.op}${b.if.value === undefined ? "" : ` ${JSON.stringify(b.if.value)}`}`];
    case "approval": return ["You, in Needs you", [b.editable ? "editable" : "approve or reject", b.expires_after && `${b.expires_after} h`].filter(Boolean).join(", ")];
    case "act": return [b.action, b.id];
    default: return [b.id, ""];
  }
}

function routinesPage() {
  const realm = next.realm;
  const data = loaded(routines.data, "graphs");
  const head = (title, sub) => pageHead(realm, title, sub);
  if (!data) return h("div", { class: "column" }, head("Routines"), h("p", { class: "lede" }, "Loading…"));
  if (data.error) return h("div", { class: "column" }, head("Routines"), h("p", { class: "note bad" }, data.error));
  const list = data.routines || [];
  if (!list.length) return h("div", { class: "column" }, head("No routine yet."),
    h("div", { class: "empty" }, h("strong", {}, "No routine yet"),
      h("span", {}, "A routine is a file in routines/ in the business folder: a trigger, then its blocks. The editor comes later.")));
  if (!list.some((r) => r.name === routines.open[realm])) routines.open[realm] = list[0].name;
  const r = list.find((x) => x.name === routines.open[realm]);
  return h("div", { class: "routines" }, graphSection(list, r), routineDetails(r));
}

function graphSection(list, r) {
  const key = `${next.realm}/${r.name}`;
  const report = routines.dry[key];
  const g = r.graph;
  return h("section", { class: "graph", "aria-label": "Routine graph" },
    h("div", { class: "graph-head" },
      h("div", { class: "picker", role: "tablist", "aria-label": "Routines" }, list.map((x) =>
        h("button", { type: "button", role: "tab", class: "pick", "aria-selected": x.name === r.name ? "true" : "false",
            onclick: () => { routines.open[next.realm] = x.name; renderPanel(); } },
          h("span", { class: `dot ${x.error ? "bad" : x.runs[0] && !x.runs[0].ok ? "warn" : "ok"}`, "aria-hidden": "true" }),
          h("span", { class: "mono" }, x.name)))),
      g && h("button", { type: "button", class: "primary", disabled: routines.running[key] ? true : null, onclick: () => dryRun(r) },
        routines.running[key] ? "Running…" : "Dry run")),
    g ? [
      h("p", { class: "graph-sub" },
        g.description ? g.description.replace(/([^.!?])$/, "$1.") : "No description.",
        " Owned by ", h("strong", {}, r.agent_title || g.agent), "."),
      h("ol", { class: "flow" },
        h("li", { class: "node" }, ...nodeBody("Trigger", ...triggerText(g.trigger, r.next_at))),
        g.blocks.map((b) => {
          const [title, sub] = blockText(b);
          const ran = report && report.blocks && report.blocks.find((x) => x.id === b.id);
          const current = routines.block[key] === b.id;
          return h("li", { class: "node" },
            h("button", { type: "button", class: "node-button", "aria-pressed": current ? "true" : "false",
                onclick: () => { routines.block[key] = current ? null : b.id; renderPanel(); } },
              ...nodeBody(BLOCK_TYPES[b.type] || b.type, title, sub),
              report && report.blocks && h("span", { class: `node-out ${!ran ? "none" : ran.notes.length ? "warn" : "ok"}` },
                !ran ? "not reached" : `${ran.notes.length ? "!" : "✓"} ${ran.items_in} → ${ran.items_out} · ${ran.secs.toFixed(1)} s`)));
        })),
      blockReport(report, routines.block[key]),
    ] : h("p", { class: "note bad" }, r.error),
    h("p", { class: "graph-foot" }, "A dry run reads and asks the model for real, takes every approval as given, and only shows what each act would do. Nothing is sent."));
}

function nodeBody(type, title, sub) {
  return [h("span", { class: "node-type" }, type), h("span", { class: "node-title" }, title), sub && h("span", { class: "node-sub" }, sub)];
}

function triggerText(t, nextAt) {
  if (t.schedule) {
    const [when, at] = scheduleText(t.schedule);
    return [when, [at, nextAt && `next ${clock(nextAt)}`].filter(Boolean).join(" · ")];
  }
  if (t.webhook) return ["Webhook", t.webhook];
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

function routineDetails(r) {
  const key = `${next.realm}/${r.name}`;
  const report = routines.dry[key];
  return h("aside", { class: "details", "aria-labelledby": "routine-details" },
    h("h2", { id: "routine-details" }, "Routine details"),
    routines.running[key] ? h("p", { class: "note" }, "Dry run going: the feeds are read and the model asked. It can take a minute.")
      : report && report.error ? h("p", { class: "note bad" }, report.error)
      : report ? dryReport(report)
      : lastRuns(r));
}

function dryReport(report) {
  const notes = report.blocks.reduce((n, b) => n + b.notes.length, 0);
  return [
    h("div", { class: "details-head" },
      h("span", { class: `state ${notes ? "warn" : "ok"}` }, notes ? `${notes} note${notes === 1 ? "" : "s"}` : "Done"),
      h("span", {}, `Dry run · ${clock(report.at)}`)),
    h("div", { class: "steps" },
      h("span", { class: "steps-label" }, "Steps"),
      report.blocks.map((b, n) => h("div", { class: "step" },
        h("span", {}, `${n + 1} · ${b.id}`),
        h("span", { class: `step-out ${b.notes.length ? "waiting" : "ok"}` }, `${b.items_in} → ${b.items_out} · ${b.secs.toFixed(1)} s`)))),
    h("div", { class: "tile" }, h("span", { class: "tile-label" }, "Model cost"), h("span", { class: "tile-value" }, report.cost.replace(/^cost: /, ""))),
  ];
}

/** What the open block did in the last dry run, under the graph. */
function blockReport(report, open) {
  if (!report || !report.blocks) return null;
  if (!open) return h("p", { class: "note" }, "Choose a block to see what it did.");
  const ran = report.blocks.find((b) => b.id === open);
  if (!ran) return h("p", { class: "note" }, `${open} was not reached in this dry run.`);
  return h("section", { class: "block-report", "aria-label": `What ${ran.id} did` },
    h("h2", {}, `What ${ran.id} did`),
    ran.lines.map((l) => h("pre", { class: "line" }, l)),
    ran.notes.map((l) => h("pre", { class: "line warn" }, l)),
    !ran.lines.length && !ran.notes.length && h("p", { class: "note" }, "Nothing to say: its items went on."));
}

/** How late a scheduled run started, past the 15 minutes of grace. */
function late(run) {
  if (!run.due_at) return null;
  const secs = (new Date(run.at) - new Date(run.due_at)) / 1000;
  return secs > 15 * 60 ? minutes(Math.round(secs)) : null;
}

function lastRuns(r) {
  return [
    h("div", { class: "steps" },
      h("span", { class: "steps-label" }, "Last runs"),
      !r.runs.length && h("p", { class: "note" }, "No run yet. Each scheduled run shows here once it ends."),
      r.runs.map((x) => h("div", { class: "step" },
        h("span", {}, clock(x.at), late(x) && h("span", { class: "late" }, ` · ${late(x)} late`)),
        h("span", { class: `step-out ${x.ok ? "ok" : "failed"}`, title: x.error || "" }, x.ok ? "Done" : "Failed")))),
    h("div", { class: "tile" }, h("span", { class: "tile-label" }, "Model cost, last 7 days"),
      h("span", { class: "tile-value" }, money(r.cost_usd, r.unpriced))),
    h("p", { class: "note" }, "Run it dry to see each block's items, what it would ask you and what it would do."),
  ];
}
