import { describe, expect, it } from "vitest"
import { draft, goes, heading, isRecord } from "./items.ts"

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
    expect(heading({ kind: "approval", title: "", source: { text: '{"a": 1}' } })).toBe("")
  })
})

describe("a routine's item that goes unless cancelled", () => {
  it("shows its words as a draft only when the founder can edit them", () => {
    expect(draft({ kind: "delayed", title: "Flows", text: "Flows, calmly.", editable: true })).toBe("Flows, calmly.")
    expect(draft({ kind: "delayed", title: "Post", text: "POST blog: posts", editable: false })).toBeNull()
  })

  it("says when it goes", () => {
    const now = new Date(2026, 10, 18, 9, 0)
    expect(goes(new Date(2026, 10, 18, 14, 30).toISOString(), now)).toBe("Goes at 14:30 unless you cancel")
    expect(goes(new Date(2026, 10, 19, 8, 5).toISOString(), now)).toBe("Goes tomorrow at 08:05 unless you cancel")
    expect(goes(new Date(2026, 10, 21, 8, 5).toISOString(), now)).toBe("Goes Sat 21 Nov at 08:05 unless you cancel")
    expect(goes(null, now)).toBe("Goes out unless you cancel")
  })
})
