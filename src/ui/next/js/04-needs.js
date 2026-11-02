// Needs you: what waits for the founder in the business, by agent, with
// the founder's answers (decision 45). The same queue as Telegram.

const DEPARTMENT_NAMES = {
  leadership: "Leadership", marketing: "Marketing", sales: "Sales", support: "Support",
  product: "Product & tech", operations: "Operations", finance: "Finance & admin",
};
// Agent colours (DESIGN.md): Airth is green; an agent gets one of the
// others from its name, so it keeps it everywhere it appears.
const AGENT_COLOURS = ["#4C8DFF", "#F39C4A", "#8B6CF6", "#E07AB8", "#3FB8C9"];
function agentColour(name) {
  if (!name) return "#4FCB8F";
  let sum = 0;
  for (const c of name) sum = (sum * 31 + c.charCodeAt(0)) >>> 0;
  return AGENT_COLOURS[sum % AGENT_COLOURS.length];
}
/** How an agent is called: its title in its config, else its file name. */
const agentName = (a) => {
  if (!a) return "Airth";
  const d = needs.agents.find((x) => x.name === a);
  return (d && d.title) || a;
};
function avatar(agent, size = 40) {
  return h("span", { class: `avatar s${size}`, "aria-hidden": "true", style: `--agent: ${agentColour(agent)}` }, initial(agentName(agent)));
}

const needs = {
  realm: null,
  items: [],
  agents: [],
  agent: undefined, // the agent whose items are open; null for Airth's own
  find: "",
  editing: null, // the item whose text is being edited
  rejecting: null, // the item being rejected
  note: null, // what the last answer did
  shownAt: {}, // when each item was first shown, for the founder's time
  runs: {}, // item id → its run's blocks, once loaded
  error: null,
};

async function loadNeeds() {
  if (!next.realm) return;
  try {
    const data = await api(`realms/${enc(next.realm)}/queue`);
    Object.assign(needs, { realm: next.realm, items: data.items || [], agents: data.agents || [], error: null });
  } catch (e) {
    Object.assign(needs, { realm: next.realm, items: [], error: e.message });
  }
  renderRail();
  if (next.route.page === "needs-you") renderPanel();
}

function needsPage() {
  if (needs.realm !== next.realm) return h("div", { class: "column" }, h("p", { class: "lede" }, "Loading…"));
  const byAgent = new Map();
  for (const it of needs.items) {
    const key = it.agent || "";
    if (!byAgent.has(key)) byAgent.set(key, []);
    byAgent.get(key).push(it);
  }
  if (needs.agent === undefined || !byAgent.has(needs.agent || "")) needs.agent = byAgent.size ? [...byAgent.keys()][0] || null : undefined;
  const open = byAgent.get(needs.agent || "") || [];
  return h("div", { class: "needs" }, agentList(byAgent), thread(open), details(open.find((i) => i.routine && i.run)));
}

function agentList(byAgent) {
  const dept = (a) => { const d = needs.agents.find((x) => x.name === a); return a ? (d && d.department) || "" : "leadership"; };
  const groups = new Map();
  const q = needs.find.toLowerCase();
  for (const [a, items] of byAgent) {
    if (q && !agentName(a).toLowerCase().includes(q) && !items.some((i) => i.title.toLowerCase().includes(q))) continue;
    const d = dept(a);
    if (!groups.has(d)) groups.set(d, []);
    groups.get(d).push([a, items]);
  }
  return h("section", { class: "agents", "aria-label": "Agents waiting for you" },
    h("label", { for: "find", class: "sr-only" }, "Search"),
    h("input", { id: "find", type: "search", placeholder: "Search", value: needs.find,
      oninput: (e) => { needs.find = e.target.value; renderPanel(); $("#find").focus(); } }),
    !byAgent.size && h("div", { class: "empty" }, h("strong", {}, "Nothing waits for you"),
      h("span", {}, "Approvals, incidents and prepared actions show here as the agents ask.")),
    [...groups].map(([d, list]) => [
      h("span", { class: "group" }, DEPARTMENT_NAMES[d] || "No department yet"),
      list.map(([a, items]) => {
        const first = items[0];
        const incident = items.some((i) => i.kind === "incident");
        return h("a", { class: "agent", href: "#", "aria-current": (a || null) === needs.agent ? "true" : null,
            onclick: (e) => { e.preventDefault(); needs.agent = a || null; needs.editing = needs.rejecting = null; renderPanel(); } },
          h("span", { class: "badged" }, avatar(a), h("span", { class: incident ? "badge bad" : "badge" }, incident ? "!" : String(items.length))),
          h("span", { class: "who" },
            h("span", { class: "line" }, h("span", { class: "name" }, agentName(a)), h("span", { class: "time" }, clock(first.created_at))),
            h("span", { class: "sub" }, `${first.title} · ${first.text}`)));
      })]));
}

function thread(items) {
  if (!items.length) return h("section", { class: "thread" });
  const a = needs.agent;
  const d = needs.agents.find((x) => x.name === a);
  return h("section", { class: "thread", "aria-label": agentName(a) },
    h("header", { class: "thread-head" }, avatar(a, 34), h("h1", {}, agentName(a)),
      d && d.department && h("span", { class: "chip" }, DEPARTMENT_NAMES[d.department] || d.department)),
    h("div", { class: "thread-body" },
      needs.error && h("p", { class: "note bad" }, needs.error),
      needs.note && h("p", { class: "note" }, needs.note),
      items.map(card)));
}

const KIND_TAGS = { approval: "Needs you", delayed: "Done unless you cancel", prepared: "Prepared · you do it", incident: "Incident" };

function card(it) {
  needs.shownAt[it.id] = needs.shownAt[it.id] || Date.now();
  const editing = needs.editing === it.id;
  const rejecting = needs.rejecting === it.id;
  // An action a task of the founder's proposed (decision 56): the task
  // stopped there, and waits for nothing else.
  const what = it.task && it.action ? `If approved: ${it.action} is done, once. The task stopped here.`
    : it.kind === "incident" ? "Seen closes it."
    : it.kind === "prepared" ? "Airth does nothing: mark it done once you did it."
    : it.routine && !it.action ? `If approved: ${it.routine} goes on.`
    : it.action ? `If approved: ${it.action} is done, once.` : null;
  const link = it.source && it.source.link;
  return h("article", { class: "approval", "aria-label": it.title },
    h("div", { class: "approval-top" },
      h("span", { class: it.kind === "incident" ? "tag bad" : "tag" }, `${it.title} · ${KIND_TAGS[it.kind] || it.kind}`),
      it.execute_at && h("span", { class: "when" }, `Done at ${clock(it.execute_at)}`)),
    editing
      ? h("textarea", { id: `edit-${it.id}`, class: "edit", rows: 5, "aria-label": "New text" }, it.text)
      : h("p", { class: it.call ? "content call" : "content" }, it.text),
    h("div", { class: "meta" },
      link && h("span", {}, "Source: ", h("a", { href: link, target: "_blank", rel: "noopener noreferrer" }, (it.source.title || link))),
      it.task && h("span", {}, "From your task: ", h("a", { href: `/#/r/${enc(next.realm)}/run/${enc(it.task.id)}` }, it.task.title || it.task.id)),
      what && h("span", {}, what)),
    rejecting && h("input", { id: `why-${it.id}`, class: "why", type: "text", placeholder: "Why? (optional)", "aria-label": "Why you reject it" }),
    h("div", { class: "buttons" }, buttons(it, editing, rejecting)));
}

function buttons(it, editing, rejecting) {
  const b = (label, cls, onclick) => h("button", { type: "button", class: cls, onclick }, label);
  if (editing) return [
    b("Save the edit", "primary", () => answer(it, "edit", $(`#edit-${it.id}`).value)),
    b("Cancel", "plain", () => { needs.editing = null; renderPanel(); })];
  if (rejecting) return [
    b(it.kind === "delayed" ? "Cancel it" : "Reject", "plain bad", () => answer(it, "reject", $(`#why-${it.id}`).value)),
    b("Keep it", "plain", () => { needs.rejecting = null; renderPanel(); })];
  if (it.kind === "incident") return [b("Seen", "primary", () => answer(it, "approve"))];
  if (it.kind === "prepared") return [b("Mark done", "primary", () => answer(it, "approve"))];
  if (it.kind === "delayed") return [b("Cancel it", "plain bad", () => { needs.rejecting = it.id; renderPanel(); })];
  return [
    b("Approve", "primary", () => answer(it, "approve")),
    it.editable && b("Edit", "plain", () => { needs.editing = it.id; renderPanel(); }),
    b("Reject", "plain bad", () => { needs.rejecting = it.id; renderPanel(); })];
}

async function answer(it, kind, text) {
  const seconds = Math.round((Date.now() - (needs.shownAt[it.id] || Date.now())) / 1000);
  try {
    const out = await api(`realms/${enc(next.realm)}/queue/${it.id}`, { answer: kind, text: text || "", revision: it.revision, seconds });
    const done = kind === "edit" ? "edited; it still waits for your approval" : out.item.status;
    needs.note = `${it.title}: ${out.report || done}`;
  } catch (e) {
    needs.note = `${it.title}: ${e.message}`;
  }
  needs.editing = needs.rejecting = null;
  needs.realm = null;
  loadNeeds();
}

/** "07:31" today, else "2 Nov". */
function clock(at) {
  const d = new Date(at);
  if (isNaN(d)) return "";
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" }) : d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

// The run an item came from, block by block (decision 49).
function details(it) {
  if (!it) return null;
  const run = needs.runs[it.id];
  if (run === undefined) {
    needs.runs[it.id] = null;
    api(`realms/${enc(next.realm)}/queue/${it.id}/run`)
      .then((r) => { needs.runs[it.id] = r; renderPanel(); })
      .catch(() => { needs.runs[it.id] = { blocks: [] }; renderPanel(); });
  }
  const blocks = (run && run.blocks) || [];
  const state = blocks.some((b) => b.outcome === "failed") ? ["Failed", "bad"]
    : blocks.some((b) => b.outcome === "waiting") ? ["Waiting for you", "warn"] : ["Done", "ok"];
  const cost = blocks.reduce((sum, b) => sum + (b.cost_usd || 0), 0);
  const secs = (b) => b.finished_at ? Math.max(0, Math.round((new Date(b.finished_at) - new Date(b.started_at)) / 1000)) : 0;
  return h("aside", { class: "details", "aria-labelledby": "details-title" },
    h("h2", { id: "details-title" }, "Routine details"),
    h("div", { class: "details-head" },
      blocks.length > 0 && h("span", { class: `state ${state[1]}` }, state[0]),
      h("span", {}, it.routine, blocks.length > 0 && ` · started ${clock(blocks[0].started_at)}`)),
    run === null || run === undefined ? h("p", { class: "note" }, "Loading…")
      : !blocks.length ? h("p", { class: "note" }, "No step was recorded for this run. Runs record each step from now on.")
      : h("div", { class: "steps" },
          h("span", { class: "steps-label" }, "Steps"),
          blocks.map((b, n) => h("div", { class: "step" },
            h("span", {}, `${n + 1} · ${b.block}`),
            h("span", { class: `step-out ${b.outcome}` },
              b.outcome === "waiting" ? `${b.items_in} waiting for you`
                : b.outcome === "failed" ? "failed"
                : `${b.items_in} → ${b.items_out} · ${secs(b)} s`)))),
    blocks.length > 0 && h("div", { class: "tile" }, h("span", { class: "tile-label" }, "Run cost"), h("span", { class: "tile-value" }, `$${cost.toFixed(2)}`)),
    h("a", { href: `/#/r/${enc(next.realm)}/routines` }, "Open the routine"));
}
