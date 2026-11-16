// The Routines page's logic, apart from React so it is tested: how a
// trigger reads, each routine's state, what needs a look, and the groups.
// The page holds three kinds of routine (decisions 30, 37, 49): drawn ones
// made of blocks, agent routines, and fixed sequences; here they are one row.

export type Kind = "graph" | "agent" | "sequence"

/** What starts a routine, as its file says. A schedule is the scheduler's
    text: "every 30m", "daily 07:00", "weekdays 09:00", "mon,thu 18:00". */
export type Trigger =
  | { kind: "schedule"; every: string }
  | { kind: "manual" }
  | { kind: "telegram"; event: string }
  | { kind: "webhook"; name: string }

export type Row = {
  kind: Kind
  name: string
  /** The agent it works for; none for a fixed sequence or the general assistant. */
  agent: string | null
  trigger: Trigger
  paused: boolean
  /** Its file does not hold: it cannot run until fixed. */
  broken: boolean
  next_at: string | null
  last: { at: string; ok: boolean; run?: string | null } | null
  /** What it may do alone (0–3): the lowest of its acts' levels. */
  level: number | null
  /** The business it belongs to, when the list holds several. */
  business?: string
}

/** The part of a Needs you item this page reads. */
export type Waiting = { kind: string; status: string; routine: string | null; run: string | null; business?: string }

export type State = "ok" | "waiting" | "incident" | "paused"

const DAY_NAMES: Record<string, string> = { mon: "Monday", tue: "Tuesday", wed: "Wednesday", thu: "Thursday", fri: "Friday", sat: "Saturday", sun: "Sunday" }
const DAY_ORDER = Object.keys(DAY_NAMES)

const and = (xs: string[]) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`)

type Parsed =
  | { every: number; unit: "minute" | "hour" | "day" }
  | { days: string[]; time: string; named?: "daily" | "weekdays" | "weekends" }

/** The scheduler's text, read as the scheduler reads it (src/objective.rs). */
function parse(every: string): Parsed | null {
  const t = every.trim().toLowerCase().split(/\s+/)
  if (t.length !== 2) return null
  if (t[0] === "every") {
    const m = t[1].match(/^(\d+)(m|min|mins|h|hr|hrs|d|day|days)$/)
    if (!m) return null
    const unit = m[2][0] === "m" ? "minute" : m[2][0] === "h" ? "hour" : "day"
    const minutes = Number(m[1]) * ({ minute: 1, hour: 60, day: 1440 } as const)[unit]
    // The scheduler takes between a minute and a year.
    if (minutes < 1 || minutes > 366 * 1440) return null
    return { every: Number(m[1]), unit }
  }
  const hm = t[1].match(/^(\d{1,2}):(\d{2})$/)
  if (!hm || Number(hm[1]) > 23 || Number(hm[2]) > 59) return null
  const time = t[1].padStart(5, "0")
  if (t[0] === "daily") return { days: DAY_ORDER, time, named: "daily" }
  if (t[0] === "weekdays") return { days: DAY_ORDER.slice(0, 5), time, named: "weekdays" }
  if (t[0] === "weekends") return { days: DAY_ORDER.slice(5), time, named: "weekends" }
  const days = t[0].split(",").map((d) => DAY_ORDER.find((n) => d.length >= 3 && d.startsWith(n)))
  if (days.some((d) => !d)) return null
  return { days: DAY_ORDER.filter((d) => days.includes(d)), time }
}

/** A schedule as the founder reads it: "weekdays 09:00" → "Weekdays at
    09:00". Never the scheduler's own text: one it cannot read is "On a
    schedule". */
export function schedule(every: string): string {
  const p = parse(every)
  if (!p) return "On a schedule"
  if ("every" in p) {
    if (p.every === 1) return `Every ${p.unit}`
    return `Every ${p.every} ${p.unit}s`
  }
  if (p.named === "daily") return `Every day at ${p.time}`
  if (p.named === "weekdays") return `Weekdays at ${p.time}`
  if (p.named === "weekends") return `Weekends at ${p.time}`
  if (p.days.length === 1) return `Every ${DAY_NAMES[p.days[0]]} at ${p.time}`
  return `${and(p.days.map((d) => `${DAY_NAMES[d]}s`))} at ${p.time}`
}

/** What starts a routine, in a sentence. */
export function triggerText(t: Trigger): string {
  switch (t.kind) {
    case "schedule": return schedule(t.every)
    case "manual": return "When you run it"
    case "telegram": return t.event === "x_post" ? "When you send a post link on Telegram" : "When you send it something on Telegram"
    case "webhook": return `When ${t.name} sends an event`
  }
}

/** A graph routine's trigger, as its file holds it. */
export function graphTrigger(t: { schedule?: string | null; telegram?: string | null; webhook?: string | null; manual?: unknown } | undefined): Trigger {
  if (t?.schedule) return { kind: "schedule", every: t.schedule }
  if (t?.telegram) return { kind: "telegram", event: t.telegram }
  if (t?.webhook) return { kind: "webhook", name: t.webhook }
  return { kind: "manual" }
}

export type Frequency = "daily" | "weekly" | "monthly" | "event" | "manual" | "other"
export const FREQUENCIES: { key: Frequency; label: string }[] = [
  { key: "daily", label: "Daily" },
  { key: "weekly", label: "Weekly" },
  { key: "monthly", label: "Monthly" },
  { key: "event", label: "On an event" },
  { key: "manual", label: "By hand" },
  { key: "other", label: "Other schedules" },
]

/** How often it runs: several times a day counts as daily. */
export function frequency(t: Trigger): Frequency {
  if (t.kind === "manual") return "manual"
  if (t.kind !== "schedule") return "event"
  const p = parse(t.every)
  // One the scheduler would refuse: its frequency is not guessed.
  if (!p) return "other"
  if ("every" in p) {
    if (p.unit !== "day") return "daily"
    return p.every < 7 ? "daily" : p.every < 28 ? "weekly" : "monthly"
  }
  return p.days.length >= 5 ? "daily" : "weekly"
}

const WAITS = new Set(["approval", "prepared", "delayed"])

/** What the founder's Needs you holds for this routine. Only a drawn
    routine's items name it; an agent routine's name its run; a fixed
    sequence leaves none. A drawn routine and an agent routine may share a
    name: each reads its own. Two businesses may too: an item counts only
    for a routine of its own business. */
function mine(row: Row, items: Waiting[]) {
  return items.filter((i) => i.status === "pending" && (i.business ?? null) === (row.business ?? null)
    && (row.kind === "graph" ? i.routine === row.name : row.kind === "agent" && !!row.last?.run && i.run === row.last.run))
}

/** One state per routine, the most pressing first: something broke (its
    file, its last run, an incident waiting), then a draft waiting for the
    founder, then paused. */
export function state(row: Row, items: Waiting[]): State {
  const its = mine(row, items)
  if (row.broken || row.last?.ok === false || its.some((i) => i.kind === "incident")) return "incident"
  if (its.some((i) => WAITS.has(i.kind))) return "waiting"
  if (row.paused) return "paused"
  return "ok"
}

/** What needs a look: incidents first, then what waits for the founder. */
export function needsLook(rows: Row[], items: Waiting[]): Row[] {
  const rank = (r: Row) => ({ incident: 0, waiting: 1 } as Record<State, number>)[state(r, items)] ?? 2
  return rows.filter((r) => rank(r) < 2).sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name))
}

export type Group = { key: string; label: string; rows: Row[]; next_at: string | null }

/** The soonest next run among a group's routines that are not paused. */
export function nextOf(rows: Row[]): string | null {
  const next = rows.filter((r) => !r.paused && r.next_at).map((r) => r.next_at!).sort()
  return next[0] ?? null
}

export type By = "agent" | "frequency" | "business"

/** The businesses in the order the list shows them: the one open first,
    then the others as the business menu lists them. */
export function businessOrder<T extends { name: string }>(current: string, all: T[]): T[] {
  return [...all.filter((b) => b.name === current), ...all.filter((b) => b.name !== current)]
}

/** The rows in groups: by agent (in the order given, then the general
    assistant and fixed sequences), by how often they run, or by business
    (in the order given; a business with no row shown has no group). */
export function groups(rows: Row[], by: By, agents: { name: string; title: string }[], businesses: { name: string; title: string }[] = []): Group[] {
  const sorted = [...rows].sort((a, b) => a.name.localeCompare(b.name))
  if (by === "business") {
    return businesses.map((b) => ({ key: b.name, label: b.title, rows: sorted.filter((r) => r.business === b.name) }))
      .filter((g) => g.rows.length)
      .map((g) => ({ ...g, next_at: nextOf(g.rows) }))
  }
  if (by === "frequency") {
    return FREQUENCIES.map((f) => ({ key: f.key, label: f.label, rows: sorted.filter((r) => frequency(r.trigger) === f.key) }))
      .filter((g) => g.rows.length)
      .map((g) => ({ ...g, next_at: nextOf(g.rows) }))
  }
  const out: Group[] = []
  for (const a of agents) {
    const its = sorted.filter((r) => r.agent === a.name)
    if (its.length) out.push({ key: a.name, label: a.title, rows: its, next_at: nextOf(its) })
  }
  const known = new Set(agents.map((a) => a.name))
  const general = sorted.filter((r) => r.kind !== "sequence" && (!r.agent || !known.has(r.agent)))
  if (general.length) out.push({ key: "-general", label: "No agent", rows: general, next_at: nextOf(general) })
  const sequences = sorted.filter((r) => r.kind === "sequence")
  if (sequences.length) out.push({ key: "-sequences", label: "Fixed sequences", rows: sequences, next_at: nextOf(sequences) })
  return out
}

export type Filter = { search: string; show: "all" | "look" | "paused"; agent: string | null }

/** The rows a filter keeps: a name containing the search, then the chosen view. */
export function filter(rows: Row[], f: Filter, items: Waiting[]): Row[] {
  const q = f.search.trim().toLowerCase()
  return rows.filter((r) => {
    if (q && !r.name.toLowerCase().includes(q)) return false
    if (f.agent && r.agent !== f.agent) return false
    const s = state(r, items)
    if (f.show === "look") return s === "incident" || s === "waiting"
    if (f.show === "paused") return r.paused
    return true
  })
}

/** Past this many routines, a group starts folded. */
export const FOLD_ABOVE = 5
