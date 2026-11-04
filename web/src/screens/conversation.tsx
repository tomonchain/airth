import { useEffect, useMemo, useRef, useState } from "react"
import { Check, ChevronDown, Copy, Workflow, X } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { NeedCard } from "@/components/need-card"
import { OrderBox } from "@/components/order-box"
import { RunTimeline } from "@/components/run-timeline"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Item, type Tasks, type Thread, type ThreadRun } from "@/lib/api"
import { stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

type Entry =
  | { kind: "run"; at: string; run: ThreadRun }
  | { kind: "item"; at: string; item: Item }
  | { kind: "order"; at: string; text: string; status?: string }

const ANSWERED: Record<string, { label: string; icon: typeof Check; cls: string }> = {
  approved: { label: "Approved", icon: Check, cls: "text-emerald-500" },
  done: { label: "Done", icon: Check, cls: "text-emerald-500" },
  rejected: { label: "Rejected", icon: X, cls: "text-muted-foreground" },
  failed: { label: "Failed", icon: X, cls: "text-red-500" },
}

/** One agent's work as a conversation: its runs, what it sent up, the
    founder's answers and orders; newest at the bottom, where the founder
    writes. */
export function Conversation({ realm, agent, onDone }: { realm: string; agent: Agent; onDone: () => void }) {
  const thread = usePoll<Thread>(() => api(`realms/${enc(realm)}/agents/${enc(agent.name)}/thread`), 4000, [realm, agent.name])
  const tasks = usePoll<Tasks>(() => api(`realms/${enc(realm)}/tasks`), 8000, [realm])
  const [sent, setSent] = useState<{ text: string; at: string }[]>([])
  useEffect(() => setSent([]), [agent.name])

  const entries = useMemo(() => {
    const e: Entry[] = []
    for (const r of thread.data?.runs ?? []) if (r.at) e.push({ kind: "run", at: r.at, run: r })
    for (const i of thread.data?.items ?? []) e.push({ kind: "item", at: i.created_at, item: i })
    for (const group of Object.values(tasks.data ?? {}))
      for (const t of group ?? []) if (t.agent === agent.name && t.order) e.push({ kind: "order", at: t.updated_at ?? t.at ?? "", text: t.order, status: t.status })
    for (const s of sent) if (!e.some((x) => x.kind === "order" && x.text === s.text)) e.push({ kind: "order", at: s.at, text: s.text, status: "starting" })
    return e.sort((a, b) => a.at.localeCompare(b.at))
  }, [thread.data, tasks.data, sent, agent.name])

  const firstNew = entries.findIndex((x) => x.kind === "item" && x.item.status === "pending")
  const end = useRef<HTMLDivElement>(null)
  const [stuck, setStuck] = useState(false)
  useEffect(() => setStuck(false), [agent.name])
  useEffect(() => {
    if (!thread.data || stuck) return
    const target = document.getElementById("first-new") ?? end.current
    target?.scrollIntoView({ block: target === end.current ? "end" : "start" })
    setStuck(true)
  }, [thread.data, stuck])

  const refresh = () => { thread.refresh(); onDone() }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-6 pt-6 pb-4">
          {!thread.data ? (
            <p className="py-20 text-center text-sm text-muted-foreground">Loading…</p>
          ) : entries.length === 0 ? (
            <Empty agent={agent} />
          ) : (
            entries.map((x, i) => (
              <div key={i}>
                {i === firstNew && (
                  <div id="first-new" className="my-6 flex scroll-mt-6 items-center gap-3 text-[11px] font-semibold tracking-[0.14em] text-sky-500 uppercase">
                    <span className="h-px flex-1 bg-sky-500/40" /> New <span className="h-px flex-1 bg-sky-500/40" />
                  </div>
                )}
                {x.kind === "order" ? <Order text={x.text} at={x.at} status={x.status} />
                  : x.kind === "run" ? <RunMessage run={x.run} />
                  : <ItemMessage realm={realm} item={x.item} onDone={refresh} />}
              </div>
            ))
          )}
          <div ref={end} />
        </div>
      </div>
      <div className="mx-auto w-full max-w-3xl px-6 pb-5">
        <OrderBox realm={realm} agent={agent.name} title={agent.title}
          onSent={(text) => { setSent((s) => [...s, { text, at: new Date().toISOString() }]); setTimeout(() => end.current?.scrollIntoView({ block: "end" }), 50); tasks.refresh() }} />
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
    <div className="mt-1.5 mb-5 flex items-center gap-1 pl-1 text-xs text-muted-foreground">
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

function Order({ text, at, status }: { text: string; at: string; status?: string }) {
  return (
    <div className="flex flex-col items-end">
      <div className="max-w-[80%] rounded-2xl rounded-br-md bg-primary px-4 py-2.5 text-[15px] leading-relaxed whitespace-pre-wrap text-primary-foreground">{text}</div>
      <div className="mt-1.5 mb-5 pr-1 text-xs text-muted-foreground">{stamp(at)}{status ? ` · ${status}` : ""}</div>
    </div>
  )
}

function RunMessage({ run }: { run: ThreadRun }) {
  const [open, setOpen] = useState(false)
  const failed = run.blocks.some((b) => b.outcome === "failed")
  return (
    <div>
      <div className="max-w-[85%] rounded-2xl rounded-bl-md bg-bubble px-4 py-3">
        <div className="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
          <Workflow className="size-3.5" /> <span className="font-mono">{run.routine}</span>
          {failed && <span className="rounded bg-red-500/15 px-1.5 py-px text-[11px] font-medium text-red-500">failed</span>}
        </div>
        <p className="text-[15px] leading-relaxed">{run.sentence}</p>
        <button onClick={() => setOpen(!open)} className="mt-2 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <ChevronDown className={cn("size-3.5 transition-transform", open && "rotate-180")} /> {open ? "Hide the run" : `See the run, ${run.blocks.length} steps`}
        </button>
        {open && <div className="mt-3 border-t pt-3"><RunTimeline run={run} /></div>}
      </div>
      <Meta at={run.at ?? ""} copy={run.sentence} />
    </div>
  )
}

function ItemMessage({ realm, item, onDone }: { realm: string; item: Item; onDone: () => void }) {
  if (item.status === "pending") {
    return (
      <div className="max-w-[85%]">
        <NeedCard realm={realm} it={item} onDone={onDone} />
        <Meta at={item.created_at} copy={item.text ?? item.title} />
      </div>
    )
  }
  const a = ANSWERED[item.status] ?? ANSWERED.done
  return (
    <div>
      <div className="max-w-[85%] rounded-2xl rounded-bl-md bg-bubble px-4 py-3 opacity-80">
        <p className="line-clamp-3 text-[15px] leading-relaxed whitespace-pre-wrap">{item.kind === "approval" && item.text ? item.text : item.title}</p>
      </div>
      <Meta at={item.created_at} copy={item.text ?? item.title}>
        <span className={cn("inline-flex items-center gap-1", a.cls)}><a.icon className="size-3.5" /> {a.label}</span>
      </Meta>
    </div>
  )
}
