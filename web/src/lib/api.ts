// Airth's API, as the server serves it (src/ui/*.rs). Nothing here decides
// what an answer may do: the server does.

const KEY = "airth-token"

// Where the key is kept. On the Mac it lasts the tab. On the phone, reached
// by its tailnet name (decision 37), it stays once paired, so Airth opens
// from the home screen as an app; whoever holds the unlocked phone holds it.
export const onPhone = location.hostname.endsWith(".ts.net")
function store(): Storage | null {
  try { return onPhone ? localStorage : sessionStorage } catch { return null }
}

// The key, from the link Airth gives (`/#token=…`).
function token(): string {
  const m = location.hash.match(/token=([0-9a-f]+)/)
  if (m) {
    store()?.setItem(KEY, m[1])
    history.replaceState(null, "", location.pathname + location.hash.replace(/[#&]?token=[0-9a-f]+/, ""))
    return m[1]
  }
  return store()?.getItem(KEY) ?? ""
}

/** Whether this page has a key: a phone opened from its home screen starts
    without one, its storage apart from the browser's. */
export const paired = () => !!token()

/** Pair with a pairing link pasted in. Whether it held a key. */
export function pair(link: string): boolean {
  const m = link.match(/token=([0-9a-f]{32,})/)
  if (!m) return false
  store()?.setItem(KEY, m[1])
  return true
}

// A phone whose key Airth refuses (a new key was made since it paired)
// forgets it and shows the pairing screen again, instead of an error with no
// way out. Only a key it sent: with none, the pairing screen is already up.
function refused(r: Response, sent: string) {
  if (r.status !== 401 || !onPhone || !sent) return
  store()?.removeItem(KEY)
  location.reload()
}

/** Keep the key Airth just made, so this page goes on working. */
export const rekey = (key: string) => { store()?.setItem(KEY, key) }

/** This page's pairing link, to pair the home-screen app from the browser. */
export const pairingLink = () => `${location.origin}/#token=${token()}`

export async function api<T>(path: string, body?: unknown, method?: "PUT" | "PATCH" | "DELETE", ms?: number): Promise<T> {
  const verb = method ?? (body === undefined ? "GET" : "POST")
  // A request that never answers would hold its page's refresh for good: a
  // read gives up after 20 s, a change (which may call a service) after 60,
  // unless the caller knows it takes longer.
  const key = token()
  const r = await fetch(`/api/${path}`, {
    method: verb,
    headers: { "X-Airth-Token": key, "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(ms ?? (verb === "GET" ? 20000 : 60000)),
  }).catch((e: Error) => { throw new Error(e.name === "TimeoutError" ? "Airth did not answer in time." : "Airth is not answering.") })
  refused(r, key)
  const data = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(data.error ?? `${r.status} ${r.statusText}`)
  return data as T
}

export const enc = encodeURIComponent

/** A business as the founder calls it: its profile's name, else its
    folder name with a capital. */
export const named = (name: string, title?: string | null) => title?.trim() || name.charAt(0).toUpperCase() + name.slice(1)

/** An agent as the founder calls it: "Ada, Chief of staff", else its title. */
export const called = (a: { title: string; first_name?: string | null }) => (a.first_name ? `${a.first_name}, ${a.title}` : a.title)

/** A request whose body or answer is not JSON (a document's bytes), with
    the same key; an error answer still says why, as JSON. */
export async function raw(path: string, init: RequestInit = {}): Promise<Response> {
  const key = token()
  const r = await fetch(`/api/${path}`, { ...init, headers: { "X-Airth-Token": key } })
    .catch(() => { throw new Error("Airth is not answering.") })
  refused(r, key)
  if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error ?? `${r.status} ${r.statusText}`)
  return r
}

export type Realm = { name: string; title?: string | null; description: string | null; archived: boolean; default: boolean; live: number; needs_approval: number }

export type AgentState = "needs" | "incident" | "working" | "idle" | "asleep"

export type Agent = {
  name: string
  title: string
  /** Its first name, besides its title (decision 94). */
  first_name?: string | null
  department: string | null
  job: string | null
  body: string
  integrations: string[]
  /** The repositories its tasks work in (decision 64). */
  repos?: string[]
  skills: string[]
  accessory: "pen" | "lens" | "book" | null
  look: string | null
  look_saved?: boolean
  state: AgentState
  waiting: number
  cost_usd: number
  seconds: number
  routines: { name: string; schedule: string | null; last: { at: string; ok: boolean } | null }[]
}

export type Activity = { agent: string; agent_title: string; routine: string; sentence: string; at: string }

export type Business = {
  name: string
  /** Its name as the founder wrote it (its profile's), when there is one. */
  title?: string | null
  description: string | null
  headline: string
  needs: number
  time: string
  cost_usd: number
  activity: Activity[]
  health: { state: string; sentence: string }
  vitals: { label: string; value: string; note: string; out: boolean }[]
  /** Stable weeks in a row toward the beta (decision 92); none without routines. */
  proof: { in_a_row: number; of: number; last_week: string; last_stable: boolean; why: string[]; sentence: string } | null
  finance: Finance
}

export type Item = {
  id: number
  status: "pending" | "approved" | "rejected" | "done" | "failed"
  kind: "approval" | "incident" | "report" | "prepared" | "delayed" | "deadline"
  agent: string | null
  routine: string | null
  run: string | null
  title: string
  text: string | null
  note: string | null
  flag: "blocking" | "note" | null
  flag_reason: string | null
  editable: boolean
  revision: number
  retry: "clear" | "ambiguous" | null
  overdue: boolean
  execute_at: string | null
  created_at: string
  /** When an unanswered draft leaves Needs you: its routine's window. */
  expires_at?: string | null
  /** The drafting step's one line on why this. */
  angle?: string | null
  /** What the draft answers or is written from: the post replied to, the article. */
  source: { link: string | null; title: string | null; text?: string | null } | null
}

export type Block = {
  block: string
  outcome: "ok" | "waiting" | "failed" | "skipped" | string
  items_in: number | null
  items_out: number | null
  cost_usd: number | null
  error: string | null
  started_at: string
  finished_at: string | null
}

export type Run = { run: string; routine: string; blocks: Block[] }

export type ThreadRun = { routine: string; run: string; sentence: string; at: string | null; blocks: Block[] }
export type Thread = { runs: ThreadRun[]; items: Item[] }
export type Task = { id: string; order?: string; line?: string; status?: string; agent?: string | null; updated_at?: string; at?: string; change?: string | null }
export type Tasks = Record<"running" | "waiting" | "review" | "done", Task[]>

export type Finance = { source?: string; cash?: string; in?: string; out?: string; runway?: string; runway_months?: number | null; unknown?: string } | null

export type Deadline = { authority: string; title: string; date: string; due: string; days_left: number; filed: boolean; period_end: string; source: string }

export type Calendar = {
  title: string
  total: number
  late: { routine: string; agent: string; at?: string }[]
  days: { date: string; day: string; weekday: string; today: boolean; runs: { agent: string; routine: string; state: string; time: string }[]; deadlines: string[] }[]
  deadlines: { items: Deadline[]; note: string }
}

export type Routine = {
  name: string
  agent_title: string | null
  cost_usd: number
  unpriced: number
  next_at: string | null
  runs: { at: string; ok: boolean; error: string | null }[]
  error?: string
  /** The file as written, and its version, for the editor (decision 69). */
  source: string
  version: string
  graph?: { description: string; agent: string; trigger: { schedule: string | null; manual: unknown; telegram: string | null; webhook: string | null }; blocks: GraphBlock[] }
  /** What its drafting block hears from the founder, newest first (decision 116). */
  feedback?: Feedback[]
}

/** One thing the founder said about a draft: an edit, a reason to reject, a redo. */
export type Feedback = { item: number; at: string; kind: "edit" | "reject" | "redo"; draft: string; said: string; edited: string }

/** A routine's block as its file says: the six kinds' fields (src/routine). */
export type GraphBlock = { id: string; type: string; [field: string]: unknown }

export type Company = {
  profile: Record<string, string | null>
  goal: Record<string, string | null>
  lines: { name: string }[]
  repos: { name: string; url: string | null }[]
  missing: string[]
}

/** A department of the business: the nine, then the founder's own
    (decision 79), in the order the server gives them. */
export type Department = { key: string; name: string; own: boolean }

// The nine's names, as the server gives them, for pages that show one
// department without the list; a founder's own reads as its key.
const NINE: Record<string, string> = {
  leadership: "Leadership", marketing: "Marketing", sales: "Sales", support: "Support", tech: "Product & tech",
  operations: "Operations", finance: "Finance & admin", legal: "Legal & compliance", data: "Data & research",
}
export function departmentName(d: string | null | undefined) {
  if (!d) return "No department"
  const spaced = d.replace(/-/g, " ")
  return NINE[d] ?? spaced.charAt(0).toUpperCase() + spaced.slice(1)
}
