// The app's address: the business, the page, and what is open in it, so a
// reload or a shared link comes back to the same place and Back moves
// between places. `#/b/<business>/<page>/<what is open>`, each part
// URL-encoded; what is open may span segments (a document's path,
// `project/12`).

export const PAGES = ["home", "overview", "agents", "needs", "routines", "tasks", "calendar", "documents", "finance", "usage", "hq", "company", "settings", "marketplace"] as const
export type PageName = (typeof PAGES)[number]

export type Address = { realm: string | null; page: PageName; sub: string | null }

const decode = (s: string) => {
  try { return decodeURIComponent(s) } catch { return s }
}

/** Reads a hash. A business alone opens its home; an unknown page is the
    home too, with nothing open. */
export function parse(hash: string): Address {
  const m = hash.match(/^#\/b\/([^/?#]+)(?:\/([^/?#]+)((?:\/[^/?#]*)*))?/)
  if (!m) return { realm: null, page: "home", sub: null }
  const realm = decode(m[1])
  const page = (PAGES as readonly string[]).includes(m[2] ?? "") ? (m[2] as PageName) : "home"
  if (page !== m[2]) return { realm, page, sub: null }
  const parts = (m[3] ?? "").split("/").slice(1).map(decode)
  const sub = parts.length && parts.every((p) => p !== "") ? parts.join("/") : null
  return { realm, page, sub }
}

/** Writes an address; each segment of what is open is encoded on its own,
    so a path keeps its slashes. */
export function write(a: { realm: string; page: PageName; sub?: string | null }): string {
  const sub = a.sub ? `/${a.sub.split("/").map(encodeURIComponent).join("/")}` : ""
  return `#/b/${encodeURIComponent(a.realm)}/${a.page}${sub}`
}

/** Whether going from one address to the next adds a step to the history
    (Back comes back to it) or only corrects the current one: a new page,
    or opening or closing something, is a step; moving from one open thing
    to the next on the same page (the next item, another tab) is not. */
export function isStep(from: Address | null, to: Address): boolean {
  if (!from || from.realm !== to.realm || from.page !== to.page) return true
  if (from.sub === to.sub) return false
  return from.sub === null || to.sub === null
}
