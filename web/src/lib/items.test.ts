import { describe, expect, it } from "vitest"
import { draft, heading, isRecord } from "./items.ts"

describe("an item's words", () => {
  it("tells a record's fields from a draft", () => {
    expect(isRecord('{ "actions": "4 done" }')).toBe(true)
    expect(isRecord("{braces} open this post, then words")).toBe(false)
    expect(isRecord("[1, 2]")).toBe(false)
    expect(isRecord("Our autumn prints are in.")).toBe(false)
    expect(isRecord(null)).toBe(false)
  })

  it("shows a draft's words, else its title, else its source's", () => {
    const report = { kind: "approval", title: "", text: '{"a": 1}', source: { text: "Week 47" } }
    expect(draft(report)).toBeNull()
    expect(heading(report)).toBe("Week 47")
    const post = { kind: "approval", title: "A post", text: "{Hello} world" }
    expect(draft(post)).toBe("{Hello} world")
    expect(draft({ kind: "incident", title: "Late", text: "detail" })).toBeNull()
    expect(heading({ kind: "incident", title: "Late" })).toBe("Late")
  })
})
