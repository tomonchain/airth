// What an item of Needs you shows, the same in the list, the open item and
// a conversation's card.

type Shown = { kind: string; title: string; text?: string | null; editable?: boolean; source?: { title?: string | null; text?: string | null } | null }

/** A record's fields, sent as JSON where a draft would be: not words to show. */
export function isRecord(text: string | null | undefined): boolean {
  if (!text || !text.trimStart().startsWith("{")) return false
  try {
    const v: unknown = JSON.parse(text)
    return typeof v === "object" && v !== null && !Array.isArray(v)
  } catch {
    return false
  }
}

/** A draft's words to approve, if it has any: never a record's fields. A
    routine's draft that goes after a window unless cancelled is one too
    (decision 153). */
export const draft = (it: Shown): string | null =>
  (it.kind === "approval" || (it.kind === "delayed" && it.editable)) && it.text && !isRecord(it.text) ? it.text : null

/** When a delayed item goes, said plainly: the clock today, else the day. */
export function goes(iso: string | null | undefined, now = new Date()): string {
  if (!iso) return "Goes out unless you cancel"
  const d = new Date(iso)
  const clock = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })
  const tomorrow = new Date(now); tomorrow.setDate(now.getDate() + 1)
  const when = d.toDateString() === now.toDateString() ? ""
    : d.toDateString() === tomorrow.toDateString() ? "tomorrow "
    : `${d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" })} `
  return `Goes ${when}at ${clock} unless you cancel`
}

/** What the item is about in one line: its title, else its source's. */
export const heading = (it: Shown): string =>
  it.title || it.source?.title || (isRecord(it.source?.text) ? "" : it.source?.text) || ""
