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
  return h("div", { class: "needs" }, agentList(byAgent), thread(byAgent.get(needs.agent || "") || []));
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
  const what = it.kind === "incident" ? "Seen closes it."
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
      : h("p", { class: "content" }, it.text),
    h("div", { class: "meta" },
      link && h("span", {}, "Source: ", h("a", { href: link, target: "_blank", rel: "noopener noreferrer" }, (it.source.title || link))),
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
