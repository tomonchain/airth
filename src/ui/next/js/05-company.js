// Team and Business, read-only: the agents by department, and the
// business's config. Both are written in the business folder (decision 44).

const company = { team: {}, profile: {} }; // by business, once loaded

function loaded(cache, path) {
  const realm = next.realm;
  if (cache[realm] === undefined) {
    cache[realm] = null;
    api(`realms/${enc(realm)}/${path}`)
      .then((d) => { cache[realm] = d; renderPanel(); })
      .catch((e) => { cache[realm] = { error: e.message }; renderPanel(); });
  }
  return cache[realm];
}

const pageHead = (label, title, sub) =>
  h("header", { class: "page-head" }, label && h("div", { class: "label" }, label), h("h1", { class: "headline" }, title), sub && h("p", { class: "date" }, sub));
const minutes = (s) => (s < 60 ? `${s} s` : `${Math.round(s / 60)} min`);
const money = (usd, unpriced) => `$${(usd || 0).toFixed(2)}${unpriced ? ` + ${unpriced} unpriced` : ""}`;

function teamPage() {
  const data = loaded(company.team, "team");
  if (!data) return h("div", { class: "column" }, pageHead(next.realm, "Team"), h("p", { class: "lede" }, "Loading…"));
  if (data.error) return h("div", { class: "column" }, pageHead(next.realm, "Team"), h("p", { class: "note bad" }, data.error));
  const agents = data.agents || [];
  const byDept = new Map();
  for (const a of agents) {
    const d = a.department || "";
    if (!byDept.has(d)) byDept.set(d, []);
    byDept.get(d).push(a);
  }
  const depts = [...byDept.keys()];
  const title = !agents.length ? "No agent yet."
    : depts.length === 1 ? `${agents.length === 1 ? "One agent" : `${agents.length} agents`}, ${agents.length === 1 ? "in" : "all in"} ${DEPARTMENT_NAMES[depts[0]] || "no department yet"}.`
    : `${agents.length} agents in ${depts.length} departments.`;
  return h("div", { class: "column wide" },
    h("div", { class: "head-row" },
      pageHead(null, title, agents.length > 0 && "Your time and cost over the last 30 days."),
      h("a", { class: "button", href: pageHash("hire") }, "Hire an agent")),
    !agents.length && h("div", { class: "empty" }, h("strong", {}, "No agent yet"),
      h("span", {}, "An agent is a file in agents/ in the business folder: its title, its department and its job.")),
    depts.map((d) => h("section", { class: "dept" },
      h("h2", {}, DEPARTMENT_NAMES[d] || "No department yet"),
      h("div", { class: "cards" }, byDept.get(d).map(agentCard)))));
}

function agentCard(a) {
  return h("article", { class: "card agent-card" },
    h("div", { class: "agent-top" },
      h("span", { class: "character", svg: character(a, 72) }),
      h("div", { class: "agent-id" }, h("h3", {}, a.title || a.name), h("span", {}, a.job || "No job written yet"),
        h("span", { class: `agent-state ${a.state || "idle"}` }, CH_STATES[a.state] || "Idle"),
        // The agent's, whatever routine it comes from: beside its state.
        a.waiting > 0 && h("a", { class: "agent-waiting", href: pageHash("needs-you") }, `${a.waiting} ${a.waiting === 1 ? "item waits" : "items wait"} for you`))),
    h("div", { class: "chips" },
      !a.routines.length && h("span", { class: "chip" }, "No routine yet"),
      a.routines.map((r) => [
        h("span", { class: "chip mono" }, r.schedule ? `${r.name} · ${r.schedule}` : r.name),
        r.last && h("span", { class: r.last.ok ? "chip ok" : "chip bad" }, `${r.last.ok ? "Ran" : "Failed"} ${clock(r.last.at)}`),
      ])),
    // Decision 65: an agent hired before keeps its look by the founder's gesture.
    !a.look_saved && h("div", { class: "keep-look" },
      h("span", {}, "Its look comes from its name for now."),
      h("button", { type: "button", class: "plain", onclick: () => keepLook(a) }, "Keep this look")),
    h("div", { class: "figures" },
      h("div", {}, h("span", {}, "Routines"), h("strong", {}, String(a.routines.length))),
      h("div", {}, h("span", {}, "Your time"), h("strong", {}, minutes(a.seconds || 0))),
      h("div", {}, h("span", {}, "Cost"), h("strong", {}, money(a.cost_usd, a.unpriced)))));
}

const IDENTITY = [
  ["legal_name", "Legal name"], ["legal_form", "Legal form"], ["country", "Country"], ["status", "Status"],
  ["registration_number", "Registration number"], ["registered_office", "Registered office"],
  ["incorporated_on", "Incorporated on"], ["fiscal_year_end", "Fiscal year end"], ["vat_number", "VAT number"],
  ["tax_regime", "Tax regime"], ["currency", "Currency"], ["bank", "Bank"], ["domain", "Domain"], ["links", "Links"],
];
const show = (v) => (v == null || v === "" ? null : Array.isArray(v) ? v.join(", ") : String(v));

/** What is left to fill, in words: "business/profile.md: registered_office"
    reads "Profile: registered office". */
function missingWords(m) {
  const [file, field] = m.split(": ");
  const where = { "business/profile.md": "Profile", "business/goal.md": "Goal" }[file]
    || file.replace(/^agents\/(.+)\.md$/, "Agent $1").replace(/^business\/lines\/(.+)\.md$/, "Line $1");
  return field ? `${where}: ${field.replace(/_/g, " ")}` : where;
}

/** Business: the company's identity, goal, lines and repositories, read from
    its folder (decision 43). */
function BusinessPage() {
  const data = loaded(company.profile, "company");
  const header = { label: next.realm, title: "Business" };
  if (!data) return html`<${Reading} header=${header}><p class="lede">Loading…</p></${Reading}>`;
  const p = data.profile || {};
  const g = data.goal || {};
  // Decision 66: a UK company's year end is its accounting reference date.
  const words = (k, label) => (k === "fiscal_year_end" && p.country === "GB" ? "Accounting reference date" : label);
  const facts = (pairs, src) => html`<dl class="facts">${pairs.map(([k, label]) => html`
    <dt>${words(k, label)}</dt><dd class=${show(src[k]) ? null : "unknown"}>${show(src[k]) || "Not filled yet"}</dd>`)}</dl>`;
  const rows = (items, row, empty) => (items || []).length
    ? html`<div class="rows">${items.map(row)}</div>` : html`<p class="note">${empty}</p>`;
  return html`<${Reading} header=${{ ...header, summary: show(p.name) || null }}>
    ${data.error && html`<p class="note bad">The config cannot be read: ${data.error}</p>`}
    <section class="card"><h2 class="label">Legal identity</h2>${facts(IDENTITY, p)}</section>
    <section class="card"><h2 class="label">Goal</h2>
      ${facts([["goal", "Goal"], ["measure", "Measure"], ["target", "Target"], ["by", "By"], ["milestones", "Milestones"]], g)}</section>
    <section class="card"><h2 class="label">Lines</h2>
      ${rows(data.lines, (l) => html`<div class="row"><span class="grow"><span class="name">${show(l.fields.title) || l.name}</span>
        <span class="sub">${[show(l.fields.kind), show(l.fields.status)].filter(Boolean).join(" · ") || "No kind or status yet"}</span></span></div>`,
        "No line yet: an editorial line, a range or a brand is a file in business/lines/.")}</section>
    <section class="card"><h2 class="label">Repositories</h2>
      ${rows(data.repos, (r) => html`<div class="row"><span class="grow"><span class="name">${r.name}</span>
        <span class="sub">${r.url || "On this Mac only"}</span></span></div>`, "No repository yet.")}</section>
    ${(data.missing || []).length > 0 && html`<section class="card"><h2 class="label">Left to fill</h2>
      <ul class="missing">${data.missing.map((m) => html`<li>${missingWords(m)}</li>`)}</ul></section>`}
    <p class="note">The config lives in the business folder; edit it there, or ask an agent through a change you adopt.</p>
  </${Reading}>`;
}

async function keepLook(a) {
  try {
    await api(`realms/${enc(next.realm)}/agents/${enc(a.name)}/look`, {});
    delete company.team[next.realm];
  } catch (e) {
    company.team[next.realm] = { error: e.message };
  }
  renderPanel();
}
