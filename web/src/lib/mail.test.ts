import { describe, expect, it } from "vitest"
import { recent, reply, went, type MailLine } from "./mail.ts"

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
    expect(went({ ...base, outcome: "failed" }, name).needs).toBe(true)
  })

  it("says plainly why a mail was not passed on, with no code word", () => {
    expect(went({ ...base, outcome: "old" }, name).text).toBe("Not passed on: it is older than 3 days.")
    for (const outcome of ["old", "before", "sorting", null] as const) {
      const w = went({ ...base, outcome }, name)
      expect(w.text).not.toMatch(/sort|dispatch|realm|\brun\b/i)
      expect(w.task).toBeUndefined()
    }
  })
})

describe("a mail's reply", () => {
  const passed = { ...base, outcome: "agent" as const, run: "r1" }
  it("waits for the founder in Needs you, until it is sent", () => {
    expect(reply({ ...passed, item: 3, item_status: "pending" })).toEqual({ text: "Its reply waits for you.", item: 3 })
    expect(reply({ ...passed, item: 3, item_status: "done", replied: true })).toEqual({ text: "Its reply was sent." })
    expect(reply({ ...passed, item: 3, item_status: "rejected" })?.text).toBe("You turned its reply down.")
  })

  it("is nothing when no reply was drafted", () => {
    expect(reply(passed)).toBeNull()
    expect(reply({ ...base, outcome: "bank", item: 7, item_status: "pending" })).toBeNull()
  })
})

describe("the line that opens Mail", () => {
  it("counts the week's mail", () => {
    const now = new Date("2026-11-18T12:00:00Z")
    const mails = [base, { ...base, date: "2026-11-01T09:00:00Z" }, { ...base, date: "2026-11-12T09:00:00Z" }]
    expect(recent(mails, now)).toBe(2)
  })
})
