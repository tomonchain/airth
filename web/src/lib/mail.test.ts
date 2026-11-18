import { describe, expect, it } from "vitest"
import { reply, went, type MailLine } from "./mail.ts"

const base: MailLine = {
  id: 1, sender: "ann@buyer.example", subject: "Lot 12", date: "2026-11-18T09:00:00Z", snippet: "", bank: false,
  outcome: null, agent: null, summary: null, run: null, item: null, item_status: null, replied: false,
}
const name = (a: string) => (a === "support" ? "Sam, Support" : a)

describe("what the interlocutor did with a mail", () => {
  it("names the agent, says what the mail is, and opens its task", () => {
    const m = { ...base, outcome: "agent" as const, agent: "support", summary: "asks when lot 12 ships", run: "r1" }
    expect(went(m, name)).toEqual({ text: "Passed to Sam, Support: asks when lot 12 ships.", tone: "agent", task: "r1" })
  })

  it("sends the founder to Needs you, to the bank flag's line while it waits", () => {
    expect(went({ ...base, outcome: "founder", summary: "a press request" }, name)).toMatchObject({ text: "For you: a press request. It is in Needs you.", needs: true })
    expect(went({ ...base, outcome: "founder" }, name).text).toBe("For you. It is in Needs you.")
    expect(went({ ...base, outcome: "bank", item: 7, item_status: "pending" }, name)).toMatchObject({ tone: "bank", item: 7 })
    expect(went({ ...base, outcome: "bank", item: 7, item_status: "done" }, name).item).toBeUndefined()
    expect(went({ ...base, outcome: "failed" }, name)).toEqual({ text: "I could not pass it on, so it is in Needs you.", tone: "you", needs: true })
  })

  it("says plainly why a mail was not passed on, opening nothing", () => {
    const cases = [
      ["old", "Not passed on: it is older than 3 days."],
      ["before", "Not passed on: it came before mail went to the team."],
      ["sorting", "Passing it on now."],
      [null, "Not passed on yet."],
    ] as const
    for (const [outcome, text] of cases) {
      expect(went({ ...base, outcome }, name)).toEqual({ text, tone: "quiet" })
    }
  })
})

describe("a mail's reply", () => {
  const passed = { ...base, outcome: "agent" as const, run: "r1", item: 3 }
  it("waits for the founder in Needs you, until it is sent", () => {
    expect(reply({ ...passed, item_status: "pending" })).toEqual({ text: "Its reply waits for you.", item: 3 })
    expect(reply({ ...passed, item_status: "done", replied: true })).toEqual({ text: "Its reply was sent." })
    expect(reply({ ...passed, item_status: "done" })).toEqual({ text: "Its reply was sent." })
  })

  it("says what came of it otherwise, with nothing left to review", () => {
    expect(reply({ ...passed, item_status: "approved" })).toEqual({ text: "You approved its reply." })
    expect(reply({ ...passed, item_status: "rejected" })).toEqual({ text: "You turned its reply down." })
    expect(reply({ ...passed, item_status: "expired" })).toEqual({ text: "Its reply expired." })
    expect(reply({ ...passed, item_status: "failed" })).toEqual({ text: "Its reply could not be sent." })
  })

  it("is nothing when no reply was drafted", () => {
    expect(reply({ ...passed, item: null })).toBeNull()
    expect(reply({ ...passed, item_status: "cancelled" })).toBeNull()
    expect(reply({ ...base, outcome: "bank", item: 7, item_status: "pending" })).toBeNull()
  })
})
