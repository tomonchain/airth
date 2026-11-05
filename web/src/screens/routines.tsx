import { useState } from "react"
import { Bot, BookOpen, Check, ChevronDown, FlaskConical, GitBranch, Hand, ListOrdered, Loader, Pause, Pencil, Play, Plus, Repeat, Sparkles, X, Zap, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { Rich } from "@/components/rich"
import { Empty, Loading, Page, Panel, Section } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type GraphBlock, type Routine } from "@/lib/api"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"
import { Remove } from "@/screens/settings-keys"

// The six blocks (CLAUDE.md): what each looks like in a routine's chain.
const BLOCK: Record<string, { icon: LucideIcon; label: string }> = {
  read: { icon: BookOpen, label: "Read" },
  ai: { icon: Sparkles, label: "AI" },
  condition: { icon: GitBranch, label: "If" },
  approval: { icon: Hand, label: "You approve" },
  act: { icon: Zap, label: "Act" },
}

const DAYS: Record<string, string> = { mon: "Mondays", tue: "Tuesdays", wed: "Wednesdays", thu: "Thursdays", fri: "Fridays", sat: "Saturdays", sun: "Sundays" }

/** A schedule as the founder reads it: "weekdays 09:00" → "Weekdays at 09:00". */
function schedule(every: string): string {
  const t = every.trim().toLowerCase()
  const m = t.match(/^every (\d+)\s*(m|min|mins|h|hr|hrs|d|day|days)$/)
  if (m) {
    const n = Number(m[1])
    const unit = m[2][0] === "m" ? "min" : m[2][0] === "h" ? (n === 1 ? "hour" : "hours") : (n === 1 ? "day" : "days")
    return n === 1 && unit !== "min" ? `Every ${unit}` : `Every ${n} ${unit}`
  }
  const at = t.match(/^(\S+) (\d{1,2}:\d{2})$/)
  if (!at) return every
  const days = ({ daily: "Every day", weekdays: "Weekdays", weekends: "Weekends" } as Record<string, string>)[at[1]]
    ?? at[1].split(",").map((d) => DAYS[d] ?? d).join(", ")
  return `${days} at ${at[2]}`
}

const clock = (iso: string) => new Date(iso).toLocaleString("en-GB", { weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })

function trigger(t: NonNullable<Routine["graph"]>["trigger"]) {
  if (t.schedule) return schedule(t.schedule)
  // Decision 63: a post link the founder sends the bot.
  if (t.telegram) return t.telegram === "x_post" ? "When you send a post link on Telegram" : `On Telegram: ${t.telegram}`
  if (t.webhook) return `On a webhook: ${t.webhook}`
  return "By hand"
}

/** A block's title and one line on what it does, from its file. */
function describe(b: GraphBlock): [string, string] {
  const s = (k: string) => (typeof b[k] === "string" ? (b[k] as string) : "")
  const n = (k: string) => (typeof b[k] === "number" ? (b[k] as number) : null)
  const list = (k: string) => (Array.isArray(b[k]) ? (b[k] as string[]) : [])
  const join = (xs: unknown[]) => xs.filter(Boolean).join(" · ")
  switch (b.type) {
    case "read":
      if (s("source") === "business") return ["Business records", s("records") === "week" ? "How the last 7 days went" : s("records")]
      if (s("source") === "rss") {
        const feeds = list("feeds").length
        return [`${feeds} news feed${feeds === 1 ? "" : "s"}`, join([n("max_age_days") && `last ${n("max_age_days")} days`, s("dedupe") && "no repeats", n("max") && `at most ${n("max")}`])]
      }
      if (s("source") === "x_post") return ["The post you sent", ""]
      return [s("source"), s("action") || s("url")]
    case "ai":
      return [b.id, join([s("prompt").replace(/^prompts\//, ""), b.over === "list" && "all at once", b.model === "fast" && "fast model",
        s("keep") && `keeps ${s("keep")}`, n("limit") && `best ${n("limit")}`, list("distinct").length > 0 && `one per ${list("distinct").join(" and ")}`])]
    case "condition": {
      const c = ((b.if as object | undefined) ?? {}) as { field?: string; op?: string; value?: unknown }
      return [b.id, `if ${c.field ?? ""} ${c.op ?? ""}${c.value === undefined ? "" : ` ${JSON.stringify(c.value)}`}`]
    }
    case "approval":
      return ["You, in Needs you", join([b.editable ? "you may edit" : "approve or reject", n("expires_after") && `expires after ${n("expires_after")} h`])]
    case "act":
      return [s("action"), b.id]
    default:
      return [b.id, ""]
  }
}

type Report = { blocks: { id: string; items_in: number; items_out: number; secs: number; lines: string[]; notes: string[] }[]; cost: string; secs: number; failed?: string[] }
type Dry = { running: boolean; report?: Report; error?: string; at?: string }

/** The business's recurring work, in three kinds: routines made of blocks
    (decision 49), agents given a task on a schedule (decision 37), and the
    fixed sequences of business/pipelines/ (decision 30). */
export function Routines({ realm, agents, onAgent }: { realm: string; agents: Agent[]; onAgent: (name: string) => void }) {
  const graphs = usePoll<{ routines: Routine[] }>(() => api(`realms/${enc(realm)}/graphs`), 15000, [realm])
  const tasks = usePoll<Tasks>(() => api(`realms/${enc(realm)}/routines`), 15000, [realm])
  const fixed = usePoll<{ pipelines: Pipeline[] }>(() => api(`realms/${enc(realm)}/business`), 30000, [realm])
  const [open, setOpen] = useState<string | null>(null)
  const [dry, setDry] = useState<Record<string, Dry>>({})
  const [form, setForm] = useState<Objective | "new" | null>(null)

  async function dryRun(name: string) {
    setDry((d) => ({ ...d, [name]: { running: true } }))
    try {
      // The feeds are read and the model asked for real: it can take minutes.
      const report = await api<Report>(`realms/${enc(realm)}/graphs/${enc(name)}/dry-run`, {}, undefined, 600000)
      setDry((d) => ({ ...d, [name]: { running: false, report, at: new Date().toISOString() } }))
    } catch (e) {
      setDry((d) => ({ ...d, [name]: { running: false, error: (e as Error).message } }))
    }
  }

  const count = (graphs.data?.routines.length ?? 0) + (tasks.data?.routines.length ?? 0) + (fixed.data?.pipelines.length ?? 0)
  return (
    <Page wide title="Routines" lede={graphs.data && tasks.data ? `${count} routine${count === 1 ? "" : "s"}, the agents' recurring work.` : undefined}
      actions={<Button size="sm" variant="outline" onClick={() => setForm("new")}><Plus /> New agent routine</Button>}>
      <Section title="Routines" note="Files in routines/, in the business folder">
        {!graphs.data ? <Loading /> : graphs.data.routines.length ? (
          <div className="space-y-2.5">
            {graphs.data.routines.map((r) => (
              <GraphCard key={r.name} r={r} agent={agents.find((x) => x.name === r.graph?.agent)} open={open === r.name}
                onOpen={() => setOpen(open === r.name ? null : r.name)} onAgent={onAgent} dry={dry[r.name]} onDry={() => dryRun(r.name)} />
            ))}
          </div>
        ) : <Empty icon={Repeat} title="No routine yet">A routine is a file in routines/ in the business folder: a trigger, then its blocks.</Empty>}
      </Section>

      <Section title="Agent routines" note="An agent, a task, a time">
        {!tasks.data ? <Loading /> : tasks.data.routines.length ? (
          <div className="space-y-2.5">
            {tasks.data.routines.map((o) => (
              <TaskCard key={o.id} realm={realm} o={o} agent={agents.find((x) => x.name === o.bot)} onAgent={onAgent} onEdit={() => setForm(o)} onChanged={tasks.refresh} />
            ))}
          </div>
        ) : (
          <Empty icon={Bot} title="No agent routine">
            {tasks.data.agents.length ? "Give an agent a task to do on a schedule: it works on it each time, as on a task you give it." : "Hire an agent first, then give it a task to do on a schedule."}
          </Empty>
        )}
      </Section>

      {fixed.data && fixed.data.pipelines.length > 0 && (
        <Section title="Fixed sequences" note="Files in business/pipelines/">
          <div className="space-y-2.5">{fixed.data.pipelines.map((p) => <PipelineCard key={p.name} realm={realm} p={p} />)}</div>
        </Section>
      )}

      {tasks.data && <TaskForm realm={realm} d={tasks.data} start={form} onClose={() => setForm(null)} onSaved={tasks.refresh} />}
    </Page>
  )
}

function Last({ ok, at }: { ok: boolean; at: string }) {
  return (
    <span className={cn("inline-flex items-center gap-1", ok ? "text-emerald-500" : "text-red-500")}>
      {ok ? <Check className="size-3.5" /> : <X className="size-3.5" />} {ago(at)}
    </span>
  )
}

function GraphCard({ r, agent, open, onOpen, onAgent, dry, onDry }: {
  r: Routine; agent?: Agent; open: boolean; onOpen: () => void; onAgent: (name: string) => void; dry?: Dry; onDry: () => void
}) {
  const [block, setBlock] = useState<string | null>(null)
  const g = r.graph
  const last = r.runs[0]
  const report = dry?.report
  return (
    <Panel className={cn("transition-colors", open && "bg-bubble/70")}>
      <div className="flex items-center gap-3 px-4 py-3.5">
        {agent ? <button onClick={() => onAgent(agent.name)} aria-label={agent.title}><Character agent={agent} size={30} still /></button> : <Bot className="size-6 text-muted-foreground" />}
        <button className="min-w-0 flex-1 text-left" onClick={onOpen} aria-expanded={open}>
          <div className="font-mono text-[13px] font-medium">{r.name}</div>
          <div className="truncate text-xs text-muted-foreground">
            {g ? <>{r.agent_title ?? g.agent} · {trigger(g.trigger)}{r.next_at ? ` · next ${clock(r.next_at)}` : ""}</> : <span className="text-red-500">This file does not hold</span>}
          </div>
        </button>
        <div className="text-right text-xs text-muted-foreground">
          {last ? <Last ok={last.ok} at={last.at} /> : "Never ran"}
          <div className="tabular-nums">{usd(r.cost_usd) || "$0"} this week</div>
        </div>
        <Button size="icon-sm" variant="ghost" onClick={onOpen} aria-label={open ? "Close" : "Open"}><ChevronDown className={cn("transition-transform", open && "rotate-180")} /></Button>
      </div>

      {!open && g && (
        <div className="-mt-1 flex flex-wrap items-center gap-1 px-4 pb-3.5">
          {g.blocks.map((b, i) => {
            const k = BLOCK[b.type] ?? BLOCK.act
            return (
              <span key={b.id} className="flex items-center gap-1">
                {i > 0 && <span className="text-muted-foreground/50">→</span>}
                <span className={cn("inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px]", b.type === "approval" && "border-amber-400/40 text-amber-600 dark:text-amber-300")}>
                  <k.icon className="size-3" /> {b.id}
                </span>
              </span>
            )
          })}
        </div>
      )}
      {!open && !g && r.error && <p className="-mt-1 px-4 pb-3.5 font-mono text-[11px] break-words text-red-500">{r.error}</p>}
      {!open && last?.error && <p className="-mt-1 line-clamp-2 px-4 pb-3.5 font-mono text-[11px] text-red-500">{last.error}</p>}

      {open && (
        <div className="grid gap-6 border-t px-4 py-4 md:grid-cols-[1fr_17rem]">
          {g ? (
            <div className="min-w-0">
              {g.description && <p className="mb-3 text-sm text-foreground/80">{g.description}</p>}
              <ol className="relative space-y-1.5 before:absolute before:top-4 before:bottom-4 before:left-[15px] before:w-px before:bg-border">
                <Node icon={Play} type="Trigger" title={trigger(g.trigger)} sub={r.next_at ? `next ${clock(r.next_at)}` : ""} />
                {g.blocks.map((b) => {
                  const k = BLOCK[b.type] ?? BLOCK.act
                  const [title, sub] = describe(b)
                  const ran = report?.blocks.find((x) => x.id === b.id)
                  return (
                    <Node key={b.id} icon={k.icon} type={k.label} title={title} sub={sub} warn={b.type === "approval"}
                      current={block === b.id} onClick={report ? () => setBlock(block === b.id ? null : b.id) : undefined}
                      out={report && (ran ? { ok: !ran.notes.length, text: `${ran.items_in} → ${ran.items_out} · ${ran.secs.toFixed(1)} s` } : { ok: null, text: "not reached" })} />
                  )
                })}
              </ol>
              {report && <BlockReport report={report} open={block} />}
              <p className="mt-4 text-xs text-muted-foreground">A dry run reads and asks the model for real, takes every approval as given, and only shows what each act would do. Nothing is sent.</p>
            </div>
          ) : <p className="font-mono text-xs break-words text-red-500">{r.error}</p>}

          <aside className="space-y-4 text-sm">
            <Button className="w-full" disabled={!g || dry?.running} onClick={onDry}>
              {dry?.running ? <><Loader className="animate-spin" /> Running dry…</> : <><FlaskConical /> Dry run</>}
            </Button>
            {dry?.running && <p className="text-xs text-muted-foreground">The feeds are read and the model asked. It can take a minute.</p>}
            {dry?.error && <p className="rounded-lg bg-red-500/10 px-3 py-2 font-mono text-xs break-words text-red-600 dark:text-red-300">{dry.error}</p>}
            {report && (
              <div className="space-y-1.5">
                <div className="flex items-baseline justify-between text-xs">
                  <span className="font-medium">Dry run{dry?.at ? ` · ${new Date(dry.at).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}` : ""}</span>
                  <span className="text-muted-foreground">{report.cost.replace(/^cost: /, "")}</span>
                </div>
                {report.failed?.map((f) => <p key={f} className="text-xs text-red-500">{f}</p>)}
              </div>
            )}
            <div>
              <div className="mb-1.5 text-xs font-medium">Last runs</div>
              {r.runs.length ? (
                <div className="space-y-1">
                  {r.runs.map((x) => (
                    <div key={x.at} className="flex items-center justify-between gap-2 text-xs" title={x.error ?? undefined}>
                      <span className="text-muted-foreground">{clock(x.at)}{late(x) && <span className="text-amber-500"> · {late(x)} late</span>}</span>
                      <span className={x.ok ? "text-emerald-500" : "text-red-500"}>{x.ok ? "Done" : "Failed"}</span>
                    </div>
                  ))}
                </div>
              ) : <p className="text-xs text-muted-foreground">No run yet. Each run shows here once it ends.</p>}
            </div>
            <div className="flex items-baseline justify-between text-xs">
              <span className="text-muted-foreground">Model cost, last 7 days</span>
              <span className="tabular-nums">{usd(r.cost_usd) || "$0"}{r.unpriced ? ` + ${r.unpriced} unpriced` : ""}</span>
            </div>
          </aside>
        </div>
      )}
    </Panel>
  )
}

/** How late a scheduled run started, past the 15 minutes of grace. */
function late(run: { at: string; due_at?: string | null }): string | null {
  if (!run.due_at) return null
  const min = Math.round((Date.parse(run.at) - Date.parse(run.due_at)) / 60000)
  if (min <= 15) return null
  return min < 120 ? `${min} min` : `${Math.round(min / 60)} h`
}

function Node({ icon: Icon, type, title, sub, warn, current, onClick, out }: {
  icon: LucideIcon; type: string; title: string; sub: string; warn?: boolean; current?: boolean; onClick?: () => void
  out?: { ok: boolean | null; text: string }
}) {
  const body = (
    <>
      <span className={cn("relative z-10 grid size-[31px] shrink-0 place-items-center rounded-lg border bg-surface", warn && "border-amber-400/50 text-amber-600 dark:text-amber-300")}><Icon className="size-4" /></span>
      <span className="min-w-0 flex-1">
        <span className="block text-[11px] text-muted-foreground">{type}</span>
        <span className="block truncate text-sm font-medium">{title}</span>
        {sub && <span className="block truncate text-xs text-muted-foreground">{sub}</span>}
      </span>
      {out && <span className={cn("shrink-0 rounded-md px-1.5 py-0.5 font-mono text-[11px]", out.ok === null ? "text-muted-foreground" : out.ok ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400" : "bg-amber-500/10 text-amber-600 dark:text-amber-400")}>{out.ok === false ? "! " : out.ok ? "✓ " : ""}{out.text}</span>}
    </>
  )
  return (
    <li>
      {onClick
        ? <button type="button" onClick={onClick} aria-pressed={current} className={cn("flex w-full items-center gap-3 rounded-lg px-0.5 py-1 text-left hover:bg-muted/50", current && "bg-muted/70")}>{body}</button>
        : <div className="flex items-center gap-3 px-0.5 py-1">{body}</div>}
    </li>
  )
}

/** What the chosen block did in the last dry run. */
function BlockReport({ report, open }: { report: Report; open: string | null }) {
  if (!open) return <p className="mt-3 text-xs text-muted-foreground">Choose a block to see what it did.</p>
  const ran = report.blocks.find((b) => b.id === open)
  if (!ran) return <p className="mt-3 text-xs text-muted-foreground">{open} was not reached in this dry run.</p>
  return (
    <section className="mt-3 space-y-1 rounded-lg border bg-surface p-3" aria-label={`What ${ran.id} did`}>
      <h3 className="mb-1.5 text-xs font-medium">What {ran.id} did</h3>
      {ran.lines.map((l, i) => <pre key={`l${i}`} className="font-mono text-[11.5px] whitespace-pre-wrap">{l}</pre>)}
      {ran.notes.map((l, i) => <pre key={`n${i}`} className="font-mono text-[11.5px] whitespace-pre-wrap text-amber-600 dark:text-amber-400">{l}</pre>)}
      {!ran.lines.length && !ran.notes.length && <p className="text-xs text-muted-foreground">Nothing to say: its items went on.</p>}
    </section>
  )
}

type Objective = {
  id: string; objective: string; every: string | null; bot: string | null; tools: string[] | null; allow: string[]; delegates: string[]
  max_minutes: number | null; wait: number | null; enabled: boolean; running: boolean; next_at: string | null; last_run: string | null; last_at: string | null; last_status: string | null
}
type Tasks = { routines: Objective[]; agents: string[]; extensions: string[] }

function TaskCard({ realm, o, agent, onAgent, onEdit, onChanged }: {
  realm: string; o: Objective; agent?: Agent; onAgent: (name: string) => void; onEdit: () => void; onChanged: () => void
}) {
  const [busy, setBusy] = useState(false)
  const path = `realms/${enc(realm)}/routines/${enc(o.id)}`
  async function run() {
    setBusy(true)
    try {
      await api(`${path}/run`, {})
      toast.success(`${o.id} started.`)
      onChanged()
      if (o.bot) onAgent(o.bot)
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  async function pause() {
    try { await api(path, { enabled: !o.enabled }, "PATCH"); toast.success(o.enabled ? `${o.id} paused.` : `${o.id} resumed.`); onChanged() }
    catch (e) { toast.error((e as Error).message) }
  }
  const what = [
    o.tools ? (o.tools.length ? `uses ${o.tools.join(", ")}` : "uses none of the business's tools") : "uses all of the business's tools",
    o.allow.length ? `may ${o.allow.map(allowLabel).join(", ").toLowerCase()} without asking` : "",
    o.delegates.length ? `may ask ${o.delegates.join(", ")}` : "",
    o.max_minutes ? `at most ${o.max_minutes} min` : "",
  ].filter(Boolean).join(" · ")
  return (
    <Panel className={cn("px-4 py-3.5", !o.enabled && "opacity-70")}>
      <div className="flex items-center gap-3">
        {agent ? <button onClick={() => onAgent(agent.name)} aria-label={agent.title}><Character agent={agent} size={30} still /></button> : <Bot className="size-6 text-muted-foreground" />}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="font-mono text-[13px] font-medium">{o.id}</span>
            {!o.enabled && <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">Paused</span>}
          </div>
          <div className="text-xs text-muted-foreground">
            {agent?.title ?? o.bot ?? "The general assistant"} · {o.every ? schedule(o.every) : "By hand"}{o.enabled && o.next_at ? ` · next ${clock(o.next_at)}` : ""}
          </div>
        </div>
        <div className="text-right text-xs text-muted-foreground">
          {o.last_status && o.last_at ? <Last ok={o.last_status === "completed"} at={o.last_at} /> : "Never ran"}
          {o.last_status && o.last_status !== "completed" && <div>{o.last_status}</div>}
        </div>
      </div>
      <p className="mt-2.5 text-sm text-foreground/85">{o.objective}</p>
      <p className="mt-1 text-xs text-muted-foreground">{what}</p>
      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        <Button size="sm" disabled={busy || o.running} onClick={run}>{o.running ? <><Loader className="animate-spin" /> Running…</> : <><Play /> Run now</>}</Button>
        <Button size="sm" variant="outline" onClick={pause}>{o.enabled ? <><Pause /> Pause</> : <><Play /> Resume</>}</Button>
        <Button size="sm" variant="outline" onClick={onEdit}><Pencil /> Edit</Button>
        <span className="flex-1" />
        <Remove label={o.id} onRemove={async () => {
          try { await api(path, undefined, "DELETE"); toast.success(`${o.id} removed.`); onChanged() }
          catch (e) { toast.error((e as Error).message) }
        }} />
      </div>
    </Panel>
  )
}

const BASE: [string, string][] = [["network", "Reach the web"], ["write", "Write files"], ["execute", "Run commands"], ["memory", "Remember"]]
const allowLabel = (v: string) => BASE.find(([k]) => k === v)?.[1] ?? (v.startsWith("network:") ? `Call ${v.slice(8)}` : v)

function Chip({ label, on, onClick }: { label: string; on: boolean; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} aria-pressed={on}
      className={cn("inline-flex h-7 items-center rounded-full border px-2.5 text-xs transition-colors",
        on ? "border-sky-500/60 bg-sky-500/10 text-foreground" : "text-muted-foreground hover:text-foreground")}>{label}</button>
  )
}

const flip = (l: string[], x: string) => (l.includes(x) ? l.filter((y) => y !== x) : [...l, x])
const NOBODY = "-"

/** Create or change an agent routine (decision 37): who, what, when, and
    what it may do without asking. */
function TaskForm({ realm, d, start, onClose, onSaved }: { realm: string; d: Tasks; start: Objective | "new" | null; onClose: () => void; onSaved: () => void }) {
  const editing = start && start !== "new" ? start : null
  const [shown, setShown] = useState<typeof start>(null)
  const [f, setF] = useState({ id: "", bot: NOBODY, objective: "", every: "", only: false, tools: [] as string[], allow: [] as string[], delegates: [] as string[], minutes: "" })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  if (start !== shown) {
    setShown(start)
    setError(null)
    const o = editing
    setF({
      id: o?.id ?? "", bot: o?.bot ?? NOBODY, objective: o?.objective ?? "", every: o?.every ?? "", only: !!o?.tools, tools: o?.tools ?? [],
      allow: o?.allow ?? [], delegates: o?.delegates ?? [], minutes: o?.max_minutes ? String(o.max_minutes) : "",
    })
  }
  // Each HTTP integration may be allowed on its own (MCP servers and skills
  // go through the tools list above).
  const detail = usePoll<{ integrations: { name: string }[] }>(start ? () => api(`realms/${enc(realm)}`) : null, 600000, [realm, !!start])
  const integrations = (detail.data?.integrations ?? []).filter((i) => d.extensions.includes(i.name)).map((i) => [`network:${i.name}`, `Call ${i.name}`] as [string, string])
  const choices = [...BASE, ...integrations]
  const others = d.routines.filter((x) => x.id !== editing?.id)
  const valid = /^[a-z0-9][a-z0-9_-]*$/.test(f.id) && f.objective.trim().length > 0

  async function save() {
    setBusy(true)
    setError(null)
    try {
      await api(`realms/${enc(realm)}/routines`, {
        id: f.id.trim(), objective: f.objective, bot: f.bot === NOBODY ? null : f.bot, every: f.every.trim() || null,
        tools: f.only ? f.tools : null, allow: f.allow, delegates: f.delegates, max_minutes: Number(f.minutes) || null,
        wait: editing?.wait ?? 10,
      })
      toast.success(editing ? `${f.id} saved.` : `${f.id} created.`)
      onSaved(); onClose()
    } catch (e) { setError((e as Error).message) }
    finally { setBusy(false) }
  }

  return (
    <Dialog open={!!start} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[90svh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{editing ? `Change ${editing.id}` : "New agent routine"}</DialogTitle>
          <DialogDescription>Each time, the agent works on the task as on one you give it: it may do what the business allows, plus what you allow here. Anything else waits for you.</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-1.5">
            <Label htmlFor="r-id">Name</Label>
            <Input id="r-id" value={f.id} disabled={!!editing} autoFocus={!editing} spellCheck={false} autoComplete="off" placeholder="weekly-article" className="font-mono"
              onChange={(e) => setF({ ...f, id: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-") })} />
          </div>
          <div className="space-y-1.5">
            <Label>Agent</Label>
            <Select value={f.bot} onValueChange={(v) => setF({ ...f, bot: String(v ?? NOBODY) })}>
              <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value={NOBODY}>The general assistant</SelectItem>
                {d.agents.map((a) => <SelectItem key={a} value={a}>{a}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="r-task">Task</Label>
          <Textarea id="r-task" rows={3} value={f.objective} onChange={(e) => setF({ ...f, objective: e.target.value })} placeholder="Write and prepare this week's article." />
        </div>
        <div className="grid grid-cols-[3fr_1fr] gap-3">
          <div className="space-y-1.5">
            <Label htmlFor="r-every">When</Label>
            <Input id="r-every" value={f.every} spellCheck={false} onChange={(e) => setF({ ...f, every: e.target.value })} placeholder="mon 08:00 · daily 07:00 · weekdays 09:00 · every 2h" className="font-mono" />
            <p className="text-xs text-muted-foreground">{f.every.trim() ? schedule(f.every) : "Empty: you run it by hand."}</p>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="r-min">Time limit</Label>
            <Input id="r-min" type="number" min={1} value={f.minutes} onChange={(e) => setF({ ...f, minutes: e.target.value })} placeholder="30" />
            <p className="text-xs text-muted-foreground">Minutes</p>
          </div>
        </div>
        <div className="space-y-2">
          <label className="flex items-center gap-2 text-sm font-medium">
            <Switch checked={f.only} onCheckedChange={(on) => setF({ ...f, only: on })} /> Only some of the business's tools
          </label>
          {f.only && (
            <div className="flex flex-wrap gap-1.5">
              {d.extensions.length ? d.extensions.map((x) => <Chip key={x} label={x} on={f.tools.includes(x)} onClick={() => setF({ ...f, tools: flip(f.tools, x) })} />)
                : <span className="text-xs text-muted-foreground">The business has no integration, MCP server or skill yet: it gets none.</span>}
            </div>
          )}
        </div>
        <div className="space-y-2">
          <Label>It may, without asking you</Label>
          <div className="flex flex-wrap gap-1.5">{choices.map(([v, l]) => <Chip key={v} label={l} on={f.allow.includes(v)} onClick={() => setF({ ...f, allow: flip(f.allow, v) })} />)}</div>
        </div>
        {others.length > 0 && (
          <div className="space-y-2">
            <Label>It may ask for work</Label>
            <div className="flex flex-wrap gap-1.5">{others.map((x) => <Chip key={x.id} label={x.bot ? `${x.id} (${x.bot})` : x.id} on={f.delegates.includes(x.id)} onClick={() => setF({ ...f, delegates: flip(f.delegates, x.id) })} />)}</div>
            <p className="text-xs text-muted-foreground">Those routines work for it within their own tools and limits, never this one's.</p>
          </div>
        )}
        {error && <p className="rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300">{error}</p>}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button disabled={!valid || busy} onClick={save}>{busy ? "Saving…" : editing ? "Save" : "Create"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

type Pipeline = {
  name: string; description?: string; schedule?: string | null; steps?: string[]; next_at?: string | null; error?: string
  runs: { started_at: string; ok: boolean; items_read: number; items_kept: number; notes: string; report: string | null; error: string | null }[]
}

function PipelineCard({ realm, p }: { realm: string; p: Pipeline }) {
  const [busy, setBusy] = useState(false)
  const [report, setReport] = useState(false)
  const last = p.runs[0]
  async function run() {
    setBusy(true)
    try { await api(`realms/${enc(realm)}/pipelines/${enc(p.name)}/run`, {}); toast.success(`${p.name} started: its run shows here when it ends.`) }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  return (
    <Panel className="px-4 py-3.5">
      <div className="flex items-center gap-3">
        <ListOrdered className="size-5 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="font-mono text-[13px] font-medium">{p.name}</div>
          <div className="text-xs text-muted-foreground">{p.schedule ? schedule(p.schedule) : "By hand"}{p.next_at ? ` · next ${clock(p.next_at)}` : ""}</div>
        </div>
        <div className="text-right text-xs text-muted-foreground">{last ? <Last ok={last.ok} at={last.started_at} /> : "Never ran"}</div>
        <Button size="sm" variant="outline" disabled={!!p.error || busy} onClick={run}><Play /> Run now</Button>
      </div>
      {p.error ? <p className="mt-2 font-mono text-[11px] break-words text-red-500">{p.error}</p> : (
        <>
          {p.description && <p className="mt-2.5 text-sm text-foreground/85">{p.description}</p>}
          {p.steps && p.steps.length > 0 && <ol className="mt-2 list-decimal space-y-0.5 pl-5 text-xs text-muted-foreground">{p.steps.map((s, i) => <li key={i}>{s}</li>)}</ol>}
        </>
      )}
      {p.runs.length > 0 && (
        <div className="mt-3 space-y-1 border-t pt-2.5">
          {p.runs.slice(0, 5).map((r) => (
            <div key={r.started_at} className="flex items-center justify-between gap-3 text-xs">
              <span className="text-muted-foreground">{clock(r.started_at)}</span>
              {r.ok ? <span>{r.items_kept} kept of {r.items_read}</span> : <span className="truncate text-red-500" title={r.error ?? ""}>{r.error ?? "Failed"}</span>}
            </div>
          ))}
        </div>
      )}
      {last?.ok && last.notes && <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{last.notes}</p>}
      {last?.report && (
        <div className="mt-2">
          <button className="text-xs text-sky-500 hover:underline" onClick={() => setReport(!report)}>{report ? "Hide the last report" : "The last report"}</button>
          {report && <div className="mt-2 max-h-80 overflow-auto rounded-lg border bg-surface p-3 text-sm"><Rich text={last.report} /></div>}
        </div>
      )}
    </Panel>
  )
}
