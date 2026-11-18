import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { Results } from "./evaluation"
import { agreement, differs, estimateText, skips, split, type Case, type Evaluation } from "@/lib/evaluation"

const at = "2026-11-17T08:00:00Z"
const one = (item: number, founder: Case["founder"], kept?: boolean, more: Partial<Case> = {}): Case =>
  ({ item, at, title: `Story ${item}`, founder, said: "", draft: `Draft ${item}`, edited: "", kept, ...more })

const evaluation: Evaluation = {
  routine: "posts",
  cases: 3,
  cost: "",
  cost_usd: 0.05,
  blocks: [
    {
      id: "pick", prompt: "prompts/pick.md", kind: "keeps", cost_usd: 0.01,
      score: { wanted: 2, wanted_kept: 1, rejected: 1, rejected_dropped: 1, rate: 2 / 3, before: 2 / 3 },
      cases: [one(11, "approve", true), one(12, "edit", false), one(13, "reject", false, { said: "off topic" })],
    },
    {
      id: "draft", prompt: "prompts/post.md", kind: "drafts", cost_usd: 0.04,
      cases: [one(11, "approve", undefined, { new: "New 11" }), one(12, "edit", undefined, { edited: "Mine 12", new: "New 12" })],
    },
  ],
  skipped: ["draft: item 13: the answer did not fit, twice", "check: neither keeps nor drafts one item at a time: nothing to compare with"],
}

describe("an evaluation (decision 151)", () => {
  it("says what a keeping block does otherwise than the founder", () => {
    expect(differs(one(1, "reject", true))).toBe(true)
    expect(differs(one(1, "approve", false))).toBe(true)
    expect(differs(one(1, "redo", true))).toBe(false)
    expect(differs(one(1, "reject", undefined))).toBe(false)
    const { differ, match } = split(evaluation.blocks[0].cases)
    expect(differ.map((c) => c.item)).toEqual([12])
    expect(match.map((c) => c.item)).toEqual([11, 13])
  })

  it("counts agreement as the founder's decisions matched", () => {
    expect(agreement(evaluation.blocks[0].score)).toEqual({ now: "2 of 3 match your decisions", before: "2 of 3" })
    expect(agreement({ wanted: 0, wanted_kept: 0, rejected: 0, rejected_dropped: 0, rate: null, before: null })).toBeNull()
  })

  it("names a skipped answer by its title, never its number", () => {
    expect(skips(evaluation)).toEqual([
      { block: "draft", title: "Story 13", why: "The answer did not fit, twice" },
      { block: "check", title: null, why: "Neither keeps nor drafts one item at a time: nothing to compare with" },
    ])
    expect(skips({ ...evaluation, blocks: [], skipped: ["pick: item 99: no answer"] })[0].title).toBe("A past answer")
  })

  it("says what a run may cost before it starts", () => {
    expect(estimateText({ cases: 12, blocks: 2 })).toBe("12 past answers × 2 AI blocks: 24 model calls, one each.")
    expect(estimateText({ cases: 1, blocks: 1 })).toBe("1 past answer × 1 AI block: 1 model call, one each.")
  })

  it("shows where a block differs first, drafts beside the founder's, and folds what was skipped", () => {
    const html = renderToStaticMarkup(<Results e={evaluation} />)
    expect(html).toContain("2 of 3 match your decisions")
    expect(html.indexOf("Where it differs from you")).toBeLessThan(html.indexOf("Where it matches you"))
    expect(html.indexOf("Story 12")).toBeLessThan(html.indexOf("Story 11"))
    expect(html).toContain("Mine 12")
    expect(html).toContain("New 12")
    expect(html).toContain("Your edit")
    expect(html).toMatch(/<details[^>]*>.*Not measured: 2/)
    expect(html).not.toContain("<details open")
    expect(html).not.toMatch(/item \d/)
  })

  it("says so when nothing was answered", () => {
    const html = renderToStaticMarkup(<Results e={{ ...evaluation, cases: 0, blocks: [], skipped: ["nothing answered: no item of posts reached you in the last 60 days"] }} />)
    expect(html).toContain("No past answers yet")
    expect(html).not.toContain("Not measured")
  })
})
