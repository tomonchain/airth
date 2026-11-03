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
  sel: null, // the open item's id
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

/** An icon from ICON_PATHS, as a component. */
function Icon({ name, size = 16 }) {
  return html`<span class="ico" aria-hidden="true" dangerouslySetInnerHTML=${{ __html: icon(name, size) }}></span>`;
}

/** What an item is, in the founder's words: its icon, tone and label. A
    blocked approval and an overdue deadline are urgent like an incident. */
function kindOf(it) {
  if (it.kind === "incident") return { icon: "alert", tone: "bad", label: "Incident", rank: 0 };
  if (it.flag === "blocking") return { icon: "shield", tone: "bad", label: "Held by the check", rank: 1 };
  if (it.kind === "deadline") return it.overdue ? { icon: "calendar", tone: "bad", label: "Overdue deadline", rank: 1 }
    : { icon: "calendar", tone: "warn", label: "Legal deadline", rank: 2 };
  if (it.kind === "approval") return { icon: "approve", tone: "warn", label: "To approve", rank: 3 };
  if (it.kind === "prepared") return { icon: "hand", tone: "info", label: "You do it", rank: 4 };
  if (it.kind === "delayed") return { icon: "clock", tone: "info", label: "Goes out unless you cancel", rank: 5 };
  if (it.kind === "report") return { icon: "doc", tone: "plain", label: "Report", rank: 6 };
  return { icon: "info", tone: "plain", label: it.kind, rank: 3 };
}
const SECTIONS = [[[0, 1], "Urgent"], [[2, 3], "To decide"], [[4], "To do yourself"], [[5], "Going out"], [[6], "To read"]];

/** The items in the order to handle them: the most pressing first, the
    oldest first within a kind (the same order as the home's first item). */
function sortedNeeds() {
  return needs.items.slice().sort((a, b) => kindOf(a).rank - kindOf(b).rank || String(a.created_at || "").localeCompare(String(b.created_at || "")));
}

/** Needs you, in the Workspace layout (decision 72): every item by
    urgency, the open one, and where it comes from. */
function NeedsPage() {
  const header = { label: next.realm, title: "Needs you" };
  if (needs.realm !== next.realm) return html`<${Workspace} header=${header} main=${html`<p class="lede">Loading…</p>`} />`;
  const items = sortedNeeds();
  if (!items.some((i) => i.id === needs.sel)) needs.sel = items.length ? items[0].id : null;
  const it = items.find((i) => i.id === needs.sel);
  const urgent = items.filter((i) => kindOf(i).rank <= 1).length;
  const summary = !items.length ? "Nothing waits for you."
    : `${items.length === 1 ? "One item waits" : `${items.length} items wait`} for you${urgent ? `, ${urgent} urgent` : ""}. Most pressing first.`;
  if (!items.length) return html`<${Workspace} header=${{ ...header, summary }} main=${html`<${AllDone} />`} />`;
  return html`<${Workspace} header=${{ ...header, summary }}
    list=${html`<${NeedsList} items=${items} />`}
    main=${html`<${Item} it=${it} pos=${items.indexOf(it)} items=${items} />`}
    details=${html`<${Origin} it=${it} />`} />`;
}

function AllDone() {
  return html`<div class="all-done">
    ${needs.note && html`<p class="toast"><${Icon} name="check" />${needs.note}</p>`}
    <span class="all-done-mark"><${Icon} name="approve" size=${40} /></span>
    <strong>Nothing waits for you</strong>
    <span>Approvals, incidents, deadlines and prepared actions show here as your agents ask.</span>
  </div>`;
}

const select = (id) => { needs.sel = id; needs.editing = needs.rejecting = needs.confirming = null; needs.note = null; renderPanel(); };

function NeedsList({ items }) {
  const q = needs.find.trim().toLowerCase();
  const shown = items.filter((i) => !q || `${i.title} ${i.text} ${agentName(i.agent)} ${i.routine || ""}`.toLowerCase().includes(q));
  return html`<div class="pane-stack" aria-label="What waits for you">
    <div class="find"><${Icon} name="search" /><label for="find" class="sr-only">Search</label>
      <input id="find" type="search" placeholder="Search" value=${needs.find} onInput=${(e) => { needs.find = e.target.value; renderPanel(); }} /></div>
    ${!shown.length && html`<p class="note">Nothing matches.</p>`}
    ${SECTIONS.map(([ranks, title]) => {
      const part = shown.filter((i) => ranks.includes(kindOf(i).rank));
      return part.length > 0 && html`<span class="group">${title}<span class="group-count">${part.length}</span></span>
        ${part.map((i) => html`<${Row} it=${i} />`)}`;
    })}
  </div>`;
}

function Row({ it }) {
  const k = kindOf(it);
  const preview = String(it.text || "").split("\n").find((l) => l.trim()) || "";
  return html`<a class="nrow" href="#" aria-current=${it.id === needs.sel ? "true" : null} onClick=${(e) => { e.preventDefault(); select(it.id); }}>
    <span class=${`kind-mark ${k.tone}`}><${Icon} name=${k.icon} /></span>
    <span class="nrow-body">
      <span class="nrow-title">${it.title}</span>
      ${preview && preview !== "{}" && html`<span class="nrow-preview">${preview}</span>`}
      <span class="nrow-meta"><span class="dot" style=${`--agent: ${agentColour(it.agent)}`}></span>${agentName(it.agent)}${it.created_at && html` · ${ago(it.created_at)}`}</span>
    </span>
  </a>`;
}

/** What answering does, said before the founder answers. */
function consequence(it) {
  // An action a task of the founder's proposed (decision 56): the task
  // stopped there, and waits for nothing else.
  return it.task && it.action ? `If approved: ${it.action} is done, once. The task stopped here.`
    : it.retry === "clear" ? "Nothing was done: Retry does it once, as the action is declared now. Seen closes it."
    : it.retry === "ambiguous" ? "It may have been done: check before you retry. Seen closes it."
    : it.kind === "incident" || it.kind === "report" ? "Seen closes it."
    : it.kind === "prepared" ? "Airth does nothing: mark it done once you did it."
    : it.kind === "deadline" ? "Airth does not file it: answer Filed once you did."
    : it.kind === "delayed" && it.execute_at ? `Goes out at ${clock(it.execute_at)} unless you cancel it.`
    : it.routine && !it.action ? `If approved: ${it.routine} goes on.`
    : it.action ? `If approved: ${it.action} is done, once.` : null;
}

function Item({ it, pos, items }) {
  needs.shownAt[it.id] = needs.shownAt[it.id] || Date.now();
  const k = kindOf(it);
  const editing = needs.editing === it.id;
  const blocked = it.flag === "blocking";
  const link = it.source && it.source.link;
  const what = consequence(it);
  const step = (d) => { const n = items[pos + d]; if (n) select(n.id); };
  const text = String(it.text || "");
  // Edit and Reject put the cursor in their field, so the keys flow on.
  useEffect(() => {
    const field = document.querySelector(`#edit-${it.id}`) || document.querySelector(`#why-${it.id}`);
    if (field && field.focus) field.focus();
  }, [editing, needs.rejecting === it.id]);
  return html`<article class="item" aria-label=${it.title}>
    ${needs.note && html`<p class="toast" role="status"><${Icon} name="check" />${needs.note}</p>`}
    ${needs.error && html`<p class="note bad">${needs.error}</p>`}
    <div class="item-top">
      <span class=${`kind-tag ${k.tone}`}><${Icon} name=${k.icon} />${k.label}</span>
      <span class="item-nav">
        <span class="item-pos">${pos + 1} of ${items.length}</span>
        <button type="button" class="icon-button" aria-label="Previous item (K)" title="Previous (K)" disabled=${pos === 0} onClick=${() => step(-1)}><${Icon} name="up" /></button>
        <button type="button" class="icon-button" aria-label="Next item (J)" title="Next (J)" disabled=${pos === items.length - 1} onClick=${() => step(1)}><${Icon} name="down" /></button>
      </span>
    </div>
    <h2 class="item-title">${it.title}</h2>
    <p class="item-from"><${Avatar} agent=${it.agent} size=${20} />${agentName(it.agent)}${it.routine && html` · <span class="mono">${it.routine}</span>`}${it.created_at && html` · ${ago(it.created_at)}`}</p>
    ${(blocked || it.flag === "note") && html`<p class=${blocked ? "callout bad" : "callout"}><${Icon} name=${blocked ? "shield" : "info"} />
      <span><strong>${blocked ? "Flagged by the check" : "Note from the check"}</strong>: ${it.flag_reason || "no reason given"}${blocked && ". Edit it, or override the flag to approve it as it is."}</span></p>`}
    ${it.note && html`<p class="callout"><${Icon} name="filter" /><span><strong>Also in this run</strong>: ${it.note}</span></p>`}
    ${editing
      ? html`<textarea id=${`edit-${it.id}`} class="edit" rows="6" aria-label="New text">${text}</textarea>`
      : text && text !== "{}" && html`<div class=${it.call ? "content call" : "content"}>${text}</div>`}
    ${it.editable && !editing && html`<span class="count">${text.length} characters</span>`}
    <div class="meta">
      ${link && html`<span><${Icon} name="link" /><a href=${link} target="_blank" rel="noopener noreferrer">${it.source.title || link}</a></span>`}
      ${it.task && html`<span><${Icon} name="tasks" />From your task: <a href=${`/classic#/r/${enc(next.realm)}/run/${enc(it.task.id)}`}>${it.task.title || it.task.id}</a></span>`}
      ${what && html`<span><${Icon} name="arrow" />${what}</span>`}
    </div>
    ${needs.rejecting === it.id && html`<input id=${`why-${it.id}`} class="why" type="text" placeholder="Why? (optional)" aria-label="Why you reject it" />`}
    <div class="buttons">${actions(it).filter(Boolean).map((a) => html`<button type="button" class=${a.cls} onClick=${a.run}>
      ${a.icon && html`<${Icon} name=${a.icon} />`}${a.label}${a.key && html`<kbd>${a.key}</kbd>`}</button>`)}</div>
  </article>`;
}

/** The answers an item takes, the main one first. A key is a shortcut. */
function actions(it) {
  const a = (label, cls, icon, run, key) => ({ label, cls, icon, run, key });
  const value = (id) => { const el = document.querySelector(id); return el ? el.value : ""; };
  const open = (field) => () => { needs[field] = it.id; renderPanel(); };
  if (needs.editing === it.id) return [
    a("Save the edit", "primary", "check", () => answer(it, "edit", value(`#edit-${it.id}`))),
    a("Cancel", "plain", null, () => { needs.editing = null; renderPanel(); })];
  if (needs.rejecting === it.id) return [
    a(it.kind === "delayed" ? "Cancel it" : "Reject", "plain bad", "x", () => answer(it, "reject", value(`#why-${it.id}`))),
    a("Keep it", "plain", null, () => { needs.rejecting = null; renderPanel(); })];
  // Decision 62: an ambiguous failure is retried once the founder checked.
  if (needs.confirming === it.id) return [
    a("Nothing was created: retry", "primary", "retry", () => answer(it, "retry", "", { checked: true })),
    a("Back", "plain", null, () => { needs.confirming = null; renderPanel(); })];
  const seen = (cls) => a("Seen", cls, "check", () => answer(it, "approve"), "A");
  if (it.retry === "clear") return [a("Retry", "primary", "retry", () => answer(it, "retry")), seen("plain")];
  if (it.retry === "ambiguous") return [a("Retry…", "plain", "retry", open("confirming")), seen("primary")];
  if (it.kind === "incident" || it.kind === "report") return [seen("primary")];
  if (it.kind === "prepared") return [a("Mark done", "primary", "check", () => answer(it, "approve"), "A")];
  if (it.kind === "deadline") return [a("Filed", "primary", "check", () => answer(it, "approve"), "A"),
    !it.overdue && a("Seen", "plain", null, () => answer(it, "reject", "seen"))];
  if (it.kind === "delayed") return [a("Cancel it", "plain bad", "x", open("rejecting"), "R")];
  // Blocked by a check: no plain Approve; the founder's override is logged.
  return [
    isBlocked(it) ? a("Override", "plain bad", "shield", () => answer(it, "override"))
      : a("Approve", "primary", "check", () => answer(it, "approve"), "A"),
    it.editable && a("Edit", isBlocked(it) ? "primary" : "plain", "pencil", open("editing"), "E"),
    a("Reject", "plain bad", "x", open("rejecting"), "R")];
}

const isBlocked = (it) => it.flag === "blocking";

async function answer(it, kind, text, extra) {
  const seconds = Math.round((Date.now() - (needs.shownAt[it.id] || Date.now())) / 1000);
  // The next item opens once this one is answered.
  const order = sortedNeeds();
  const after = order[order.indexOf(it) + 1] || order[order.indexOf(it) - 1];
  try {
    const out = await api(`realms/${enc(next.realm)}/queue/${it.id}`, { answer: kind, text: text || "", revision: it.revision, seconds, ...extra });
    const done = kind === "edit" ? "edited; it still waits for your approval" : out.item.status;
    needs.note = `${it.title}: ${out.report || done}`;
    if (kind !== "edit" && after) needs.sel = after.id;
  } catch (e) {
    needs.note = `${it.title}: ${e.message}`;
  }
  needs.editing = needs.rejecting = needs.confirming = null;
  needs.realm = null;
  loadNeeds();
}

// Shortcuts on Needs you: J and K move, A gives the main answer when it
// has one (Approve, Seen, Filed, Mark done), E edits, R rejects. Never while
// typing.
document.addEventListener("keydown", (e) => {
  if (next.route.page !== "needs-you" || e.metaKey || e.ctrlKey || e.altKey || search.open) return;
  if (/^(INPUT|TEXTAREA|SELECT)$/.test((e.target && e.target.tagName) || "")) return;
  const items = sortedNeeds();
  const pos = items.findIndex((i) => i.id === needs.sel);
  const key = e.key.toLowerCase();
  if (key === "j" || e.key === "ArrowDown") { if (items[pos + 1]) { e.preventDefault(); select(items[pos + 1].id); } return; }
  if (key === "k" || e.key === "ArrowUp") { if (pos > 0) { e.preventDefault(); select(items[pos - 1].id); } return; }
  const act = pos >= 0 && actions(items[pos]).find((x) => x && x.key && x.key.toLowerCase() === key);
  if (act) { e.preventDefault(); act.run(); }
});

/** "07:31" today, else "2 Nov". */
function clock(at) {
  const d = new Date(at);
  if (isNaN(d)) return "";
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" }) : d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

const STEP_ICONS = { ok: "check", waiting: "clock", failed: "x" };

/** Where the open item comes from: its agent, its routine and the run,
    block by block (decision 49). */
function Origin({ it }) {
  if (!it) return null;
  const d = needs.agents.find((x) => x.name === it.agent);
  const dept = it.agent ? d && d.department : "leadership";
  return html`<div class="details origin" aria-labelledby="details-title">
    <h2 id="details-title">Where it comes from</h2>
    <div class="origin-agent"><${Avatar} agent=${it.agent} size=${34} />
      <span><strong>${agentName(it.agent)}</strong><span class="sub">${DEPARTMENT_NAMES[dept] || "No department yet"}</span></span></div>
    ${it.routine && it.run ? html`<${Run} it=${it} />`
      : html`<p class="note">${it.kind === "deadline" ? "From the business's legal calendar." : it.task ? "From a task you started." : it.routine ? `From ${it.routine}.` : "Not from a routine's run."}</p>`}
    ${it.routine && html`<a class="origin-link" href=${pageHash("routines")}><${Icon} name="routines" />Open the routine</a>`}
  </div>`;
}

function Run({ it }) {
  const run = needs.runs[it.id];
  if (run === undefined) {
    needs.runs[it.id] = null;
    api(`realms/${enc(next.realm)}/queue/${it.id}/run`)
      .then((r) => { needs.runs[it.id] = r; renderPanel(); })
      .catch(() => { needs.runs[it.id] = { blocks: [] }; renderPanel(); });
  }
  if (!run) return html`<p class="note">Loading…</p>`;
  const blocks = run.blocks || [];
  if (!blocks.length) return html`<p class="note">No step was recorded for this run. Runs record each step from now on.</p>`;
  const cost = blocks.reduce((sum, b) => sum + (b.cost_usd || 0), 0);
  const secs = (b) => (b.finished_at ? Math.max(0, Math.round((new Date(b.finished_at) - new Date(b.started_at)) / 1000)) : 0);
  return html`<div class="run" aria-label="The run's steps">
    <span class="run-label"><span class="mono">${it.routine}</span> · started ${clock(blocks[0].started_at)}</span>
    ${blocks.map((b) => html`<div class=${`rstep ${b.outcome}`}>
      <span class="rstep-mark"><${Icon} name=${STEP_ICONS[b.outcome] || "check"} size=${12} /></span>
      <span class="rstep-name">${b.block}</span>
      <span class="rstep-out">${b.outcome === "waiting" ? `${b.items_in} for you` : b.outcome === "failed" ? "failed" : `${b.items_in} → ${b.items_out} · ${secs(b)} s`}</span>
    </div>`)}
    <span class="run-cost">Run cost <strong>$${cost.toFixed(2)}</strong></span>
  </div>`;
}
