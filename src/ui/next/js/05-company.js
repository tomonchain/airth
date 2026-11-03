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
    pageHead(null, title, agents.length > 0 && "Your time and cost over the last 30 days."),
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
        h("span", { class: `agent-state ${a.state || "idle"}` }, CH_STATES[a.state] || "Idle"))),
    h("div", { class: "chips" },
      !a.routines.length && h("span", { class: "chip" }, "No routine yet"),
      a.routines.map((r) => [
        h("span", { class: "chip mono" }, r.schedule ? `${r.name} · ${r.schedule}` : r.name),
        r.last && h("span", { class: r.last.ok ? "chip ok" : "chip bad" }, `${r.last.ok ? "Ran" : "Failed"} ${clock(r.last.at)}`),
      ]),
      a.waiting > 0 && h("a", { class: "chip warn", href: pageHash("needs-you") }, `${a.waiting} ${a.waiting === 1 ? "waits" : "wait"} for you`)),
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

function businessPage() {
  const data = loaded(company.profile, "company");
  if (!data) return h("div", { class: "column" }, pageHead(next.realm, "Business"), h("p", { class: "lede" }, "Loading…"));
  const p = data.profile || {};
  const g = data.goal || {};
  // Decision 66: a UK company's year end is its accounting reference date.
  const words = (k, label) => k === "fiscal_year_end" && p.country === "GB" ? "Accounting reference date" : label;
  const facts = (pairs, src) => h("dl", { class: "facts" }, pairs.map(([k, label]) => [
    h("dt", {}, words(k, label)), h("dd", { class: show(src[k]) ? null : "unknown" }, show(src[k]) || "Not filled yet")]));
  return h("div", { class: "column" },
    pageHead(next.realm, show(p.name) || next.realm),
    data.error && h("p", { class: "note bad" }, `The config cannot be read: ${data.error}`),
    h("section", { class: "card" }, h("h2", { class: "label" }, "Legal identity"), facts(IDENTITY, p)),
    h("section", { class: "card" }, h("h2", { class: "label" }, "Goal"),
      facts([["goal", "Goal"], ["measure", "Measure"], ["target", "Target"], ["by", "By"], ["milestones", "Milestones"]], g)),
    h("section", { class: "card" }, h("h2", { class: "label" }, "Lines"),
      (data.lines || []).length
        ? h("div", { class: "rows" }, data.lines.map((l) => h("div", { class: "row" },
            h("span", { class: "grow" }, h("span", { class: "name" }, show(l.fields.title) || l.name),
              h("span", { class: "sub" }, [show(l.fields.kind), show(l.fields.status)].filter(Boolean).join(" · ") || "No kind or status yet")))))
        : h("p", { class: "note" }, "No line yet: an editorial line, a range or a brand is a file in business/lines/.")),
    h("section", { class: "card" }, h("h2", { class: "label" }, "Repositories"),
      (data.repos || []).length
        ? h("div", { class: "rows" }, data.repos.map((r) => h("div", { class: "row" },
            h("span", { class: "grow" }, h("span", { class: "name" }, r.name), h("span", { class: "sub" }, r.url || "On this Mac only")))))
        : h("p", { class: "note" }, "No repository yet.")),
    (data.missing || []).length > 0 && h("section", { class: "card" }, h("h2", { class: "label" }, "Left to fill"),
      h("ul", { class: "missing" }, data.missing.map((m) => h("li", { class: "mono" }, m)))),
    h("p", { class: "note" }, "The config lives in the business folder; edit it there, or ask an agent through a change you adopt."));
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
