import { useState } from "react"
import { Bot, BookOpen, Check, ChevronDown, GitBranch, GraduationCap, Hand, ListOrdered, Loader, Pause, Pencil, Play, Plus, Repeat, Sparkles, Workflow, X, Zap, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { Rich } from "@/components/rich"
import { RoutineCanvas, schedule, triggerText } from "@/components/routine-canvas"
import type { Declared } from "@/components/routine-editor"
import { Empty, Loading, Page, Panel, Section } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Feedback, type Routine } from "@/lib/api"
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

// Not a routine name: those are lowercase letters, digits and dashes.
const NEW = " new"

const clock = (iso: string) => new Date(iso).toLocaleString("en-GB", { weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })

/** The business's recurring work, in three kinds: routines made of blocks
    (decision 49), drawn on a canvas (decision 91); agents given a task on a
    schedule (decision 37); and the fixed sequences of business/pipelines/
    (decision 30). */
export function Routines({ realm, agents, onAgent }: { realm: string; agents: Agent[]; onAgent: (name: string) => void }) {
  const graphs = usePoll<{ routines: Routine[]; actions: Declared[]; prompts: string[] }>(() => api(`realms/${enc(realm)}/graphs`), 15000, [realm])
  const tasks = usePoll<Tasks>(() => api(`realms/${enc(realm)}/routines`), 15000, [realm])
  const fixed = usePoll<{ pipelines: Pipeline[] }>(() => api(`realms/${enc(realm)}/business`), 30000, [realm])
  const [form, setForm] = useState<Objective | "new" | null>(null)
  // The routine open on the canvas, NEW for one being drawn.
  const [canvas, setCanvas] = useState<string | null>(null)

  if (canvas && graphs.data) {
    const routine = canvas === NEW ? null : graphs.data.routines.find((r) => r.name === canvas) ?? null
    return (
      <div className="min-w-0 flex-1 p-3">
        <RoutineCanvas key={routine?.name ?? NEW} realm={realm} routine={routine} agents={agents}
          actions={graphs.data.actions} prompts={graphs.data.prompts} onClose={() => setCanvas(null)}
          onSaved={async (name) => { await graphs.refresh(); setCanvas(name) }} />
      </div>
    )
  }

  const count = (graphs.data?.routines.length ?? 0) + (tasks.data?.routines.length ?? 0) + (fixed.data?.pipelines.length ?? 0)
  return (
    <Page wide title="Routines" lede={graphs.data && tasks.data ? `${count} routine${count === 1 ? "" : "s"}, the agents' recurring work.` : undefined}
      actions={<div className="flex gap-2">
        <Button size="sm" disabled={!graphs.data || !agents.length} onClick={() => setCanvas(NEW)} title={agents.length ? undefined : "Hire an agent first"}><Workflow /> New routine</Button>
        <Button size="sm" variant="outline" onClick={() => setForm("new")}><Plus /> New agent routine</Button>
      </div>}>
      <Section title="Routines" note="Files in routines/, in the business folder">
        {!graphs.data ? <Loading /> : graphs.data.routines.length ? (
          <div className="space-y-2.5">
            {graphs.data.routines.map((r) => (
              <GraphCard key={r.name} realm={realm} r={r} agent={agents.find((x) => x.name === r.graph?.agent)} onOpen={() => setCanvas(r.name)} onAgent={onAgent} onChanged={graphs.refresh} />
            ))}
          </div>
        ) : <Empty icon={Repeat} title="No routine yet">Draw one: a trigger, then blocks you drag in and link.</Empty>}
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

/** A routine in the list: who, when, its chain of blocks; it opens on the canvas. */
function GraphCard({ realm, r, agent, onOpen, onAgent, onChanged }: { realm: string; r: Routine; agent?: Agent; onOpen: () => void; onAgent: (name: string) => void; onChanged: () => void }) {
  const g = r.graph
  const last = r.runs[0]
  return (
    <Panel className="transition-colors hover:bg-bubble/50">
      <div className="flex items-center gap-3 px-4 py-3.5">
        {agent ? <button onClick={() => onAgent(agent.name)} aria-label={agent.title}><Character agent={agent} size={30} still /></button> : <Bot className="size-6 text-muted-foreground" />}
        <button className="min-w-0 flex-1 text-left" onClick={onOpen}>
          <div className="font-mono text-[13px] font-medium">{r.name}</div>
          <div className="truncate text-xs text-muted-foreground">
            {g ? <>{r.agent_title ?? g.agent} · {triggerText(g.trigger)}{r.next_at ? ` · next ${clock(r.next_at)}` : ""}{late(last) && <span className="text-amber-500"> · last run {late(last)} late</span>}</> : <span className="text-red-500">This file does not hold</span>}
          </div>
        </button>
        <div className="text-right text-xs text-muted-foreground">
          {last ? <Last ok={last.ok} at={last.at} /> : "Never ran"}
          <div className="tabular-nums">{usd(r.cost_usd) || "$0"} this week{r.unpriced ? ` + ${r.unpriced} unpriced` : ""}</div>
        </div>
        <Button size="sm" variant="outline" onClick={onOpen}><Workflow /> Open</Button>
      </div>
      {g && (
        <button className="-mt-1 flex w-full flex-wrap items-center gap-1 px-4 pb-3.5 text-left" onClick={onOpen}>
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
        </button>
      )}
      {!!r.feedback?.length && <Learned realm={realm} said={r.feedback} onChanged={onChanged} />}
      {!g && r.error && <p className="-mt-1 px-4 pb-3.5 font-mono text-[11px] break-words text-red-500">{r.error}</p>}
      {last?.error && <p className="-mt-1 line-clamp-2 px-4 pb-3.5 font-mono text-[11px] text-red-500">{last.error}</p>}
    </Panel>
  )
}

/** What the routine's next drafts hear from the founder (decision 116):
    their last edits, reasons to reject and redos. One that no longer holds
    can be taken back. */
function Learned({ realm, said, onChanged }: { realm: string; said: Feedback[]; onChanged: () => void }) {
  const [open, setOpen] = useState(false)
  async function forget(f: Feedback) {
    try {
      await api(`realms/${enc(realm)}/queue/${f.item}/forget`, {})
      toast.success("Forgotten: the next drafts no longer hear it.")
      onChanged()
    } catch (e) {
      toast.error((e as Error).message)
    }
  }
  const cut = (t: string) => (t.length > 160 ? `${t.slice(0, 160)}…` : t)
  return (
    <div className="border-t px-4 py-2.5">
      <button className="flex w-full items-center gap-2 text-left text-xs text-muted-foreground hover:text-foreground" onClick={() => setOpen(!open)}>
        <GraduationCap className="size-3.5" />
        <span>Learned from you: {said.length} note{said.length === 1 ? "" : "s"}, heard by the next drafts</span>
        <ChevronDown className={cn("ml-auto size-3.5 transition-transform", open && "rotate-180")} />
      </button>
      {open && (
        <ul className="mt-2 space-y-2">
          {said.map((f) => (
            <li key={f.item} className="group flex gap-3 rounded-lg bg-bubble px-3 py-2 text-xs leading-relaxed">
              <div className="min-w-0 flex-1">
                {f.kind === "edit" ? (
                  <>
                    <p className="text-muted-foreground line-through decoration-muted-foreground/40">{cut(f.draft)}</p>
                    <p className="mt-0.5">{cut(f.edited)}</p>
                  </>
                ) : (
                  <>
                    <p><b className="font-medium">{f.kind === "redo" ? "Redo:" : "Rejected:"}</b> “{f.said}”</p>
                    <p className="mt-0.5 text-muted-foreground">{cut(f.draft)}</p>
                  </>
                )}
                <p className="mt-0.5 text-[11px] text-muted-foreground">{ago(f.at)}</p>
              </div>
              <Button size="xs" variant="ghost" className="shrink-0 self-start text-muted-foreground opacity-0 group-hover:opacity-100 max-md:opacity-100" onClick={() => forget(f)}>Forget</Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** How late a scheduled run started, past the 15 minutes of grace. */
function late(run?: { at: string; due_at?: string | null }): string | null {
  if (!run?.due_at) return null
  const min = Math.round((Date.parse(run.at) - Date.parse(run.due_at)) / 60000)
  if (min <= 15) return null
  return min < 120 ? `${min} min` : `${Math.round(min / 60)} h`
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
              <SelectTrigger className="w-full"><SelectValue>{(v: string) => (v === NOBODY ? "The general assistant" : v)}</SelectValue></SelectTrigger>
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
          {report && <div className="mt-2 max-h-80 overflow-auto rounded-lg bg-surface p-3 text-sm"><Rich text={last.report} /></div>}
        </div>
      )}
    </Panel>
  )
}
