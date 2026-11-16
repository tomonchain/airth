import { describe, expect, it } from "vitest"
import type { Item, Routine } from "@/lib/api"
import { agentRow, filter, frequency, graphRow, group, needsLook, scheduleText, sequenceRow, state, triggerText } from "./routines"

describe("triggerText", () => {
  it("says every kind of schedule the engine accepts in plain words", () => {
    expect(triggerText({ schedule: "daily 07:00" })).toBe("Every day at 07:00")
    expect(triggerText({ schedule: "weekdays 09:00" })).toBe("Weekdays at 09:00")
    expect(triggerText({ schedule: "weekends 10:30" })).toBe("Weekends at 10:30")
    expect(triggerText({ schedule: "mon 08:00" })).toBe("Mondays at 08:00")
    expect(triggerText({ schedule: "mon,thu 18:00" })).toBe("Mondays and Thursdays at 18:00")
    expect(triggerText({ schedule: "fri,mon,wed 7:30" })).toBe("Mondays, Wednesdays and Fridays at 07:30")
    expect(triggerText({ schedule: "monday,friday 09:00" })).toBe("Mondays and Fridays at 09:00")
    expect(triggerText({ schedule: "mon,tue,wed,thu,fri 09:00" })).toBe("Weekdays at 09:00")
    expect(triggerText({ schedule: "every 30m" })).toBe("Every 30 minutes")
    expect(triggerText({ schedule: "every 1min" })).toBe("Every minute")
    expect(triggerText({ schedule: "every 1h" })).toBe("Every hour")
    expect(triggerText({ schedule: "every 2hrs" })).toBe("Every 2 hours")
    expect(triggerText({ schedule: "every 1d" })).toBe("Every day")
    expect(triggerText({ schedule: "every 3days" })).toBe("Every 3 days")
    expect(triggerText({ schedule: "every 120m" })).toBe("Every 2 hours")
  })

  it("says the events and the founder's own start", () => {
    expect(triggerText({ telegram: "x_post" })).toBe("When you send a post link")
    expect(triggerText({ webhook: "shop-order" })).toBe("When the shop-order webhook is called")
    expect(triggerText({ manual: true })).toBe("When you run it")
    expect(triggerText({})).toBe("When you run it")
  })

  it("never shows a schedule's own syntax", () => {
    for (const odd of ["0 10 1 * *", "monthly 1 10:00", "daily 25:00", "every 0m", "someday 08:00"]) {
      expect(scheduleText(odd)).toBe("On a schedule Airth cannot read")
    }
  })
})

describe("frequency", () => {
  it("groups by how often a routine runs", () => {
    expect(frequency({ schedule: "every 2h" })).toBe("daily")
    expect(frequency({ schedule: "daily 07:00" })).toBe("daily")
    expect(frequency({ schedule: "weekdays 09:00" })).toBe("daily")
    expect(frequency({ schedule: "mon 08:00" })).toBe("weekly")
    expect(frequency({ schedule: "weekends 10:00" })).toBe("weekly")
    expect(frequency({ schedule: "every 7d" })).toBe("weekly")
    expect(frequency({ schedule: "every 30d" })).toBe("monthly")
    expect(frequency({ telegram: "x_post" })).toBe("event")
    expect(frequency({ webhook: "shop-order" })).toBe("event")
    expect(frequency({ manual: true })).toBe("manual")
  })
})

let id = 0
const item = (over: Partial<Item>): Item => ({
  id: ++id, status: "pending", kind: "approval", agent: null, routine: null, run: null, title: "An item", text: null, note: null,
  flag: null, flag_reason: null, editable: false, revision: 0, retry: null, overdue: false, execute_at: null, created_at: "2026-11-16T08:00:00Z", source: null,
  ...over,
})

const routine = (name: string, over: Partial<Routine> = {}): Routine => ({
  name, agent_title: "Watcher", cost_usd: 0, unpriced: 0, next_at: "2026-11-17T07:00:00Z", runs: [], source: "", version: "",
  graph: { description: "", agent: "watcher", trigger: { schedule: "daily 07:00", manual: null, telegram: null, webhook: null }, blocks: [] },
  ...over,
})

describe("needsLook", () => {
  it("lists routines with an incident, a failed last run, a file that does not hold, or something waiting for the founder", () => {
    const items = [
      item({ kind: "incident", routine: "daily-watch", agent: "watcher", title: "No heartbeat" }),
      item({ kind: "approval", routine: "daily-posts", agent: "watcher" }),
      item({ kind: "prepared", routine: "replies", agent: "watcher" }),
      // Not waiting for the founder: answered, a report, another business's.
      item({ kind: "approval", routine: "weekly-report", status: "approved" }),
      item({ kind: "report", routine: "weekly-report" }),
    ]
    const rows = [
      graphRow("n", routine("daily-watch"), {}, items),
      graphRow("n", routine("daily-posts"), {}, items),
      graphRow("n", routine("replies"), {}, items),
      graphRow("n", routine("weekly-report"), {}, items),
      graphRow("n", routine("broken", { graph: undefined, error: "block 'send': not a declared action" })),
      graphRow("n", routine("ok-again", { runs: [{ at: "2026-11-16T07:00:00Z", ok: true, error: null }, { at: "2026-11-15T07:00:00Z", ok: false, error: "timeout" }] })),
      agentRow("n", { id: "weekly-article", every: "mon 08:00", bot: "writer", enabled: true, next_at: null, last_at: "2026-11-16T08:00:00Z", last_status: "failed" }),
      agentRow("n", { id: "stopped-one", every: null, bot: null, enabled: true, next_at: null, last_at: "2026-11-16T08:00:00Z", last_status: "stopped" }),
      agentRow("n", { id: "fine", every: null, bot: null, enabled: false, next_at: null, last_at: "2026-11-16T08:00:00Z", last_status: "completed" }),
      sequenceRow("n", { name: "watch", runs: [{ started_at: "2026-11-16T06:00:00Z", ok: false, error: "feed down" }] }),
    ]
    const look = needsLook(rows)
    // Incidents (and failures, broken files) first, then waiting; by name within.
    expect(look.map((l) => l.row.name)).toEqual(["broken", "daily-watch", "stopped-one", "watch", "weekly-article", "daily-posts", "replies"])
    expect(look.find((l) => l.row.name === "daily-watch")!.reasons.map((r) => r.kind)).toEqual(["incident"])
    expect(look.find((l) => l.row.name === "weekly-article")!.reasons).toEqual([{ kind: "failed", error: "failed", at: "2026-11-16T08:00:00Z" }])
    expect(look.find((l) => l.row.name === "replies")!.reasons.map((r) => r.kind)).toEqual(["waiting"])
  })

  it("is empty when every routine is fine, so the section is not shown", () => {
    const rows = [graphRow("n", routine("daily-watch", { runs: [{ at: "2026-11-16T07:00:00Z", ok: true, error: null }] })), agentRow("n", { id: "x", every: null, bot: null, enabled: false, next_at: null, last_at: null, last_status: null })]
    expect(needsLook(rows)).toEqual([])
  })

  it("matches items to a routine by name and agent, never another agent's", () => {
    const items = [item({ kind: "incident", routine: "daily-watch", agent: "someone-else" })]
    expect(needsLook([graphRow("n", routine("daily-watch"), {}, items)])).toEqual([])
  })
})

describe("state", () => {
  it("puts an incident before waiting, and waiting before paused", () => {
    const o = { id: "a", every: null, bot: null, enabled: false, next_at: null, last_at: null, last_status: null }
    expect(state(agentRow("n", o))).toBe("paused")
    expect(state(agentRow("n", o, null, [item({ kind: "approval", routine: "a" })]))).toBe("waiting")
    expect(state(agentRow("n", o, null, [item({ kind: "approval", routine: "a" }), item({ kind: "incident", routine: "a" })]))).toBe("incident")
    expect(state(agentRow("n", { ...o, enabled: true }))).toBe("ok")
  })
})

describe("graphRow", () => {
  it("finds the agent of a file that does not load, when its source names one", () => {
    expect(graphRow("n", routine("odd", { graph: undefined, error: "x", source: '{"agent": "writer", "blocks": []}' })).agent).toBe("writer")
    expect(graphRow("n", routine("odd", { graph: undefined, error: "x", source: "not json" })).agent).toBeNull()
  })
})

describe("graphRow's level", () => {
  it("is the lowest level of its acts' actions", () => {
    const blocks = [{ id: "pick", type: "ai" }, { id: "send", type: "act", action: "typefully.create_draft" }, { id: "report", type: "act", action: "airth.report" }]
    const r = routine("daily-posts", { graph: { ...routine("x").graph!, blocks } })
    const row = graphRow("n", r, { "typefully.create_draft": 1, "airth.report": 3, other: 0 })
    expect(row.level).toBe(1)
    expect(row.levels).toEqual([{ action: "typefully.create_draft", level: 1 }, { action: "airth.report", level: 3 }])
    expect(graphRow("n", routine("no-act")).level).toBeNull()
  })
})

describe("group and filter", () => {
  const rows = [
    ...["a", "b", "c", "d", "e", "f"].map((n) => graphRow("n", routine(n, { next_at: `2026-11-1${n === "c" ? 6 : 8}T07:00:00Z` }))),
    agentRow("n", { id: "article", every: "mon 08:00", bot: "writer", enabled: false, next_at: null, last_at: null, last_status: null }, "Writer"),
    sequenceRow("other", { name: "watch", schedule: null, runs: [] }),
  ]

  it("groups by agent, with the group's earliest next run, no agent last", () => {
    const g = group(rows, "agent")
    expect(g.map((x) => [x.label, x.rows.length])).toEqual([["Watcher", 6], ["Writer", 1], ["No agent", 1]])
    expect(g[0].next).toBe("2026-11-16T07:00:00Z")
  })

  it("groups by frequency in a fixed order, and by business in the order given", () => {
    expect(group(rows, "frequency").map((x) => x.label)).toEqual(["Daily", "Weekly", "By hand"])
    expect(group(rows, "business", [{ name: "other", title: "Other Ltd" }, { name: "n", title: "Northwind" }]).map((x) => x.label)).toEqual(["Other Ltd", "Northwind"])
  })

  it("filters by name, by state and by agent", () => {
    expect(filter(rows, { query: "ART", status: "all", agent: null }).map((r) => r.name)).toEqual(["article"])
    expect(filter(rows, { query: "", status: "paused", agent: null }).map((r) => r.name)).toEqual(["article"])
    expect(filter(rows, { query: "", status: "all", agent: "writer" }).map((r) => r.name)).toEqual(["article"])
    expect(filter(rows, { query: "", status: "look", agent: null })).toEqual([])
  })
})
