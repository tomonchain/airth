import { describe, expect, it } from "vitest"
import { exchanges, firstLine, isOpen, openByDefault, remember, remembered, summary } from "./fold.ts"

describe("exchanges", () => {
  it("starts one at each of the founder's messages", () => {
    expect(exchanges(["founder", "work", "answer", "founder", "answer"])).toEqual([{ start: 0, end: 3 }, { start: 3, end: 5 }])
    expect(exchanges(["founder", "founder", "answer"])).toEqual([{ start: 0, end: 1 }, { start: 1, end: 3 }])
  })

  it("keeps a task passed on by an agent in one, from its first turn", () => {
    expect(exchanges(["work", "answer"])).toEqual([{ start: 0, end: 2 }])
    expect(exchanges([])).toEqual([])
  })
})

describe("openByDefault", () => {
  it("folds what is over, except the last", () => {
    expect(openByDefault("done", false)).toBe(false)
    expect(openByDefault("done", true)).toBe(true)
    expect(openByDefault("running", false)).toBe(true)
    expect(openByDefault("waiting", false)).toBe(true)
    expect(openByDefault("failed", false)).toBe(false)
    expect(openByDefault("stopped", false)).toBe(false)
    expect(openByDefault("failed", true)).toBe(true)
  })

  it("gives way to the founder's choice", () => {
    expect(isOpen(false, true)).toBe(true)
    expect(isOpen(true, false)).toBe(false)
    expect(isOpen(true, null)).toBe(true)
  })
})

describe("summary", () => {
  it("says the request and the answer's first line", () => {
    expect(summary("List the orders paid and not shipped.\nAll of them.", "Four orders are paid.\n\nDetails…", "done"))
      .toEqual({ request: "List the orders paid and not shipped.", result: "Four orders are paid." })
  })

  it("says where it stands when it is not over, or nothing was answered", () => {
    expect(summary("Reorder", "A draft", "waiting").result).toBe("Waiting for you")
    expect(summary("Reorder", null, "failed").result).toBe("Failed")
    expect(summary("Reorder", null, "running").result).toBe("Working")
    expect(summary("Reorder", "Half done", "stopped").result).toBe("Stopped")
    expect(summary("Reorder", null, "done").result).toBe("Done")
  })

  it("cuts long lines and drops Markdown's marks", () => {
    expect(firstLine("**Three** prints, see [the sheet](https://x.y) and `stock`", 80)).toBe("Three prints, see the sheet and stock")
    expect(firstLine("## Week 47", 80)).toBe("Week 47")
    expect(firstLine("1. Review the deck", 80)).toBe("Review the deck")
    expect(firstLine("2) Send it", 80)).toBe("Send it")
    expect(firstLine("\n\n- first point", 80)).toBe("first point")
    const long = firstLine("a".repeat(200), 80)
    expect(long).toHaveLength(80)
    expect(long.endsWith("…")).toBe(true)
  })
})

describe("remembered", () => {
  const store = () => {
    const m = new Map<string, string>()
    return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) }
  }

  it("keeps the founder's choice per exchange", () => {
    const s = store()
    expect(remembered(s, "atelier:t1:0")).toBeNull()
    remember(s, "atelier:t1:0", true)
    remember(s, "atelier:t1:3", false)
    expect(remembered(s, "atelier:t1:0")).toBe(true)
    expect(remembered(s, "atelier:t1:3")).toBe(false)
  })

  it("does without a storage that refuses", () => {
    const broken = { getItem: () => { throw new Error("denied") }, setItem: () => { throw new Error("denied") } }
    expect(remembered(broken, "k")).toBeNull()
    expect(() => remember(broken, "k", true)).not.toThrow()
    expect(remembered(null, "k")).toBeNull()
  })
})
