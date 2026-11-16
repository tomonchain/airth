import { useEffect, useMemo, useState } from "react"
import { Bot, BookOpen, Building2, Check, ChevronDown, ChevronRight, GitBranch, GraduationCap, Hand, Loader, Pause, Pencil, Play, Plus, Repeat, Search, Sparkles, Workflow, X, Zap, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { Rich } from "@/components/rich"
import { RoutineCanvas } from "@/components/routine-canvas"
import type { Declared } from "@/components/routine-editor"
import { Empty, Loading, Page, Panel, Section, Seg } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Feedback, type Routine } from "@/lib/api"
import { FOLD_ABOVE, businessOrder, filter, graphTrigger, groups, needsLook, schedule, state, triggerText, type By, type Filter, type Row, type State, type Waiting } from "@/lib/routine-list"
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

const STATES: Record<State, { label: string; dot: string }> = {
  ok: { label: "Running well", dot: "bg-emerald-500" },
  waiting: { label: "Waits for you", dot: "bg-amber-500" },
  incident: { label: "Incident", dot: "bg-red-500" },
  paused: { label: "Paused", dot: "bg-muted-foreground/50" },
}

// The autonomy table (CLAUDE.md), for a routine's level.
const LEVELS = ["Prepare: you take the action", "Propose: you approve it", "Act with delay: you may cancel", "Act: it reports after"]

const clock = (iso: string) => new Date(iso).toLocaleString("en-GB", { weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })

/** When a routine next runs, as short as it can be said. */
function next(iso: string | null): string {
  if (!iso) return "—"
  const d = new Date(iso), now = new Date()
  const days = Math.round((new Date(d.toDateString()).getTime() - new Date(now.toDateString()).getTime()) / 86400000)
  const time = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })
  if (days === 0) return `Today ${time}`
  if (days === 1) return `Tomorrow ${time}`
  if (days > 1 && days < 7) return `${d.toLocaleDateString("en-GB", { weekday: "short" })} ${time}`
  return `${d.toLocaleDateString("en-GB", { day: "numeric", month: "short" })} ${time}`
}

// Folded groups, and how the list is grouped, are kept on this device.
function stored<T>(key: string, fallback: T): T {
  try { const v = localStorage.getItem(key); return v ? (JSON.parse(v) as T) : fallback } catch { return fallback }
}
function store(key: string, v: unknown) {
  try { localStorage.setItem(key, JSON.stringify(v)) } catch { /* private window */ }
}

// A routine open in the panel; `business` when it is another business's.
type Open = ({ kind: Row["kind"]; name: string; edit?: boolean } | { kind: "new-graph" }) & { business?: string }

type Graphs = { routines: Routine[]; actions: Declared[]; prompts: string[] }
type Levels = Record<string, Record<string, number> | null>

/** One list of rows from a business's three kinds of routine. */
function toRows(business: string, routines: Routine[], objectives: Objective[], pipelines: Pipeline[], levels: Levels | null): Row[] {
  const out: Row[] = []
  for (const r of routines) {
    const g = r.graph
    const acts = (g?.blocks ?? []).filter((b) => b.type === "act" && typeof b.action === "string").map((b) => b.action as string)
    const its = g ? levels?.[g.agent] : undefined
    out.push({
      kind: "graph", name: r.name, agent: g?.agent ?? null, trigger: graphTrigger(g?.trigger), paused: false, broken: !g, next_at: r.next_at,
      last: r.runs[0] ? { at: r.runs[0].at, ok: r.runs[0].ok } : null,
      // An action the business does not declare commits, and starts at 1 (decision 96).
      level: its && acts.length ? Math.min(...acts.map((a) => its[a] ?? 1)) : null, business,
    })
  }
  for (const o of objectives) {
    out.push({
      kind: "agent", name: o.id, agent: o.bot, trigger: o.every ? { kind: "schedule", every: o.every } : { kind: "manual" }, paused: !o.enabled, broken: false,
      next_at: o.enabled ? o.next_at : null,
      last: o.last_status && o.last_at ? { at: o.last_at, ok: o.last_status === "completed" || o.last_status === "running", run: o.last_run } : null, level: null, business,
    })
  }
  for (const p of pipelines) {
    out.push({
      kind: "sequence", name: p.name, agent: null, trigger: p.schedule ? { kind: "schedule", every: p.schedule } : { kind: "manual" }, paused: false,
      broken: !!p.error, next_at: p.next_at ?? null, last: p.runs[0] ? { at: p.runs[0].started_at, ok: p.runs[0].ok } : null, level: null, business,
    })
  }
  return out
}

/** A drawn routine's agents' levels for their actions; one not read is
    left unknown, so its level is not said rather than guessed. */
async function readLevels(realm: string, routines: Routine[]): Promise<Levels> {
  const owners = [...new Set(routines.map((r) => r.graph?.agent).filter((a): a is string => !!a))].sort()
  return Object.fromEntries(await Promise.all(owners.map(async (a) => {
    const l = await api<{ actions: { action: string; level: number }[] }>(`realms/${enc(realm)}/agents/${enc(a)}/levels`).catch(() => null)
    return [a, l && Object.fromEntries(l.actions.map((x) => [x.action, x.level]))]
  })))
}

/** Another business's routines, read from that business alone, with what
    its panel needs: its agents, its editors' actions and prompts, its
    items in Needs you. */
type Away = { realm: string; graphs: Graphs; tasks: Tasks; pipelines: Pipeline[]; agents: Agent[]; items: Waiting[]; rows: Row[] }

async function readBusiness(realm: string): Promise<Away> {
  const r = enc(realm)
  const [graphs, tasks, fixed, team, queue] = await Promise.all([
    api<Graphs>(`realms/${r}/graphs`),
    api<Tasks>(`realms/${r}/routines`),
    api<{ pipelines: Pipeline[] }>(`realms/${r}/business`),
    api<{ agents: Agent[] }>(`realms/${r}/team`),
    api<{ items: Waiting[] }>(`realms/${r}/queue`),
  ])
  const levels = await readLevels(realm, graphs.routines)
  return {
    realm, graphs, tasks, pipelines: fixed.pipelines, agents: team.agents,
    items: queue.items.filter((i) => i.kind !== "idea").map((i) => ({ ...i, business: realm })),
    rows: toRows(realm, graphs.routines, tasks.routines, fixed.pipelines, levels),
  }
}

/** The business's recurring work, in three kinds: routines made of blocks
    (decision 49), drawn on a canvas (decision 91); agents given a task on a
    schedule (decision 37); and the fixed sequences of business/pipelines/
    (decision 30). One list: what needs a look first, then the rest grouped;
    a routine opens in a panel on the right. */
export function Routines({ realm, agents, items: here, businesses, onAgent }: {
  realm: string; agents: Agent[]; items: Waiting[]; businesses: { name: string; title: string }[]; onAgent: (name: string) => void
}) {
  const graphs = usePoll<Graphs>(() => api(`realms/${enc(realm)}/graphs`), 15000, [realm])
  const tasks = usePoll<Tasks>(() => api(`realms/${enc(realm)}/routines`), 15000, [realm])
  const fixed = usePoll<{ pipelines: Pipeline[] }>(() => api(`realms/${enc(realm)}/business`), 30000, [realm])

  // A drawn routine's level: the lowest of its agent's levels for its acts.
  const owners = [...new Set((graphs.data?.routines ?? []).map((r) => r.graph?.agent).filter((a): a is string => !!a))].sort()
  const levels = usePoll<Levels>(owners.length && graphs.data ? () => readLevels(realm, graphs.data!.routines) : null, 60000, [realm, owners.join(",")])

  const rows = useMemo<Row[]>(() => toRows(realm, graphs.data?.routines ?? [], tasks.data?.routines ?? [], fixed.data?.pipelines ?? [], levels.data),
    [realm, graphs.data, tasks.data, fixed.data, levels.data])

  const [f, setF] = useState<Filter>({ search: "", show: "all", agent: null })
  const [by, setBy] = useState<By>(() => stored("airth-routines-by", "agent"))
  useEffect(() => store("airth-routines-by", by), [by])

  // Grouped by business, every business's routines are listed, as the HQ
  // shows them; each is read from its own business, and only while this
  // grouping is chosen. An item counts only for its own business's routines.
  const order = businessOrder(realm, businesses)
  const others = order.filter((b) => b.name !== realm)
  const grouping = by === "business" && businesses.length > 1 ? "business" : by === "business" ? "agent" : by
  const away = usePoll<Away[]>(grouping === "business"
    ? async () => (await Promise.all(others.map((b) => readBusiness(b.name).catch(() => null)))).filter((x): x is Away => !!x)
    : null, 30000, [realm, grouping, others.map((b) => b.name).join(",")])
  const elsewhere = useMemo(() => (grouping === "business" ? away.data ?? [] : []), [grouping, away.data])
  const items = useMemo(() => [...here.map((i) => ({ ...i, business: realm })), ...elsewhere.flatMap((b) => b.items)], [here, realm, elsewhere])
  const all = useMemo(() => [...rows, ...elsewhere.flatMap((b) => b.rows)], [rows, elsewhere])
  const [folded, setFolded] = useState<Record<string, boolean>>(() => stored("airth-routines-folded", {}))
  const fold = (key: string, v: boolean) => setFolded((x) => { const y = { ...x, [key]: v }; store("airth-routines-folded", y); return y })
  const [open, setOpen] = useState<Open | null>(null)
  const [newTask, setNewTask] = useState(false)

  const loaded = !!(graphs.data && tasks.data && fixed.data)
  const shown = filter(all, f, items)
  const look = needsLook(shown, items)
  const grouped = groups(shown, grouping, agents.map((a) => ({ name: a.name, title: a.title })), order)
  const owned = agents.filter((a) => rows.some((r) => r.agent === a.name))
  const face = (name: string | null, business = realm) => (business === realm ? agents : elsewhere.find((b) => b.realm === business)?.agents ?? []).find((a) => a.name === name)
  const opening = (r: Row) => setOpen({ kind: r.kind, name: r.name, business: r.business })
  const there = open?.business && open.business !== realm ? elsewhere.find((b) => b.realm === open.business) : undefined

  return (
    <Page wide title="Routines" lede={loaded ? `${rows.length} routine${rows.length === 1 ? "" : "s"}, the agents' recurring work.` : undefined}
      actions={<div className="flex gap-2">
        <Button size="sm" disabled={!graphs.data || !agents.length} onClick={() => setOpen({ kind: "new-graph" })} title={agents.length ? undefined : "Hire an agent first"}><Workflow /> New routine</Button>
        <Button size="sm" variant="outline" onClick={() => setNewTask(true)}><Plus /> New agent routine</Button>
      </div>}>
      {!loaded ? <Loading /> : !rows.length ? (
        <Empty icon={Repeat} title="No routine yet">Draw one: a trigger, then blocks you drag in and link. Or give an agent a task to do on a schedule.</Empty>
      ) : (
        <>
          <Toolbar f={f} setF={setF} by={grouping} setBy={setBy} agents={owned} several={businesses.length > 1} />
          {f.show !== "paused" && look.length > 0 && (
            <Section title="Needs a look" note={`${look.length} routine${look.length === 1 ? "" : "s"}`}>
              <Panel className="divide-y divide-border/60 overflow-hidden">
                {look.map((r) => <RoutineRow key={`${r.business}:${r.kind}:${r.name}`} r={r} items={items} face={face} why onOpen={() => opening(r)}
                  where={grouping === "business" ? order.find((b) => b.name === r.business)?.title : undefined} />)}
              </Panel>
            </Section>
          )}
          {f.show === "look" ? (!look.length && <p className="text-sm text-muted-foreground">{f.search || f.agent ? "No routine matches." : "Nothing needs a look."}</p>)
            : !shown.length ? <p className="text-sm text-muted-foreground">No routine matches.</p> : (
            <section className="space-y-3">
              {grouping === "business" && !away.data && <p className="text-xs text-muted-foreground">Reading the other businesses' routines…</p>}
              <Columns />
              {grouped.map((g) => {
                const key = `${realm}:${grouping}:${g.key}`
                const closed = folded[key] ?? g.rows.length > FOLD_ABOVE
                const a = grouping === "agent" ? face(g.key) : undefined
                const lead = a ? <Character agent={a} size={20} still /> : grouping === "agent" ? <Bot className="size-5 text-muted-foreground" /> : grouping === "business" ? <Building2 className="size-4 text-muted-foreground" /> : null
                return (
                  <Panel key={g.key} className="overflow-hidden">
                    <button onClick={() => fold(key, !closed)} aria-expanded={!closed}
                      className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left hover:bg-accent/40">
                      <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition-transform", closed && "-rotate-90")} />
                      {lead}
                      <span className="min-w-0 truncate text-sm font-semibold">{g.label}</span>
                      <span className="rounded-full bg-muted px-1.5 text-[11px] font-medium text-muted-foreground tabular-nums">{g.rows.length}</span>
                      <span className="ml-auto shrink-0 text-xs text-muted-foreground">{g.next_at ? <>Next <span className="tabular-nums">{next(g.next_at)}</span></> : "Nothing scheduled"}</span>
                    </button>
                    {!closed && (
                      <div className="divide-y divide-border/60 border-t border-border/60">
                        {g.rows.map((r) => <RoutineRow key={`${r.business}:${r.kind}:${r.name}`} r={r} items={items} face={face} onOpen={() => opening(r)} />)}
                      </div>
                    )}
                  </Panel>
                )
              })}
            </section>
          )}
        </>
      )}
      {there ? (
        // Another business's routine: its own data, saved to it, and its
        // agents opened in it.
        <RoutineSheet realm={there.realm} open={open} setOpen={(o) => setOpen(o && { ...o, business: there.realm })} agents={there.agents} items={there.items} rows={there.rows}
          onAgent={(name) => { location.hash = `#/b/${enc(there.realm)}/agents/${enc(name)}` }}
          graphs={{ data: there.graphs, refresh: away.refresh }} tasks={{ data: there.tasks, refresh: away.refresh }} fixed={there.pipelines} />
      ) : (
        <RoutineSheet realm={realm} open={open} setOpen={setOpen} agents={agents} items={items} rows={rows} onAgent={onAgent}
          graphs={graphs} tasks={tasks} fixed={fixed.data?.pipelines ?? []} />
      )}
      {tasks.data && (
        <Dialog open={newTask} onOpenChange={setNewTask}>
          <DialogContent className="max-h-[90svh] overflow-y-auto sm:max-w-xl">
            <DialogHeader>
              <DialogTitle>New agent routine</DialogTitle>
              <DialogDescription>{TASK_HINT}</DialogDescription>
            </DialogHeader>
            <TaskEditor realm={realm} d={tasks.data} start={null} onCancel={() => setNewTask(false)} onSaved={tasks.refresh} />
          </DialogContent>
        </Dialog>
      )}
    </Page>
  )
}

function Toolbar({ f, setF, by, setBy, agents, several }: {
  f: Filter; setF: (f: Filter) => void; by: By; setBy: (b: By) => void; agents: Agent[]; several: boolean
}) {
  const ANY = "-any"
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-40 flex-1 sm:max-w-64">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input value={f.search} onChange={(e) => setF({ ...f, search: e.target.value })} placeholder="Search routines" aria-label="Search routines" className="h-8 pl-8" />
      </div>
      <Seg label="Show" value={f.show} onPick={(show) => setF({ ...f, show })} options={[["all", "All"], ["look", "Needs a look"], ["paused", "Paused"]]} />
      {agents.length > 1 && (
        <Select value={f.agent ?? ANY} onValueChange={(v) => setF({ ...f, agent: v === ANY || !v ? null : String(v) })}>
          <SelectTrigger size="sm" className="h-8 w-40" aria-label="Agent"><SelectValue>{(v: string) => (v === ANY ? "Any agent" : agents.find((a) => a.name === v)?.title ?? v)}</SelectValue></SelectTrigger>
          <SelectContent>
            <SelectItem value={ANY}>Any agent</SelectItem>
            {agents.map((a) => <SelectItem key={a.name} value={a.name}>{a.title}</SelectItem>)}
          </SelectContent>
        </Select>
      )}
      <div className="flex items-center gap-1.5 sm:ml-auto">
        <span className="text-xs text-muted-foreground">Group by</span>
        <Seg label="Group by" value={by} onPick={setBy} options={[["agent", "Agent"], ["frequency", "Frequency"], ...(several ? [["business", "Business"] as [By, string]] : [])]} />
      </div>
    </div>
  )
}

// The row's columns, on a computer; on a phone the row is two lines.
const COLS = "md:grid-cols-[0.625rem_minmax(0,1.2fr)_minmax(0,1.4fr)_7.5rem_7.5rem_2.25rem]"

function Columns() {
  return (
    <div className={cn("hidden gap-x-3 px-3 text-[11px] font-medium text-muted-foreground md:grid", COLS)}>
      <span /><span>Routine</span><span>Trigger</span><span>Next run</span><span>Last run</span><span className="text-right">Level</span>
    </div>
  )
}

function Dot({ s }: { s: State }) {
  return <span className={cn("size-2 shrink-0 rounded-full", STATES[s].dot)} title={STATES[s].label} aria-label={STATES[s].label} />
}

function LastRun({ last }: { last: Row["last"] }) {
  if (!last) return <span className="text-muted-foreground">Never ran</span>
  return (
    <span className={cn("inline-flex items-center gap-1", last.ok ? "text-muted-foreground" : "text-red-500")}>
      {last.ok ? <Check className="size-3.5 text-emerald-500" /> : <X className="size-3.5" />} {last.ok ? ago(last.at) : `Failed ${ago(last.at)}`}
    </span>
  )
}

function Level({ n, kind }: { n: number | null; kind: Row["kind"] }) {
  if (n === null) {
    const why = kind === "graph" ? "Not known: it takes no action, or its agent's levels could not be read." : "Agent routines and fixed sequences follow the business's permissions."
    return <span className="text-muted-foreground/60" title={why}>—</span>
  }
  return <span className="rounded-md border px-1 py-px font-mono text-[11px] tabular-nums" title={`Level ${n} · ${LEVELS[n]}`}>L{n}</span>
}

/** Why a routine needs a look, in a few words. */
function why(r: Row, items: Waiting[]): string {
  if (r.broken) return "Its file does not hold"
  if (r.last && !r.last.ok) return "Its last run failed"
  return state(r, items) === "incident" ? "An incident waits in Needs you" : "Waits for you in Needs you"
}

function RoutineRow({ r, items, face, why: say, where, onOpen }: {
  r: Row; items: Waiting[]; face: (n: string | null, business?: string) => Agent | undefined; why?: boolean; where?: string; onOpen: () => void
}) {
  const s = state(r, items)
  const a = face(r.agent, r.business)
  return (
    <button onClick={onOpen} className={cn("grid w-full grid-cols-[0.625rem_minmax(0,1fr)_auto] items-center gap-x-3 px-3 py-2.5 text-left transition-colors hover:bg-accent/50", COLS)}>
      <Dot s={s} />
      <span className="min-w-0">
        <span className="flex items-center gap-1.5">
          {say && a && <Character agent={a} size={16} still />}
          <span className={cn("truncate font-mono text-[13px]", r.paused && "text-muted-foreground")}>{r.name}</span>
        </span>
        <span className="block truncate text-xs text-muted-foreground md:hidden">{triggerText(r.trigger)}{r.next_at ? ` · ${next(r.next_at)}` : ""}</span>
        {say && <span className={cn("block truncate text-xs", s === "incident" ? "text-red-500" : "text-amber-600 dark:text-amber-400")}>{why(r, items)}{where && <span className="text-muted-foreground"> · {where}</span>}</span>}
      </span>
      <span className="hidden truncate text-xs text-muted-foreground md:block">{triggerText(r.trigger)}</span>
      <span className="hidden text-xs text-muted-foreground tabular-nums md:block">{r.paused ? "Paused" : next(r.next_at)}</span>
      <span className="text-xs max-md:text-right"><LastRun last={r.last} /></span>
      <span className="hidden text-right md:block"><Level n={r.level} kind={r.kind} /></span>
    </button>
  )
}

/** A routine open on the right: what it is and how it went, and its editor,
    without leaving the list. A drawn routine's canvas takes the panel wide. */
function RoutineSheet({ realm, open, setOpen, agents, items, rows, onAgent, graphs, tasks, fixed }: {
  realm: string; open: Open | null; setOpen: (o: Open | null) => void; agents: Agent[]; items: Waiting[]; rows: Row[]; onAgent: (n: string) => void
  graphs: { data: { routines: Routine[]; actions: Declared[]; prompts: string[] } | null; refresh: () => Promise<void> | void }
  tasks: { data: Tasks | null; refresh: () => Promise<void> | void }
  fixed: Pipeline[]
}) {
  const row = open && open.kind !== "new-graph" ? rows.find((r) => r.kind === open.kind && r.name === open.name) : undefined
  const graph = open?.kind === "graph" ? graphs.data?.routines.find((r) => r.name === open.name) : undefined
  const task = open?.kind === "agent" ? tasks.data?.routines.find((o) => o.id === open.name) : undefined
  const pipe = open?.kind === "sequence" ? fixed.find((p) => p.name === open.name) : undefined
  const drawing = open?.kind === "new-graph" || (open?.kind === "graph" && !!open.edit)
  const a = agents.find((x) => x.name === row?.agent)
  const s = row ? state(row, items) : "ok"
  return (
    // While a routine is drawn, only the canvas's own back arrow leaves it:
    // a click beside it or Escape would lose what is not saved.
    <Sheet open={!!open} onOpenChange={(o) => !o && !drawing && setOpen(null)}>
      <SheetContent showCloseButton={!drawing}
        className={cn("gap-0 p-0 data-[side=right]:w-full", drawing ? "data-[side=right]:sm:max-w-[min(80rem,96vw)]" : "data-[side=right]:sm:max-w-xl")}>
        {drawing && graphs.data ? (
          <div className="flex h-full min-h-0 flex-col p-3">
            <SheetTitle className="sr-only">{graph ? `Edit ${graph.name}` : "New routine"}</SheetTitle>
            <RoutineCanvas key={graph?.name ?? "new"} realm={realm} routine={graph ?? null} agents={agents}
              actions={graphs.data.actions} prompts={graphs.data.prompts}
              onClose={() => setOpen(graph ? { kind: "graph", name: graph.name } : null)}
              onSaved={async (name) => { await graphs.refresh(); setOpen({ kind: "graph", name, edit: true }) }} />
          </div>
        ) : row ? (
          <div className="flex h-full min-h-0 flex-col">
            <SheetHeader className="border-b pr-12">
              <SheetTitle className="flex items-center gap-2 font-mono text-[15px]"><Dot s={s} /> {row.name}</SheetTitle>
              <SheetDescription>{row.kind === "sequence" ? "Fixed sequence" : a?.title ?? "No agent"} · {triggerText(row.trigger)}</SheetDescription>
            </SheetHeader>
            <div className="min-h-0 flex-1 space-y-5 overflow-y-auto p-4">
              <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-sm sm:grid-cols-4">
                <Fact label="State"><span className="inline-flex items-center gap-1.5"><Dot s={s} />{STATES[s].label}</span></Fact>
                <Fact label="Next run">{row.paused ? "Paused" : next(row.next_at)}</Fact>
                <Fact label="Last run"><LastRun last={row.last} /></Fact>
                <Fact label="Level"><Level n={row.level} kind={row.kind} /></Fact>
              </dl>
              {(s === "incident" || s === "waiting") && <p className={cn("rounded-lg px-3 py-2 text-xs", s === "incident" ? "bg-red-500/10 text-red-600 dark:text-red-300" : "bg-amber-500/10 text-amber-700 dark:text-amber-300")}>{why(row, items)}.</p>}
              {graph && <GraphDetail realm={realm} r={graph} agent={a} onAgent={onAgent} onEdit={() => setOpen({ kind: "graph", name: graph.name, edit: true })} onChanged={graphs.refresh} />}
              {task && tasks.data && (open?.kind === "agent" && open.edit
                ? <TaskEditor key={task.id} realm={realm} d={tasks.data} start={task} onCancel={() => setOpen({ kind: "agent", name: task.id })} onSaved={tasks.refresh} />
                : <TaskDetail realm={realm} o={task} onAgent={onAgent} onEdit={() => setOpen({ kind: "agent", name: task.id, edit: true })} onChanged={tasks.refresh} onRemoved={() => setOpen(null)} />)}
              {pipe && <PipelineDetail realm={realm} p={pipe} />}
            </div>
          </div>
        ) : (
          <div className="p-6"><SheetTitle className="sr-only">Routine</SheetTitle><Loading /></div>
        )}
      </SheetContent>
    </Sheet>
  )
}

/** How late a scheduled run started, past the 15 minutes of grace. */
function late(run?: { at: string; due_at?: string | null }): string | null {
  if (!run?.due_at) return null
  const min = Math.round((Date.parse(run.at) - Date.parse(run.due_at)) / 60000)
  if (min <= 15) return null
  return min < 120 ? `${min} min` : `${Math.round(min / 60)} h`
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-[11px] text-muted-foreground">{label}</dt>
      <dd className="mt-0.5 truncate text-[13px]">{children}</dd>
    </div>
  )
}

/** A drawn routine: who, its chain of blocks, what it learned; Edit opens the canvas. */
function GraphDetail({ realm, r, agent, onAgent, onEdit, onChanged }: { realm: string; r: Routine; agent?: Agent; onAgent: (name: string) => void; onEdit: () => void; onChanged: () => void }) {
  const g = r.graph
  const last = r.runs[0]
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={onEdit}><Workflow /> Edit</Button>
        {agent && <Button size="sm" variant="outline" onClick={() => onAgent(agent.name)}><Character agent={agent} size={16} still /> {agent.title}</Button>}
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">{usd(r.cost_usd) || "$0"} this week{r.unpriced ? ` + ${r.unpriced} unpriced` : ""}</span>
      </div>
      {g?.description && <p className="text-sm text-foreground/85">{g.description}</p>}
      {g && (
        <div className="flex flex-wrap items-center gap-1">
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
      {!g && r.error && <p className="font-mono text-[11px] break-words text-red-500">{r.error}</p>}
      {last?.error && <p className="line-clamp-3 font-mono text-[11px] text-red-500">{last.error}</p>}
      {late(last) && <p className="text-xs text-amber-600 dark:text-amber-400">Its last run started {late(last)} late.</p>}
      {r.runs.length > 0 && (
        <div className="space-y-1">
          <h3 className="text-xs font-semibold">Last runs</h3>
          {r.runs.slice(0, 5).map((x) => (
            <div key={x.at} className="flex items-center justify-between gap-3 text-xs">
              <span className="text-muted-foreground">{clock(x.at)}</span>
              {x.ok ? <span className="text-emerald-500">Ran</span> : <span className="truncate text-red-500" title={x.error ?? ""}>{x.error ?? "Failed"}</span>}
            </div>
          ))}
        </div>
      )}
      {!!r.feedback?.length && <Learned realm={realm} said={r.feedback} onChanged={onChanged} />}
    </>
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
    <div className="border-t pt-3">
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

type Objective = {
  id: string; objective: string; every: string | null; bot: string | null; tools: string[] | null; allow: string[]; delegates: string[]
  max_minutes: number | null; wait: number | null; enabled: boolean; running: boolean; next_at: string | null; last_run: string | null; last_at: string | null; last_status: string | null
}
type Tasks = { routines: Objective[]; agents: string[]; extensions: string[] }

/** An agent routine: its task, what it may do, and run, pause, edit, remove. */
function TaskDetail({ realm, o, onAgent, onEdit, onChanged, onRemoved }: {
  realm: string; o: Objective; onAgent: (name: string) => void; onEdit: () => void; onChanged: () => void; onRemoved: () => void
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
    <>
      <div className="flex flex-wrap items-center gap-1.5">
        <Button size="sm" disabled={busy || o.running} onClick={run}>{o.running ? <><Loader className="animate-spin" /> Running…</> : <><Play /> Run now</>}</Button>
        <Button size="sm" variant="outline" onClick={pause}>{o.enabled ? <><Pause /> Pause</> : <><Play /> Resume</>}</Button>
        <Button size="sm" variant="outline" onClick={onEdit}><Pencil /> Edit</Button>
        <span className="flex-1" />
        <Remove label={o.id} onRemove={async () => {
          try { await api(path, undefined, "DELETE"); toast.success(`${o.id} removed.`); onRemoved(); onChanged() }
          catch (e) { toast.error((e as Error).message) }
        }} />
      </div>
      <p className="text-sm text-foreground/85">{o.objective}</p>
      <p className="text-xs text-muted-foreground">{what}</p>
      {o.last_status && o.last_status !== "completed" && <p className="text-xs text-muted-foreground">Last: {o.last_status}</p>}
    </>
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
const TASK_HINT = "Each time, the agent works on the task as on one you give it: it may do what the business allows, plus what you allow here. Anything else waits for you."

/** Create or change an agent routine (decision 37): who, what, when, and
    what it may do without asking. In a dialog for a new one, in the panel
    for one that exists. */
function TaskEditor({ realm, d, start, onCancel, onSaved }: { realm: string; d: Tasks; start: Objective | null; onCancel: () => void; onSaved: () => void }) {
  const o = start
  const [f, setF] = useState({
    id: o?.id ?? "", bot: o?.bot ?? NOBODY, objective: o?.objective ?? "", every: o?.every ?? "", only: !!o?.tools, tools: o?.tools ?? [],
    allow: o?.allow ?? [], delegates: o?.delegates ?? [], minutes: o?.max_minutes ? String(o.max_minutes) : "",
  })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Each HTTP integration may be allowed on its own (MCP servers and skills
  // go through the tools list above).
  const detail = usePoll<{ integrations: { name: string }[] }>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  const integrations = (detail.data?.integrations ?? []).filter((i) => d.extensions.includes(i.name)).map((i) => [`network:${i.name}`, `Call ${i.name}`] as [string, string])
  const choices = [...BASE, ...integrations]
  const others = d.routines.filter((x) => x.id !== o?.id)
  const valid = /^[a-z0-9][a-z0-9_-]*$/.test(f.id) && f.objective.trim().length > 0

  async function save() {
    setBusy(true)
    setError(null)
    try {
      await api(`realms/${enc(realm)}/routines`, {
        id: f.id.trim(), objective: f.objective, bot: f.bot === NOBODY ? null : f.bot, every: f.every.trim() || null,
        tools: f.only ? f.tools : null, allow: f.allow, delegates: f.delegates, max_minutes: Number(f.minutes) || null,
        wait: o?.wait ?? 10,
      })
      toast.success(o ? `${f.id} saved.` : `${f.id} created.`)
      onSaved(); onCancel()
    } catch (e) { setError((e as Error).message) }
    finally { setBusy(false) }
  }

  return (
    <div className="space-y-4">
      {o && <p className="text-xs text-muted-foreground">{TASK_HINT}</p>}
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="r-id">Name</Label>
          <Input id="r-id" value={f.id} disabled={!!o} autoFocus={!o} spellCheck={false} autoComplete="off" placeholder="weekly-article" className="font-mono"
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
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>Cancel</Button>
        <Button disabled={!valid || busy} onClick={save}>{busy ? "Saving…" : o ? "Save" : "Create"}</Button>
      </div>
    </div>
  )
}

type Pipeline = {
  name: string; description?: string; schedule?: string | null; steps?: string[]; next_at?: string | null; error?: string
  runs: { started_at: string; ok: boolean; items_read: number; items_kept: number; notes: string; report: string | null; error: string | null }[]
}

/** A fixed sequence: its steps, its last runs and report; it runs by hand too. */
function PipelineDetail({ realm, p }: { realm: string; p: Pipeline }) {
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
    <>
      <div><Button size="sm" disabled={!!p.error || busy} onClick={run}><Play /> Run now</Button></div>
      {p.error ? <p className="font-mono text-[11px] break-words text-red-500">{p.error}</p> : (
        <>
          {p.description && <p className="text-sm text-foreground/85">{p.description}</p>}
          {p.steps && p.steps.length > 0 && <ol className="list-decimal space-y-0.5 pl-5 text-xs text-muted-foreground">{p.steps.map((s, i) => <li key={i}>{s}</li>)}</ol>}
        </>
      )}
      {p.runs.length > 0 && (
        <div className="space-y-1 border-t pt-3">
          <h3 className="text-xs font-semibold">Last runs</h3>
          {p.runs.slice(0, 5).map((r) => (
            <div key={r.started_at} className="flex items-center justify-between gap-3 text-xs">
              <span className="text-muted-foreground">{clock(r.started_at)}</span>
              {r.ok ? <span>{r.items_kept} kept of {r.items_read}</span> : <span className="truncate text-red-500" title={r.error ?? ""}>{r.error ?? "Failed"}</span>}
            </div>
          ))}
        </div>
      )}
      {last?.ok && last.notes && <p className="line-clamp-2 text-xs text-muted-foreground">{last.notes}</p>}
      {last?.report && (
        <div>
          <button className="inline-flex items-center gap-1 text-xs text-sky-500 hover:underline" onClick={() => setReport(!report)}><ChevronRight className={cn("size-3 transition-transform", report && "rotate-90")} />{report ? "Hide the last report" : "The last report"}</button>
          {report && <div className="mt-2 max-h-80 overflow-auto rounded-lg bg-surface p-3 text-sm"><Rich text={last.report} /></div>}
        </div>
      )}
    </>
  )
}
