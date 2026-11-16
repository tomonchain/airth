import { describe, expect, it } from "vitest"
import { workspacePath } from "./workspace-link.ts"

describe("workspacePath", () => {
  it("takes a relative path inside the workspace, normalised", () => {
    expect(workspacePath("docs/finance/q3-plan.md")).toBe("docs/finance/q3-plan.md")
    expect(workspacePath("./docs/./a.md")).toBe("docs/a.md")
    expect(workspacePath("docs//minutes/2026-11-16.md")).toBe("docs/minutes/2026-11-16.md")
    expect(workspacePath("docs/brand/Brand%20book.pdf")).toBe("docs/brand/Brand book.pdf")
    expect(workspacePath("business/profile.md")).toBe("business/profile.md")
  })

  it("refuses anything that leaves the workspace or is not a path", () => {
    for (const bad of [
      "../secrets.env", "docs/../../x", "docs/..", "..", "/etc/passwd", "~/x", "docs/%2e%2e/x", "docs/%252e%252e/x",
      "C:\\x", "C:/x", "docs\\x", "file:///etc/passwd", "javascript:alert(1)", "data:text/html,x", "vbscript:x",
      "https://example.com/x", "mailto:a@b.c", "docs/a.md?x=1", "docs/a.md#top", "#top", "docs/", "docs/.", "", "  ", "docs/%E0%A4%A",
      "docs/a\u007f.md", "docs/a\u0085.md",
    ]) expect(workspacePath(bad), bad).toBeNull()
    expect(workspacePath(null)).toBeNull()
  })

  it("takes only what reads as a file, not an address without its scheme", () => {
    for (const bad of ["acme.com", "www.acme.com/pricing", "acme.co.uk/about.html", "john@example.com", "docs/minutes", "README", ".env"]) {
      expect(workspacePath(bad), bad).toBeNull()
    }
    expect(workspacePath("plan.md")).toBe("plan.md")
    expect(workspacePath("docs/brand/logo.svg")).toBe("docs/brand/logo.svg")
  })
})
