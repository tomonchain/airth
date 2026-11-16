import { test } from "node:test"
import assert from "node:assert/strict"
import { layout, shown, type Span } from "./calendar-layout.ts"

const at = (...times: string[]): Span[] => times.map(shown)

test("two events at the same time take two columns", () => {
  const { placed, overflows } = layout(at("08:30", "08:30"))
  assert.deepEqual(placed.map((p) => [p.column, p.span, p.columns]), [[0, 1, 2], [1, 1, 2]])
  assert.equal(overflows.length, 0)
})

test("a chain where the ends do not touch needs two columns", () => {
  // A overlaps B, B overlaps C, A does not touch C.
  const { placed } = layout(at("08:00", "08:20", "08:40"))
  assert.deepEqual(placed.map((p) => [p.column, p.columns]), [[0, 2], [1, 2], [0, 2]])
})

test("five events at once show three columns and a +2", () => {
  const { placed, overflows } = layout(at("14:00", "14:00", "14:00", "14:00", "14:00"))
  assert.equal(placed.length, 3)
  assert.ok(placed.every((p) => p.columns === 3 && p.folded))
  assert.deepEqual(placed.map((p) => p.column).sort(), [0, 1, 2])
  assert.equal(overflows.length, 1)
  assert.equal(overflows[0].hidden.length, 2)
  assert.equal(overflows[0].slot.length, 5)
})

test("a display length makes close starts overlap; touching ends do not", () => {
  // 08:00 and 08:05 overlap though neither has a duration; 08:00 and 08:30
  // only touch.
  assert.deepEqual(layout(at("08:00", "08:05")).placed.map((p) => p.columns), [2, 2])
  assert.deepEqual(layout(at("08:00", "08:30")).placed.map((p) => [p.column, p.columns]), [[0, 1], [0, 1]])
})

test("an event spreads right over columns free for its whole height", () => {
  // A long run at 08:00 beside two short ones; at 09:00 only the long one is
  // still there, so the next spreads over columns 1 and 2.
  const { placed } = layout([{ start: 480, end: 600 }, { start: 480, end: 510 }, { start: 480, end: 510 }, { start: 540, end: 570 }])
  assert.deepEqual(placed[3], { index: 3, column: 1, span: 2, columns: 3, folded: false })
  // The short ones at 08:00 are beside each other: no room to spread.
  assert.deepEqual(placed.slice(0, 3).map((p) => p.span), [1, 1, 1])
})

test("the founder's Monday morning", () => {
  // weekly-report 08:00, morning-brief 08:05, daily-plan 08:20, two at 08:30.
  const { placed, overflows } = layout(at("08:00", "08:05", "08:20", "08:30", "08:30"))
  assert.equal(placed.length + overflows.reduce((n, o) => n + o.hidden.length, 0), 5)
  assert.ok(placed.every((p) => p.columns === 3))
  assert.deepEqual(overflows.map((o) => o.hidden), [[4]])
})
