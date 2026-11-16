import { describe, expect, it } from "vitest"
import { isStep, PAGES, parse, write } from "./address.ts"

describe("parse and write", () => {
  it("round-trips every page, with and without something open", () => {
    for (const page of PAGES) {
      for (const sub of [null, "x", "project/12"]) {
        const hash = write({ realm: "northwind", page, sub })
        expect(parse(hash)).toEqual({ realm: "northwind", page, sub })
      }
    }
  })

  it("keeps a document's path, its slashes, spaces and accents", () => {
    const sub = "docs/légal/Plan Q3 #2 & co.md"
    const hash = write({ realm: "atelier", page: "documents", sub })
    expect(hash).toBe("#/b/atelier/documents/docs/l%C3%A9gal/Plan%20Q3%20%232%20%26%20co.md")
    expect(parse(hash)).toEqual({ realm: "atelier", page: "documents", sub })
  })

  it("encodes the business's name", () => {
    expect(write({ realm: "a b", page: "home" })).toBe("#/b/a%20b/home")
    expect(parse("#/b/a%20b/home").realm).toBe("a b")
  })

  it("reads what the app wrote before: a business alone, an agent", () => {
    expect(parse("#/b/northwind")).toEqual({ realm: "northwind", page: "home", sub: null })
    expect(parse("#/b/northwind/agents/writer")).toEqual({ realm: "northwind", page: "agents", sub: "writer" })
    expect(parse("#/b/northwind/home/conversation")).toEqual({ realm: "northwind", page: "home", sub: "conversation" })
  })

  it("falls back to the home on what it cannot read", () => {
    expect(parse("")).toEqual({ realm: null, page: "home", sub: null })
    expect(parse("#/welcome")).toEqual({ realm: null, page: "home", sub: null })
    expect(parse("#/b/northwind/nowhere/12")).toEqual({ realm: "northwind", page: "home", sub: null })
    expect(parse("#/b/northwind/tasks/")).toEqual({ realm: "northwind", page: "tasks", sub: null })
    expect(parse("#/b/northwind/tasks/a//b")).toEqual({ realm: "northwind", page: "tasks", sub: null })
    // A malformed escape is kept as written rather than breaking the app.
    expect(parse("#/b/northwind/documents/50%25%ZZ").sub).toBe("50%25%ZZ")
  })
})

describe("isStep", () => {
  const at = (page: (typeof PAGES)[number], sub: string | null = null, realm = "n") => ({ realm, page, sub })
  it("adds a step for a new page, a new business, or opening and closing", () => {
    expect(isStep(null, at("home"))).toBe(true)
    expect(isStep(at("home"), at("tasks"))).toBe(true)
    expect(isStep(at("tasks"), at("tasks", null, "other"))).toBe(true)
    expect(isStep(at("tasks"), at("tasks", "t1"))).toBe(true)
    expect(isStep(at("tasks", "t1"), at("tasks"))).toBe(true)
  })

  it("only corrects the address moving from one open thing to the next, or for nothing new", () => {
    expect(isStep(at("needs", "4"), at("needs", "5"))).toBe(false)
    expect(isStep(at("tasks", "t1"), at("tasks", "t1"))).toBe(false)
  })
})
