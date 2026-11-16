import { useEffect, useMemo, useRef, useState } from "react"
import { Archive, ArchiveRestore, ArrowUp, Check, ChevronDown, Circle, CircleDot, Ellipsis, FolderKanban, Pencil, Plus, SkipForward, Trash2, X } from "lucide-react"
import { Back } from "@/components/page"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { MicButton } from "@/components/mic"
import { useRun, TaskThread, type Passed } from "@/components/task-thread"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { usePoll } from "@/hooks/use-poll"
import { api, called, enc, type Agent, type Item, type Project, type Tasks as TasksData } from "@/lib/api"
import { glance, ProjectView } from "@/screens/project"
import { ago, stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

type Run = {
  id: string; title: string | null; archived: boolean; objective: string; status: string; live: boolean; updated_at: string
  needs_approval: boolean; change: string | null; plan_done: number; plan_total: number; routine: string | null; asked_by: string | null
}
type Route = { agent?: string; why?: string; order: string; agents: string[]; error?: string }

// A task done by no agent of the team is shown as Airth's own.
const AIRTH = { name: "airth", title: "Airth", department: null, state: "idle" } as unknown as Agent
const NOBODY = "-"
const AUTO = "auto"
const name = (r: Run) => r.title || r.objective.split("\n")[0] || r.id

function group(r: Run, waiting: Set<string>) {
  if (r.needs_approval || waiting.has(r.id)) return "Waiting for you"
  if (r.live) return "Running"
  if (r.change === "open") return "To review"
  return "Done"
}
const ORDER = ["Waiting for you", "Running", "To review", "Done"]

/** Projects (decision 128): every task of the business, yours and those
    its routines started; a task that passed work on to the team is a
    project, its tasks under it. Ask the team for something; follow, rename,
    archive or delete a task; read its plan and details. */
export function Tasks({ realm, agents, items, open, onOpen, onAgent, prefill, tasks: shared, onTasks, startProject }: {
  realm: string; agents: Agent[]; items: Item[]; open: string | null; onOpen: (id: string | null) => void; onAgent: (name: string) => void
  tasks: TasksData | null; onTasks: () => void
  prefill?: { text: string; at: number } | null
  /** A project opened from another page. */
  startProject?: { id: number; at: number } | null
}) {
  const runs = usePoll<Run[]>(() => api(`realms/${enc(realm)}/runs`), 5000, [realm])
  // The app's list, refreshed every few seconds: not asked again here.
  const tasks = { data: shared, refresh: onTasks }
  const [archived, setArchived] = useState(false)
  // Two levels, never mixed: the projects, and every task.
  const [view, setView] = useState<"projects" | "tasks">("projects")
  // The projects (decision 129): the open ones first in the list, the done
  // ones with what is archived.
  const projects = usePoll<{ projects: Project[] }>(() => api(`realms/${enc(realm)}/projects`), 5000, [realm])
  const [shown, setShown] = useState<number | null>(null)
  useEffect(() => setShown(null), [realm])
  useEffect(() => { if (startProject) { setShown(startProject.id); onOpen(null) } }, [startProject]) // eslint-disable-line react-hooks/exhaustive-deps
  const byRun = useMemo(() => new Map((runs.data ?? []).map((r) => [r.id, r])), [runs.data])
  const all = projects.data?.projects ?? []
  const listed = all.filter((p) => (p.status === "done") === archived)
  const project = all.find((p) => p.id === shown) ?? null
  const agentOf = useMemo(() => {
    const m = new Map<string, string>()
    for (const g of Object.values(tasks.data ?? {})) for (const t of g) if (t.agent) m.set(t.id, t.agent)
    return m
  }, [tasks.data])
  const waiting = new Set((tasks.data?.waiting ?? []).map((t) => t.id))
  // Each task shows once, in the Tasks list, with its project's name.
  const projectOf = useMemo(() => {
    const m = new Map<string, string>()
    for (const p of all) for (const r of [p.run, ...p.tasks.map((t) => t.run)]) if (r) m.set(r, p.name)
    return m
  }, [all])
  const list = (runs.data ?? []).filter((r) => r.archived === archived)
  const live = (runs.data ?? []).filter((r) => !r.archived).length
  // A plan's tasks (decision 120) sit under the task that passed them on,
  // which is grouped by the most pressing of them all.
  const fromOf = useMemo(() => {
    const m = new Map<string, string>()
    for (const g of Object.values(tasks.data ?? {})) for (const t of g) if (t.from?.task) m.set(t.id, t.from.task)
    return m
  }, [tasks.data])
  const kids = (id: string) => list.filter((r) => fromOf.get(r.id) === id)
  const top = list.filter((r) => { const p = fromOf.get(r.id); return !p || !list.some((x) => x.id === p) })
  // A plan's whole tree, however deep, each task with its depth under the
  // task it hangs from; `seen` keeps a loop from running forever.
  const tree = (r: Run, depth = 0, seen = new Set<string>()): { r: Run; depth: number }[] =>
    seen.has(r.id) ? [] : (seen.add(r.id), [{ r, depth }, ...kids(r.id).flatMap((k) => tree(k, depth + 1, seen))])
  const planGroup = (r: Run) => ORDER[Math.min(...tree(r).map((x) => ORDER.indexOf(group(x.r, waiting))))]
  const groups = ORDER.map((g) => [g, top.filter((r) => planGroup(r) === g)] as const).filter(([, l]) => l.length)
  const titleOf = (n: string | null | undefined) => { const a = agents.find((x) => x.name === n); return a ? called(a) : n ?? "An agent" }
  const passedOf = (id: string): Passed[] => kids(id).map((k) => ({ id: k.id, agent: agentOf.get(k.id) ?? "", title: titleOf(agentOf.get(k.id)), order: name(k), status: k.live ? "running" : k.status }))
  const fromTitle = (id: string) => { const p = fromOf.get(id); return p ? titleOf(agentOf.get(p)) : undefined }
  const current = runs.data?.find((r) => r.id === open) ?? null

  // Each project's place: under way, not started yet, or done.
  const stage = (p: Project) => {
    if (p.status === "done") return "Completed"
    const g = glance(p, byRun)
    return g.running || g.done > 0 || g.stuck > 0 ? "In progress" : "Planned"
  }
  const projectGroups = (["In progress", "Planned", "Completed"] as const)
    .map((g) => [g, listed.filter((p) => stage(p) === g)] as const).filter(([, l]) => l.length)
  const [asking, setAsking] = useState(false)
  // Shown by itself while the list is empty, until the founder closes it.
  const [dismissed, setDismissed] = useState(false)
  useEffect(() => { if (prefill) setAsking(true) }, [prefill])
  const [folded, setFolded] = useState<Set<string>>(new Set())
  const fold = (g: string) => setFolded((f) => { const n = new Set(f); if (n.has(g)) n.delete(g); else n.add(g); return n })

  if (current) return (
    <TaskView key={current.id} realm={realm} run={current} agent={agents.find((x) => x.name === agentOf.get(current.id)) ?? AIRTH} items={items} onBack={() => onOpen(null)}
      backLabel={project ? project.name : view === "tasks" ? "Tasks" : "Projects"}
      from={fromTitle(current.id)} passed={passedOf(current.id)} onOpenTask={onOpen}
      onChange={() => { runs.refresh(); tasks.refresh() }} onGone={() => { onOpen(null); runs.refresh(); tasks.refresh() }} onAgent={onAgent} />
  )
  if (project) return (
    <ProjectView key={project.id} realm={realm} project={project} runs={byRun} agents={agents} onBack={() => setShown(null)}
      onTask={onOpen} onChange={() => { projects.refresh(); runs.refresh(); tasks.refresh() }} />
  )
  const empty = view === "projects" ? !listed.length : !groups.length
  const composing = asking || (empty && !archived && !dismissed)
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      {/* Linear's way: the views on top, one dense list under, groups that fold. */}
      <header className="flex h-12 shrink-0 items-center gap-1 border-b pr-3 pl-4 max-md:h-[60px] max-md:pl-[68px]">
        <nav role="tablist" aria-label="Projects or tasks" className="flex items-center gap-0.5">
          {([["projects", "Projects", all.filter((p) => p.status !== "done").length, FolderKanban], ["tasks", "Tasks", live, CircleDot]] as const).map(([v, l, n, Icon]) => (
            <button key={v} role="tab" aria-selected={view === v} onClick={() => { setView(v); setArchived(false) }}
              className={cn("flex h-7 items-center gap-1.5 rounded-md px-2 text-[13px] transition-colors", view === v ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent/50 hover:text-foreground")}>
              <Icon className="size-3.5" />{l}<span className="text-xs text-muted-foreground tabular-nums">{n}</span>
            </button>
          ))}
        </nav>
        <span className="mx-1.5 h-4 w-px bg-border max-sm:hidden" />
        <div className="flex items-center gap-0.5 max-sm:hidden">
          {[[false, view === "projects" ? "Open" : "Current"], [true, view === "projects" ? "Done" : "Archived"]].map(([v, l]) => (
            <button key={String(v)} onClick={() => setArchived(v as boolean)} className={cn("h-7 rounded-md px-2 text-xs", archived === v ? "text-foreground" : "text-muted-foreground hover:text-foreground")}>{l as string}</button>
          ))}
        </div>
        <span className="flex-1" />
        <Button size="sm" variant={composing ? "secondary" : "default"} className="h-7 gap-1 px-2.5 text-[13px]" onClick={() => { if (composing) { setAsking(false); setDismissed(true) } else setAsking(true) }}>
          {composing ? <X /> : <Plus />}<span className="max-sm:sr-only">{composing ? "Close" : "Ask the team"}</span>
        </Button>
      </header>
      {/* Hidden, not unmounted: a closed composer keeps what was typed and
          does not take a request it already took again. */}
      {(
        <div className={cn("shrink-0 border-b bg-muted/30 py-1", !composing && "hidden")}>
          <div className="mx-auto max-w-3xl"><Ask realm={realm} agents={agents} prefill={prefill} onStarted={(id) => { runs.refresh(); tasks.refresh(); setShown(null); setView("tasks"); setAsking(false); onOpen(id) }} /></div>
        </div>
      )}
      {/* On a phone the filter has its own line. */}
      <div className="flex items-center gap-0.5 border-b px-3 py-1.5 sm:hidden">
        {[[false, view === "projects" ? "Open" : "Current"], [true, view === "projects" ? "Done" : "Archived"]].map(([v, l]) => (
          <button key={String(v)} onClick={() => setArchived(v as boolean)} className={cn("h-7 rounded-md px-2.5 text-[13px]", archived === v ? "bg-accent text-foreground" : "text-muted-foreground")}>{l as string}</button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-24 md:pb-4">
        {(view === "projects" ? !projects.data : !runs.data) ? <p className="px-5 py-4 text-sm text-muted-foreground">Loading…</p>
          : empty ? (
            <div className="px-6 py-16 text-center">
              {view === "projects" ? <FolderKanban className="mx-auto size-7 text-muted-foreground/50" /> : <CircleDot className="mx-auto size-7 text-muted-foreground/50" />}
              <p className="mt-3 text-sm font-medium">{view === "projects" ? (archived ? "No project done yet." : "No project yet.") : archived ? "Nothing archived." : "No task yet."}</p>
              {!archived && <p className="mx-auto mt-1 max-w-sm text-sm text-muted-foreground">{view === "projects" ? "Ask the team for something bigger than a task: it is planned as a project, with milestones and tasks." : "Ask the team for something: it goes to the agent that fits."}</p>}
            </div>
          ) : view === "projects" ? projectGroups.map(([g, l]) => (
            <section key={g}>
              <GroupHead label={g} count={l.length} icon={<StatusIcon kind={g === "Completed" ? "done" : g === "Planned" ? "todo" : "running"} />} folded={folded.has(g)} onFold={() => fold(g)} />
              {!folded.has(g) && l.map((p) => {
                const { done: ended, total, next, when } = glance(p, byRun)
                const lead = agents.find((a) => a.name === p.agent)
                const pct = total ? Math.round((100 * ended) / total) : 0
                return (
                  <button key={p.id} onClick={() => { setShown(p.id); onOpen(null) }} className="flex h-11 w-full items-center gap-3 border-b border-border/60 px-4 text-left text-[13px] hover:bg-accent/50 md:px-5">
                    <FolderKanban className="size-4 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 flex-1 truncate font-medium">{p.name}</span>
                    {next && <span className={cn("max-w-56 truncate text-xs text-muted-foreground max-md:hidden", when?.late && "text-red-500")}>{next.title}{when ? ` · ${when.text.split(" · ")[0]}` : ""}</span>}
                    <span className="flex w-16 shrink-0 items-center justify-end gap-1.5 text-xs text-muted-foreground tabular-nums"><Ring share={total ? ended / total : 0} />{pct}%</span>
                    {lead ? <Character agent={lead} size={20} still /> : <span className="size-5 shrink-0" />}
                    <span className="w-12 shrink-0 text-right text-xs text-muted-foreground max-sm:hidden">{stamp(p.updated_at)}</span>
                  </button>
                )
              })}
            </section>
          )) : groups.map(([g, l]) => (
            <section key={g}>
              <GroupHead label={g} count={l.length} icon={<StatusIcon kind={KIND[g]} />} folded={folded.has(g)} onFold={() => fold(g)} />
              {!folded.has(g) && l.flatMap((parent) => tree(parent)).map(({ r, depth }) => {
                const a = agents.find((x) => x.name === agentOf.get(r.id)) ?? AIRTH
                const child = depth > 0
                const team = kids(r.id)
                // Said once, on the task its plan hangs from.
                const inProject = child && projectOf.get(fromOf.get(r.id) ?? "") === projectOf.get(r.id) ? null : projectOf.get(r.id)
                const kind: Kind = r.status === "failed" ? "failed" : r.status === "stopped" ? "stopped" : KIND[group(r, waiting)]
                return (
                  <button key={r.id} onClick={() => { setShown(null); onOpen(r.id) }} title={r.objective}
                    className="flex h-11 w-full items-center gap-3 border-b border-border/60 px-4 text-left text-[13px] hover:bg-accent/50 md:px-5" style={depth ? { paddingLeft: 20 + 24 * depth } : undefined}>
                    <StatusIcon kind={kind} />
                    <span className={cn("min-w-0 flex-1 truncate", team.length > 0 && "font-medium")}>{name(r)}</span>
                    {team.length > 0 && <span className="shrink-0 text-xs text-muted-foreground tabular-nums max-sm:hidden">{team.filter((k) => k.status === "completed").length}/{team.length}</span>}
                    {r.plan_total > 0 && !team.length && <span className="shrink-0 text-xs text-muted-foreground tabular-nums max-sm:hidden">{r.plan_done}/{r.plan_total} steps</span>}
                    {inProject && <span className="inline-flex max-w-40 shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-xs text-muted-foreground max-md:hidden"><FolderKanban className="size-3 shrink-0" /><span className="truncate">{inProject}</span></span>}
                    {r.routine && <span className="max-w-32 shrink-0 truncate text-xs text-muted-foreground max-md:hidden">{r.routine}</span>}
                    <Character agent={a} size={20} still={!r.live} />
                    <span className="w-12 shrink-0 text-right text-xs text-muted-foreground max-sm:hidden">{stamp(r.updated_at)}</span>
                  </button>
                )
              })}
            </section>
          ))}
      </div>
    </div>
  )
}

type Kind = "waiting" | "running" | "review" | "done" | "todo" | "failed" | "stopped"
const KIND: Record<string, Kind> = { "Waiting for you": "waiting", Running: "running", "To review": "review", Done: "done" }

/** Linear's status marks: a ring that fills as the work goes, a check when
    done, a cross when it failed. */
function StatusIcon({ kind }: { kind: Kind }) {
  const tone = { waiting: "text-amber-500", running: "text-yellow-500", review: "text-emerald-500", done: "text-indigo-500", todo: "text-muted-foreground", failed: "text-red-500", stopped: "text-muted-foreground" }[kind]
  const fill = { waiting: 0.25, running: 0.5, review: 0.75 } as Record<Kind, number>
  return (
    <svg viewBox="0 0 14 14" className={cn("size-3.5 shrink-0", tone)} aria-label={kind}>
      {kind === "done" ? (<><circle cx="7" cy="7" r="6.5" fill="currentColor" /><path d="M4.2 7.2l1.9 1.9 3.7-3.9" fill="none" stroke="var(--background)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" /></>)
        : kind === "failed" || kind === "stopped" ? (<><circle cx="7" cy="7" r="6.5" fill="currentColor" opacity={kind === "stopped" ? 0.5 : 1} /><path d="M4.8 4.8l4.4 4.4M9.2 4.8l-4.4 4.4" stroke="var(--background)" strokeWidth="1.5" strokeLinecap="round" /></>)
        : (<>
          <circle cx="7" cy="7" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray={kind === "todo" ? "2 1.6" : undefined} />
          {fill[kind] ? <circle cx="7" cy="7" r="2.5" fill="none" stroke="currentColor" strokeWidth="5" strokeDasharray={`${fill[kind] * 15.71} 15.71`} transform="rotate(-90 7 7)" /> : null}
        </>)}
    </svg>
  )
}

/** A project's progress, as a ring. */
function Ring({ share }: { share: number }) {
  return (
    <svg viewBox="0 0 16 16" className="size-4 shrink-0 -rotate-90" aria-hidden>
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="2" />
      <circle cx="8" cy="8" r="6" fill="none" stroke={share >= 1 ? "rgb(99 102 241)" : "rgb(16 185 129)"} strokeWidth="2" strokeLinecap={share > 0 ? "round" : undefined} strokeDasharray={`${share * 37.7} 37.7`} />
    </svg>
  )
}

function GroupHead({ label, count, icon, folded, onFold }: { label: string; count: number; icon: React.ReactNode; folded: boolean; onFold: () => void }) {
  return (
    <button onClick={onFold} aria-expanded={!folded} className="sticky top-0 z-10 flex h-9 w-full items-center gap-2 border-b bg-muted/60 px-4 text-left text-[13px] backdrop-blur md:px-5">
      <ChevronDown className={cn("size-3.5 text-muted-foreground transition-transform", folded && "-rotate-90")} />
      {icon}
      <span className="font-medium">{label}</span>
      <span className="text-muted-foreground tabular-nums">{count}</span>
    </button>
  )
}

/** Ask the team: the order goes to the agent that fits (or the one named),
    or to Airth itself, with a model and a repository to work in. */
function Ask({ realm, agents, prefill, onStarted }: { realm: string; agents: Agent[]; prefill?: { text: string; at: number } | null; onStarted: (id: string) => void }) {
  const [text, setText] = useState("")
  const box = useRef<HTMLTextAreaElement>(null)
  // Asked from elsewhere (⌘⇧O, a document's ⌘L): the box, filled and ready.
  useEffect(() => {
    if (!prefill) return
    setText(prefill.text)
    requestAnimationFrame(() => { const el = box.current; if (el) { el.focus(); el.selectionStart = el.selectionEnd = el.value.length } })
  }, [prefill])
  const [who, setWho] = useState<string>("route")
  const [options, setOptions] = useState(false)
  const [why, setWhy] = useState<string | null>(null)
  const [model, setModel] = useState(AUTO)
  const [repo, setRepo] = useState("")
  const [busy, setBusy] = useState(false)
  const detail = usePoll<{ repos: { name: string; main: boolean }[] }>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  // The model list (slow: the providers are asked) only when Airth itself works.
  const models = usePoll<{ providers: { id: string; name: string; configured: boolean; models: string[] }[] }>(who === NOBODY ? () => api(`realms/${enc(realm)}/models`) : null, 600000, [realm, who === NOBODY])
  const repos = detail.data?.repos ?? []

  async function send() {
    const t = text.trim()
    if (!t || busy) return
    setBusy(true)
    try {
      let agent = who
      if (who === "route") {
        // The command bar's rule: the agent named, else the one that fits.
        const r = await api<Route>(`realms/${enc(realm)}/orders/route`, { text: t })
        if (!r.agent) { setWho(NOBODY); setWhy(r.error ?? "No agent fits: choose one, or let Airth do it."); return }
        agent = r.agent
        setWhy(`${((x) => (x ? called(x) : r.agent))(agents.find((a) => a.name === r.agent))}: ${r.why}`)
      }
      const out = agent === NOBODY
        ? await api<{ id: string }>(`realms/${enc(realm)}/runs`, { objective: t, model, repo: repo || null })
        : await api<{ id: string }>(`realms/${enc(realm)}/orders`, { text: t, agent, watched: true })
      setText("")
      onStarted(out.id)
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }

  const whoLabel = (v: string) => (v === "route" ? "The agent that fits" : v === NOBODY ? "Airth itself" : agents.find((a) => a.name === v)?.title ?? v)
  return (
    <div className="mx-3">
      {/* One line to ask, as on the Home; who does it and with what, under "…". */}
      <div className="flex items-end gap-0.5 rounded-[22px] bg-bubble py-1 pr-1 pl-4">
        <textarea ref={box} value={text} onChange={(e) => setText(e.target.value)} rows={1} placeholder="Ask the team…" aria-label="Ask the team for something"
          onKeyDown={(e) => {
            const wide = matchMedia("(min-width: 768px)").matches
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing && (wide || e.metaKey || e.ctrlKey)) { e.preventDefault(); send() }
          }}
          className="max-h-40 min-h-9 flex-1 resize-none bg-transparent py-1.5 text-sm leading-6 outline-none field-sizing-content placeholder:text-muted-foreground" />
        <button type="button" onClick={() => setOptions(!options)} aria-label="Who does it" title="Who does it"
          className={cn("mb-0.5 grid size-8 shrink-0 place-items-center rounded-full text-muted-foreground hover:bg-accent hover:text-foreground", (options || who !== "route") && "bg-accent text-foreground")}>
          <Ellipsis className="size-4" />
        </button>
        <MicButton value={text} onChange={setText} className="mb-0.5" />
        <Button size="icon-sm" className="mb-0.5 size-8 shrink-0 rounded-full" disabled={!text.trim() || busy} onClick={send} aria-label="Start" title="Start (↵)"><ArrowUp /></Button>
      </div>
      {(options || who !== "route") && (
      <div className="mt-1.5 flex flex-wrap items-center gap-1.5 px-1">
        <Select value={who} onValueChange={(v) => { setWho(String(v)); setWhy(null) }}>
          <SelectTrigger size="sm" className="h-7 max-w-40 text-xs"><SelectValue>{(v: string) => whoLabel(v)}</SelectValue></SelectTrigger>
          <SelectContent>
            <SelectItem value="route">The agent that fits</SelectItem>
            {agents.map((a) => <SelectItem key={a.name} value={a.name}>{a.title}</SelectItem>)}
            <SelectItem value={NOBODY}>Airth itself</SelectItem>
          </SelectContent>
        </Select>
        {who === NOBODY && (
          <Select value={model} onValueChange={(v) => setModel(String(v))}>
            <SelectTrigger size="sm" className="h-7 max-w-36 text-xs" title="The model for this task"><SelectValue>{(v: string) => (v === AUTO ? "Model: auto" : v.split("/").slice(1).join("/"))}</SelectValue></SelectTrigger>
            <SelectContent>
              <SelectItem value={AUTO}>Auto: the business's</SelectItem>
              {(models.data?.providers ?? []).filter((p) => p.configured).flatMap((p) => p.models.map((m) => <SelectItem key={`${p.id}/${m}`} value={`${p.id}/${m}`}>{p.name} · {m}</SelectItem>))}
            </SelectContent>
          </Select>
        )}
        {who === NOBODY && repos.length > 1 && (
          <Select value={repo || repos[0].name} onValueChange={(v) => setRepo(String(v))}>
            <SelectTrigger size="sm" className="h-7 max-w-32 text-xs" title="The repository it works in"><SelectValue /></SelectTrigger>
            <SelectContent>{repos.map((r) => <SelectItem key={r.name} value={r.name}>{r.name}</SelectItem>)}</SelectContent>
          </Select>
        )}
      </div>
      )}
      {why && <p className="mt-1.5 px-1 text-[11px] text-muted-foreground">{why}</p>}
    </div>
  )
}

const MARK: Record<string, React.ReactNode> = {
  done: <Check className="size-3.5 text-emerald-500" />,
  in_progress: <CircleDot className="size-3.5 text-sky-500" />,
  skipped: <SkipForward className="size-3.5 text-muted-foreground" />,
  pending: <Circle className="size-3.5 text-muted-foreground/60" />,
}

type Full = {
  live: boolean
  state: {
    objective: string; status: string; iterations: number; events: unknown[]; input_tokens: number; output_tokens: number; updated_at: string; id: string
    plan?: { title: string; status: string; note?: string }[]; origin?: { objective: string; trigger: string } | null
    usage?: { model: string; input: number; cache_read: number }[]; tier?: string | null; change?: { status: string } | null
  }
}

function TaskView({ realm, run, agent, items, onBack, backLabel = "Projects", onChange, onGone, onAgent, from, passed, onOpenTask }: {
  realm: string; run: Run; agent: Agent; items: Item[]; onBack: () => void; backLabel?: string; onChange: () => void; onGone: () => void; onAgent: (name: string) => void
  /** Who passed this task on, and what it passed on (decision 120). */
  from?: string; passed?: Passed[]; onOpenTask?: (id: string) => void
}) {
  const { view } = useRun(realm, run.id, run.live)
  const full = view as unknown as Full | null
  const [renaming, setRenaming] = useState<string | null>(null)
  const [sure, setSure] = useState(false)
  const [msg, setMsg] = useState("")
  const path = `realms/${enc(realm)}/runs/${enc(run.id)}`
  useEffect(() => setSure(false), [run.id])

  async function rename() {
    try { await api(path, { title: (renaming ?? "").trim() }, "PATCH"); setRenaming(null); onChange() } catch (e) { toast.error((e as Error).message) }
  }
  async function archive() {
    try { await api(`${path}/archive`, { archived: !run.archived }); toast.success(run.archived ? "Back in the open tasks." : "Archived."); onChange() } catch (e) { toast.error((e as Error).message) }
  }
  async function remove() {
    try { await api(path, undefined, "DELETE"); toast.success("Task deleted."); onGone() } catch (e) { toast.error((e as Error).message); setSure(false) }
  }
  async function follow() {
    const t = msg.trim()
    if (!t) return
    try { await api(`${path}/messages`, { message: t }); setMsg(""); onChange() } catch (e) { toast.error((e as Error).message) }
  }

  const s = full?.state
  const plan = s?.plan ?? []
  const closed = plan.filter((p) => p.status === "done" || p.status === "skipped").length
  const usage = s?.usage ?? []
  const input = usage.reduce((a, u) => a + u.input, 0), cached = usage.reduce((a, u) => a + u.cache_read, 0)
  const rows: [string, React.ReactNode][] = s ? ([
    ["Status", run.live ? "Running" : s.status],
    s.origin ? ["Started by", <span title={s.origin.trigger}>{s.origin.objective}</span>] : null,
    ["Iterations", String(s.iterations)],
    ["Tool calls", String(s.events.length)],
    s.input_tokens || s.output_tokens ? ["Tokens", `${Math.round(s.input_tokens / 1000)}k in · ${Math.round(s.output_tokens / 1000)}k out`] : null,
    usage.length ? ["Model", usage[usage.length - 1].model] : null,
    s.tier ? ["Routing", s.tier === "fast" ? "Fast model (a simple ask)" : "Main model"] : null,
    cached ? ["From cache", `${input ? Math.round((100 * cached) / input) : 0}% of input`] : null,
    s.change ? ["Change", s.change.status] : null,
    ["Updated", ago(s.updated_at)],
    ["Task", <span className="font-mono">{s.id}</span>],
  ] as ([string, React.ReactNode] | null)[]).filter((x): x is [string, React.ReactNode] => !!x) : []

  return (
    <div className="flex min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex shrink-0 items-center gap-2 border-b px-5 py-3 max-md:min-h-[60px] max-md:pr-3 max-md:pl-[68px]">
          <Back label={backLabel} onClick={onBack} always />
          {renaming !== null ? (
            <>
              <Input autoFocus value={renaming} onChange={(e) => setRenaming(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") rename(); if (e.key === "Escape") setRenaming(null) }} className="h-8" aria-label="Task name" />
              <Button size="sm" onClick={rename}>Rename</Button>
              <Button size="icon-sm" variant="ghost" aria-label="Cancel" onClick={() => setRenaming(null)}><X /></Button>
            </>
          ) : (
            <>
              <h2 className="min-w-0 flex-1 truncate text-sm font-semibold" title={run.objective}>{name(run)}</h2>
              {agent !== AIRTH && <Button size="sm" variant="ghost" className="max-md:hidden" onClick={() => onAgent(agent.name)}>Open {agent.title}</Button>}
              <Button size="icon-sm" variant="ghost" title="Rename" aria-label="Rename" onClick={() => setRenaming(name(run))}><Pencil /></Button>
              <Button size="icon-sm" variant="ghost" disabled={run.live} title={run.archived ? "Back to the open tasks" : "Archive"} aria-label={run.archived ? "Unarchive" : "Archive"} onClick={archive}>{run.archived ? <ArchiveRestore /> : <Archive />}</Button>
              {sure ? <Button size="sm" variant="destructive" onClick={remove}>Delete for good?</Button>
                : <Button size="icon-sm" variant="ghost" disabled={run.live} className="hover:text-red-500" title="Delete" aria-label="Delete" onClick={() => { setSure(true); setTimeout(() => setSure(false), 4000) }}><Trash2 /></Button>}
            </>
          )}
        </header>
        <div className="no-dock min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-3xl px-4 pt-5 pb-4 [overflow-wrap:anywhere] md:px-6 md:pt-6">
            <TaskThread realm={realm} id={run.id} agent={agent} items={items} fresh={false} onChange={onChange}
              from={from} passed={passed} onAgent={(a) => { const t = passed?.find((p) => p.agent === a); if (t && onOpenTask) onOpenTask(t.id); else onAgent(a) }} />
          </div>
        </div>
        <div className="dock-lift mx-auto w-full max-w-3xl px-3 pb-3 md:px-6 md:pb-5">
          <div className="flex items-end gap-2 rounded-2xl bg-bubble px-3 py-2">
            <textarea value={msg} onChange={(e) => setMsg(e.target.value)} rows={1} placeholder={run.live ? "Add to what it is doing…" : "Go on with this task…"} aria-label="Follow up"
              onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); follow() } }}
              className="max-h-40 min-h-8 flex-1 resize-none bg-transparent px-1 py-1.5 text-[13.5px] outline-none field-sizing-content placeholder:text-muted-foreground" />
            <Button size="icon-sm" disabled={!msg.trim()} onClick={follow} aria-label="Send"><ArrowUp /></Button>
          </div>
        </div>
      </div>
      <aside className="w-(--side) shrink-0 overflow-y-auto border-l p-4 text-sm max-md:hidden">
        <h3 className="mb-2 flex items-center justify-between text-xs font-semibold text-muted-foreground uppercase">Plan{plan.length > 0 && <span className="font-normal normal-case">{closed}/{plan.length}</span>}</h3>
        {plan.length ? (
          <>
            <div className="mb-3 h-1 overflow-hidden rounded-full bg-muted"><span className="block h-full bg-emerald-500 transition-all" style={{ width: `${(100 * closed) / plan.length}%` }} /></div>
            <ol className="space-y-2">
              {plan.map((p, i) => (
                <li key={i} className="flex gap-2">
                  <span className="mt-0.5">{MARK[p.status] ?? MARK.pending}</span>
                  <span className="min-w-0"><span className={cn("block text-[13px]", p.status === "skipped" && "text-muted-foreground line-through")}>{p.title}</span>{p.note && <span className="block text-xs text-muted-foreground">{p.note}</span>}</span>
                </li>
              ))}
            </ol>
          </>
        ) : <p className="text-xs text-muted-foreground">{run.live ? "The plan shows here as soon as the work takes more than a couple of steps." : "No plan for this task."}</p>}
        <h3 className="mt-6 mb-2 text-xs font-semibold text-muted-foreground uppercase">Details</h3>
        <dl className="space-y-1.5 text-xs">
          {rows.map(([k, v]) => <div key={k} className="flex gap-2"><dt className="w-24 shrink-0 text-muted-foreground">{k}</dt><dd className="min-w-0 break-words">{v}</dd></div>)}
        </dl>
      </aside>
    </div>
  )
}
