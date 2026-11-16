import { describe, expect, it } from "vitest"
import { greeting, moment } from "./greeting.ts"

// 16 November 2026 is a Monday.
const at = (day: number, hour: number, minute = 0) => new Date(2026, 10, day, hour, minute)

describe("moment", () => {
  it("cuts the day at 5, 12, 18 and 23", () => {
    expect(moment(at(16, 4, 59))).toBe("night")
    expect(moment(at(16, 5))).toBe("morning")
    expect(moment(at(16, 11, 59))).toBe("morning")
    expect(moment(at(16, 12))).toBe("afternoon")
    expect(moment(at(16, 18))).toBe("evening")
    expect(moment(at(16, 23))).toBe("night")
  })
})

describe("greeting", () => {
  it("speaks to the moment and the day", () => {
    expect(["New week, Thomas. Where do we start?", "Good morning, Thomas. What's first this week?"]).toContain(greeting(at(16, 9), "Thomas"))
    expect(["Happy Friday, Thomas", "Friday afternoon, Thomas. What's left?"]).toContain(greeting(at(20, 15), "Thomas"))
    expect(greeting(at(21, 11), "Thomas")).toMatch(/Saturday|Weekend|today/)
    expect(greeting(at(17, 2), "Thomas")).toMatch(/late|up|quiet hour/)
  })

  it("stays the same through a moment of a day, and moves on with the day", () => {
    expect(greeting(at(17, 9), "Thomas")).toBe(greeting(at(17, 11, 30), "Thomas"))
    // Tuesday to Thursday afternoons: never twice the same in a row.
    const days = [17, 18, 19].map((d) => greeting(at(d, 14), "Thomas"))
    expect(days[0]).not.toBe(days[1])
    expect(days[1]).not.toBe(days[2])
  })

  it("keeps a night's line past midnight", () => {
    expect(greeting(at(17, 23, 30), "Thomas")).toBe(greeting(at(18, 2), "Thomas"))
    expect(greeting(at(18, 23, 30), "Thomas")).not.toBe(greeting(at(17, 23, 30), "Thomas"))
  })

  it("reads well without a name", () => {
    for (let h = 0; h < 24; h++) {
      const g = greeting(at(18, h), null)
      expect(g).not.toContain("{")
      expect(g).not.toMatch(/^,|, [.?]|,$/)
    }
    expect(greeting(at(18, 9), "Ada")).toContain("Ada")
  })
})
