import { describe, expect, it } from "vitest"
import { chronological, during, moment, slots, type Kind, type Span } from "./timeline.ts"

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

const h = (hour: number, minute = 0) => Date.UTC(2026, 10, 16, hour, minute)

describe("during", () => {
  const spans: Span[] = [
    { id: "nine", from: h(9), to: h(14, 5) },
    { id: "eleven", from: h(11), to: h(11, 30) },
    { id: "now", from: h(16), to: null },
  ]

  it("finds the task an event came during", () => {
    expect(during(spans, h(12))).toBe("nine")
    expect(during(spans, h(9))).toBe("nine")
    expect(during(spans, h(14, 5))).toBe("nine")
    // A task still going holds everything after its start.
    expect(during(spans, h(23))).toBe("now")
  })

  it("leaves what came between tasks to the conversation", () => {
    expect(during(spans, h(8))).toBeNull()
    expect(during(spans, h(15))).toBeNull()
    expect(during(spans, null)).toBeNull()
  })

  it("gives an event during two tasks to the one started last", () => {
    expect(during(spans, h(11, 15))).toBe("eleven")
  })
})

describe("slots", () => {
  // A task: the order at 9, the answer at 9:10, the founder again at
  // 14:00, the answer at 14:05.
  const turns = [h(9), h(9, 10), h(14), h(14, 5)]

  it("puts an event between the messages around it", () => {
    // The founder's bug: an approval at noon came after the whole task,
    // under the founder's words of 14:00.
    expect(slots(turns, [h(12)], true)).toEqual([2])
    expect(slots(turns, [h(9, 5), h(14, 1), h(15)], true)).toEqual([1, 3, 4])
  })

  it("puts an event at the same moment as a message after it", () => {
    expect(slots(turns, [h(9, 10)], true)).toEqual([2])
  })

  it("places nothing before messages older than the record of times", () => {
    // A run begun before decision 148, then gone on with.
    expect(slots([h(9), null, null, h(14)], [h(10)], true)).toEqual([3])
    expect(slots([h(9), null, null, h(14)], [h(15)], true)).toEqual([4])
  })

  it("keeps a task without times whole, with what came during it above", () => {
    expect(slots([h(9), null, null], [h(10), h(12)], false)).toEqual([0, 0])
  })

  it("puts an event of unknown time last", () => {
    expect(slots(turns, [null], true)).toEqual([4])
  })
})
