import { describe, expect, it } from "vitest"
import { reply, went, type MailLine } from "./mail.ts"

const base: MailLine = {
  id: 1, sender: "ann@b.example", subject: "Lot 12", date: "2026-11-18T09:00:00Z", snippet: "", bank: false,
  outcome: null, agent: null, summary: null, run: null, item: null, item_status: null, replied: false,
}
const name = (a: string) => (a === "writer" ? "Writer" : a)

describe("where a mail went", () => {
  it("names the agent, says what the mail is, and opens its task", () => {
    const m = { ...base, outcome: "agent" as const, agent: "writer", summary: "asks when the lot ships", run: "r1" }
    expect(went(m, name)).toEqual({ text: "To Writer: asks when the lot ships", tone: "agent", task: "r1" })
  })

  it("sends the founder to Needs you, to the bank flag's line while it waits", () => {
    expect(went({ ...base, outcome: "founder", summary: "a press request" }, name)).toMatchObject({ text: "Sent to you: a press request", needs: true })
    expect(went({ ...base, outcome: "bank", item: 7, item_status: "pending" }, name)).toMatchObject({ text: "Bank details: sent to you", item: 7 })
    expect(went({ ...base, outcome: "bank", item: 7, item_status: "done" }, name).item).toBeUndefined()
    expect(went({ ...base, outcome: "failed" }, name).needs).toBe(true)
  })

  it("says plainly why a mail was not passed on, with no code word", () => {
    expect(went({ ...base, outcome: "old" }, name).text).toBe("Not passed on: older than 3 days")
    for (const outcome of ["old", "before", "sorting", null] as const) {
      const text = went({ ...base, outcome }, name).text
      expect(text).not.toMatch(/sort|dispatch|realm|run/i)
      expect(went({ ...base, outcome }, name).task).toBeUndefined()
    }
  })
})

describe("a mail's reply", () => {
  const sent = { ...base, outcome: "agent" as const, run: "r1" }
  it("waits for the founder in Needs you, until it is sent", () => {
    expect(reply({ ...sent, item: 3, item_status: "pending" })).toEqual({ text: "Its reply waits for you", item: 3 })
    expect(reply({ ...sent, item: 3, item_status: "done", replied: true })).toEqual({ text: "Reply sent" })
    expect(reply({ ...sent, item: 3, item_status: "rejected" })?.text).toBe("You turned its reply down")
  })

  it("is nothing when no reply was drafted", () => {
    expect(reply(sent)).toBeNull()
    expect(reply({ ...base, outcome: "bank", item: 7, item_status: "pending" })).toBeNull()
  })
})
