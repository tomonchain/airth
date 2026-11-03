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
function Avatar({ agent, size = 40 }) {
  return html`<span class=${`avatar s${size}`} aria-hidden="true" style=${`--agent: ${agentColour(agent)}`}>${initial(agentName(agent))}</span>`;
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

/** Needs you, in the Workspace layout (decision 72): the agents waiting,
    the open agent's items, and the run an item came from. */
function NeedsPage() {
  const header = { label: next.realm, title: "Needs you" };
  if (needs.realm !== next.realm) return html`<${Workspace} header=${header} main=${html`<p class="lede">Loading…</p>`} />`;
  const byAgent = new Map();
  for (const it of needs.items) {
    const key = it.agent || "";
    if (!byAgent.has(key)) byAgent.set(key, []);
    byAgent.get(key).push(it);
  }
  if (needs.agent === undefined || !byAgent.has(needs.agent || "")) needs.agent = byAgent.size ? [...byAgent.keys()][0] || null : undefined;
  const open = byAgent.get(needs.agent || "") || [];
  const n = needs.items.length;
  const summary = !n ? "Nothing waits for you." : `${n === 1 ? "One item waits" : `${n} items wait`} for you, from ${byAgent.size === 1 ? "one agent" : `${byAgent.size} agents`}.`;
  return html`<${Workspace} header=${{ ...header, summary }}
    list=${html`<${AgentList} byAgent=${byAgent} />`}
    main=${html`<${Thread} items=${open} />`}
    details=${html`<${Details} it=${open.find((i) => i.routine && i.run)} />`} />`;
}

function AgentList({ byAgent }) {
  const dept = (a) => { const d = needs.agents.find((x) => x.name === a); return a ? (d && d.department) || "" : "leadership"; };
  const groups = new Map();
  const q = needs.find.toLowerCase();
  for (const [a, items] of byAgent) {
    if (q && !agentName(a).toLowerCase().includes(q) && !items.some((i) => i.title.toLowerCase().includes(q))) continue;
    const d = dept(a);
    if (!groups.has(d)) groups.set(d, []);
    groups.get(d).push([a, items]);
  }
  const choose = (e, a) => { e.preventDefault(); needs.agent = a || null; needs.editing = needs.rejecting = null; renderPanel(); };
  return html`<div class="pane-stack" aria-label="Agents waiting for you">
    <label for="find" class="sr-only">Search</label>
    <input id="find" type="search" placeholder="Search" value=${needs.find} onInput=${(e) => { needs.find = e.target.value; renderPanel(); }} />
    ${!byAgent.size && html`<div class="empty"><strong>Nothing waits for you</strong>
      <span>Approvals, incidents and prepared actions show here as the agents ask.</span></div>`}
    ${[...groups].map(([d, list]) => html`<span class="group">${DEPARTMENT_NAMES[d] || "No department yet"}</span>
      ${list.map(([a, items]) => {
        const first = items[0];
        const incident = items.some((i) => i.kind === "incident");
        return html`<a class="agent" href="#" aria-current=${(a || null) === needs.agent ? "true" : null} onClick=${(e) => choose(e, a)}>
          <span class="badged"><${Avatar} agent=${a} /><span class=${incident ? "badge bad" : "badge"}>${incident ? "!" : String(items.length)}</span></span>
          <span class="who"><span class="line"><span class="name">${agentName(a)}</span><span class="time">${clock(first.created_at)}</span></span>
            <span class="sub">${first.title} · ${first.text}</span></span>
        </a>`;
      })}`)}
  </div>`;
}

function Thread({ items }) {
  if (!items.length) return null;
  const a = needs.agent;
  const d = needs.agents.find((x) => x.name === a);
  return html`<div class="thread" aria-label=${agentName(a)}>
    <div class="thread-head"><${Avatar} agent=${a} size=${34} /><h2>${agentName(a)}</h2>
      ${d && d.department && html`<span class="chip">${DEPARTMENT_NAMES[d.department] || d.department}</span>`}</div>
    ${needs.error && html`<p class="note bad">${needs.error}</p>`}
    ${needs.note && html`<p class="note">${needs.note}</p>`}
    ${items.map((it) => html`<${Card} it=${it} />`)}
  </div>`;
}

const KIND_TAGS = { approval: "Needs you", delayed: "Done unless you cancel", prepared: "Prepared · you do it", incident: "Incident", report: "Report", deadline: "Legal deadline" };

function Card({ it }) {
  needs.shownAt[it.id] = needs.shownAt[it.id] || Date.now();
  const editing = needs.editing === it.id;
  const rejecting = needs.rejecting === it.id;
  // An action a task of the founder's proposed (decision 56): the task
  // stopped there, and waits for nothing else.
  const what = it.task && it.action ? `If approved: ${it.action} is done, once. The task stopped here.`
    : it.retry === "clear" ? "Nothing was done: Retry does it once, as the action is declared now. Seen closes it."
    : it.retry === "ambiguous" ? "It may have been done: check before you retry. Seen closes it."
    : it.kind === "incident" || it.kind === "report" ? "Seen closes it."
    : it.kind === "prepared" ? "Airth does nothing: mark it done once you did it."
    : it.kind === "deadline" ? "Airth does not file it: answer Filed once you did."
    : it.routine && !it.action ? `If approved: ${it.routine} goes on.`
    : it.action ? `If approved: ${it.action} is done, once.` : null;
  const link = it.source && it.source.link;
  const blocked = it.flag === "blocking";
  return html`<article class="approval" aria-label=${it.title}>
    <div class="approval-top">
      <span class=${it.kind === "incident" ? "tag bad" : "tag"}>${it.title} · ${KIND_TAGS[it.kind] || it.kind}</span>
      ${it.execute_at && html`<span class="when">Done at ${clock(it.execute_at)}</span>`}
    </div>
    ${(blocked || it.flag === "note") && html`<p class=${blocked ? "flag blocking" : "flag"}>
      <strong>${blocked ? "Flagged by the check" : "Note from the check"}</strong>: ${it.flag_reason || "no reason given"}${blocked && ". Edit it, or override the flag to approve it as it is."}</p>`}
    ${it.note && html`<p class="flag"><strong>Also in this run</strong>: ${it.note}</p>`}
    ${editing
      ? html`<textarea id=${`edit-${it.id}`} class="edit" rows="5" aria-label="New text">${it.text}</textarea>`
      : html`<p class=${it.call ? "content call" : "content"}>${it.text}</p>`}
    <div class="meta">
      ${link && html`<span>Source: <a href=${link} target="_blank" rel="noopener noreferrer">${it.source.title || link}</a></span>`}
      ${it.task && html`<span>From your task: <a href=${`/classic#/r/${enc(next.realm)}/run/${enc(it.task.id)}`}>${it.task.title || it.task.id}</a></span>`}
      ${what && html`<span>${what}</span>`}
    </div>
    ${rejecting && html`<input id=${`why-${it.id}`} class="why" type="text" placeholder="Why? (optional)" aria-label="Why you reject it" />`}
    <div class="buttons">${buttons(it, editing, rejecting)}</div>
  </article>`;
}

function buttons(it, editing, rejecting) {
  const b = (label, cls, onclick) => html`<button type="button" class=${cls} onClick=${onclick}>${label}</button>`;
  const value = (id) => { const el = document.querySelector(id); return el ? el.value : ""; };
  if (editing) return [
    b("Save the edit", "primary", () => answer(it, "edit", value(`#edit-${it.id}`))),
    b("Cancel", "plain", () => { needs.editing = null; renderPanel(); })];
  if (rejecting) return [
    b(it.kind === "delayed" ? "Cancel it" : "Reject", "plain bad", () => answer(it, "reject", value(`#why-${it.id}`))),
    b("Keep it", "plain", () => { needs.rejecting = null; renderPanel(); })];
  // Decision 62: an ambiguous failure is retried once the founder checked.
  if (needs.confirming === it.id) return [
    b("Nothing was created: retry", "primary", () => answer(it, "retry", "", { checked: true })),
    b("Back", "plain", () => { needs.confirming = null; renderPanel(); })];
  if (it.retry === "clear") return [b("Retry", "primary", () => answer(it, "retry")), b("Seen", "plain", () => answer(it, "approve"))];
  if (it.retry === "ambiguous") return [
    b("Retry…", "plain", () => { needs.confirming = it.id; renderPanel(); }),
    b("Seen", "primary", () => answer(it, "approve"))];
  if (it.kind === "incident" || it.kind === "report") return [b("Seen", "primary", () => answer(it, "approve"))];
  if (it.kind === "prepared") return [b("Mark done", "primary", () => answer(it, "approve"))];
  if (it.kind === "deadline") return [b("Filed", "primary", () => answer(it, "approve")),
    !it.overdue && b("Seen", "plain", () => answer(it, "reject", "seen"))];
  if (it.kind === "delayed") return [b("Cancel it", "plain bad", () => { needs.rejecting = it.id; renderPanel(); })];
  // Blocked by a check: no plain Approve; the founder's override is logged.
  return [
    isBlocked(it) ? b("Override", "plain bad", () => answer(it, "override"))
      : b("Approve", "primary", () => answer(it, "approve")),
    it.editable && b("Edit", isBlocked(it) ? "primary" : "plain", () => { needs.editing = it.id; renderPanel(); }),
    b("Reject", "plain bad", () => { needs.rejecting = it.id; renderPanel(); })];
}

const isBlocked = (it) => it.flag === "blocking";

async function answer(it, kind, text, extra) {
  const seconds = Math.round((Date.now() - (needs.shownAt[it.id] || Date.now())) / 1000);
  try {
    const out = await api(`realms/${enc(next.realm)}/queue/${it.id}`, { answer: kind, text: text || "", revision: it.revision, seconds, ...extra });
    const done = kind === "edit" ? "edited; it still waits for your approval" : out.item.status;
    needs.note = `${it.title}: ${out.report || done}`;
  } catch (e) {
    needs.note = `${it.title}: ${e.message}`;
  }
  needs.editing = needs.rejecting = needs.confirming = null;
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
function Details({ it }) {
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
  const secs = (b) => (b.finished_at ? Math.max(0, Math.round((new Date(b.finished_at) - new Date(b.started_at)) / 1000)) : 0);
  return html`<div class="details" aria-labelledby="details-title">
    <h2 id="details-title">Routine details</h2>
    <div class="details-head">
      ${blocks.length > 0 && html`<span class=${`state ${state[1]}`}>${state[0]}</span>`}
      <span>${it.routine}${blocks.length > 0 && ` · started ${clock(blocks[0].started_at)}`}</span>
    </div>
    ${run === null || run === undefined ? html`<p class="note">Loading…</p>`
      : !blocks.length ? html`<p class="note">No step was recorded for this run. Runs record each step from now on.</p>`
      : html`<div class="steps"><span class="steps-label">Steps</span>${blocks.map((b, n) => html`<div class="step">
          <span>${n + 1} · ${b.block}</span>
          <span class=${`step-out ${b.outcome}`}>${b.outcome === "waiting" ? `${b.items_in} waiting for you` : b.outcome === "failed" ? "failed" : `${b.items_in} → ${b.items_out} · ${secs(b)} s`}</span>
        </div>`)}</div>`}
    ${blocks.length > 0 && html`<div class="tile"><span class="tile-label">Run cost</span><span class="tile-value">$${cost.toFixed(2)}</span></div>`}
    <a href=${pageHash("routines")}>Open the routine</a>
  </div>`;
}
