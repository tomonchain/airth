// The HQ (docs/design/mockups/hq.html, decision 53): one section per
// business, never a total across them. Health, vitals and the activity
// sentences come in words from the server, built by code from Airth's
// records; finance waits for the bank (decision 54).

const hq = { data: undefined };

const HEALTH = { healthy: "Healthy", watch: "Watch", "at risk": "At risk", "not started": "Not started" };

/** The HQ, in the Reading layout (decision 72): the sentence that says
    what matters today is the header's summary. */
function HqPage() {
  if (hq.data === undefined) {
    hq.data = null;
    api("hq").then((d) => { hq.data = d; renderPanel(); }).catch((e) => { hq.data = { error: e.message }; renderPanel(); });
  }
  const d = hq.data;
  const header = { label: today(), title: "HQ" };
  if (!d) return html`<${Reading} kind="hq" header=${header}><p class="lede">Loading…</p></${Reading}>`;
  if (d.error) return html`<${Reading} kind="hq" header=${header}><p class="note bad">${d.error}</p></${Reading}>`;
  const first = d.businesses.find((b) => b.needs > 0);
  const needs = d.needs > 0 && html`<a class="needs-link" href=${pageHash("needs-you", first.name)}>
    <span class="needs-dot" aria-hidden="true"></span>${d.needs === 1 ? "1 thing needs you" : `${d.needs} things need you`}<span aria-hidden="true"> →</span></a>`;
  return html`<${Reading} kind="hq" header=${{ ...header, summary: d.headline, actions: needs }}>
    ${d.businesses.length
      ? html`<section class="businesses" aria-label="Health by business">${d.businesses.map((b) => html`<${BusinessSection} b=${b} />`)}</section>`
      : html`<div class="empty"><strong>No business yet</strong><span>Create one with New business: its section will show here.</span></div>`}
    <p class="hq-foot">Each business has its own section; figures are never added up across businesses. A vital turns amber only when it leaves its normal range.</p>
  </${Reading}>`;
}

function BusinessSection({ b }) {
  const state = b.health.state;
  return html`<div class=${`business-section ${state.replace(" ", "-")}`}>
    <div class="business-top">
      <h2><a href=${pageHash("needs-you", b.name)}>${b.name}</a></h2>
      <span class="health"><span class="health-dot" aria-hidden="true"></span>${HEALTH[state] || state}</span>
    </div>
    <p class="health-why">${b.health.sentence}</p>
    ${b.vitals.length
      ? html`<div class="vitals">${b.vitals.map((v) => html`<div class=${v.out ? "vital out" : "vital"}>
          <span class="vital-label">${v.label}</span><span class="vital-value">${v.value}</span><span class="vital-note">${v.note}</span></div>`)}</div>`
      : html`<p class="no-activity">No vitals yet: they come with its routines, then from its own server.</p>`}
    <${Finance} f=${b.finance} />
    <div class="activity">
      <span class="block-label">Latest activity<span class="block-sub">Your time this week ${b.time} · model cost ${money(b.cost_usd, 0)}</span></span>
      ${b.activity.length
        ? html`<ol>${b.activity.map((a) => html`<li>
            <span class="agent-mark" aria-hidden="true" style=${`--agent: ${agentColour(a.agent)}`}></span>
            <span class="activity-text"><strong>${a.agent_title || a.agent || "Airth"}</strong> · ${a.sentence}</span>
            <span class="activity-at">${a.at}</span></li>`)}</ol>`
        : html`<span class="no-activity">No run yet</span>`}
    </div>
  </div>`;
}

/** Finance (decisions 54 and 68): a bank's figures or those entered by
    hand, why they are unknown, or the empty state. A figure the source
    does not give (money in, by hand) is a dash. */
function Finance({ f }) {
  const month = new Date().toLocaleDateString("en-GB", { month: "long" });
  const sub = !f ? "No figures yet" : f.unknown ? `Unknown: ${f.unknown}` : f.source;
  const cells = [["Cash", "cash"], [`In, ${month}`, "in"], [`Out, ${month}`, "out"], ["Runway", "runway"]];
  return html`<div class="finance">
    <span class="block-label">Finance<span class=${f && f.unknown ? "block-sub warn" : "block-sub"}>${sub}</span></span>
    <div class="finance-row">${cells.map(([label, key]) => html`<div class="finance-cell">
      <span class="vital-label">${label}</span>
      <span class=${f && !f.unknown ? "finance-value known" : "finance-value"}>${f && !f.unknown ? f[key] ?? "—" : f ? "Unknown" : "—"}</span></div>`)}</div>
  </div>`;
}
