// How the day and week grids place events that overlap, as Google Calendar
// does. Pure, so it is tested without a browser.

/** A run has no duration: it is shown, and overlaps, as this many minutes,
    the height of one title line in the grid. */
export const SHOWN_MINUTES = 30
/** Past this many columns, the rest of a group folds into a "+N" pill. */
export const MAX_COLUMNS = 3

export type Span = { start: number; end: number }

/** Where an event goes: its column, how many columns it spans to the right,
    how many columns its group has, and whether the group folded some events
    into a pill (it then leaves room for it on the right). */
export type Placed = { index: number; column: number; span: number; columns: number; folded: boolean }

/** The events of a group that did not fit, shown as one pill at `start`. */
export type Overflow = { start: number; end: number; hidden: number[]; slot: number[] }

export type Layout = { placed: Placed[]; overflows: Overflow[] }

const overlaps = (a: Span, b: Span) => a.start < b.end && b.start < a.end

/** Lay out a day's events. Events that overlap, directly or through others,
    form a group. Each takes the first free column of its group; a group is
    as wide as the day, split into its columns, and an event spreads right
    over the columns free for its whole height. Events past `max` columns are
    not placed: each run of overlapping ones becomes an overflow. */
export function layout(events: Span[], max = MAX_COLUMNS): Layout {
  const order = events
    .map((e, index) => ({ ...e, index }))
    .sort((a, b) => a.start - b.start || b.end - a.end || a.index - b.index)
  const placed: Placed[] = []
  const overflows: Overflow[] = []
  let group: typeof order = []
  let groupEnd = -Infinity

  const close = () => {
    if (!group.length) return
    // First free column for each event, in start order.
    const ends: number[] = []
    const column = new Map<number, number>()
    for (const e of group) {
      let c = ends.findIndex((end) => end <= e.start)
      if (c < 0) c = ends.length
      ends[c] = e.end
      column.set(e.index, c)
    }
    const columns = Math.min(ends.length, max)
    const folded = ends.length > columns
    const visible = group.filter((e) => column.get(e.index)! < columns)
    for (const e of visible) {
      const c = column.get(e.index)!
      let span = 1
      while (
        c + span < columns &&
        !visible.some((o) => column.get(o.index) === c + span && overlaps(o, e))
      ) span++
      placed.push({ index: e.index, column: c, span, columns, folded })
    }
    // The hidden ones, grouped by overlap among themselves.
    const hidden = group.filter((e) => column.get(e.index)! >= columns)
    const first = overflows.length
    let pill: Overflow | null = null
    for (const e of hidden) {
      if (pill && e.start < pill.end) {
        pill.hidden.push(e.index)
        pill.end = Math.max(pill.end, e.end)
      } else {
        pill = { start: e.start, end: e.end, hidden: [e.index], slot: [] }
        overflows.push(pill)
      }
    }
    // Everything at the pill's time, for the list it opens.
    for (const p of overflows.slice(first)) {
      p.slot = group.filter((e) => overlaps(e, p)).map((e) => e.index)
    }
    group = []
    groupEnd = -Infinity
  }

  for (const e of order) {
    if (e.start >= groupEnd) close()
    group.push(e)
    groupEnd = Math.max(groupEnd, e.end)
  }
  close()
  placed.sort((a, b) => a.index - b.index)
  return { placed, overflows }
}

/** An event at "HH:MM", as minutes since midnight, shown SHOWN_MINUTES long. */
export function shown(time: string): Span {
  const [h, m] = time.split(":").map(Number)
  const start = h * 60 + m
  return { start, end: start + SHOWN_MINUTES }
}
