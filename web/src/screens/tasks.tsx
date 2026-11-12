import { useEffect, useMemo, useRef, useState } from "react"
import { Archive, ArchiveRestore, ArrowUp, Check, Circle, CircleDot, CornerDownRight, FolderKanban, Pencil, SkipForward, Trash2, X } from "lucide-react"
import { Back, SideHead } from "@/components/page"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { MicButton } from "@/components/mic"
import { useRun, TaskThread, type Passed } from "@/components/task-thread"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { usePoll } from "@/hooks/use-poll"
import { api, called, enc, type Agent, type Item, type Tasks as TasksData } from "@/lib/api"
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
export function Tasks({ realm, agents, items, open, onOpen, onAgent, prefill, tasks: shared, onTasks }: {
  realm: string; agents: Agent[]; items: Item[]; open: string | null; onOpen: (id: string | null) => void; onAgent: (name: string) => void
  tasks: TasksData | null; onTasks: () => void
  prefill?: { text: string; at: number } | null
}) {
  const runs = usePoll<Run[]>(() => api(`realms/${enc(realm)}/runs`), 5000, [realm])
  // The app's list, refreshed every few seconds: not asked again here.
  const tasks = { data: shared, refresh: onTasks }
  const [archived, setArchived] = useState(false)
  const agentOf = useMemo(() => {
    const m = new Map<string, string>()
    for (const g of Object.values(tasks.data ?? {})) for (const t of g) if (t.agent) m.set(t.id, t.agent)
    return m
  }, [tasks.data])
  const waiting = new Set((tasks.data?.waiting ?? []).map((t) => t.id))
  const list = (runs.data ?? []).filter((r) => r.archived === archived)
  // A plan's tasks (decision 120) sit under the task that passed them on,
  // which is grouped by the most pressing of them all.
  const fromOf = useMemo(() => {
    const m = new Map<string, string>()
    for (const g of Object.values(tasks.data ?? {})) for (const t of g) if (t.from?.task) m.set(t.id, t.from.task)
    return m
  }, [tasks.data])
  const kids = (id: string) => list.filter((r) => fromOf.get(r.id) === id)
  const top = list.filter((r) => { const p = fromOf.get(r.id); return !p || !list.some((x) => x.id === p) })
  const planGroup = (r: Run) => ORDER[Math.min(...[r, ...kids(r.id)].map((x) => ORDER.indexOf(group(x, waiting))))]
  const groups = ORDER.map((g) => [g, top.filter((r) => planGroup(r) === g)] as const).filter(([, l]) => l.length)
  const titleOf = (n: string | null | undefined) => { const a = agents.find((x) => x.name === n); return a ? called(a) : n ?? "An agent" }
  const passedOf = (id: string): Passed[] => kids(id).map((k) => ({ id: k.id, agent: agentOf.get(k.id) ?? "", title: titleOf(agentOf.get(k.id)), order: name(k), status: k.live ? "running" : k.status }))
  const fromTitle = (id: string) => { const p = fromOf.get(id); return p ? titleOf(agentOf.get(p)) : undefined }
  const current = runs.data?.find((r) => r.id === open) ?? null

  return (
    <div className="flex min-w-0 flex-1">
      <aside className={cn("flex w-(--side) shrink-0 flex-col border-r max-md:w-full max-md:border-r-0", current && "max-md:hidden")}>
        <SideHead title="Projects" />
        <Ask realm={realm} agents={agents} prefill={prefill} onStarted={(id) => { runs.refresh(); tasks.refresh(); onOpen(id) }} />
        <div className="flex items-center gap-1 px-4 pt-3 pb-1.5 text-xs">
          {[[false, "Open"], [true, "Archived"]].map(([v, l]) => (
            <button key={String(v)} onClick={() => setArchived(v as boolean)} className={cn("rounded-md px-2 py-1", archived === v ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")}>{l as string}</button>
          ))}
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-3">
          {!runs.data ? <p className="px-2 py-2 text-sm text-muted-foreground">Loading…</p>
            : !groups.length ? <p className="px-2 py-6 text-center text-sm text-muted-foreground">{archived ? "Nothing archived." : "No project or task yet."}</p>
            : groups.map(([g, l]) => (
              <section key={g} className="mb-2">
                <h2 className="px-2 pt-2 pb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{g} · {l.length}</h2>
                {l.flatMap((parent) => [parent, ...kids(parent.id)]).map((r) => {
                  const a = agents.find((x) => x.name === agentOf.get(r.id)) ?? AIRTH
                  const child = fromOf.has(r.id) && list.some((x) => x.id === fromOf.get(r.id))
                  const team = kids(r.id)
                  const plan = team.length ? `project ${team.filter((k) => k.status === "completed").length}/${team.length} done` : null
                  return (
                    <button key={r.id} onClick={() => onOpen(r.id)} className={cn("flex w-full items-center gap-2.5 rounded-lg px-2 py-2 text-left hover:bg-accent/60", child && "pl-7", open === r.id && "bg-accent")}>
                      {child && <CornerDownRight className="size-3.5 shrink-0 text-muted-foreground" />}
                      <Character agent={a} size={22} still={!r.live} />
                      <span className="min-w-0 flex-1">
                        <span className={cn("block truncate text-[13px]", team.length > 0 && "font-medium")}>{name(r)}</span>
                        <span className="block truncate text-[11px] text-muted-foreground">
                          {[a.title, plan, r.routine && `routine ${r.routine}`, r.plan_total ? `${r.plan_done}/${r.plan_total} steps` : null, r.status !== "completed" && r.status].filter(Boolean).join(" · ")}
                        </span>
                      </span>
                      <span className="shrink-0 text-[11px] text-muted-foreground">{stamp(r.updated_at)}</span>
                    </button>
                  )
                })}
              </section>
            ))}
        </div>
      </aside>
      {current ? (
        <TaskView key={current.id} realm={realm} run={current} agent={agents.find((x) => x.name === agentOf.get(current.id)) ?? AIRTH} items={items} onBack={() => onOpen(null)}
          from={fromTitle(current.id)} passed={passedOf(current.id)} onOpenTask={onOpen}
          onChange={() => { runs.refresh(); tasks.refresh() }} onGone={() => { onOpen(null); runs.refresh(); tasks.refresh() }} onAgent={onAgent} />
      ) : (
        <div className="grid flex-1 place-items-center p-8 text-center max-md:hidden">
          <div>
            <FolderKanban className="mx-auto size-8 text-muted-foreground/50" />
            <p className="mt-3 text-sm font-medium">Choose a project or a task, or ask the team for something.</p>
            <p className="mt-1 text-sm text-muted-foreground">Each one shows here with what was done, its plan and its change.</p>
          </div>
        </div>
      )}
    </div>
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
    <div className="mx-3 rounded-xl bg-surface p-2.5">
      <textarea ref={box} value={text} onChange={(e) => setText(e.target.value)} rows={2} placeholder="Ask the team for something…" aria-label="Ask the team for something"
        onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); send() } }}
        className="w-full resize-none bg-transparent px-1 text-sm outline-none placeholder:text-muted-foreground" />
      <div className="flex flex-wrap items-center gap-1.5">
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
        <span className="flex-1" />
        <MicButton value={text} onChange={setText} />
        <Button size="icon-sm" disabled={!text.trim() || busy} onClick={send} aria-label="Start" title="Start (⌘↵)"><ArrowUp /></Button>
      </div>
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

function TaskView({ realm, run, agent, items, onBack, onChange, onGone, onAgent, from, passed, onOpenTask }: {
  realm: string; run: Run; agent: Agent; items: Item[]; onBack: () => void; onChange: () => void; onGone: () => void; onAgent: (name: string) => void
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
        <header className="flex shrink-0 items-center gap-2 border-b px-5 py-3 max-md:px-3">
          <Back label="Projects" onClick={onBack} />
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
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-3xl px-4 pt-5 pb-4 [overflow-wrap:anywhere] md:px-6 md:pt-6">
            <TaskThread realm={realm} id={run.id} agent={agent} items={items} fresh={false} onChange={onChange}
              from={from} passed={passed} onAgent={(a) => { const t = passed?.find((p) => p.agent === a); if (t && onOpenTask) onOpenTask(t.id); else onAgent(a) }} />
          </div>
        </div>
        <div className="mx-auto w-full max-w-3xl px-3 pb-3 md:px-6 md:pb-5">
          <div className="flex items-end gap-2 rounded-2xl border bg-surface px-3 py-2">
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
