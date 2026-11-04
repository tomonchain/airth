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

export async function api<T>(path: string, body?: unknown): Promise<T> {
  const r = await fetch(`/api/${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { "X-Airth-Token": token(), "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const data = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(data.error ?? `${r.status} ${r.statusText}`)
  return data as T
}

export const enc = encodeURIComponent

export type Realm = { name: string; description: string | null; archived: boolean; default: boolean; live: number; needs_approval: number }

export type AgentState = "needs" | "incident" | "working" | "idle" | "asleep"

export type Agent = {
  name: string
  title: string
  department: string | null
  job: string | null
  accessory: "pen" | "lens" | "book" | null
  look: string | null
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
export type Task = { id: string; order?: string; line?: string; status?: string; agent?: string | null; updated_at?: string; at?: string }
export type Tasks = Record<"running" | "waiting" | "review" | "done", Task[]>
