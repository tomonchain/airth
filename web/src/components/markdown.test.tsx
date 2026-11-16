import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { FileLinks, Markdown, chart, csv, figure } from "./markdown"

const html = (text: string) => renderToStaticMarkup(<Markdown text={text} />)

describe("a link to a workspace file (decision 146)", () => {
  const opened: string[] = []
  const inApp = (text: string) => renderToStaticMarkup(<FileLinks.Provider value={(p) => opened.push(p)}><Markdown text={text} /></FileLinks.Provider>)

  it("opens in the app, never as an address", () => {
    const out = inApp("See [the Q3 plan](docs/finance/q3-plan.md).")
    expect(out).toMatch(/<button type="button"[^>]*data-file="docs\/finance\/q3-plan.md"[^>]*>the Q3 plan<\/button>/)
    expect(out).not.toContain("href=")
  })

  it("is plain text where nothing opens files", () => {
    const out = html("See [the Q3 plan](docs/finance/q3-plan.md).")
    expect(out).toContain("<span>the Q3 plan</span>")
    expect(out).not.toContain("<button")
  })

  it("stays text when it leaves the workspace or is not a path", () => {
    for (const bad of ["../secrets.env", "/etc/passwd", "docs/../../x", "docs/%2e%2e/x", "C:\\x", "file:///etc/passwd", "javascript:alert(1)"]) {
      const out = inApp(`[x](${bad})`)
      expect(out, bad).not.toContain("<button")
      expect(out, bad).not.toContain("href=")
    }
  })

  it("leaves web links as they were", () => {
    const out = inApp("[the deck](https://docs.example.com/deck)")
    expect(out).toContain('href="https://docs.example.com/deck"')
    expect(out).toContain('rel="noopener noreferrer"')
  })
})

const REPORT = `# Sales pipeline 2026-W47

**12 deals** open, 3 new this week.

| Deal | Stage | Value |
| --- | --- | --- |
| Acme | Proposal | €12,400 |
| Globex | Closing | €8,950 |

- Follow up with Acme
- Send Globex the contract
`

describe("a report", () => {
  const out = html(REPORT)

  it("shows its heading, bold, table and list formatted, not as raw Markdown", () => {
    expect(out).toMatch(/<h3[^>]*>Sales pipeline 2026-W47<\/h3>/)
    expect(out).toContain('<strong class="font-semibold">12 deals</strong>')
    expect(out).toMatch(/<table[^>]*>/)
    expect(out).toMatch(/<th[^>]*>Deal<\/th>/)
    expect(out).toMatch(/<ul[^>]*>\s*<li>Follow up with Acme<\/li>/)
    for (const raw of ["# ", "**", "| ---", "- Follow"]) expect(out).not.toContain(raw)
  })

  it("aligns the column of figures right, and only that one", () => {
    expect(out).toMatch(/<th[^>]*data-figure="true"[^>]*>Value<\/th>/)
    expect(out).toMatch(/<td[^>]*data-figure="true"[^>]*>€12,400<\/td>/)
    expect(out).not.toMatch(/<td[^>]*data-figure="true"[^>]*>Acme<\/td>/)
    expect(out).not.toMatch(/<th[^>]*data-figure="true"[^>]*>Stage<\/th>/)
  })

  it("keeps a table scrolling inside its frame", () => {
    expect(out).toMatch(/<div class="[^"]*overflow-x-auto[^"]*"><table/)
  })
})

describe("GitHub's extensions", () => {
  it("shows a task list and strikethrough", () => {
    const out = html("- [x] Draft sent\n- [ ] Call back\n\n~~old price~~")
    const boxes = out.match(/<input [^>]*>/g) ?? []
    expect(boxes).toHaveLength(2)
    expect(boxes.every((b) => b.includes('type="checkbox"') && b.includes('disabled=""'))).toBe(true)
    expect(boxes[0]).toContain('checked=""')
    expect(boxes[1]).not.toContain("checked")
    expect(out).toMatch(/<del[^>]*>old price<\/del>/)
  })
})

describe("untrusted content", () => {
  it("never renders raw HTML", () => {
    const out = html('Hello <script>alert(1)</script> <img src="https://x.test/a.png" onerror="alert(2)">')
    expect(out).not.toContain("<script")
    expect(out).not.toContain("<img")
    expect(out).not.toContain("onerror")
  })

  it("loads no image: shows its alt text and domain instead", () => {
    const out = html("![Pipeline chart](https://tracker.example.com/pixel.png?u=1)")
    expect(out).not.toContain("<img")
    expect(out).toContain("Pipeline chart · tracker.example.com")
    expect(out).not.toContain("pixel.png")
  })

  it("drops javascript: and data: links, keeping their text", () => {
    const out = html("[click](javascript:alert(1)) and [file](data:text/html,<b>x</b>) and [v](vbscript:x)")
    expect(out).not.toMatch(/href=/)
    expect(out).toContain("click")
    expect(out).toContain("file")
  })

  it("keeps mailto links, and no relative or anchor link", () => {
    const out = html("[write](mailto:team@example.com), [up](../secrets.env) and [here](#top)")
    expect(out).toContain('href="mailto:team@example.com"')
    expect(out.match(/href=/g)).toHaveLength(1)
    expect(out).toContain("up")
    expect(out).toContain("here")
  })

  it("opens a web link in a new tab, without the opener, its domain on hover", () => {
    const out = html("[the deck](https://docs.example.com/deck)")
    expect(out).toContain('href="https://docs.example.com/deck"')
    expect(out).toContain('target="_blank"')
    expect(out).toContain('rel="noopener noreferrer"')
    expect(out).toContain('title="docs.example.com"')
  })
})

describe("Airth's blocks", () => {
  it("shows a csv block as a table, figures right", () => {
    const out = html("```csv\nMonth,Revenue\nOctober,1200\nNovember,1450\n```")
    expect(out).toMatch(/<th[^>]*>Month<\/th>/)
    expect(out).toMatch(/<td[^>]*data-figure="true"[^>]*>1450<\/td>/)
  })

  it("shows a chart block as a chart, and a broken one as code", () => {
    const chart = html('```chart\n{"title":"Signups","labels":["Mon","Tue"],"series":[{"values":[3,5]}]}\n```')
    expect(chart).toContain("<figure")
    expect(chart).toContain("Signups")
    expect(html("```chart\nnot json\n```")).toMatch(/<pre[^>]*>not json<\/pre>/)
  })

  it("reads a csv block's separator outside quotes", () => {
    expect(csv('"Q1, US, foo";Revenue\nx;1')).toEqual([["Q1, US, foo", "Revenue"], ["x", "1"]])
  })

  it("takes a chart only with text where text goes and finite numbers", () => {
    expect(chart('{"labels":["Mon"],"series":[{"values":[3]}]}')).not.toBeNull()
    expect(chart('{"labels":["Mon","Tue"],"series":[{"values":[3,null]}]}')).not.toBeNull()
    for (const bad of [
      '{"labels":[{"x":1}],"series":[{"values":[3]}]}',
      '{"title":{"x":1},"labels":["Mon"],"series":[{"values":[3]}]}',
      '{"labels":["Mon"],"series":[{"name":[1],"values":[3]}]}',
      '{"labels":["Mon"],"series":[{"values":["3"]}]}',
      '{"labels":["Mon"],"series":[]}',
      '{"type":"pie","labels":["Mon"],"series":[{"values":[3]}]}',
      "null",
    ]) expect(chart(bad), bad).toBeNull()
    // Rendered, a malformed chart is code, never a crash.
    expect(html('```chart\n{"labels":[{"x":1}],"series":[{"values":[3]}]}\n```')).toMatch(/<pre/)
  })

  it("draws a bar below zero with no height, never a negative one", () => {
    const out = html('```chart\n{"labels":["Mon","Tue"],"series":[{"values":[-2,4]}]}\n```')
    const heights = [...out.matchAll(/height:([^%;"]+)%/g)].map((m) => Number(m[1]))
    expect(heights).toEqual([0, 100])
  })

  it("shows other code in a scrolling block, in a mono font", () => {
    const out = html("```sh\nairth doctor\n```\n\nRun `airth ui`.")
    expect(out).toMatch(/<pre class="[^"]*overflow-x-auto[^"]*font-mono[^"]*"><code>airth doctor<\/code><\/pre>/)
    expect(out).toMatch(/<code class="[^"]*font-mono[^"]*">airth ui<\/code>/)
  })
})

describe("figure", () => {
  it("knows a figure from a word", () => {
    for (const f of ["1,200", "-3.5%", "€12", "1 200 €", "+4", "$1.2k", "0"]) expect(figure(f), f).toBe(true)
    for (const w of ["Acme", "Q4", "n/a", "12 deals", ""]) expect(figure(w), w).toBe(false)
  })
})
