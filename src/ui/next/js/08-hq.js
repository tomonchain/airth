// The HQ (docs/design/mockups/hq.html, decision 53): one section per
// business, never a total across them. Health, vitals and the activity
// sentences come in words from the server, built by code from Airth's
// records; finance waits for the bank (decision 54).

const hq = { data: undefined };

const HEALTH = { healthy: "Healthy", watch: "Watch", "at risk": "At risk", "not started": "Not started" };

function hqPage() {
  if (hq.data === undefined) {
    hq.data = null;
    api("hq").then((d) => { hq.data = d; renderPanel(); }).catch((e) => { hq.data = { error: e.message }; renderPanel(); });
  }
  const d = hq.data;
  const head = (title) => h("header", { class: "page-head" }, h("div", { class: "date" }, today()), h("h1", { class: "headline" }, title));
  if (!d) return h("div", { class: "column hq" }, head("Your businesses"), h("p", { class: "lede" }, "Loading…"));
  if (d.error) return h("div", { class: "column hq" }, head("Your businesses"), h("p", { class: "note bad" }, d.error));
  const first = d.businesses.find((b) => b.needs > 0);
  return h("div", { class: "column hq" },
    h("header", { class: "page-head" },
      h("div", { class: "date" }, today()),
      h("h1", { class: "headline" }, d.headline),
      d.needs > 0 && h("a", { class: "needs-link", href: pageHash("needs-you", first.name) },
        h("span", { class: "needs-dot", "aria-hidden": "true" }),
        d.needs === 1 ? "1 thing needs you" : `${d.needs} things need you`, h("span", { "aria-hidden": "true" }, " →"))),
    d.businesses.length
      ? h("section", { class: "businesses", "aria-label": "Health by business" }, d.businesses.map(businessSection))
      : h("div", { class: "empty" }, h("strong", {}, "No business yet"), h("span", {}, "Create one with New business: its section will show here.")),
    h("p", { class: "hq-foot" }, "Each business has its own section; figures are never added up across businesses. A vital turns amber only when it leaves its normal range."));
}

function businessSection(b) {
  const state = b.health.state;
  return h("div", { class: `business-section ${state.replace(" ", "-")}` },
    h("div", { class: "business-top" },
      h("h2", {}, h("a", { href: pageHash("needs-you", b.name) }, b.name)),
      h("span", { class: "health" }, h("span", { class: "health-dot", "aria-hidden": "true" }), HEALTH[state] || state)),
    h("p", { class: "health-why" }, b.health.sentence),
    b.vitals.length ? h("div", { class: "vitals" }, b.vitals.map((v) =>
      h("div", { class: v.out ? "vital out" : "vital" },
        h("span", { class: "vital-label" }, v.label),
        h("span", { class: "vital-value" }, v.value),
        h("span", { class: "vital-note" }, v.note))))
      : h("p", { class: "no-activity" }, "No vitals yet: they come with its routines, then from its own server."),
    finance(b.finance),
    h("div", { class: "activity" },
      h("span", { class: "block-label" }, "Latest activity", h("span", { class: "block-sub" }, `Your time this week ${b.time} · model cost ${money(b.cost_usd, 0)}`)),
      b.activity.length
        ? h("ol", {}, b.activity.map((a) => h("li", {},
            h("span", { class: "agent-mark", "aria-hidden": "true", style: `--agent: ${agentColour(a.agent)}` }),
            h("span", { class: "activity-text" }, h("strong", {}, a.agent_title || a.agent || "Airth"), ` · ${a.sentence}`),
            h("span", { class: "activity-at" }, a.at))))
        : h("span", { class: "no-activity" }, "No run yet")));
}

/** Finance (decisions 54 and 68): a bank's figures or those entered by
    hand, why they are unknown, or the empty state. A figure the source
    does not give (money in, by hand) is a dash. */
function finance(f) {
  const month = new Date().toLocaleDateString("en-GB", { month: "long" });
  const sub = !f ? "No figures yet" : f.unknown ? `Unknown: ${f.unknown}` : f.source;
  const cells = [["Cash", "cash"], [`In, ${month}`, "in"], [`Out, ${month}`, "out"], ["Runway", "runway"]];
  return h("div", { class: "finance" },
    h("span", { class: "block-label" }, "Finance", h("span", { class: f && f.unknown ? "block-sub warn" : "block-sub" }, sub)),
    h("div", { class: "finance-row" }, cells.map(([label, key]) =>
      h("div", { class: "finance-cell" },
        h("span", { class: "vital-label" }, label),
        h("span", { class: f && !f.unknown ? "finance-value known" : "finance-value" }, f && !f.unknown ? f[key] ?? "—" : f ? "Unknown" : "—")))));
}
