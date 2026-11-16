import { describe, expect, it } from "vitest"
import { ago, left, took } from "./time.ts"

const now = Date.parse("2026-11-16T12:00:00Z")
const before = (s: number) => new Date(now - s * 1000).toISOString()
const after = (s: number) => new Date(now + s * 1000).toISOString()

describe("ago", () => {
  it("says how long ago, in the largest unit that fits", () => {
    expect(ago(before(10), now)).toBe("just now")
    expect(ago(before(35 * 60), now)).toBe("35m ago")
    expect(ago(before(5 * 3600), now)).toBe("5h ago")
    expect(ago(before(3 * 86400), now)).toBe("3d ago")
  })

  it("says nothing without a time", () => {
    expect(ago(null, now)).toBe("")
  })
})

describe("left", () => {
  it("counts down to a deadline", () => {
    expect(left(after(30), now)).toBe("expiring")
    expect(left(after(40 * 60), now)).toBe("40m left")
    expect(left(after(5 * 3600), now)).toBe("5h left")
    expect(left(after(3 * 86400), now)).toBe("3d left")
  })
})

describe("took", () => {
  it("says how long a run took", () => {
    expect(took("2026-11-16T12:00:00Z", null)).toBe("")
    expect(took("2026-11-16T12:00:00Z", "2026-11-16T12:00:00.500Z")).toBe("<1s")
    expect(took("2026-11-16T12:00:00Z", "2026-11-16T12:01:05Z")).toBe("1m 5s")
  })
})
