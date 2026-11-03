// Calendar (docs/design/mockups/calendar.html): the business's routine runs
// over the seven days from today, from their schedules, with what became of
// today's, the scheduled runs that started late in the last seven days, and
// the legal deadlines of the next twelve months (decision 66).

const calendar = { data: {} };

// What a run's state reads as; a planned run says nothing more.
const RUN_STATES = { done: "Done", late: "Started late", failed: "Failed", "not run": "Did not run", next: "Next" };

/** Calendar, in the Board layout (decision 72). */
function CalendarPage() {
  const realm = next.realm;
  const data = loaded(calendar.data, "calendar");
  const header = { label: realm, title: "Calendar" };
  if (!data) return html`<${Board} header=${header}><p class="lede">Loading…</p></${Board}>`;
  if (data.error) return html`<${Board} header=${header}><p class="note bad">${data.error}</p></${Board}>`;
  return html`<${Board} header=${{ ...header, summary: data.title }}>
    <div class="calendar"><div class="cal">
      <section class="week" aria-label="The week from today">${data.days.map((day) => html`
        <div class=${day.today ? "day today" : "day"}>
          <div class="day-head"><span class="weekday">${day.today ? `${day.weekday} · today` : day.weekday}</span><span class="date-num">${day.day}</span></div>
          ${(day.deadlines || []).map((d) => html`<div class="slot deadline"><span class="slot-text"><span class="time">Due</span><span class="routine-name">${d}</span></span></div>`)}
          ${day.runs.length ? day.runs.map((r) => html`<${Slot} r=${r} />`) : !(day.deadlines || []).length && html`<span class="nothing">Nothing scheduled</span>`}
        </div>`)}</section>
      <${Deadlines} d=${data.deadlines} />
      <aside class="late-runs" aria-labelledby="late-runs">
        <h2 id="late-runs">Started late, last 7 days</h2>
        ${data.late.length
          ? data.late.map((l) => html`<div class="late-row"><span class="name">${l.routine}</span><span class="sub">Due ${l.due}, started ${minutes(l.late_by * 60)} late</span></div>`)
          : html`<p class="note">None: every scheduled run started on time.</p>`}
        <p class="note">A run is late when it starts more than 15 minutes after its time: the Mac was asleep, or Airth stopped.</p>
      </aside>
    </div></div>
  </${Board}>`;
}

/** The legal deadlines: each with its date, days left and source, or why
    its date is unknown. */
function Deadlines({ d }) {
  if (!d) return null;
  const when = (i) => (i.filed ? `Filed · due ${i.due}`
    : i.days_left < 0 ? `Overdue · was due ${i.due}`
    : `Due ${i.due} · ${i.days_left === 0 ? "today" : i.days_left === 1 ? "tomorrow" : `in ${i.days_left} days`}`);
  return html`<aside class="late-runs deadlines" aria-labelledby="deadlines">
    <h2 id="deadlines">Deadlines</h2>
    ${d.items.length
      ? d.items.map((i) => html`<div class="late-row">
          <span class="name">${i.title}</span>
          <span class="sub">${i.authority}${i.period_end ? ` · for the period to ${i.period_end}` : ""}</span>
          ${i.unknown ? html`<span class="sub warn">${i.unknown}</span>` : html`<span class=${!i.filed && i.days_left <= 30 ? "sub warn" : "sub"}>${when(i)}</span>`}
          ${i.source && html`<a class="sub" href=${i.source} target="_blank" rel="noopener">Source</a>`}
        </div>`)
      : html`<p class="note">No deadline in the next twelve months.</p>`}
    <p class="note">${d.note}</p>
  </aside>`;
}

function Slot({ r }) {
  const state = RUN_STATES[r.state];
  return html`<div class=${`slot ${r.state.replace(" ", "-")}`}>
    <span class="agent-dot" aria-hidden="true" style=${`--agent: ${agentColour(r.agent)}`}></span>
    <span class="slot-text"><span class="time">${r.time}</span><span class="routine-name">${r.routine}</span>${state && html`<span class="run-state">${state}</span>`}</span>
  </div>`;
}
