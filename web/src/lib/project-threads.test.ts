import { describe, expect, it } from "vitest"
import { inThread, projectLines, type Threaded } from "./project-threads.ts"

const task = (id: string, at: string, project?: number | null) => ({ id, at, project })
const list = (done: Threaded[], running: Threaded[] = []) => ({ running, waiting: [], review: [], done })

describe("which thread a task goes to", () => {
  const general = task("a", "2026-11-16T09:00:00Z")
  const planned = task("b", "2026-11-16T09:00:00Z", 3)
  it("shows every task on an agent's page", () => {
    expect([general, planned].filter((t) => inThread(t, undefined))).toEqual([general, planned])
  })
  it("leaves a project's tasks out of the Home", () => {
    expect([general, planned].filter((t) => inThread(t, null))).toEqual([general])
    expect(inThread(task("c", "2026-11-16T09:00:00Z", null), null)).toBe(true)
  })
  it("keeps a project's thread to its own tasks", () => {
    expect([general, planned].filter((t) => inThread(t, 3))).toEqual([planned])
    expect([general, planned].filter((t) => inThread(t, 4))).toEqual([])
  })
})

describe("the Home's line per project", () => {
  const now = new Date("2026-11-16T12:00:00Z")
  const projects = [{ id: 1, name: "Website redesign" }, { id: 2, name: "Year's end" }]

  it("counts each project's tasks that moved this week, the latest first", () => {
    const lines = projectLines(list(
      [task("a", "2026-11-14T10:00:00Z", 1), task("b", "2026-11-15T10:00:00Z", 2), task("c", "2026-11-16T08:00:00Z")],
      [task("d", "2026-11-16T11:00:00+00:00", 1)],
    ), projects, now)
    expect(lines).toEqual([
      { id: 1, name: "Website redesign", count: 2, at: "2026-11-16T11:00:00+00:00" },
      { id: 2, name: "Year's end", count: 1, at: "2026-11-15T10:00:00Z" },
    ])
  })

  it("says nothing of a quiet project, or of one it does not know", () => {
    expect(projectLines(list([task("a", "2026-11-01T10:00:00Z", 1), task("b", "2026-11-16T10:00:00Z", 9)]), projects, now)).toEqual([])
    expect(projectLines(null, projects, now)).toEqual([])
  })
})
