import { useEffect, useMemo, useRef, useState } from "react"
import { CalendarClock, Check, ChevronLeft, ChevronRight, X } from "lucide-react"
import { Character } from "@/components/character"
import { Empty, Loading } from "@/components/page"
import { Button } from "@/components/ui/button"
import { isNarrow } from "@/hooks/use-narrow"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Calendar as Data } from "@/lib/api"
import { cn } from "@/lib/utils"

type View = "day" | "week" | "month" | "agenda"
type Run = Data["days"][number]["runs"][number]
type Day = Data["days"][number]

const VIEWS: { id: View; label: string }[] = [
  { id: "day", label: "Day" }, { id: "week", label: "Week" }, { id: "month", label: "Month" }, { id: "agenda", label: "Agenda" },
]
const HOUR = 56 // px per hour in the day and week grids: a 30-minute run is 28px

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`
const add = (d: Date, n: number) => { const x = new Date(d); x.setDate(x.getDate() + n); return x }
const monday = (d: Date) => add(d, -((d.getDay() + 6) % 7))
const fmt = (d: Date, o: Intl.DateTimeFormatOptions) => d.toLocaleDateString("en-GB", o)

// What a view shows: its first day, how many days, and its name.
function range(view: View, anchor: Date) {
  if (view === "day") return { from: anchor, days: 1, label: fmt(anchor, { weekday: "long", day: "numeric", month: "long", year: "numeric" }) }
  if (view === "week") {
    const from = monday(anchor), to = add(from, 6)
    return { from, days: 7, label: `${fmt(from, { day: "numeric", month: from.getMonth() === to.getMonth() ? undefined : "short" })} – ${fmt(to, { day: "numeric", month: "short", year: "numeric" })}` }
  }
  if (view === "month") {
    const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1)
    return { from: monday(first), days: 42, label: fmt(first, { month: "long", year: "numeric" }) }
  }
  return { from: anchor, days: 28, label: `From ${fmt(anchor, { day: "numeric", month: "long" })}` }
}

const STATE: Record<string, { cls: string; label: string }> = {
  done: { cls: "", label: "Ran" },
  late: { cls: "", label: "Ran late" },
  failed: { cls: "ring-1 ring-red-500/60", label: "Failed" },
  "not run": { cls: "opacity-50 line-through", label: "Did not run" },
  next: { cls: "ring-1 ring-sky-500/70", label: "Next" },
  planned: { cls: "", label: "Planned" },
}

/** The routines' runs and the legal deadlines, by day, week, month or as an
    agenda; each run opens its agent. */
export function Calendar({ realm, agents, onAgent }: { realm: string; agents: Agent[]; onAgent: (name: string) => void }) {
  const [view, setView] = useState<View>(() => (localStorage.getItem("airth-cal-view") as View) || (isNarrow() ? "agenda" : "week"))
  const [anchor, setAnchor] = useState(() => new Date())
  useEffect(() => { try { localStorage.setItem("airth-cal-view", view) } catch { /* private window */ } }, [view])
  const r = range(view, anchor)
  const { data } = usePoll<Data>(() => api(`realms/${enc(realm)}/calendar?from=${iso(r.from)}&days=${r.days}`), 30000, [realm, view, iso(r.from)])
  const step = (n: number) => setAnchor((a) => (view === "day" ? add(a, n) : view === "month" ? new Date(a.getFullYear(), a.getMonth() + n, 1) : add(a, 7 * n)))

  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (/^(INPUT|TEXTAREA|SELECT)$/.test((e.target as HTMLElement)?.tagName ?? "") || e.metaKey || e.ctrlKey) return
      const k = e.key.toLowerCase()
      if (k === "t") setAnchor(new Date())
      else if (k === "arrowleft" || k === "j") step(-1)
      else if (k === "arrowright" || k === "k") step(1)
      else if (k === "d") setView("day")
      else if (k === "w") setView("week")
      else if (k === "m") setView("month")
      else if (k === "a") setView("agenda")
    }
    addEventListener("keydown", on)
    return () => removeEventListener("keydown", on)
  })

  const face = (name: string) => agents.find((a) => a.name === name)

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex flex-wrap items-center gap-3 border-b px-6 py-3.5">
        <h1 className="text-lg font-semibold tracking-tight">Calendar</h1>
        <Button variant="outline" size="sm" onClick={() => setAnchor(new Date())}>Today</Button>
        <div className="flex">
          <Button variant="ghost" size="icon-sm" aria-label="Before" onClick={() => step(-1)}><ChevronLeft /></Button>
          <Button variant="ghost" size="icon-sm" aria-label="After" onClick={() => step(1)}><ChevronRight /></Button>
        </div>
        <span className="text-sm font-medium">{r.label}</span>
        {data && <span className="text-xs text-muted-foreground">{data.title}</span>}
        <div className="ml-auto flex rounded-lg bg-muted p-0.5">
          {VIEWS.map((v) => (
            <button key={v.id} onClick={() => setView(v.id)} title={`${v.label} (${v.id[0].toUpperCase()})`}
              className={cn("rounded-md px-3 py-1 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground", view === v.id && "bg-background text-foreground shadow-sm")}>
              {v.label}
            </button>
          ))}
        </div>
      </header>
      {!data ? <div className="p-6"><Loading /></div>
        : view === "month" ? <Month days={data.days} anchor={anchor} face={face} onAgent={onAgent} onDay={(d) => { setAnchor(d); setView("day") }} />
        : view === "agenda" ? <Agenda data={data} face={face} onAgent={onAgent} />
        : <Grid days={data.days} face={face} onAgent={onAgent} onDay={(d) => { setAnchor(d); setView("day") }} />}
      <Planning realm={realm} face={face} onAgent={onAgent} />
    </div>
  )
}

type Planned = { at: string; name: string; kind: string; bot: string | null; status?: string; run?: string | null }

/** The agent routines and fixed sequences (the classic Planning page): the
    coming week's times and how the last week's went. Block routines are in
    the calendar above. */
function Planning({ realm, face, onAgent }: { realm: string; face: Face; onAgent: (n: string) => void }) {
  const { data } = usePoll<{ coming: Planned[]; past: Planned[] }>(() => api(`realms/${enc(realm)}/planning`), 60000, [realm])
  if (!data || (!data.coming.length && !data.past.length)) return null
  const when = (iso: string) => new Date(iso).toLocaleString("en-GB", { weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })
  const row = (x: Planned, past: boolean) => {
    const a = x.bot ? face(x.bot) : undefined
    return (
      <button key={`${x.name}${x.at}`} disabled={!a} onClick={() => a && onAgent(a.name)} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs enabled:hover:bg-accent/60">
        <span className="w-24 shrink-0 text-muted-foreground tabular-nums">{when(x.at)}</span>
        <span className="min-w-0 flex-1 truncate font-mono">{x.name}</span>
        <span className="shrink-0 text-muted-foreground">{a?.title ?? (x.kind === "pipeline" ? "Fixed sequence" : x.bot ?? "")}</span>
        {past && <span className={cn("w-16 shrink-0 text-right", x.status === "completed" ? "text-emerald-500" : x.status === "failed" ? "text-red-500" : "text-muted-foreground")}>{x.status}</span>}
      </button>
    )
  }
  return (
    <section className="grid max-h-56 shrink-0 grid-cols-2 gap-6 overflow-y-auto border-t px-6 py-3">
      <div><h2 className="mb-1 text-xs font-semibold">Agent routines and fixed sequences · coming 7 days</h2>{data.coming.length ? data.coming.slice(0, 30).map((x) => row(x, false)) : <p className="text-xs text-muted-foreground">Nothing scheduled.</p>}</div>
      <div><h2 className="mb-1 text-xs font-semibold">Last 7 days</h2>{data.past.length ? data.past.map((x) => row(x, true)) : <p className="text-xs text-muted-foreground">Nothing ran.</p>}</div>
    </section>
  )
}

type Face = (name: string) => Agent | undefined

function Chip({ run, face, onAgent, compact, slot }: { run: Run; face: Face; onAgent: (n: string) => void; compact?: boolean; slot?: boolean }) {
  const a = face(run.agent)
  const s = STATE[run.state] ?? STATE.planned
  const hue = `var(--dept-${a?.department?.trim().toLowerCase().replace(/[^a-z]/g, "") || "none"}, var(--dept-none))`
  return (
    <button onClick={() => onAgent(run.agent)} title={`${run.routine} · ${run.time} · ${s.label}`}
      className={cn("flex w-full items-center gap-1.5 overflow-hidden rounded-md px-1.5 text-left transition-[filter] hover:brightness-110", compact ? "h-5" : slot ? "h-full" : "h-full min-h-6 items-start py-1", s.cls)}
      style={{ background: `color-mix(in oklch, ${hue} 22%, transparent)`, borderLeft: `3px solid ${hue}` }}>
      {!compact && a && <Character agent={a} size={14} still className="mt-px" />}
      <span className="min-w-0 flex-1 truncate text-[11px] leading-tight">
        <span className="font-medium">{compact ? `${run.time} ` : ""}{run.routine}</span>
        {slot && <span className="ml-1.5 text-[10px] opacity-70">{run.time}</span>}
        {!compact && !slot && <span className="block text-[10px] opacity-75">{run.time} · {s.label}</span>}
      </span>
      {run.state === "done" && <Check className="size-3 shrink-0 text-emerald-500" />}
      {run.state === "failed" && <X className="size-3 shrink-0 text-red-500" />}
    </button>
  )
}

function Grid({ days, face, onAgent, onDay }: { days: Day[]; face: Face; onAgent: (n: string) => void; onDay: (d: Date) => void }) {
  const scroller = useRef<HTMLDivElement>(null)
  useEffect(() => { scroller.current?.scrollTo({ top: HOUR * 6 }) }, [days.length])
  const [now, setNow] = useState(new Date())
  useEffect(() => { const t = setInterval(() => setNow(new Date()), 60000); return () => clearInterval(t) }, [])
  const cols = { gridTemplateColumns: `56px repeat(${days.length}, minmax(0, 1fr))` }
  const hasDeadline = days.some((d) => d.deadlines.length)
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid border-b" style={cols}>
        <div />
        {days.map((d) => (
          <button key={d.date} onClick={() => onDay(new Date(d.date + "T12:00"))} className="border-l px-2 py-2 text-left hover:bg-accent/40">
            <span className="text-[11px] text-muted-foreground uppercase">{d.weekday}</span>
            <span className={cn("ml-1.5 inline-grid size-7 place-items-center rounded-full text-sm font-semibold tabular-nums", d.today && "bg-sky-500 text-white")}>{d.day}</span>
          </button>
        ))}
      </div>
      {hasDeadline && (
        <div className="grid border-b" style={cols}>
          <div className="py-1 pr-2 text-right text-[10px] text-muted-foreground">Due</div>
          {days.map((d) => (
            <div key={d.date} className="space-y-0.5 border-l p-1">
              {d.deadlines.map((t, i) => <div key={i} className="truncate rounded bg-amber-500/20 px-1.5 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-300">{t}</div>)}
            </div>
          ))}
        </div>
      )}
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
        <div className="relative grid" style={{ ...cols, height: HOUR * 24 }}>
          <div className="relative">
            {Array.from({ length: 24 }, (_, h) => (
              <span key={h} className="absolute right-2 -translate-y-1/2 text-[10px] text-muted-foreground tabular-nums" style={{ top: h * HOUR }}>{h ? `${String(h).padStart(2, "0")}:00` : ""}</span>
            ))}
          </div>
          {days.map((d) => (
            <div key={d.date} className={cn("relative border-l", d.today && "bg-sky-500/[0.03]")}>
              {Array.from({ length: 24 }, (_, h) => <div key={h} className="absolute inset-x-0 border-t border-border/60" style={{ top: h * HOUR }} />)}
              {d.runs.map((run, i) => {
                const [hh, mm] = run.time.split(":").map(Number)
                return (
                  <div key={i} className="absolute inset-x-1 px-px" style={{ top: (hh + mm / 60) * HOUR + 1, height: HOUR / 2 - 2 }}>
                    <Chip run={run} face={face} onAgent={onAgent} slot />
                  </div>
                )
              })}
              {d.today && (
                <div className="absolute inset-x-0 z-10 flex items-center" style={{ top: (now.getHours() + now.getMinutes() / 60) * HOUR }}>
                  <span className="-ml-1 size-2 rounded-full bg-red-500" /><span className="h-px flex-1 bg-red-500" />
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

function Month({ days, anchor, face, onAgent, onDay }: { days: Day[]; anchor: Date; face: Face; onAgent: (n: string) => void; onDay: (d: Date) => void }) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid grid-cols-7 border-b">
        {days.slice(0, 7).map((d) => <div key={d.weekday} className="px-2 py-1.5 text-[11px] text-muted-foreground uppercase">{d.weekday}</div>)}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6">
        {days.map((d) => {
          const out = new Date(d.date + "T12:00").getMonth() !== anchor.getMonth()
          const shown = d.runs.slice(0, 3)
          return (
            <div key={d.date} className={cn("min-h-0 overflow-hidden border-r border-b p-1", out && "bg-muted/30")}>
              <button onClick={() => onDay(new Date(d.date + "T12:00"))}
                className={cn("mb-0.5 grid size-6 place-items-center rounded-full text-xs tabular-nums hover:bg-accent", d.today && "bg-sky-500 font-semibold text-white hover:bg-sky-500", out && "text-muted-foreground")}>{d.day}</button>
              <div className="space-y-0.5">
                {d.deadlines.map((t, i) => <div key={i} className="truncate rounded bg-amber-500/20 px-1 text-[10.5px] font-medium text-amber-700 dark:text-amber-300">{t}</div>)}
                {shown.map((run, i) => <Chip key={i} run={run} face={face} onAgent={onAgent} compact />)}
                {d.runs.length > shown.length && (
                  <button onClick={() => onDay(new Date(d.date + "T12:00"))} className="px-1 text-[10.5px] text-muted-foreground hover:text-foreground">+{d.runs.length - shown.length} more</button>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

function Agenda({ data, face, onAgent }: { data: Data; face: Face; onAgent: (n: string) => void }) {
  const days = useMemo(() => data.days.filter((d) => d.runs.length || d.deadlines.length), [data])
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl px-6 py-6">
        {!days.length ? <Empty icon={CalendarClock} title="Nothing in these four weeks" /> : days.map((d) => (
          <section key={d.date} className="mb-5 flex gap-5">
            <div className="w-16 shrink-0 pt-1 text-right">
              <div className={cn("text-xl font-semibold tabular-nums", d.today && "text-sky-500")}>{d.day}</div>
              <div className="text-[11px] text-muted-foreground uppercase">{d.weekday}</div>
            </div>
            <div className="min-w-0 flex-1 space-y-1 border-l pl-5">
              {d.deadlines.map((t, i) => (
                <div key={i} className="flex items-center gap-2 rounded-lg bg-amber-500/15 px-3 py-2 text-sm font-medium text-amber-700 dark:text-amber-300"><CalendarClock className="size-4" /> {t}</div>
              ))}
              {d.runs.map((run, i) => {
                const a = face(run.agent)
                const s = STATE[run.state] ?? STATE.planned
                return (
                  <button key={i} onClick={() => onAgent(run.agent)} className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm hover:bg-accent/60">
                    <span className="w-11 font-mono text-xs text-muted-foreground tabular-nums">{run.time}</span>
                    {a && <Character agent={a} size={20} still />}
                    <span className="flex-1 font-mono text-[13px]">{run.routine}</span>
                    <span className="text-xs text-muted-foreground">{a?.title}</span>
                    <span className={cn("w-20 text-right text-xs", run.state === "failed" ? "text-red-500" : run.state === "next" ? "text-sky-500" : run.state === "done" ? "text-emerald-500" : "text-muted-foreground")}>{s.label}</span>
                  </button>
                )
              })}
            </div>
          </section>
        ))}
      </div>
    </div>
  )
}
