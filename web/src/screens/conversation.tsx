import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { ArrowDown, Check, ChevronDown, Copy, Workflow, X } from "lucide-react"
import { toast } from "sonner"
import { StreamText } from "@/components/beautifui/stream-text"
import { Character } from "@/components/character"
import { FoldedLine, OpenLine, useFold } from "@/components/fold-line"
import { Typing } from "@/components/typing"
import { NeedCard } from "@/components/need-card"
import { OrderBox } from "@/components/order-box"
import { RunTimeline } from "@/components/run-timeline"
import { startOf, TaskThread, type Between } from "@/components/task-thread"
import { usePoll } from "@/hooks/use-poll"
import { usePrefs, WIDTH, ZOOM } from "@/hooks/use-prefs"
import { api, called, enc, type Agent, type Item, type Tasks, type Thread, type ThreadRun } from "@/lib/api"
import { stamp } from "@/lib/time"
import { chronological, during, moment, type Span } from "@/lib/timeline"
import { cn } from "@/lib/utils"

type Entry =
  | { kind: "run"; at: string; run: ThreadRun }
  | { kind: "item"; at: string; item: Item }
  | { kind: "task"; at: string; id: string }

// What the founder's answer reads like, on the right.
const ANSWERED: Record<string, { label: string; icon: typeof Check; cls: string; reply: (it: Item) => string }> = {
  approved: { label: "Approved", icon: Check, cls: "text-emerald-500", reply: () => "Approved." },
  done: { label: "Done", icon: Check, cls: "text-emerald-500", reply: (it) => (it.kind === "approval" ? "Approved. Done." : it.kind === "prepared" ? "Done on my side." : "Seen.") },
  rejected: { label: "Rejected", icon: X, cls: "text-muted-foreground", reply: (it) => (it.kind === "delayed" ? "Cancel it." : "Rejected.") },
  failed: { label: "Failed", icon: X, cls: "text-red-500", reply: () => "Approved, but it failed." },
}

/** One agent's work as a conversation: its runs, what it sent up, the
    founder's answers and orders; newest at the bottom, where the founder
    writes. */
export function Conversation({ realm, agent, agents = [], onAgent, tasks: list, onDone, top }: {
  realm: string; agent: Agent; agents?: Agent[]; onAgent?: (name: string) => void; tasks: Tasks | null; onDone: () => void
  /** At the top of the page: on a phone, it starts below Airth's floating mark. */
  top?: boolean
}) {
  const { prefs } = usePrefs()
  const thread = usePoll<Thread>(() => api(`realms/${enc(realm)}/agents/${enc(agent.name)}/thread`), 4000, [realm, agent.name])
  // The app's list, refreshed every few seconds: not asked again here.
  const tasks = { data: list, refresh: onDone }
  // Tasks started here and not yet listed by the server.
  const [started, setStarted] = useState<string[]>([])
  useEffect(() => setStarted([]), [agent.name])
  const taskIds = useMemo(() => {
    const ids = new Set<string>(started)
    for (const group of Object.values(tasks.data ?? {})) for (const t of group ?? []) if (t.agent === agent.name) ids.add(t.id)
    return [...ids].sort()
  }, [tasks.data, started, agent.name])
  const running = (tasks.data?.running ?? []).some((t) => t.agent === agent.name) || started.some((id) => !(tasks.data?.done ?? []).some((t) => t.id === id))
  const latest = taskIds[taskIds.length - 1]
  // A task whose change was adopted or thrown away is closed: the next
  // message starts a new one (the server refuses to go on with it).
  const closedBy = Object.values(tasks.data ?? {}).flat().find((t) => t.id === latest)?.change
  const closed = closedBy === "adopted" || closedBy === "discarded" ? closedBy : null
  const [mine] = useState(() => new Set<string>())
  // What the founder just sent, until its task shows it: the message that
  // started the agent, then those written while it works, in their order.
  type Sent = { text: string; task: string | null; at: string; queued?: boolean }
  const [outbox, setOutbox] = useState<Sent[]>([])
  useEffect(() => setOutbox([]), [agent.name])
  const echoed = useCallback((text: string) => setOutbox((o) => { const i = o.findIndex((s) => s.text === text); return i < 0 ? o : [...o.slice(0, i), ...o.slice(i + 1)] }), [])

  const { entries, inside } = useMemo(() => {
    const events: Entry[] = []
    for (const r of thread.data?.runs ?? []) if (r.at) events.push({ kind: "run", at: r.at, run: r })
    // A task's questions show in its own chat, not as items.
    // A report is read in Needs you, on the phone and on Telegram; in the
    // conversation it came between the founder and the agent (decision 134).
    for (const i of thread.data?.items ?? []) if (!(i.run && taskIds.includes(i.run) && !i.routine) && i.kind !== "report") events.push({ kind: "item", at: i.created_at, item: i })
    // A task goes at its start; what came while it went on is shown inside
    // it, between its messages, each at its time (decision 148).
    const listed = new Map(Object.values(tasks.data ?? {}).flat().map((t) => [t.id, t.at ?? t.updated_at]))
    const shown = taskIds.slice(-8)
    const spans: Span[] = shown.flatMap((id) => {
      const from = moment(startOf(id))
      return from === null ? [] : [{ id, from, to: moment(listed.get(id)) }]
    })
    const e: Entry[] = shown.map((id) => ({ kind: "task", at: startOf(id), id }))
    const inside = new Map<string, Entry[]>()
    for (const x of events) {
      const task = during(spans, moment(x.at))
      if (task) inside.set(task, [...(inside.get(task) ?? []), x])
      else e.push(x)
    }
    for (const [task, xs] of inside) inside.set(task, chronological(xs))
    // In time order, as a chat is: what waits for the founder stays where
    // it was sent, marked new, not moved after later messages.
    return { entries: chronological(e), inside }
  }, [thread.data, taskIds, tasks.data])

  // Each entry as chat bubbles: the agent on the left, the founder on the
  // right. An answered item is the agent's message, then the founder's answer.
  // "fold": an answered item, which draws its own line.
  type Unit = { side: "left" | "right" | "task" | "fold"; key: string; at: string; waiting?: boolean; node: (last: boolean) => React.ReactNode }
  // A run or an item as bubbles: an answered item is the agent's message,
  // then the founder's answer, folded into one line unless it is the last.
  function eventUnits(x: Entry, last = false): Unit[] {
    if (x.kind === "run") return [{ side: "left", key: `r${x.run.run}`, at: x.at, node: () => <RunMessage run={x.run} stream={fresh(`r${x.run.run}`)} /> }]
    if (x.kind !== "item") return []
    const it = x.item
    const ask: Unit = { side: "left", key: `i${it.id}`, at: x.at, waiting: it.status === "pending", node: () => <ItemMessage realm={realm} item={it} onDone={refresh} stream={fresh(`i${it.id}`)} /> }
    if (it.status === "pending") return [ask]
    const a = ANSWERED[it.status] ?? ANSWERED.done
    const answer: Unit = { side: "right", key: `a${it.id}`, at: x.at, node: () => <Mine text={a.reply(it)} at={it.created_at} /> }
    return [{ side: "fold", key: `i${it.id}`, at: x.at, node: () => (
      <Answered storeKey={`${realm}:item:${it.id}`} byDefault={last} request={it.title} result={a.label} failed={it.status === "failed"} when={stamp(it.created_at)}>
        {place(ask, true)}{place(answer, true)}
      </Answered>
    ) }]
  }
  const units = useMemo(() => {
    const u: Unit[] = []
    // The newest task: its last exchange stays open.
    const lastTask = entries.filter((e) => e.kind === "task").at(-1)?.id
    for (const x of entries) {
      if (x.kind === "task") {
        // Who passed it on, and what it passed on (decision 120).
        const all = Object.values(tasks.data ?? {}).flat()
        const titleOf = (name: string | null | undefined) => { const a = agents.find((g) => g.name === name); return a ? called(a) : name ?? "An agent" }
        const self = all.find((t) => t.id === x.id)
        const passed = all.filter((t) => t.from?.task === x.id && t.agent).map((t) => ({ id: t.id, agent: t.agent!, title: titleOf(t.agent), order: t.order ?? "", status: t.status }))
        const between: Between[] = (inside.get(x.id) ?? []).flatMap((e) => eventUnits(e)).map((b) => ({
          key: b.key, at: moment(b.at), side: b.side === "right" ? "right" : "left", waiting: b.waiting, node: (last) => place(b, last),
        }))
        u.push({ side: "task", key: `t${x.id}`, at: x.at, node: () => <TaskThread realm={realm} id={x.id} agent={agent} items={thread.data?.items ?? []} fresh={mine.has(x.id)} echoes={outbox.filter((s) => s.task === x.id).map((s) => s.text)} onEcho={echoed} onChange={() => { thread.refresh(); tasks.refresh(); onDone() }}
          from={self?.from ? titleOf(self.from.agent) : undefined} passed={passed} onAgent={onAgent} between={between} latest={x.id === lastTask} /> })
      }
      else u.push(...eventUnits(x, x === entries.at(-1)))
    }
    return u
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries, inside, realm, agent, tasks.data, agents])
  // The first of what waits for the founder, wherever it shows (inside a
  // task or not): the line marking what is new goes above it.
  const firstNew = useMemo(() => {
    const waiting = [...entries, ...[...inside.values()].flat()].filter((x) => x.kind === "item" && x.item.status === "pending")
    const first = chronological(waiting)[0]
    return first?.kind === "item" ? `i${first.item.id}` : null
  }, [entries, inside])
  // What was there when the thread opened shows at once; what arrives while
  // the founder watches comes in, the agent's words written as they stream.
  const known = useRef<Set<string> | null>(null)
  useEffect(() => { known.current = null }, [agent.name])
  if (thread.data && !known.current) known.current = new Set(units.map((u) => u.key))
  function fresh(key: string) { return !!known.current && !known.current.has(key) }
  // A routine at work (a task at work shows in its own chat).
  const busy = agent.state === "working" && !running
  // Like a chat: the thread opens at its newest message and stays there as
  // messages and tasks load in, unless the founder scrolled up to read.
  const scroller = useRef<HTMLDivElement>(null)
  const content = useRef<HTMLDivElement>(null)
  const pinned = useRef(true)
  const toBottom = (smooth = false) => scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: smooth ? "smooth" : "auto" })
  useEffect(() => { pinned.current = true; toBottom() }, [agent.name])
  useEffect(() => {
    if (!content.current) return
    const o = new ResizeObserver(() => pinned.current && toBottom())
    o.observe(content.current)
    return () => o.disconnect()
  }, [])
  // Scrolled up to read: a button brings the newest back.
  const [away, setAway] = useState(false)
  function scrolled() {
    const el = scroller.current
    if (el) pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80
    setAway(!pinned.current)
  }

  // The box floats over the thread, which scrolls under it: the thread
  // ends with room for the box, as tall as the box is now.
  const dock = useRef<HTMLDivElement>(null)
  const [room, setRoom] = useState(96)
  useEffect(() => {
    if (!dock.current) return
    const o = new ResizeObserver(([e]) => setRoom(Math.ceil(e.target.getBoundingClientRect().height)))
    o.observe(dock.current)
    return () => o.disconnect()
  }, [])

  function refresh() { thread.refresh(); onDone() }

  // A run's or an item's bubbles, on their side, in the conversation or
  // inside a task; the line marking what is new above the first that waits.
  function place(x: Unit, lastOfGroup: boolean) {
    return (
      <>
        {x.key === firstNew && (
          <div id="first-new" className="my-6 flex scroll-mt-6 items-center gap-3 text-[11px] font-semibold tracking-[0.14em] text-sky-500 uppercase">
            <span className="h-px flex-1 bg-sky-500/40" /> New <span className="h-px flex-1 bg-sky-500/40" />
          </div>
        )}
        {x.side === "fold" ? x.node(lastOfGroup) : x.side === "left" ? (
          <div className={cn("flex items-end gap-2.5", lastOfGroup ? "mb-5" : "mb-1.5", fresh(x.key) && "fade-up")}>
            <span className="mb-6 hidden w-8 shrink-0 md:block">{lastOfGroup && <Character agent={agent} size={32} still />}</span>
            <div className="min-w-0 flex-1">{x.node(lastOfGroup)}</div>
          </div>
        ) : (
          <div className={cn(lastOfGroup ? "mb-5" : "mb-1.5", fresh(x.key) && "fade-up")}>{x.node(lastOfGroup)}</div>
        )}
      </>
    )
  }

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
      <div ref={scroller} onScroll={scrolled} className="no-dock min-h-0 flex-1 overflow-y-auto">
        <div ref={content} className={cn("mx-auto px-3 pt-4 [overflow-wrap:anywhere] md:px-6 md:pt-6", top && "max-md:pt-[68px]", WIDTH[prefs.width] ?? WIDTH.normal)} style={{ zoom: ZOOM[prefs.text] ?? 1, paddingBottom: room + 16 }}>
          {!thread.data ? (
            <p className="py-20 text-center text-sm text-muted-foreground">Loading…</p>
          ) : units.length === 0 ? (
            <Empty agent={agent} />
          ) : (
            units.map((x, i) => {
              const lastOfGroup = units[i + 1]?.side !== x.side || units[i + 1]?.key === firstNew
              return <div key={x.key}>{x.side === "task" ? x.node(lastOfGroup) : place(x, lastOfGroup)}</div>
            })
          )}
          {outbox.map((s, i) => (
            <div key={`${s.at}${i}`} className="mb-5 fade-up"><Mine text={s.text} at={s.at} note={s.queued ? "Waiting: it reads this at its next step" : undefined} /></div>
          ))}
          {outbox.length > 0 && !outbox[0].queued && (
            <div className="mb-5 flex items-end gap-2.5 fade-up">
              <span className="hidden w-8 shrink-0 md:block"><Character agent={{ ...agent, state: "working" }} size={32} /></span>
              <div className="inline-flex items-center rounded-2xl rounded-bl-md bg-bubble px-4 py-3.5"><Typing /></div>
            </div>
          )}
          {thread.data && busy && !outbox.length && <Working agent={agent} />}
        </div>
      </div>
      <div ref={dock} className={cn("dock-lift pointer-events-none absolute inset-x-0 bottom-0 mx-auto w-full px-3 pb-3 md:px-6 md:pb-5 [&>*]:pointer-events-auto", WIDTH[prefs.width] ?? WIDTH.normal)}>
        {away && (
          <button onClick={() => { pinned.current = true; setAway(false); toBottom(true) }} aria-label="Newest messages"
            className="absolute -top-11 right-4 grid size-8 place-items-center md:right-auto md:left-1/2 md:-translate-x-1/2 rounded-full border bg-popover text-muted-foreground shadow-md fade-up hover:text-foreground">
            <ArrowDown className="size-4" />
          </button>
        )}
        <OrderBox realm={realm} agent={agent.name} title={called(agent)} task={closed ? undefined : latest} closed={closed} working={running}
          tools={[...(agent.integrations ?? []), ...(agent.skills ?? [])]} others={agents.filter((a) => a.name !== agent.name)} onOther={onAgent}
          onSending={(text, task) => {
            if (task) mine.add(task)
            setOutbox((o) => [...o, { text, task, at: new Date().toISOString(), queued: running || o.length > 0 }])
            pinned.current = true
            requestAnimationFrame(() => toBottom(true))
          }}
          onSent={(id) => {
            if (id) { mine.add(id); setStarted((s) => (s.includes(id) ? s : [...s, id])); setOutbox((o) => o.map((s) => (s.task ? s : { ...s, task: id }))) }
            tasks.refresh()
          }}
          onFailed={() => setOutbox((o) => o.slice(0, -1))} />
      </div>
    </div>
  )
}

/** A routine of the agent's at work, as someone typing. */
function Working({ agent }: { agent: Agent }) {
  return (
    <div className="mb-5 flex items-end gap-2.5 fade-up">
      <span className="hidden w-8 shrink-0 md:block"><Character agent={{ ...agent, state: "working" }} size={32} /></span>
      <div className="inline-flex items-center gap-3 rounded-2xl rounded-bl-md bg-bubble px-4 py-3">
        <Typing />
        <span className="text-xs text-muted-foreground">Running {agent.routines.find((r) => r.schedule)?.name ?? "a routine"}</span>
      </div>
    </div>
  )
}

function Empty({ agent }: { agent: Agent }) {
  return (
    <div className="flex flex-col items-center py-24 text-center">
      <Character agent={agent} size={96} />
      <h2 className="mt-5 text-lg font-semibold">{called(agent)}</h2>
      <p className="mt-1.5 max-w-sm text-sm text-muted-foreground">{agent.job ?? "No job written yet."}</p>
      <p className="mt-4 text-sm text-muted-foreground">Nothing yet. Give it an order below.</p>
    </div>
  )
}

function Meta({ at, copy, children }: { at: string; copy?: string; children?: React.ReactNode }) {
  return (
    <div className="mt-1 flex items-center gap-1 pl-1 text-[11px] text-muted-foreground">
      {copy && (
        <button className="grid size-6 place-items-center rounded-md hover:bg-accent hover:text-foreground" aria-label="Copy"
          onClick={() => { navigator.clipboard.writeText(copy); toast.success("Copied") }}>
          <Copy className="size-3.5" />
        </button>
      )}
      <span className="px-1">{stamp(at)}</span>
      {children}
    </div>
  )
}

/** Something the agent sent up and the founder answered: one line, what
    it asked and the answer, unless open (by default or by the founder). */
function Answered({ storeKey, byDefault, request, result, failed, when, children }: {
  storeKey: string; byDefault: boolean; request: string; result: string; failed: boolean; when: string; children: React.ReactNode
}) {
  const [open, toggle] = useFold(storeKey, byDefault)
  const outcome = failed ? "failed" : "done"
  if (!open) return <FoldedLine request={request} result={result} outcome={outcome} when={when} onOpen={toggle} />
  return <><OpenLine result={result} outcome={outcome} when={when} onFold={toggle} />{children}</>
}

/** The founder's message, on the right, in their colour. */
function Mine({ text, at, note }: { text: string; at: string; note?: string }) {
  return (
    <div className="flex flex-col items-end">
      <div className="max-w-[88%] rounded-2xl rounded-br-md bg-me md:max-w-[75%] px-3.5 py-2 text-[13.5px] leading-relaxed whitespace-pre-wrap text-me-foreground shadow-sm">{text}</div>
      <div className="mt-1 pr-1 text-[11px] text-muted-foreground">{stamp(at)}{note ? ` · ${note}` : ""}</div>
    </div>
  )
}

function RunMessage({ run, stream }: { run: ThreadRun; stream?: boolean }) {
  const [open, setOpen] = useState(false)
  const failed = run.blocks.some((b) => b.outcome === "failed")
  return (
    <div>
      <div className="rounded-2xl rounded-bl-md bg-bubble px-4 py-3 md:max-w-[85%]">
        <div className="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
          <Workflow className="size-3.5" /> <span className="font-mono">{run.routine}</span>
          {failed && <span className="rounded bg-red-500/15 px-1.5 py-px text-[11px] font-medium text-red-500">failed</span>}
        </div>
        <p className="text-[13.5px] leading-relaxed">{stream ? <StreamText text={run.sentence} /> : run.sentence}</p>
        <button onClick={() => setOpen(!open)} className="mt-2 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <ChevronDown className={cn("size-3.5 transition-transform", open && "rotate-180")} /> {open ? "Hide the run" : `See the run, ${run.blocks.length} steps`}
        </button>
        {open && <div className="mt-3 border-t pt-3"><RunTimeline run={run} /></div>}
      </div>
      <Meta at={run.at ?? ""} copy={run.sentence} />
    </div>
  )
}

function ItemMessage({ realm, item, onDone, stream }: { realm: string; item: Item; onDone: () => void; stream?: boolean }) {
  if (item.status === "pending") {
    return (
      <div className="md:max-w-[85%]">
        <NeedCard realm={realm} it={item} onDone={onDone} stream={stream} />
        <Meta at={item.created_at} copy={item.text ?? item.title} />
      </div>
    )
  }
  return (
    <div>
      <div className="rounded-2xl rounded-bl-md bg-bubble px-4 py-3 md:max-w-[85%]">
        <p className="line-clamp-3 text-[13.5px] leading-relaxed whitespace-pre-wrap">{stream ? <StreamText text={item.kind === "approval" && item.text ? item.text : item.title} /> : item.kind === "approval" && item.text ? item.text : item.title}</p>
      </div>
      <Meta at={item.created_at} copy={item.text ?? item.title} />
    </div>
  )
}
