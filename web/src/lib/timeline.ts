// A conversation in time order: the agent's runs, what it sent up to Needs
// you, and the founder's tasks with it, each at its place in time. Kept here,
// apart from the screen, so it is tested.

/** A moment as the server writes it, in milliseconds: with `Z` or an offset,
    with or without fractions of a second (any number of digits). A time
    without a zone is the server's, so UTC. Null when there is none. */
export function moment(at: string | null | undefined): number | null {
  if (!at) return null
  let s = at.trim()
  // More than milliseconds: the rest changes nothing a person would see.
  s = s.replace(/(\.\d{3})\d+/, "$1")
  if (/T\d{2}:\d{2}(:\d{2}(\.\d+)?)?$/.test(s)) s += "Z"
  const ms = Date.parse(s)
  return Number.isNaN(ms) ? null : ms
}

export type Kind = "task" | "run" | "item"

// At the same moment, the founder's task comes before what answers it.
const RANK: Record<Kind, number> = { task: 0, run: 1, item: 2 }

/** The entries in time order, oldest first. Those with no time come last,
    in the order given: they are the newest (a task just started, not yet
    listed). Ties keep the founder first, then the order given. */
export function chronological<T extends { kind: Kind; at: string | null | undefined }>(entries: T[]): T[] {
  return entries
    .map((e, i) => ({ e, i, ms: moment(e.at) }))
    .sort((a, b) => {
      if (a.ms === null || b.ms === null) return a.ms === b.ms ? a.i - b.i : a.ms === null ? 1 : -1
      return a.ms - b.ms || RANK[a.e.kind] - RANK[b.e.kind] || a.i - b.i
    })
    .map((x) => x.e)
}

/** A task in time: from its start to its last activity; open (`to` null)
    while it is not listed yet. */
export type Span = { id: string; from: number; to: number | null }

/** The task an event at `at` came during, if any: of the tasks whose span
    holds it, the one started last. A task goes at its start, and what came
    during it is shown inside it, between its messages (decision 148). */
export function during(spans: Span[], at: number | null): string | null {
  if (at === null) return null
  let found: Span | null = null
  for (const s of spans) {
    if (s.from <= at && (s.to === null || at <= s.to) && (!found || s.from > found.from)) found = s
  }
  return found?.id ?? null
}

/** Where each event goes among a task's turns, given each turn's time:
    the index of the first turn that began after it, or `turns.length` for
    after them all. A turn with no time is older than the record of times:
    nothing is placed before it. A task without any time (`timed` false)
    keeps the old way: everything that came during it above it, whole. */
export function slots(turns: (number | null)[], events: (number | null)[], timed: boolean): number[] {
  return events.map((at) => {
    if (!timed) return 0
    if (at === null) return turns.length
    const i = turns.findIndex((t) => t !== null && t > at)
    return i < 0 ? turns.length : i
  })
}
