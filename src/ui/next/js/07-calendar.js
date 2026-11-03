// Calendar (docs/design/mockups/calendar.html): the business's routine runs
// over the seven days from today, from their schedules, with what became of
// today's, the scheduled runs that started late in the last seven days, and
// the legal deadlines of the next twelve months (decision 66).

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
          (day.deadlines || []).map((d) => h("div", { class: "slot deadline" },
            h("span", { class: "slot-text" }, h("span", { class: "time" }, "Due"), h("span", { class: "routine-name" }, d)))),
          day.runs.length ? day.runs.map(slot) : !(day.deadlines || []).length && h("span", { class: "nothing" }, "Nothing scheduled")))),
      deadlinesBlock(data.deadlines),
      h("aside", { class: "late-runs", "aria-labelledby": "late-runs" },
        h("h2", { id: "late-runs" }, "Started late, last 7 days"),
        data.late.length
          ? data.late.map((l) => h("div", { class: "late-row" },
              h("span", { class: "name" }, l.routine),
              h("span", { class: "sub" }, `Due ${l.due}, started ${minutes(l.late_by * 60)} late`)))
          : h("p", { class: "note" }, "None: every scheduled run started on time."),
        h("p", { class: "note" }, "A run is late when it starts more than 15 minutes after its time: the Mac was asleep, or Airth stopped."))));
}

/** The legal deadlines: each with its date, days left and source, or why
    its date is unknown. */
function deadlinesBlock(d) {
  if (!d) return null;
  return h("aside", { class: "late-runs deadlines", "aria-labelledby": "deadlines" },
    h("h2", { id: "deadlines" }, "Deadlines"),
    d.items.length
      ? d.items.map((i) => h("div", { class: "late-row" },
          h("span", { class: "name" }, i.title),
          h("span", { class: "sub" }, i.authority, i.period_end ? ` · for the period to ${i.period_end}` : ""),
          i.unknown
            ? h("span", { class: "sub warn" }, i.unknown)
            : h("span", { class: !i.filed && i.days_left <= 30 ? "sub warn" : "sub" }, i.filed ? `Filed · due ${i.due}`
              : i.days_left < 0 ? `Overdue · was due ${i.due}`
              : `Due ${i.due} · ${i.days_left === 0 ? "today" : i.days_left === 1 ? "tomorrow" : `in ${i.days_left} days`}`),
          i.source && h("a", { class: "sub", href: i.source, target: "_blank", rel: "noopener" }, "Source")))
      : h("p", { class: "note" }, "No deadline in the next twelve months."),
    h("p", { class: "note" }, d.note));
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
