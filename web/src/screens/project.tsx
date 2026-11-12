import { useEffect, useRef, useState } from "react"
import { ArrowUp, Check, CircleCheck, Flag, Loader, MessageSquare, Pencil, RotateCcw, X } from "lucide-react"
import { toast } from "sonner"
import { Back } from "@/components/page"
import { Character } from "@/components/character"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { api, called, enc, type Agent, type Project } from "@/lib/api"
import { sendOrder } from "@/lib/orders"
import { cn } from "@/lib/utils"

/** What a project's task view needs of a run. */
export type RunLite = { id: string; status: string; live: boolean }

type Said = "to come" | "running" | "done" | "failed" | "stopped" | "not started"

/** Where a task of the project stands, from its run. */
export function said(t: Project["tasks"][number], runs?: Map<string, RunLite>): Said {
  // The runs this page polls are the freshest; Airth's word otherwise.
  if (t.skipped) return "not started"
  if (!runs || (t.run && !runs.has(t.run))) return t.state ?? (t.run ? "done" : "to come")
  const r = t.run ? runs.get(t.run) : undefined
  if (!t.run || !r) return "to come"
  if (r.live) return "running"
  if (r.status === "completed") return "done"
  return r.status === "failed" ? "failed" : "stopped"
}

const TONE: Record<Said, string> = {
  "to come": "bg-muted text-muted-foreground",
  running: "bg-sky-500/10 text-sky-600 dark:text-sky-300",
  done: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-300",
  failed: "bg-red-500/10 text-red-500",
  stopped: "bg-amber-500/10 text-amber-600 dark:text-amber-300",
  "not started": "bg-muted text-muted-foreground line-through",
}

/** A project at a glance: tasks done of all, the first milestone not
    reached with its date, and how many tasks failed or stopped. */
export function glance(p: Project, runs?: Map<string, RunLite>) {
  const states = p.tasks.map((t) => said(t, runs))
  const reached = (m: number) => {
    const of = p.tasks.map((t, i) => [t, states[i]] as const).filter(([t]) => t.milestone === m)
    return of.length > 0 && of.every(([, s]) => s === "done")
  }
  const next = p.milestones.find((m) => !reached(m.id)) ?? null
  return {
    done: states.filter((s) => s === "done").length,
    total: p.tasks.length,
    running: states.includes("running"),
    stuck: states.filter((s) => s === "failed" || s === "stopped").length,
    next,
    when: next?.due ? due(next.due, false) : null,
  }
}

const DAY = 86400000
/** A milestone's date, said against today: "in 12 days", "today", "3 days late". */
export function due(iso: string, reached: boolean): { text: string; late: boolean } {
  const d = new Date(`${iso}T12:00:00`)
  const days = Math.round((d.getTime() - Date.now()) / DAY)
  const date = d.toLocaleDateString("en-GB", { day: "numeric", month: "short" })
  if (reached) return { text: date, late: false }
  if (days > 0) return { text: `${date} · in ${days} day${days > 1 ? "s" : ""}`, late: false }
  if (days === 0) return { text: `${date} · today`, late: false }
  return { text: `${date} · ${-days} day${days < -1 ? "s" : ""} late`, late: true }
}

/** One project (decision 129): what it is for, its milestones with their
    dates, every task under the milestone it serves, and a line to add to
    it, which goes to the agent leading it. */
export function ProjectView({ realm, project: p, runs, agents, onBack, onTask, onChange }: {
  realm: string; project: Project; runs: Map<string, RunLite>; agents: Agent[]
  onBack: () => void; onTask: (run: string) => void; onChange: () => void
}) {
  const [naming, setNaming] = useState<string | null>(null)
  const [describing, setDescribing] = useState<string | null>(null)
  const [more, setMore] = useState("")
  const [busy, setBusy] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  useEffect(() => { setNaming(null); setDescribing(null); setMore("") }, [p.id])
  useEffect(() => {
    const el = box.current
    if (!el) return
    el.style.height = "auto"
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`
  }, [more])

  const lead = agents.find((a) => a.name === p.agent)
  const ofAgent = (n: string) => agents.find((a) => a.name === n)
  const states = p.tasks.map((t) => said(t, runs))
  const done = states.filter((s) => s === "done").length
  const groups: { id: number | null; title: string | null; due: string | null }[] = [
    ...(p.tasks.some((t) => t.milestone === null) ? [{ id: null, title: null, due: null }] : []),
    ...p.milestones.map((m) => ({ id: m.id, title: m.title, due: m.due })),
  ]

  async function save(body: Record<string, string>) {
    try { await api(`realms/${enc(realm)}/projects/${p.id}`, body, "PUT"); onChange(); return true }
    catch (e) { toast.error((e as Error).message); return false }
  }
  async function add() {
    const t = more.trim()
    if (!t || busy) return
    setBusy(true)
    // The agent leading it adds to it, in the task the project was planned
    // in, so the project's conversation stays one.
    const ask = `For project ${p.id}, "${p.name}": ${t}`
    try {
      if (p.run) await api(`realms/${enc(realm)}/runs/${enc(p.run)}/messages`, { message: ask })
      else await sendOrder(realm, ask, p.agent)
      setMore("")
      toast.success(`Sent to ${lead ? called(lead) : p.agent}`, { description: "Its plan for this comes to Needs you." })
      onChange()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex shrink-0 items-center gap-2 border-b px-5 py-3 max-md:px-3">
        <Back label="Projects" onClick={onBack} />
        {naming !== null ? (
          <>
            <Input autoFocus value={naming} onChange={(e) => setNaming(e.target.value)} aria-label="Project name" className="h-8"
              onKeyDown={async (e) => { if (e.key === "Enter" && (await save({ name: naming }))) setNaming(null); if (e.key === "Escape") setNaming(null) }} />
            <Button size="sm" onClick={async () => { if (await save({ name: naming })) setNaming(null) }}>Rename</Button>
            <Button size="icon-sm" variant="ghost" aria-label="Cancel" onClick={() => setNaming(null)}><X /></Button>
          </>
        ) : (
          <>
            <h2 className="min-w-0 flex-1 truncate text-sm font-semibold">{p.name}</h2>
            {p.run && <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => onTask(p.run!)}><MessageSquare /> <span className="max-md:hidden">Conversation</span></Button>}
            <Button size="icon-sm" variant="ghost" title="Rename" aria-label="Rename" onClick={() => setNaming(p.name)}><Pencil /></Button>
            {p.status === "active"
              ? <Button size="sm" variant="outline" onClick={() => save({ status: "done" })}><Check /> Done</Button>
              : <Button size="sm" variant="outline" onClick={() => save({ status: "active" })}><RotateCcw /> Reopen</Button>}
          </>
        )}
      </header>
      <div className="no-dock min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-4 pt-6 pb-6 [overflow-wrap:anywhere] md:px-6">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            {lead && <Character agent={lead} size={18} still />}
            <span>{lead ? `Led by ${called(lead)}` : "Led by the team"}</span>
            {p.status === "done" && <span className="flex items-center gap-1 rounded-md bg-emerald-500/10 px-1.5 py-0.5 text-emerald-600 dark:text-emerald-300"><CircleCheck className="size-3" /> Done</span>}
          </div>
          {describing !== null ? (
            <div className="mt-3">
              <textarea autoFocus value={describing} onChange={(e) => setDescribing(e.target.value)} rows={3} aria-label="Description"
                className="w-full resize-none rounded-xl border bg-surface px-3 py-2 text-[15px] leading-relaxed outline-none focus:border-ring" />
              <div className="mt-2 flex gap-2">
                <Button size="sm" onClick={async () => { if (await save({ description: describing })) setDescribing(null) }}>Save</Button>
                <Button size="sm" variant="ghost" onClick={() => setDescribing(null)}>Cancel</Button>
              </div>
            </div>
          ) : (
            <button onClick={() => setDescribing(p.description)} title="Edit the description"
              className="group mt-3 block w-full rounded-lg text-left text-[15px] leading-relaxed whitespace-pre-wrap text-foreground/85 hover:text-foreground">
              {p.description || <span className="text-muted-foreground">Add a description…</span>}
              <Pencil className="ml-1.5 inline size-3.5 text-muted-foreground opacity-0 group-hover:opacity-100" />
            </button>
          )}

          <div className="mt-6 flex items-center gap-3">
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
              <span className="block h-full rounded-full bg-emerald-500 transition-all" style={{ width: `${p.tasks.length ? (100 * done) / p.tasks.length : 0}%` }} />
            </div>
            <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{done} of {p.tasks.length} tasks done</span>
          </div>

          {groups.map((g) => {
            const tasks = p.tasks.map((t, i) => ({ t, s: states[i] })).filter(({ t }) => t.milestone === g.id)
            const reached = tasks.length > 0 && tasks.every(({ s }) => s === "done")
            const when = g.due ? due(g.due, reached) : null
            return (
              <section key={g.id ?? "none"} className="mt-7">
                {g.title && (
                  <h3 className="mb-2 flex items-center gap-2 text-sm font-semibold">
                    <Flag className={cn("size-3.5", reached ? "text-emerald-500" : "text-muted-foreground")} />
                    <span className="min-w-0 flex-1 truncate">{g.title}</span>
                    {when && <span className={cn("shrink-0 text-xs font-normal", when.late ? "text-red-500" : "text-muted-foreground")}>{when.text}</span>}
                  </h3>
                )}
                {tasks.length ? (
                  <ul className="divide-y rounded-xl border bg-surface">
                    {tasks.map(({ t, s }) => {
                      const a = ofAgent(t.agent)
                      return (
                        <li key={t.id}>
                          <button disabled={!t.run} onClick={() => t.run && onTask(t.run)} title={t.skipped ?? undefined}
                            className="flex w-full items-start gap-3 px-3.5 py-3 text-left enabled:hover:bg-accent/50 disabled:cursor-default">
                            {a ? <Character agent={a} size={24} still={s !== "running"} /> : <span className="size-6 shrink-0 rounded-full bg-muted" />}
                            <span className="min-w-0 flex-1">
                              <span className="block text-xs font-medium text-muted-foreground">{a ? called(a) : t.agent}</span>
                              <span className="line-clamp-2 text-[13.5px]">{t.task.split("\n")[0]}</span>
                              {t.skipped && <span className="mt-0.5 block text-xs text-muted-foreground">Not started: {t.skipped}</span>}
                            </span>
                            <span className={cn("mt-0.5 shrink-0 rounded-md px-1.5 py-0.5 text-[11px]", TONE[s])}>{s}</span>
                          </button>
                        </li>
                      )
                    })}
                  </ul>
                ) : <p className="rounded-xl border border-dashed px-3.5 py-3 text-xs text-muted-foreground">No task for this milestone yet.</p>}
              </section>
            )
          })}
        </div>
      </div>
      <div className="dock-lift mx-auto w-full max-w-3xl px-3 pb-3 md:px-6 md:pb-5">
        <div className="flex items-end gap-1.5 rounded-[22px] border bg-surface py-1 pr-1 pl-4 shadow-[0_2px_10px_-4px_rgb(0_0_0/0.08)] focus-within:border-ring">
          <textarea ref={box} value={more} onChange={(e) => setMore(e.target.value)} rows={1} disabled={busy}
            placeholder="Add to this project…" aria-label="Add to this project"
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); add() } }}
            className="max-h-40 min-h-9 flex-1 resize-none bg-transparent py-1.5 text-sm leading-6 outline-none placeholder:text-muted-foreground" />
          <Button size="icon-sm" className="mb-0.5 size-8 shrink-0 rounded-full" disabled={busy || !more.trim()} onClick={add} aria-label="Send">
            {busy ? <Loader className="animate-spin" /> : <ArrowUp />}
          </Button>
        </div>
        <p className="mt-1.5 px-2 text-xs text-muted-foreground">{lead ? called(lead) : "The agent leading it"} plans it; the plan comes to Needs you.</p>
      </div>
    </div>
  )
}
