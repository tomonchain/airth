import type { Item, Routine } from "@/lib/api"

// The Routines page's logic, kept apart from its look so it is tested
// (decision 142): how a trigger is said, how often it runs, what state a
// routine is in, and which routines need a look.

/** What starts a routine, as its file says (src/routine): one of these. */
export type Trigger = { schedule?: string | null; manual?: unknown; telegram?: string | null; webhook?: string | null }

const DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
const DAY_NAMES = ["Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays", "Sundays"]

type Parsed =
  | { kind: "every"; minutes: number }
  | { kind: "at"; days: number[]; time: string }

/** A schedule as the engine reads it (`Schedule::parse` in src/objective.rs):
    `every 30m`, `every 2h`, `every 1d`, `daily 08:30`, `weekdays 09:00`,
    `weekends 10:00`, `mon,thu 18:00`. Anything else: null. */
export function parseSchedule(text: string): Parsed | null {
  const words = text.trim().toLowerCase().split(/\s+/)
  if (words.length !== 2) return null
  const [first, second] = words
  if (first === "every") {
    const m = second.match(/^(\d+)(m|min|mins|h|hr|hrs|d|day|days)$/)
    if (!m) return null
    const n = Number(m[1])
    const minutes = m[2][0] === "m" ? n : m[2][0] === "h" ? n * 60 : n * 1440
    return minutes >= 1 ? { kind: "every", minutes } : null
  }
  const t = second.match(/^(\d{1,2}):(\d{2})$/)
  if (!t || Number(t[1]) > 23 || Number(t[2]) > 59) return null
  const time = `${t[1].padStart(2, "0")}:${t[2]}`
  if (first === "daily") return { kind: "at", days: [0, 1, 2, 3, 4, 5, 6], time }
  if (first === "weekdays") return { kind: "at", days: [0, 1, 2, 3, 4], time }
  if (first === "weekends") return { kind: "at", days: [5, 6], time }
  const days: number[] = []
  for (const d of first.split(",")) {
    const i = DAYS.findIndex((n) => d.length >= 3 && d.startsWith(n))
    if (i < 0) return null
    if (!days.includes(i)) days.push(i)
  }
  return { kind: "at", days: days.sort((a, b) => a - b), time }
}

const list = (xs: string[]) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`)

/** A schedule in plain words: "daily 07:00" → "Every day at 07:00". Never
    the schedule's own syntax: one Airth cannot read is said so. */
export function scheduleText(text: string): string {
  const p = parseSchedule(text)
  if (!p) return "On a schedule Airth cannot read"
  if (p.kind === "every") {
    const m = p.minutes
    if (m % 1440 === 0) return m === 1440 ? "Every day" : `Every ${m / 1440} days`
    if (m % 60 === 0) return m === 60 ? "Every hour" : `Every ${m / 60} hours`
    return m === 1 ? "Every minute" : `Every ${m} minutes`
  }
  const d = p.days.join()
  const days = d === "0,1,2,3,4,5,6" ? "Every day" : d === "0,1,2,3,4" ? "Weekdays" : d === "5,6" ? "Weekends" : list(p.days.map((i) => DAY_NAMES[i]))
  return `${days} at ${p.time}`
}

/** What starts a routine, said plainly. */
export function triggerText(t: Trigger): string {
  if (typeof t.schedule === "string") return scheduleText(t.schedule)
  if (t.telegram) return t.telegram === "x_post" ? "When you send a post link" : "When you send it on Telegram"
  if (t.webhook) return `When the ${t.webhook} webhook is called`
  return "When you run it"
}

export type Frequency = "daily" | "weekly" | "monthly" | "event" | "manual"
export const FREQUENCIES: [Frequency, string][] = [["daily", "Daily"], ["weekly", "Weekly"], ["monthly", "Monthly"], ["event", "On an event"], ["manual", "By hand"]]

/** How often a routine runs, for grouping: once a day or more is daily
    (weekdays included), a few days a week is weekly. */
export function frequency(t: Trigger): Frequency {
  if (typeof t.schedule === "string") {
    const p = parseSchedule(t.schedule)
    if (!p) return "daily"
    if (p.kind === "every") return p.minutes <= 1440 * 6 ? "daily" : p.minutes < 1440 * 28 ? "weekly" : "monthly"
    return p.days.length >= 5 ? "daily" : "weekly"
  }
  if (t.telegram || t.webhook) return "event"
  return "manual"
}

/** One line of the list, whatever the kind: a routine of blocks (decision
    49), an agent given a task on a schedule (decision 37), or a fixed
    sequence (decision 30). */
export type Row = {
  key: string
  realm: string
  kind: "graph" | "agent" | "sequence"
  name: string
  agent: string | null
  agentTitle: string | null
  trigger: Trigger
  enabled: boolean
  next: string | null
  last: { at: string; ok: boolean; error: string | null } | null
  /** Why its file is refused, for one that does not load. */
  broken: string | null
  /** The lowest level among its Act blocks' actions (N0–N3): below 2, the
      routine cannot finish without the founder. Null without a declared act. */
  level: number | null
  levels: { action: string; level: number }[]
  /** Its items in Needs you still waiting: incidents, approvals. */
  items: Item[]
}

/** The business's pending items that are this routine's. */
const its = (items: Item[], name: string, agent: string | null) =>
  items.filter((it) => it.status === "pending" && it.routine === name && (!it.agent || !agent || it.agent === agent))

/** A routine of blocks as a row; `levels` is its agent's level per action. */
/** The agent a routine's file names, read from its source when the file
    does not load as a routine (it may still be valid JSON). */
function namedAgent(source: string): string | null {
  try {
    const a = (JSON.parse(source) as { agent?: unknown }).agent
    return typeof a === "string" && a ? a : null
  } catch { return null }
}

export function graphRow(realm: string, r: Routine, levels: Record<string, number> = {}, items: Item[] = []): Row {
  const g = r.graph
  const agent = g?.agent ?? namedAgent(r.source)
  const acts = (g?.blocks ?? []).filter((b) => b.type === "act" && typeof b.action === "string").map((b) => b.action as string)
  const known = [...new Set(acts)].filter((a) => a in levels).map((action) => ({ action, level: levels[action] }))
  return {
    key: `${realm}/graph/${r.name}`, realm, kind: "graph", name: r.name,
    agent, agentTitle: r.agent_title ?? agent,
    trigger: g?.trigger ?? {}, enabled: true, next: r.next_at,
    last: r.runs[0] ?? null,
    broken: g ? null : r.error ?? "This file does not hold",
    level: known.length ? Math.min(...known.map((l) => l.level)) : null, levels: known,
    items: its(items, r.name, agent),
  }
}

export type AgentRoutine = { id: string; every: string | null; bot: string | null; enabled: boolean; next_at: string | null; last_at: string | null; last_status: string | null }

/** An agent routine as a row. Its runs end completed, failed or stopped;
    the last two count as a failure, as the server's summary says. */
export function agentRow(realm: string, o: AgentRoutine, title?: string | null, items: Item[] = []): Row {
  const failed = o.last_status === "failed" || o.last_status === "stopped"
  return {
    key: `${realm}/agent/${o.id}`, realm, kind: "agent", name: o.id,
    agent: o.bot, agentTitle: title ?? o.bot,
    trigger: o.every ? { schedule: o.every } : { manual: true }, enabled: o.enabled,
    next: o.enabled ? o.next_at : null,
    last: o.last_at && o.last_status ? { at: o.last_at, ok: !failed, error: failed ? o.last_status : null } : null,
    broken: null, level: null, levels: [], items: its(items, o.id, o.bot),
  }
}

export type Sequence = { name: string; schedule?: string | null; next_at?: string | null; error?: string; runs: { started_at: string; ok: boolean; error: string | null }[] }

export function sequenceRow(realm: string, p: Sequence, items: Item[] = []): Row {
  const last = p.runs[0]
  return {
    key: `${realm}/sequence/${p.name}`, realm, kind: "sequence", name: p.name,
    agent: null, agentTitle: null,
    trigger: p.schedule ? { schedule: p.schedule } : { manual: true }, enabled: true,
    next: p.next_at ?? null,
    last: last ? { at: last.started_at, ok: last.ok, error: last.error } : null,
    broken: p.error ?? null, level: null, levels: [], items: its(items, p.name, null),
  }
}

export type Reason =
  | { kind: "incident"; item: Item }
  | { kind: "broken"; error: string }
  | { kind: "failed"; error: string | null; at: string }
  | { kind: "waiting"; item: Item }

/** Items in Needs you the founder has to act on for a routine: an approval,
    or an action prepared for them to carry out (level 0). */
const WAITING: Item["kind"][] = ["approval", "prepared"]

/** Why a routine needs a look, most serious first; empty when it does not. */
export function reasons(row: Row): Reason[] {
  const out: Reason[] = []
  for (const it of row.items) if (it.kind === "incident") out.push({ kind: "incident", item: it })
  if (row.broken) out.push({ kind: "broken", error: row.broken })
  if (row.last && !row.last.ok) out.push({ kind: "failed", error: row.last.error, at: row.last.at })
  for (const it of row.items) if (WAITING.includes(it.kind)) out.push({ kind: "waiting", item: it })
  return out
}

export type State = "ok" | "waiting" | "incident" | "paused"

/** A routine's state, for its pill: an incident (or a failed run, or a file
    that does not hold) first, then waiting for the founder, then paused. */
export function state(row: Row): State {
  const r = reasons(row)
  if (r.some((x) => x.kind !== "waiting")) return "incident"
  if (r.length) return "waiting"
  return row.enabled ? "ok" : "paused"
}

/** The "Needs a look" section: routines with an incident, a failed last
    run, a file that does not hold, or something waiting for the founder.
    Incidents first, then by name. Empty: the section is not shown. */
export function needsLook(rows: Row[]): { row: Row; reasons: Reason[] }[] {
  const rank = (rs: Reason[]) => (rs.some((x) => x.kind !== "waiting") ? 0 : 1)
  return rows
    .map((row) => ({ row, reasons: reasons(row) }))
    .filter((x) => x.reasons.length > 0)
    .sort((a, b) => rank(a.reasons) - rank(b.reasons) || a.row.name.localeCompare(b.row.name))
}

export type GroupBy = "agent" | "frequency" | "business"

export type Group = { key: string; label: string; agent: string | null; rows: Row[]; next: string | null }

/** Rows in groups: by agent (no agent last), by how often (in a fixed
    order), or by business (in the order given). Each group knows its
    earliest next run. */
export function group(rows: Row[], by: GroupBy, businesses: { name: string; title: string }[] = []): Group[] {
  const groups = new Map<string, Group>()
  const take = (key: string, label: string, agent: string | null, row: Row) => {
    const g = groups.get(key) ?? { key, label, agent, rows: [], next: null }
    g.rows.push(row)
    if (row.next && (!g.next || Date.parse(row.next) < Date.parse(g.next))) g.next = row.next
    groups.set(key, g)
  }
  for (const row of rows) {
    if (by === "agent") take(row.agent ? `agent:${row.agent}` : "agent:", row.agentTitle ?? row.agent ?? "No agent", row.agent, row)
    else if (by === "frequency") {
      const f = frequency(row.trigger)
      take(`frequency:${f}`, FREQUENCIES.find(([k]) => k === f)![1], null, row)
    } else take(`business:${row.realm}`, businesses.find((b) => b.name === row.realm)?.title ?? row.realm, null, row)
  }
  const out = [...groups.values()]
  for (const g of out) g.rows.sort((a, b) => a.name.localeCompare(b.name))
  if (by === "frequency") return FREQUENCIES.map(([k]) => groups.get(`frequency:${k}`)).filter((g): g is Group => !!g)
  if (by === "business") return businesses.map((b) => groups.get(`business:${b.name}`)).filter((g): g is Group => !!g)
  return out.sort((a, b) => (a.agent ? 0 : 1) - (b.agent ? 0 : 1) || a.label.localeCompare(b.label))
}

/** Groups of more than five routines start folded. */
export const FOLDED_FROM = 6

export type Filter = { query: string; status: "all" | "look" | "paused"; agent: string | null }

export function filter(rows: Row[], f: Filter): Row[] {
  const q = f.query.trim().toLowerCase()
  return rows.filter((r) =>
    (!q || r.name.toLowerCase().includes(q)) &&
    (!f.agent || r.agent === f.agent) &&
    (f.status === "all" || (f.status === "look" ? reasons(r).length > 0 : !r.enabled)))
}
