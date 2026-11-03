// Calendar (docs/design/mockups/calendar.html): the business's routine runs
// over the seven days from today, from their schedules, with what became of
// today's, and the scheduled runs that started late in the last seven days.
// Deadlines come with the legal calendars.

const calendar = { data: {} };

// What a run's state reads as; a planned run says nothing more.
const RUN_STATES = { done: "Done", late: "Started late", failed: "Failed", "not run": "Did not run", next: "Next" };

function calendarPage() {
  const realm = next.realm;
  const data = loaded(calendar.data, "calendar");
  if (!data) return h("div", { class: "column" }, pageHead(realm, "Calendar"), h("p", { class: "lede" }, "Loading…"));
  if (data.error) return h("div", { class: "column" }, pageHead(realm, "Calendar"), h("p", { class: "note bad" }, data.error));
  return h("div", { class: "column calendar" },
    pageHead(realm, data.title),
    h("div", { class: "cal" },
      h("section", { class: "week", "aria-label": "The week from today" },
        data.days.map((day) => h("div", { class: day.today ? "day today" : "day" },
          h("div", { class: "day-head" },
            h("span", { class: "weekday" }, day.today ? `${day.weekday} · today` : day.weekday),
            h("span", { class: "date-num" }, day.day)),
          day.runs.length ? day.runs.map(slot) : h("span", { class: "nothing" }, "Nothing scheduled")))),
      h("aside", { class: "late-runs", "aria-labelledby": "late-runs" },
        h("h2", { id: "late-runs" }, "Started late, last 7 days"),
        data.late.length
          ? data.late.map((l) => h("div", { class: "late-row" },
              h("span", { class: "name" }, l.routine),
              h("span", { class: "sub" }, `Due ${l.due}, started ${minutes(l.late_by * 60)} late`)))
          : h("p", { class: "note" }, "None: every scheduled run started on time."),
        h("p", { class: "note" }, "A run is late when it starts more than 15 minutes after its time: the Mac was asleep, or Airth stopped."))));
}

function slot(r) {
  const state = RUN_STATES[r.state];
  return h("div", { class: `slot ${r.state.replace(" ", "-")}` },
    h("span", { class: "agent-dot", "aria-hidden": "true", style: `--agent: ${agentColour(r.agent)}` }),
    h("span", { class: "slot-text" },
      h("span", { class: "time" }, r.time),
      h("span", { class: "routine-name" }, r.routine),
      state && h("span", { class: "run-state" }, state)));
}
