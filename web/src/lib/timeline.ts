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

/** Where a task goes: at its last activity, since it is shown whole and ends
    with its newest messages (the founder's last words and the answer);
    at its start when it is not listed yet. */
export function taskAt(listedAt: string | null | undefined, startedAt: string): string {
  return listedAt || startedAt
}
