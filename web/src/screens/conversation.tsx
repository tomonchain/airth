import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Check, ChevronDown, Copy, Workflow, X } from "lucide-react"
import { toast } from "sonner"
import { StreamText } from "@/components/beautifui/stream-text"
import { Character } from "@/components/character"
import { Typing } from "@/components/typing"
import { NeedCard } from "@/components/need-card"
import { OrderBox } from "@/components/order-box"
import { RunTimeline } from "@/components/run-timeline"
import { startOf, TaskThread } from "@/components/task-thread"
import { usePoll } from "@/hooks/use-poll"
import { usePrefs, WIDTH, ZOOM } from "@/hooks/use-prefs"
import { api, enc, type Agent, type Item, type Tasks, type Thread, type ThreadRun } from "@/lib/api"
import { stamp } from "@/lib/time"
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
export function Conversation({ realm, agent, onDone }: { realm: string; agent: Agent; onDone: () => void }) {
  const { prefs } = usePrefs()
  const thread = usePoll<Thread>(() => api(`realms/${enc(realm)}/agents/${enc(agent.name)}/thread`), 4000, [realm, agent.name])
  const tasks = usePoll<Tasks>(() => api(`realms/${enc(realm)}/tasks`), 8000, [realm])
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
  // What the founder just sent, until its task shows it.
  const [outbox, setOutbox] = useState<{ text: string; task: string | null; at: string } | null>(null)
  useEffect(() => setOutbox(null), [agent.name])
  const echoed = useCallback(() => setOutbox(null), [])

  const entries = useMemo(() => {
    const e: Entry[] = []
    for (const r of thread.data?.runs ?? []) if (r.at) e.push({ kind: "run", at: r.at, run: r })
    // A task's questions show in its own chat, not as items.
    for (const i of thread.data?.items ?? []) if (!(i.run && taskIds.includes(i.run) && !i.routine)) e.push({ kind: "item", at: i.created_at, item: i })
    for (const id of taskIds.slice(-8)) e.push({ kind: "task", at: startOf(id), id })
    // What waits for the founder comes last, as unread messages do.
    const waits = (x: Entry) => (x.kind === "item" && x.item.status === "pending" ? 1 : 0)
    return e.sort((a, b) => waits(a) - waits(b) || a.at.localeCompare(b.at))
  }, [thread.data, taskIds])

  // Each entry as chat bubbles: the agent on the left, the founder on the
  // right. An answered item is the agent's message, then the founder's answer.
  const units = useMemo(() => {
    const u: { side: "left" | "right" | "task"; key: string; node: (last: boolean) => React.ReactNode; isNew?: boolean }[] = []
    for (const x of entries) {
      if (x.kind === "task") u.push({ side: "task", key: `t${x.id}`, node: () => <TaskThread realm={realm} id={x.id} agent={agent} items={thread.data?.items ?? []} fresh={mine.has(x.id)} echo={outbox?.task === x.id ? outbox.text : undefined} onEcho={echoed} onChange={() => { thread.refresh(); tasks.refresh(); onDone() }} /> })
      else if (x.kind === "run") u.push({ side: "left", key: `r${x.run.run}`, node: () => <RunMessage run={x.run} stream={fresh(`r${x.run.run}`)} /> })
      else {
        const it = x.item
        u.push({ side: "left", key: `i${it.id}`, isNew: it.status === "pending", node: () => <ItemMessage realm={realm} item={it} onDone={refresh} stream={fresh(`i${it.id}`)} /> })
        if (it.status !== "pending") {
          const a = ANSWERED[it.status] ?? ANSWERED.done
          u.push({ side: "right", key: `a${it.id}`, node: () => <Mine text={a.reply(it)} at={it.created_at} /> })
        }
      }
    }
    return u
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries, realm, agent])
  const firstNew = units.findIndex((x) => x.isNew)
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
  function scrolled() {
    const el = scroller.current
    if (el) pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80
  }

  function refresh() { thread.refresh(); onDone() }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div ref={scroller} onScroll={scrolled} className="min-h-0 flex-1 overflow-y-auto">
        <div ref={content} className={cn("mx-auto px-6 pt-6 pb-4", WIDTH[prefs.width] ?? WIDTH.normal)} style={{ zoom: ZOOM[prefs.text] ?? 1 }}>
          {!thread.data ? (
            <p className="py-20 text-center text-sm text-muted-foreground">Loading…</p>
          ) : units.length === 0 ? (
            <Empty agent={agent} />
          ) : (
            units.map((x, i) => {
              const lastOfGroup = units[i + 1]?.side !== x.side || i + 1 === firstNew
              return (
                <div key={x.key}>
                  {i === firstNew && (
                    <div id="first-new" className="my-6 flex scroll-mt-6 items-center gap-3 text-[11px] font-semibold tracking-[0.14em] text-sky-500 uppercase">
                      <span className="h-px flex-1 bg-sky-500/40" /> New <span className="h-px flex-1 bg-sky-500/40" />
                    </div>
                  )}
                  {x.side === "task" ? x.node(lastOfGroup) : x.side === "left" ? (
                    <div className={cn("flex items-end gap-2.5", lastOfGroup ? "mb-5" : "mb-1.5", fresh(x.key) && "fade-up")}>
                      <span className="mb-6 w-8 shrink-0">{lastOfGroup && <Character agent={agent} size={32} still />}</span>
                      <div className="min-w-0 flex-1">{x.node(lastOfGroup)}</div>
                    </div>
                  ) : (
                    <div className={cn(lastOfGroup ? "mb-5" : "mb-1.5", fresh(x.key) && "fade-up")}>{x.node(lastOfGroup)}</div>
                  )}
                </div>
              )
            })
          )}
          {outbox && (
            <>
              <div className="mb-5 fade-up"><Mine text={outbox.text} at={outbox.at} /></div>
              <div className="mb-5 flex items-end gap-2.5 fade-up">
                <span className="w-8 shrink-0"><Character agent={{ ...agent, state: "working" }} size={32} /></span>
                <div className="inline-flex items-center rounded-2xl rounded-bl-md bg-bubble px-4 py-3.5"><Typing /></div>
              </div>
            </>
          )}
          {thread.data && busy && !outbox && <Working agent={agent} />}
        </div>
      </div>
      <div className={cn("mx-auto w-full px-6 pb-5", WIDTH[prefs.width] ?? WIDTH.normal)}>
        <OrderBox realm={realm} agent={agent.name} title={agent.title} task={closed ? undefined : latest} closed={closed} working={running}
          onSending={(text, task) => {
            if (task) mine.add(task)
            setOutbox({ text, task, at: new Date().toISOString() })
            pinned.current = true
            requestAnimationFrame(() => toBottom(true))
          }}
          onSent={(id) => {
            if (id) { mine.add(id); setStarted((s) => (s.includes(id) ? s : [...s, id])); setOutbox((o) => o && { ...o, task: id }) }
            tasks.refresh()
          }}
          onFailed={() => setOutbox(null)} />
      </div>
    </div>
  )
}

/** A routine of the agent's at work, as someone typing. */
function Working({ agent }: { agent: Agent }) {
  return (
    <div className="mb-5 flex items-end gap-2.5 fade-up">
      <span className="w-8 shrink-0"><Character agent={{ ...agent, state: "working" }} size={32} /></span>
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
      <h2 className="mt-5 text-lg font-semibold">{agent.title}</h2>
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

/** The founder's message, on the right, in their colour. */
function Mine({ text, at, note }: { text: string; at: string; note?: string }) {
  return (
    <div className="flex flex-col items-end">
      <div className="max-w-[75%] rounded-2xl rounded-br-md bg-me px-3.5 py-2 text-[13.5px] leading-relaxed whitespace-pre-wrap text-me-foreground shadow-sm">{text}</div>
      <div className="mt-1 pr-1 text-[11px] text-muted-foreground">{stamp(at)}{note ? ` · ${note}` : ""}</div>
    </div>
  )
}

function RunMessage({ run, stream }: { run: ThreadRun; stream?: boolean }) {
  const [open, setOpen] = useState(false)
  const failed = run.blocks.some((b) => b.outcome === "failed")
  return (
    <div>
      <div className="max-w-[85%] rounded-2xl rounded-bl-md bg-bubble px-4 py-3">
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
      <div className="max-w-[85%]">
        <NeedCard realm={realm} it={item} onDone={onDone} stream={stream} />
        <Meta at={item.created_at} copy={item.text ?? item.title} />
      </div>
    )
  }
  return (
    <div>
      <div className="max-w-[85%] rounded-2xl rounded-bl-md bg-bubble px-4 py-3">
        <p className="line-clamp-3 text-[13.5px] leading-relaxed whitespace-pre-wrap">{stream ? <StreamText text={item.kind === "approval" && item.text ? item.text : item.title} /> : item.kind === "approval" && item.text ? item.text : item.title}</p>
      </div>
      <Meta at={item.created_at} copy={item.text ?? item.title} />
    </div>
  )
}
