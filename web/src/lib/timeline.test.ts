import { describe, expect, it } from "vitest"
import { chronological, moment, taskAt, type Kind } from "./timeline.ts"

const e = (kind: Kind, at: string | null, name: string) => ({ kind, at, name })
const names = (xs: { name: string }[]) => xs.map((x) => x.name)

describe("moment", () => {
  it("reads the server's times, whatever their form", () => {
    expect(moment("2026-11-16T09:00:00Z")).toBe(Date.UTC(2026, 10, 16, 9))
    expect(moment("2026-11-16T09:00:00+00:00")).toBe(Date.UTC(2026, 10, 16, 9))
    expect(moment("2026-11-16T10:00:00+01:00")).toBe(Date.UTC(2026, 10, 16, 9))
    expect(moment("2026-11-16T09:00:00.123456789+00:00")).toBe(Date.UTC(2026, 10, 16, 9, 0, 0, 123))
    expect(moment("2026-11-16T09:00:00.5Z")).toBe(Date.UTC(2026, 10, 16, 9, 0, 0, 500))
    // No zone: the server's, so UTC, wherever the browser is.
    expect(moment("2026-11-16T09:00:00")).toBe(Date.UTC(2026, 10, 16, 9))
    expect(moment(null)).toBeNull()
    expect(moment("")).toBeNull()
    expect(moment("not a time")).toBeNull()
  })
})

describe("chronological", () => {
  it("interleaves tasks, runs and Needs you items by time", () => {
    const out = chronological([
      e("task", "2026-11-16T14:05:00Z", "task"),
      e("run", "2026-11-16T08:00:00Z", "morning run"),
      e("item", "2026-11-16T12:00:00.250+00:00", "approval at noon"),
      e("run", "2026-11-16T15:00:00Z", "afternoon run"),
    ])
    expect(names(out)).toEqual(["morning run", "approval at noon", "task", "afternoon run"])
  })

  it("compares times, not strings: a fraction of a second or an offset never jumps ahead", () => {
    // As strings, ".5+00:00" sorts before "Z" and "10:00+01:00" after "09:30Z".
    const out = chronological([
      e("item", "2026-11-16T09:00:00Z", "first"),
      e("run", "2026-11-16T09:00:00.5+00:00", "half a second later"),
      e("item", "2026-11-16T10:00:00+01:00", "nine o'clock UTC"),
      e("run", "2026-11-16T09:30:00Z", "half past nine"),
    ])
    expect(names(out)).toEqual(["first", "nine o'clock UTC", "half a second later", "half past nine"])
  })

  it("puts the founder's task before what answers it at the same moment, then keeps the given order", () => {
    const out = chronological([
      e("item", "2026-11-16T09:00:00Z", "item"),
      e("run", "2026-11-16T09:00:00Z", "run"),
      e("task", "2026-11-16T09:00:00Z", "task"),
      e("item", "2026-11-16T09:00:00Z", "second item"),
    ])
    expect(names(out)).toEqual(["task", "run", "item", "second item"])
  })

  it("puts what has no time yet last, in the order given", () => {
    const out = chronological([e("task", null, "just started"), e("run", "2026-11-16T09:00:00Z", "run"), e("item", null, "unknown")])
    expect(names(out)).toEqual(["run", "just started", "unknown"])
  })
})

describe("taskAt", () => {
  it("places a task at its last activity, else at its start", () => {
    // The founder's bug: a task started at 9, last spoken to at 14:05; an
    // approval at noon used to come after it, under the founder's last words.
    const started = "2026-11-16T09:00:00.000Z"
    const out = chronological([
      e("task", taskAt("2026-11-16T14:05:00Z", started), "task"),
      e("item", "2026-11-16T12:00:00Z", "approval at noon"),
    ])
    expect(names(out)).toEqual(["approval at noon", "task"])
    expect(taskAt(undefined, started)).toBe(started)
  })
})
