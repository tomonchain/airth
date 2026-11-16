// The main chat folds each finished exchange into one line: what the
// founder asked, what came of it, and when. Kept here, apart from the
// screen, so it is tested.

/** An exchange: a message of the founder's and everything up to the next
    one. `start` and `end` are indexes of turns, `end` excluded. */
export type Exchange = { start: number; end: number }

/** The task's turns grouped into exchanges: each begins with one of the
    founder's messages (the order first). */
export function exchanges(kinds: string[]): Exchange[] {
  const out: Exchange[] = []
  kinds.forEach((k, i) => {
    if (k === "founder" || i === 0) out.push({ start: i, end: i + 1 })
    else out[out.length - 1].end = i + 1
  })
  return out
}

/** Where an exchange stands: still going, waiting for the founder, or
    over (failed, stopped or done). */
export type Outcome = "running" | "waiting" | "failed" | "stopped" | "done"

/** Folded once it is over, except the conversation's last exchange: one
    still going and one waiting for the founder stay open. */
export function openByDefault(outcome: Outcome, last: boolean): boolean {
  return last || outcome === "running" || outcome === "waiting"
}

/** Open when the founder said so for this exchange, else as by default. */
export function isOpen(byDefault: boolean, saved: boolean | null): boolean {
  return saved ?? byDefault
}

/** The first line of a text, without Markdown's marks, cut at `max`. */
export function firstLine(text: string, max: number): string {
  const line = (text.split("\n").find((l) => l.trim()) ?? "").trim()
    .replace(/^#{1,6}\s+|^[-*•]\s+|^>\s*/, "")
    .replace(/\*\*|__|`|~~/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
  return line.length > max ? `${line.slice(0, max - 1).trimEnd()}…` : line
}

/** The folded line: the request, and its result in a few words. */
export function summary(request: string, answer: string | null, outcome: Outcome): { request: string; result: string } {
  const result = outcome === "failed" ? "Failed"
    : outcome === "waiting" ? "Waiting for you"
    : outcome === "running" ? "Working"
    : outcome === "stopped" ? "Stopped"
    : answer ? firstLine(answer, 90) : "Done"
  return { request: firstLine(request, 80), result }
}

/** The founder's choice for an exchange, as kept in the browser: open,
    closed, or none. A storage that cannot be read gives none. */
export function remembered(store: Pick<Storage, "getItem"> | null, key: string): boolean | null {
  try {
    const v = store?.getItem(`airth-fold:${key}`)
    return v === "open" ? true : v === "closed" ? false : null
  } catch { return null }
}

/** Keep the founder's choice; a storage that refuses (a private window)
    leaves it for this page only. */
export function remember(store: Pick<Storage, "setItem"> | null, key: string, open: boolean) {
  try { store?.setItem(`airth-fold:${key}`, open ? "open" : "closed") } catch { /* kept for this page only */ }
}
