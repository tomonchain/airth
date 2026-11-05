// Airth's API, as the server serves it (src/ui/*.rs). Nothing here decides
// what an answer may do: the server does.

const KEY = "airth-token"

// The key, from the link Airth gives (`/#token=…`), kept for this tab.
function token(): string {
  const m = location.hash.match(/token=([0-9a-f]+)/)
  if (m) {
    sessionStorage.setItem(KEY, m[1])
    history.replaceState(null, "", location.pathname + location.hash.replace(/[#&]?token=[0-9a-f]+/, ""))
    return m[1]
  }
  return sessionStorage.getItem(KEY) ?? ""
}

export async function api<T>(path: string, body?: unknown, method?: "PUT" | "PATCH" | "DELETE", ms?: number): Promise<T> {
  const verb = method ?? (body === undefined ? "GET" : "POST")
  // A request that never answers would hold its page's refresh for good: a
  // read gives up after 20 s, a change (which may call a service) after 60,
  // unless the caller knows it takes longer.
  const r = await fetch(`/api/${path}`, {
    method: verb,
    headers: { "X-Airth-Token": token(), "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(ms ?? (verb === "GET" ? 20000 : 60000)),
  }).catch((e: Error) => { throw new Error(e.name === "TimeoutError" ? "Airth did not answer in time." : "Airth is not answering.") })
  const data = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(data.error ?? `${r.status} ${r.statusText}`)
  return data as T
}

export const enc = encodeURIComponent

/** A request whose body or answer is not JSON (a document's bytes), with
    the same key; an error answer still says why, as JSON. */
export async function raw(path: string, init: RequestInit = {}): Promise<Response> {
  const r = await fetch(`/api/${path}`, { ...init, headers: { "X-Airth-Token": token() } })
    .catch(() => { throw new Error("Airth is not answering.") })
  if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error ?? `${r.status} ${r.statusText}`)
  return r
}

export type Realm = { name: string; description: string | null; archived: boolean; default: boolean; live: number; needs_approval: number }

export type AgentState = "needs" | "incident" | "working" | "idle" | "asleep"

export type Agent = {
  name: string
  title: string
  department: string | null
  job: string | null
  body: string
  integrations: string[]
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
  description: string | null
  headline: string
  needs: number
  time: string
  cost_usd: number
  activity: Activity[]
  health: { state: string; sentence: string }
  vitals: { label: string; value: string; note: string; out: boolean }[]
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
  source: { link: string | null; title: string | null } | null
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
  graph?: { description: string; agent: string; trigger: { schedule: string | null; manual: unknown; telegram: string | null; webhook: string | null }; blocks: GraphBlock[] }
}

/** A routine's block as its file says: the six kinds' fields (src/routine). */
export type GraphBlock = { id: string; type: string; [field: string]: unknown }

export type Company = {
  profile: Record<string, string | null>
  goal: Record<string, string | null>
  lines: { name: string }[]
  repos: { name: string; url: string | null }[]
  missing: string[]
}

export const DEPARTMENTS = ["leadership", "marketing", "sales", "support", "product", "operations", "finance"] as const
