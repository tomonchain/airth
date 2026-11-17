// What an item of Needs you shows, the same in the list, the open item and
// a conversation's card.

type Shown = { kind: string; title: string; text?: string | null; source?: { title?: string | null; text?: string | null } | null }

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

/** A draft's words to approve, if it has any: never a record's fields. */
export const draft = (it: Shown): string | null => (it.kind === "approval" && it.text && !isRecord(it.text) ? it.text : null)

/** What the item is about in one line: its title, else its source's. */
export const heading = (it: Shown): string => it.title || it.source?.title || it.source?.text || ""
