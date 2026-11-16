import { useState } from "react"
import { Bot, Building2, CalendarClock, Check, ChevronDown, ChevronRight, GraduationCap, ListOrdered, Loader, Pause, Pencil, Play, Plus, Repeat, Search, Workflow, X } from "lucide-react"
import { toast } from "sonner"
import { LEVELS } from "@/components/agent-page"
import { Character } from "@/components/character"
import { Rich } from "@/components/rich"
import { RoutineCanvas } from "@/components/routine-canvas"
import type { Declared } from "@/components/routine-editor"
import { Empty, Loading, Page, Panel, Section, Seg } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Sheet, SheetContent, SheetDescription, SheetTitle } from "@/components/ui/sheet"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Feedback, type Item, type Routine } from "@/lib/api"
import {
  FOLDED_FROM, agentRow, filter, graphRow, group, needsLook, reasons, scheduleText, sequenceRow, state, triggerText,
  type Filter, type Group, type GroupBy, type Reason, type Row,
} from "@/lib/routines"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"
import { Remove } from "@/screens/settings-keys"

// A run's day and time; past the coming week, its month too.
const clock = (iso: string) => new Date(iso).toLocaleString("en-GB", {
  weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
  ...(Math.abs(Date.parse(iso) - Date.now()) > 6 * 86400000 ? { month: "short" } : {}),
})

/** Everything the page shows of one business: its routines of each kind,
    as rows, with what the editors need. */
type Loaded = { realm: string; routines: Routine[]; actions: Declared[]; prompts: string[]; tasks: Tasks; pipelines: Pipeline[]; agents: Agent[]; rows: Row[] }

async function load(realm: string): Promise<Loaded> {
  const r = enc(realm)
  const [graphs, tasks, fixed, team, queue] = await Promise.all([
    api<{ routines: Routine[]; actions: Declared[]; prompts: string[] }>(`realms/${r}/graphs`),
    api<Tasks>(`realms/${r}/routines`),
    api<{ pipelines: Pipeline[] }>(`realms/${r}/business`).catch(() => ({ pipelines: [] as Pipeline[] })),
    api<{ agents: Agent[] }>(`realms/${r}/team`),
    api<{ items: Item[] }>(`realms/${r}/queue`),
  ])
  // A routine's level comes from its agent's levels (decision 140).
  const owners = [...new Set(graphs.routines.map((x) => x.graph?.agent).filter((a): a is string => !!a))]
  const levels: Record<string, Record<string, number>> = Object.fromEntries(await Promise.all(owners.map(async (a) => {
    const l = await api<{ actions: { action: string; level: number }[] }>(`realms/${r}/agents/${enc(a)}/levels`).catch(() => ({ actions: [] }))
    return [a, Object.fromEntries(l.actions.map((x) => [x.action, x.level]))]
  })))
  const title = (name: string | null) => (name ? team.agents.find((a) => a.name === name)?.title ?? name : null)
  const rows = [
    ...graphs.routines.map((x) => graphRow(realm, x, levels[x.graph?.agent ?? ""], queue.items)),
    ...tasks.routines.map((o) => agentRow(realm, o, title(o.bot), queue.items)),
    ...fixed.pipelines.map((p) => sequenceRow(realm, p, queue.items)),
  ].map((row) => ({ ...row, agentTitle: title(row.agent) ?? row.agentTitle }))
  return { realm, routines: graphs.routines, actions: graphs.actions, prompts: graphs.prompts, tasks, pipelines: fixed.pipelines, agents: team.agents, rows }
}

// Which groups the founder folded or opened, in this browser only.
const FOLDS = "airth-routines-folded"
function readFolds(): Record<string, boolean> {
  try { return JSON.parse(localStorage.getItem(FOLDS) ?? "{}") } catch { return {} }
}

const BY: [GroupBy, string][] = [["agent", "Agent"], ["frequency", "Frequency"], ["business", "Business"]]
const ALL = "*"

type Open = { realm: string; key: string } | { realm: string; key: null }

/** The business's recurring work, in three kinds: routines made of blocks
    (decision 49), drawn on a canvas (decision 91); agents given a task on a
    schedule (decision 37); and the fixed sequences of business/pipelines/
    (decision 30). What needs a look comes first; the rest is grouped, and
    a routine opens in a panel beside the list. */
export function Routines({ realm, agents, onAgent, businesses }: { realm: string; agents: Agent[]; onAgent: (name: string) => void; businesses: { name: string; title: string }[] }) {
  const [by, setBy] = useState<GroupBy>("agent")
  const [f, setF] = useState<Filter>({ query: "", status: "all", agent: null })
  const [folds, setFolds] = useState(readFolds)
  // The routine open in the panel; a null key is a new routine being drawn.
  const [open, setOpen] = useState<Open | null>(null)
  const [form, setForm] = useState<{ realm: string; start: Objective | "new" } | null>(null)

  const here = usePoll(() => load(realm), 15000, [realm])
  // Grouped by business, every business's routines are listed, as the HQ
  // shows them; each one's are read from its own business.
  const others = businesses.filter((b) => b.name !== realm)
  const away = usePoll(by === "business" && others.length ? () => Promise.all(others.map((b) => load(b.name).catch(() => null))) : null, 30000, [realm, by, others.map((b) => b.name).join()])
  const loaded = [here.data, ...(by === "business" ? away.data ?? [] : [])].filter((x): x is Loaded => !!x)
  const refresh = () => { here.refresh(); if (by === "business") away.refresh() }

  const all = loaded.flatMap((l) => l.rows)
  const shown = filter(all, f)
  const look = f.status === "all" ? needsLook(shown) : []
  const order = [...businesses.filter((b) => b.name === realm), ...others]
  const groups = group(shown, by, order)
  const owners = [...new Map(all.filter((r) => r.agent).map((r) => [r.agent!, r.agentTitle ?? r.agent!])).entries()].sort((a, b) => a[1].localeCompare(b[1]))
  const agentOf = (r: { realm: string; agent: string | null }) => loaded.find((l) => l.realm === r.realm)?.agents.find((a) => a.name === r.agent) ?? (r.realm === realm ? agents.find((a) => a.name === r.agent) : undefined)

  // A search opens every group, so nothing it finds is folded away.
  const foldKey = (g: Group) => `${by === "business" ? "" : realm}|${g.key}`
  const folded = (g: Group) => !f.query.trim() && (folds[foldKey(g)] ?? g.rows.length >= FOLDED_FROM)
  function toggle(g: Group) {
    const next = { ...folds, [foldKey(g)]: !folded(g) }
    setFolds(next)
    try { localStorage.setItem(FOLDS, JSON.stringify(next)) } catch { /* private window */ }
  }

  const count = here.data?.rows.length ?? 0
  const busy = by === "business" && others.length > 0 && !away.data
  return (
    <Page wide title="Routines" lede={here.data ? `${count} routine${count === 1 ? "" : "s"}, the agents' recurring work.` : undefined}
      actions={<div className="flex gap-2">
        <Button size="sm" disabled={!here.data || !agents.length} onClick={() => setOpen({ realm, key: null })} title={agents.length ? undefined : "Hire an agent first"}><Workflow /> New routine</Button>
        <Button size="sm" variant="outline" disabled={!here.data} onClick={() => setForm({ realm, start: "new" })}><Plus /> New agent routine</Button>
      </div>}>
      {!here.data ? <Loading /> : !all.length ? (
        <Empty icon={Repeat} title="No routine yet">Draw one: a trigger, then blocks you drag in and link. Or give an agent a task to do on a schedule.</Empty>
      ) : (
        <>
          <div className="-mt-2 flex flex-wrap items-center gap-2">
            <label className="flex h-8 w-56 items-center gap-2 rounded-lg border bg-transparent px-2.5 text-sm text-muted-foreground focus-within:ring-2 focus-within:ring-ring/40 max-sm:w-full">
              <Search className="size-4 shrink-0" />
              <input value={f.query} onChange={(e) => setF({ ...f, query: e.target.value })} placeholder="Search by name" aria-label="Search routines by name" className="min-w-0 flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
              {f.query && <button aria-label="Clear the search" onClick={() => setF({ ...f, query: "" })}><X className="size-3.5" /></button>}
            </label>
            <Seg label="Show" value={f.status} options={[["all", "All"], ["look", "Needs a look"], ["paused", "Paused"]]} onPick={(status) => setF({ ...f, status })} />
            <Select value={f.agent ?? ALL} onValueChange={(v) => setF({ ...f, agent: v === ALL || v == null ? null : String(v) })}>
              <SelectTrigger size="sm" className="w-44" aria-label="Agent"><SelectValue>{(v: string) => (v === ALL ? "Every agent" : owners.find(([a]) => a === v)?.[1] ?? v)}</SelectValue></SelectTrigger>
              <SelectContent>
                <SelectItem value={ALL}>Every agent</SelectItem>
                {owners.map(([a, t]) => <SelectItem key={a} value={a}>{t}</SelectItem>)}
              </SelectContent>
            </Select>
            <span className="flex-1" />
            <span className="text-xs text-muted-foreground">Group by</span>
            <Select value={by} onValueChange={(v) => v && setBy(v as GroupBy)}>
              <SelectTrigger size="sm" className="w-32" aria-label="Group by"><SelectValue>{(v: string) => BY.find(([k]) => k === v)?.[1] ?? v}</SelectValue></SelectTrigger>
              <SelectContent>{BY.map(([k, l]) => <SelectItem key={k} value={k}>{l}</SelectItem>)}</SelectContent>
            </Select>
          </div>

          {look.length > 0 && (
            <Section title="Needs a look" note={`${look.length} routine${look.length === 1 ? "" : "s"}`}>
              <Panel className="divide-y overflow-hidden">
                {look.map(({ row, reasons }) => (
                  <button key={row.key} onClick={() => setOpen({ realm: row.realm, key: row.key })} className="flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-accent/50">
                    <Dot row={row} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="truncate font-mono text-[13px] font-medium">{row.name}</span>
                        <span className="truncate text-xs text-muted-foreground">{row.agentTitle ?? "No agent"}</span>
                      </div>
                      <div className={cn("truncate text-xs", reasons[0].kind === "waiting" ? "text-amber-600 dark:text-amber-300" : "text-red-500")}>
                        {said(reasons[0])}{reasons.length > 1 && <span className="text-muted-foreground"> · and {reasons.length - 1} more</span>}
                      </div>
                    </div>
                    <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
                  </button>
                ))}
              </Panel>
            </Section>
          )}

          <section className="space-y-2.5">
            {busy && <p className="text-xs text-muted-foreground">Reading the other businesses' routines…</p>}
            {groups.length > 0 && (
              <div className="grid grid-cols-[8.5rem_minmax(0,1fr)_minmax(0,1.15fr)_8.5rem_8.5rem_2.75rem] gap-x-4 px-4 text-[11.5px] font-semibold tracking-wide text-muted-foreground uppercase max-md:hidden">
                <span>State</span><span>Routine</span><span>When</span><span>Next run</span><span>Last run</span><span className="text-right">Level</span>
              </div>
            )}
            {groups.map((g) => {
              const closed = folded(g)
              const agent = by === "agent" && g.agent ? agentOf({ realm, agent: g.agent }) : undefined
              return (
                <Panel key={g.key} className="overflow-hidden">
                  <button onClick={() => toggle(g)} aria-expanded={!closed} className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/40">
                    <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition-transform", closed && "-rotate-90")} />
                    {by === "agent" ? (agent ? <Character agent={agent} size={24} still /> : <Bot className="size-5 text-muted-foreground" />)
                      : by === "business" ? <Building2 className="size-5 text-muted-foreground" /> : <CalendarClock className="size-5 text-muted-foreground" />}
                    <span className="min-w-0 flex-1 md:flex md:items-baseline md:gap-3">
                      <span className="block truncate font-medium">{g.label}</span>
                      <span className="block shrink-0 text-xs text-muted-foreground">{g.rows.length} routine{g.rows.length === 1 ? "" : "s"}<span className="md:hidden"> · {g.next ? `next ${clock(g.next)}` : "nothing scheduled"}</span></span>
                    </span>
                    <span className="shrink-0 text-xs text-muted-foreground max-md:hidden">{g.next ? `Next ${clock(g.next)}` : "Nothing scheduled"}</span>
                  </button>
                  {!closed && <div className="divide-y border-t">{g.rows.map((r) => <Line key={r.key} row={r} withAgent={by !== "agent"} onOpen={() => setOpen({ realm: r.realm, key: r.key })} />)}</div>}
                </Panel>
              )
            })}
            {!groups.length && <p className="px-1 py-6 text-center text-sm text-muted-foreground">No routine matches{f.query ? ` “${f.query}”` : ""}.</p>}
          </section>
        </>
      )}

      <Sheet open={!!open} onOpenChange={(o) => !o && setOpen(null)}>
        <SheetContent side="right" className="w-full gap-0 p-0 data-[side=right]:w-full data-[side=right]:sm:max-w-[min(1180px,94vw)]">
          {open && <Detail open={open} loaded={loaded.find((l) => l.realm === open.realm)} agentOf={agentOf} onAgent={onAgent}
            onClose={() => setOpen(null)} onChanged={refresh} onOpen={setOpen}
            onEdit={(o) => setForm({ realm: open.realm, start: o })} />}
        </SheetContent>
      </Sheet>

      {form && loaded.some((l) => l.realm === form.realm) && (
        <TaskForm realm={form.realm} d={loaded.find((l) => l.realm === form.realm)!.tasks} start={form.start} onClose={() => setForm(null)} onSaved={refresh} />
      )}
    </Page>
  )
}

const STATE = {
  ok: { label: "Healthy", dot: "bg-emerald-500" },
  waiting: { label: "Waiting for you", dot: "bg-amber-500" },
  incident: { label: "Incident", dot: "bg-red-500" },
  paused: { label: "Paused", dot: "border border-muted-foreground/70" },
}

/** The pill's word: an incident says which kind. */
function stateLabel(row: Row): string {
  const s = state(row)
  if (s !== "incident") return STATE[s].label
  const first = reasons(row)[0]
  return first.kind === "failed" ? "Failed" : first.kind === "broken" ? "Does not load" : "Incident"
}

function Dot({ row }: { row: Row }) {
  return <span className={cn("size-2 shrink-0 rounded-full", STATE[state(row)].dot)} aria-hidden />
}

function Pill({ row }: { row: Row }) {
  return <span className="inline-flex min-w-0 items-center gap-1.5 text-xs"><Dot row={row} /><span className="truncate">{stateLabel(row)}</span></span>
}

/** One reason a routine needs a look, in a line. */
function said(r: Reason): string {
  switch (r.kind) {
    case "incident": return `Incident: ${r.item.title || "see Needs you"}`
    case "broken": return `Its file does not load: ${r.error}`
    case "failed": return `Last run failed ${ago(r.at)}${r.error && r.error !== "failed" ? `: ${r.error}` : ""}`
    case "waiting": return `Waiting for you: ${r.item.title || (r.item.kind === "prepared" ? "an action to carry out" : "an approval")}`
  }
}

function Level({ row }: { row: Row }) {
  if (row.level === null) {
    const why = row.kind === "graph" ? "No declared action in its Act blocks" : row.kind === "agent" ? "It works as a task: each action asks at its own level" : "A fixed sequence: it takes no action of its own"
    return <span className="text-muted-foreground/60" title={why}>—</span>
  }
  const each = row.levels.map((l) => `${l.action}: ${l.level} · ${LEVELS[l.level].name}`).join("\n")
  return (
    <span title={`Level ${row.level} · ${LEVELS[row.level].name}: ${LEVELS[row.level].hint}\n${each}`}
      className={cn("inline-flex h-5 items-center rounded-md px-1.5 font-mono text-[11px] tabular-nums", row.level < 2 ? "bg-amber-500/12 text-amber-700 dark:text-amber-300" : "bg-muted text-muted-foreground")}>
      L{row.level}
    </span>
  )
}

function NextRun({ row }: { row: Row }) {
  if (!row.enabled) return <span className="text-muted-foreground">Paused</span>
  if (row.next) return <span className="tabular-nums">{clock(row.next)}</span>
  return <span className="text-muted-foreground">{row.trigger.schedule ? "Not planned" : "On demand"}</span>
}

function LastRun({ row }: { row: Row }) {
  if (row.broken) return <span className="text-red-500">Does not load</span>
  if (!row.last) return <span className="text-muted-foreground">Never ran</span>
  return <Last ok={row.last.ok} at={row.last.at} />
}

/** A routine in its group: state, name, when, next and last run, level. */
function Line({ row, withAgent, onOpen }: { row: Row; withAgent: boolean; onOpen: () => void }) {
  const kind = row.kind === "agent" ? "Agent routine" : row.kind === "sequence" ? "Fixed sequence" : null
  const who = [withAgent ? row.agentTitle ?? "No agent" : null, kind].filter(Boolean).join(" · ")
  return (
    <button onClick={onOpen} className="w-full text-left text-[13px] transition-colors hover:bg-accent/50">
      <div className="grid min-h-11 grid-cols-[8.5rem_minmax(0,1fr)_minmax(0,1.15fr)_8.5rem_8.5rem_2.75rem] items-center gap-x-4 px-4 py-2 max-md:hidden">
        <Pill row={row} />
        <span className="min-w-0">
          <span className="block truncate font-mono font-medium">{row.name}</span>
          {who && <span className="block truncate text-xs text-muted-foreground">{who}</span>}
        </span>
        <span className="truncate text-foreground/85">{triggerText(row.trigger)}</span>
        <span className="truncate text-xs"><NextRun row={row} /></span>
        <span className="truncate text-xs"><LastRun row={row} /></span>
        <span className="text-right"><Level row={row} /></span>
      </div>
      <div className="px-4 py-2.5 md:hidden">
        <div className="flex items-center gap-2">
          <Dot row={row} />
          <span className="min-w-0 flex-1 truncate font-mono font-medium">{row.name}</span>
          <Level row={row} />
        </div>
        <div className="mt-0.5 truncate pl-4 text-xs text-muted-foreground">{triggerText(row.trigger)} · <NextRun row={row} /></div>
        <div className="truncate pl-4 text-xs text-muted-foreground"><span className="text-foreground/80">{stateLabel(row)}</span> · <LastRun row={row} />{who && ` · ${who}`}</div>
      </div>
    </button>
  )
}

function Last({ ok, at }: { ok: boolean; at: string }) {
  return (
    <span className={cn("inline-flex items-center gap-1", ok ? "text-emerald-600 dark:text-emerald-500" : "text-red-500")}>
      {ok ? <Check className="size-3.5" /> : <X className="size-3.5" />} {ok ? ago(at) : `Failed ${ago(at)}`}
    </span>
  )
}

/** What the panel shows: a routine of blocks on its canvas, with its runs
    and what needs a look above; an agent routine or a fixed sequence as
    its card. */
function Detail({ open, loaded, agentOf, onAgent, onClose, onChanged, onOpen, onEdit }: {
  open: Open; loaded?: Loaded; agentOf: (r: { realm: string; agent: string | null }) => Agent | undefined; onAgent: (name: string) => void
  onClose: () => void; onChanged: () => void; onOpen: (o: Open) => void; onEdit: (o: Objective) => void
}) {
  if (!loaded) return <div className="p-6"><Loading /></div>
  const agents = loaded.agents
  if (open.key === null) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <SheetTitle className="sr-only">New routine</SheetTitle>
        <SheetDescription className="sr-only">Draw a routine: a trigger, then blocks you drag in and link.</SheetDescription>
        <div className="min-h-0 flex-1 p-3">
          <RoutineCanvas key="new" realm={loaded.realm} routine={null} agents={agents} actions={loaded.actions} prompts={loaded.prompts}
            onClose={onClose} onSaved={async (name) => { onChanged(); onOpen({ realm: loaded.realm, key: `${loaded.realm}/graph/${name}` }) }} />
        </div>
      </div>
    )
  }
  const row = loaded.rows.find((r) => r.key === open.key)
  if (!row) return <div className="p-6 text-sm text-muted-foreground">This routine is no longer in the business.</div>
  const agent = agentOf(row)
  const why = reasons(row)
  const graph = row.kind === "graph" ? loaded.routines.find((r) => r.name === row.name) : undefined
  const cost = graph?.cost_usd ?? 0
  const unpriced = graph?.unpriced ?? 0
  const lateBy = late(graph?.runs[0])
  const head = (
    <div className="space-y-2.5 border-b px-5 pt-4 pb-3.5 pr-14">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <SheetTitle className="font-mono text-base">{row.name}</SheetTitle>
        <Pill row={row} />
        <Level row={row} />
      </div>
      <SheetDescription className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
        {agent ? (
          <button className="inline-flex items-center gap-1.5 hover:text-foreground" onClick={() => onAgent(agent.name)}><Character agent={agent} size={18} still /> {agent.title}</button>
        ) : <span>{row.kind === "sequence" ? "Fixed sequence" : "The general assistant"}</span>}
        <span>·</span><span>{triggerText(row.trigger)}</span>
        <span>·</span><span>Next: <NextRun row={row} /></span>
        <span>·</span><span>Last: <LastRun row={row} /></span>
        {row.kind === "graph" && <><span>·</span><span>{usd(cost) || "$0"} this week{unpriced ? ` + ${unpriced} unpriced` : ""}</span></>}
        {lateBy && <><span>·</span><span className="text-amber-600 dark:text-amber-300">last run started {lateBy} late</span></>}
      </SheetDescription>
      {why.length > 0 && (
        <ul className="space-y-1 rounded-lg bg-muted/60 px-3 py-2 text-xs">
          {why.map((r, i) => <li key={i} className={r.kind === "waiting" ? "text-amber-700 dark:text-amber-300" : "text-red-600 dark:text-red-400"}>{said(r)}{(r.kind === "incident" || r.kind === "waiting") && <span className="text-muted-foreground"> · in Needs you</span>}</li>)}
        </ul>
      )}
    </div>
  )

  if (graph) {
    const routine = graph
    return (
      <div className="flex h-full min-h-0 flex-col">
        {head}
        {!!routine.feedback?.length && <div className="max-h-[30svh] shrink-0 overflow-y-auto"><Learned realm={loaded.realm} said={routine.feedback} onChanged={onChanged} /></div>}
        <div className="min-h-[420px] flex-1 border-t p-3">
          <RoutineCanvas key={routine.name} realm={loaded.realm} routine={routine} agents={agents} actions={loaded.actions} prompts={loaded.prompts}
            onClose={onClose} onSaved={async () => onChanged()} />
        </div>
      </div>
    )
  }
  const o = row.kind === "agent" ? loaded.tasks.routines.find((x) => x.id === row.name) : undefined
  const p = row.kind === "sequence" ? loaded.pipelines.find((x) => x.name === row.name) : undefined
  return (
    <div className="flex h-full min-h-0 flex-col">
      {head}
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {o && <TaskCard realm={loaded.realm} o={o} agent={agent} onAgent={onAgent} onEdit={() => onEdit(o)} onChanged={onChanged} />}
        {p && <PipelineCard realm={loaded.realm} p={p} />}
      </div>
    </div>
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
            {agent?.title ?? o.bot ?? "The general assistant"} · {o.every ? scheduleText(o.every) : "By hand"}{o.enabled && o.next_at ? ` · next ${clock(o.next_at)}` : ""}
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
            <p className="text-xs text-muted-foreground">{f.every.trim() ? scheduleText(f.every) : "Empty: you run it by hand."}</p>
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
          <div className="text-xs text-muted-foreground">{p.schedule ? scheduleText(p.schedule) : "By hand"}{p.next_at ? ` · next ${clock(p.next_at)}` : ""}</div>
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
