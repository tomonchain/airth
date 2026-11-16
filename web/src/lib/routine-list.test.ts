import { test } from "vitest"
import assert from "node:assert/strict"
import { businessOrder, filter, frequency, graphTrigger, groups, needsLook, schedule, state, triggerText, type Row, type Waiting } from "./routine-list.ts"

const row = (name: string, more: Partial<Row> = {}): Row => ({
  kind: "graph", name, agent: "writer", trigger: { kind: "schedule", every: "daily 07:00" },
  paused: false, broken: false, next_at: null, last: null, level: null, ...more,
})
const item = (kind: string, routine: string | null, more: Partial<Waiting> = {}): Waiting => ({ kind, status: "pending", routine, run: null, ...more })

test("each kind of schedule the scheduler takes reads as a sentence", () => {
  const cases: [string, string][] = [
    ["every 30m", "Every 30 minutes"],
    ["every 1m", "Every minute"],
    ["every 1h", "Every hour"],
    ["every 2h", "Every 2 hours"],
    ["every 1d", "Every day"],
    ["every 3days", "Every 3 days"],
    ["daily 07:00", "Every day at 07:00"],
    ["daily 7:00", "Every day at 07:00"],
    ["weekdays 09:00", "Weekdays at 09:00"],
    ["weekends 10:30", "Weekends at 10:30"],
    ["mon 08:00", "Every Monday at 08:00"],
    ["monday 08:00", "Every Monday at 08:00"],
    ["thu,mon 18:00", "Mondays and Thursdays at 18:00"],
    ["mon,wed,fri 08:00", "Mondays, Wednesdays and Fridays at 08:00"],
  ]
  for (const [text, read] of cases) assert.equal(schedule(text), read, text)
})

test("a schedule the scheduler would refuse never shows its raw text", () => {
  for (const text of ["0 7 * * *", "sometimes", "every week", "daily noon", "every 0m", "every 367d", "daily 25:00", "mon 08:60"]) assert.equal(schedule(text), "On a schedule", text)
})

test("each kind of trigger reads as a sentence", () => {
  assert.equal(triggerText(graphTrigger({ schedule: "daily 07:00" })), "Every day at 07:00")
  assert.equal(triggerText(graphTrigger({ manual: true })), "When you run it")
  assert.equal(triggerText(graphTrigger(undefined)), "When you run it")
  assert.equal(triggerText(graphTrigger({ telegram: "x_post" })), "When you send a post link on Telegram")
  assert.equal(triggerText(graphTrigger({ webhook: "statera" })), "When statera sends an event")
})

test("how often a routine runs", () => {
  const s = (every: string) => frequency({ kind: "schedule", every })
  assert.equal(s("every 2h"), "daily")
  assert.equal(s("daily 07:00"), "daily")
  assert.equal(s("weekdays 09:00"), "daily")
  assert.equal(s("weekends 09:00"), "weekly")
  assert.equal(s("mon 08:00"), "weekly")
  assert.equal(s("every 7d"), "weekly")
  assert.equal(s("every 30d"), "monthly")
  assert.equal(frequency({ kind: "telegram", event: "x_post" }), "event")
  assert.equal(frequency({ kind: "webhook", name: "statera" }), "event")
  assert.equal(frequency({ kind: "manual" }), "manual")
  assert.equal(s("daily 25:00"), "other")
})

test("a routine's state: what broke first, then what waits, then paused", () => {
  assert.equal(state(row("a"), []), "ok")
  assert.equal(state(row("a", { paused: true }), []), "paused")
  assert.equal(state(row("a"), [item("approval", "a")]), "waiting")
  assert.equal(state(row("a"), [item("prepared", "a")]), "waiting")
  assert.equal(state(row("a"), [item("incident", "a")]), "incident")
  assert.equal(state(row("a", { last: { at: "2026-11-16T07:00:00Z", ok: false } }), []), "incident")
  assert.equal(state(row("a", { broken: true }), []), "incident")
  // An incident outranks a draft waiting, and both outrank a pause.
  assert.equal(state(row("a", { paused: true }), [item("approval", "a"), item("incident", "a")]), "incident")
  assert.equal(state(row("a", { paused: true }), [item("approval", "a")]), "waiting")
  // What is answered, or another routine's, or a report, is not waiting.
  assert.equal(state(row("a"), [item("approval", "a", { status: "approved" })]), "ok")
  assert.equal(state(row("a"), [item("approval", "b")]), "ok")
  assert.equal(state(row("a"), [item("report", "a")]), "ok")
  // An agent routine's draft names its run, not the routine; a drawn
  // routine of the same name does not read it, nor the other way round.
  const agentRow = row("a", { kind: "agent", last: { at: "2026-11-16T07:00:00Z", ok: true, run: "r1" } })
  assert.equal(state(agentRow, [item("approval", null, { run: "r1" })]), "waiting")
  assert.equal(state(agentRow, [item("approval", "a")]), "ok")
  assert.equal(state(row("a"), [item("approval", null, { run: "r1" })]), "ok")
  assert.equal(state(row("a", { kind: "sequence" }), [item("approval", "a")]), "ok")
})

test("needs a look: incidents, then what waits, and nothing when all is well", () => {
  const rows = [
    row("calm"), row("drafted"), row("failed", { last: { at: "2026-11-16T07:00:00Z", ok: false } }), row("asleep", { paused: true }), row("alarm"),
  ]
  const items = [item("approval", "drafted"), item("incident", "alarm")]
  assert.deepEqual(needsLook(rows, items).map((r) => r.name), ["alarm", "failed", "drafted"])
  assert.deepEqual(needsLook([row("calm"), row("asleep", { paused: true })], []), [])
})

test("groups by agent, then no agent and fixed sequences, each with its next run", () => {
  const rows = [
    row("b", { agent: "writer", next_at: "2026-11-17T08:00:00Z" }),
    row("a", { agent: "writer", next_at: "2026-11-16T08:00:00Z" }),
    row("p", { agent: "writer", paused: true, next_at: "2026-11-16T06:00:00Z" }),
    row("w", { agent: "watcher" }),
    row("g", { kind: "agent", agent: null }),
    row("s", { kind: "sequence", agent: null }),
  ]
  const g = groups(rows, "agent", [{ name: "watcher", title: "Watcher" }, { name: "writer", title: "Writer" }])
  assert.deepEqual(g.map((x) => [x.label, x.rows.map((r) => r.name)]), [
    ["Watcher", ["w"]], ["Writer", ["a", "b", "p"]], ["No agent", ["g"]], ["Fixed sequences", ["s"]],
  ])
  // A paused routine's time is not the group's next run.
  assert.equal(g[1].next_at, "2026-11-16T08:00:00Z")
})

test("groups by frequency, in a fixed order, empty ones left out", () => {
  const rows = [
    row("m", { trigger: { kind: "schedule", every: "mon 08:00" } }),
    row("d", { trigger: { kind: "schedule", every: "daily 07:00" } }),
    row("t", { trigger: { kind: "telegram", event: "x_post" } }),
  ]
  assert.deepEqual(groups(rows, "frequency", []).map((x) => x.label), ["Daily", "Weekly", "On an event"])
})

test("filters: a name, what needs a look, paused, one agent", () => {
  const rows = [row("weekly-report"), row("replies", { agent: "post-writer" }), row("watch", { paused: true })]
  const items = [item("approval", "replies")]
  const names = (f: Parameters<typeof filter>[1]) => filter(rows, f, items).map((r) => r.name)
  assert.deepEqual(names({ search: "REP", show: "all", agent: null }), ["weekly-report", "replies"])
  assert.deepEqual(names({ search: "", show: "look", agent: null }), ["replies"])
  assert.deepEqual(names({ search: "", show: "paused", agent: null }), ["watch"])
  assert.deepEqual(names({ search: "", show: "all", agent: "post-writer" }), ["replies"])
})

test("groups by business: the open one first, then the others in the menu's order", () => {
  const all = [{ name: "atelier", title: "Atelier" }, { name: "hedgen", title: "Hedgen" }, { name: "numirama", title: "Numirama" }]
  const order = businessOrder("hedgen", all)
  assert.deepEqual(order.map((b) => b.name), ["hedgen", "atelier", "numirama"])
  const rows = [
    row("b", { business: "atelier", next_at: "2026-11-17T07:00:00Z" }),
    row("a", { business: "atelier", next_at: "2026-11-16T07:00:00Z" }),
    row("watch", { business: "hedgen" }),
  ]
  const g = groups(rows, "business", [], order)
  // Numirama has no routine shown: no group.
  assert.deepEqual(g.map((x) => [x.label, x.rows.map((r) => r.name)]), [["Hedgen", ["watch"]], ["Atelier", ["a", "b"]]])
  assert.equal(g[1].next_at, "2026-11-16T07:00:00Z")
})

test("an item counts only for a routine of its own business, even under the same name", () => {
  const here = row("weekly-report", { business: "hedgen" })
  const there = row("weekly-report", { business: "atelier" })
  const items = [item("incident", "weekly-report", { business: "atelier" })]
  assert.equal(state(here, items), "ok")
  assert.equal(state(there, items), "incident")
  assert.deepEqual(needsLook([here, there], items).map((r) => r.business), ["atelier"])
  assert.deepEqual(filter([here, there], { search: "", show: "look", agent: null }, items).map((r) => r.business), ["atelier"])
})

test("an agent filter names its business when the list holds several", () => {
  const rows = [row("a", { agent: "chief-of-staff", business: "hedgen" }), row("b", { agent: "chief-of-staff", business: "atelier" })]
  const f = { search: "", show: "all" as const, agent: "atelier/chief-of-staff" }
  assert.deepEqual(filter(rows, f, []).map((r) => r.name), ["b"])
  assert.deepEqual(filter(rows, { ...f, agent: "chief-of-staff" }, []).map((r) => r.name), ["a", "b"])
})
