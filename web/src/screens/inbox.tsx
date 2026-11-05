import { useEffect, useMemo, useRef, useState } from "react"
import { ChevronDown, ChevronUp, CircleCheckBig, ExternalLink, ListChecks, MessageSquare, Search } from "lucide-react"
import { Character } from "@/components/character"
import { kindOf, NeedCard } from "@/components/need-card"
import { Button, buttonVariants } from "@/components/ui/button"
import { Kbd } from "@/components/ui/kbd"
import type { Agent, Item } from "@/lib/api"
import { ago, stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

// Most pressing first: what broke, what the check holds, what is due.
export function rank(it: Item) {
  if (it.kind === "incident") return 0
  if (it.flag === "blocking" || (it.kind === "deadline" && it.overdue)) return 1
  return { deadline: 2, approval: 3, prepared: 4, delayed: 5, report: 6, incident: 0 }[it.kind] ?? 6
}

const SECTIONS = [
  { label: "Urgent", ranks: [0, 1] },
  { label: "To decide", ranks: [2, 3] },
  { label: "To do yourself", ranks: [4] },
  { label: "Going out", ranks: [5] },
  { label: "To read", ranks: [6] },
]

const preview = (it: Item) => (it.kind === "approval" && it.text ? it.text : it.title).replace(/\s+/g, " ")

/** Needs you: what the team sent up, as a mailbox. The list by urgency on
    the left, the open item in full on the right; answering opens the next.
    J and K move, A, E and R answer. */
export function Inbox({ realm, items, agents, onDone, onAgent, onTask }: {
  realm: string; items: Item[]; agents: Agent[]; onDone: () => void; onAgent: (name: string) => void; onTask: (id: string) => void
}) {
  const [q, setQ] = useState("")
  const sorted = useMemo(
    () => [...items].sort((a, b) => rank(a) - rank(b) || a.created_at.localeCompare(b.created_at))
      .filter((it) => !q || `${preview(it)} ${it.agent} ${it.routine}`.toLowerCase().includes(q.toLowerCase())),
    [items, q],
  )
  const [sel, setSel] = useState<number | null>(null)
  const pos = sorted.findIndex((i) => i.id === sel)
  const open = pos >= 0 ? sorted[pos] : sorted[0] ?? null
  const at = open ? sorted.indexOf(open) : -1
  // Where to go once the open item is answered: the next one, else the one before.
  const [after, setAfter] = useState<number | null>(null)

  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || /^(INPUT|TEXTAREA|SELECT)$/.test((e.target as HTMLElement)?.tagName ?? "")) return
      const k = e.key.toLowerCase()
      if ((k === "j" || k === "arrowdown") && at < sorted.length - 1) { e.preventDefault(); setSel(sorted[at + 1].id) }
      if ((k === "k" || k === "arrowup") && at > 0) { e.preventDefault(); setSel(sorted[at - 1].id) }
    }
    addEventListener("keydown", on)
    return () => removeEventListener("keydown", on)
  }, [at, sorted])

  useEffect(() => {
    if (after != null && sorted.some((i) => i.id === after)) { setSel(after); setAfter(null) }
  }, [sorted, after])

  const agent = (name: string | null) => agents.find((a) => a.name === name)

  // Moving to the next item, the open one leaves by the top and the next
  // comes from the bottom; to the one before, the other way round.
  const last = useRef<{ item: Item; at: number } | null>(null)
  const [leaving, setLeaving] = useState<{ item: Item; dir: "down" | "up" } | null>(null)
  const [dir, setDir] = useState<"down" | "up" | null>(null)
  useEffect(() => {
    const prev = last.current
    last.current = open ? { item: open, at } : null
    if (!prev || !open || prev.item.id === open.id) { if (!prev) setDir(null); return }
    const still = sorted.findIndex((i) => i.id === prev.item.id)
    const d = still === -1 || at > still ? "down" : "up"
    setDir(d)
    setLeaving({ item: prev.item, dir: d })
    const t = setTimeout(() => setLeaving(null), 320)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open?.id])

  return (
    <>
      <div className="flex w-[340px] shrink-0 flex-col border-r">
        <div className="flex items-baseline justify-between px-4 pt-4 pb-1">
          <h1 className="text-base font-semibold">Needs you</h1>
          <span className="text-xs text-muted-foreground tabular-nums">{items.length}</span>
        </div>
        <div className="px-3 py-2">
          <label className="flex h-9 items-center gap-2 rounded-lg bg-muted px-3 text-sm text-muted-foreground focus-within:ring-2 focus-within:ring-ring/40">
            <Search className="size-4" />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search" className="flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
          </label>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-3">
          {SECTIONS.map((s) => {
            const list = sorted.filter((i) => s.ranks.includes(rank(i)))
            if (!list.length) return null
            return (
              <section key={s.label} className="mt-2">
                <h2 className="flex items-center justify-between px-2.5 pt-1 pb-1.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
                  {s.label} <span className="tabular-nums">{list.length}</span>
                </h2>
                <ul className="space-y-0.5">
                  {list.map((it) => {
                    const a = agent(it.agent)
                    const k = kindOf(it)
                    return (
                      <li key={it.id}>
                        <button
                          onClick={() => setSel(it.id)}
                          className={cn("flex w-full gap-3 rounded-xl px-2.5 py-2.5 text-left transition-colors hover:bg-accent/70", open?.id === it.id && "bg-accent")}
                        >
                          {a ? <Character agent={a} size={30} still className="mt-0.5" /> : <span className="grid size-[30px] shrink-0 place-items-center rounded-full bg-muted"><k.icon className="size-4" /></span>}
                          <span className="min-w-0 flex-1">
                            <span className="flex items-center gap-2">
                              <span className="truncate text-[13px] font-medium">{a?.title ?? "Airth"}</span>
                              <span className={cn("inline-flex shrink-0 items-center gap-1 rounded px-1 text-[10.5px] font-medium", k.tone)}><k.icon className="size-3" />{k.label}</span>
                              <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">{stamp(it.created_at)}</span>
                            </span>
                            <span className="mt-0.5 line-clamp-2 text-[12.5px] leading-snug text-muted-foreground">{preview(it)}</span>
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </section>
            )
          })}
        </div>
      </div>

      {!open ? (
        <div className="flex flex-1 flex-col items-center justify-center text-center">
          <CircleCheckBig className="size-10 text-emerald-500/80" />
          <p className="mt-4 text-base font-medium">All clear</p>
          <p className="mt-1 text-sm text-muted-foreground">Nothing waits for you. What the team sends up lands here.</p>
        </div>
      ) : (
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-12 shrink-0 items-center gap-1 border-b px-4">
            <Button variant="ghost" size="icon-sm" disabled={at <= 0} onClick={() => setSel(sorted[at - 1].id)} aria-label="Previous (K)"><ChevronUp /></Button>
            <Button variant="ghost" size="icon-sm" disabled={at >= sorted.length - 1} onClick={() => setSel(sorted[at + 1].id)} aria-label="Next (J)"><ChevronDown /></Button>
            <span className="ml-1 text-xs text-muted-foreground tabular-nums">{at + 1} of {sorted.length}</span>
            <span className="ml-auto hidden items-center gap-1 text-xs text-muted-foreground md:flex">
              <Kbd>J</Kbd><Kbd>K</Kbd> to move
            </span>
          </div>
          <div className="relative min-h-0 flex-1 overflow-hidden">
            {leaving && (
              <div
                key={`leave-${leaving.item.id}`}
                className={cn("pointer-events-none absolute inset-0 overflow-hidden bg-surface duration-300 ease-in-out animate-out fill-mode-forwards",
                  leaving.dir === "down" ? "slide-out-to-top-full" : "slide-out-to-bottom-full")}
              >
                <div className="mx-auto max-w-2xl px-8 py-7">
                  <Header it={leaving.item} agent={agent(leaving.item.agent)} onAgent={onAgent} onTask={onTask} />
                  <NeedCard realm={realm} it={leaving.item} page onDone={() => {}} still />
                </div>
              </div>
            )}
            <div
              key={`open-${open.id}`}
              className={cn("absolute inset-0 overflow-y-auto bg-surface",
                dir && "duration-300 ease-in-out animate-in", dir === "down" && "slide-in-from-bottom-full", dir === "up" && "slide-in-from-top-full")}
            >
              <div className="mx-auto max-w-2xl px-8 py-7">
                <Header it={open} agent={agent(open.agent)} onAgent={onAgent} onTask={onTask} />
                <NeedCard
                  key={`${open.id}-${open.revision}`} realm={realm} it={open} page
                  onDone={() => { const n = sorted[at + 1] ?? sorted[at - 1]; setAfter(n?.id ?? null); onDone() }}
                />
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  )
}

function Header({ it, agent, onAgent, onTask }: { it: Item; agent?: Agent; onAgent: (name: string) => void; onTask: (id: string) => void }) {
  const k = kindOf(it)
  return (
    <header>
      <span className={cn("inline-flex items-center gap-1.5 rounded-md px-2 py-0.5 text-xs font-medium", k.tone)}><k.icon className="size-3.5" /> {k.label}</span>
      {!(it.kind === "approval" && it.text) && <h2 className="mt-3 text-xl leading-snug font-semibold tracking-tight">{it.title}</h2>}
      <div className="mt-3 flex items-center gap-2.5 text-sm">
        {agent && <Character agent={agent} size={26} still />}
        <span className="font-medium">{agent?.title ?? "Airth"}</span>
        {it.routine && <span className="font-mono text-xs text-muted-foreground">{it.routine}</span>}
        <span className="text-xs text-muted-foreground">· {ago(it.created_at)}</span>
        <span className="ml-auto flex items-center gap-1">
          {it.source?.link && (
            <a href={it.source.link} target="_blank" rel="noreferrer" className={cn(buttonVariants({ variant: "ghost", size: "sm" }), "text-muted-foreground")}>
              <ExternalLink /> Source
            </a>
          )}
          {/* What a task asked: the task itself, with its plan and what it did so far. A routine's item has its run in Routines. */}
          {it.run && !it.routine && <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => onTask(it.run!)}><ListChecks /> Its task</Button>}
          {agent && <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => onAgent(agent.name)}><MessageSquare /> Conversation</Button>}
        </span>
      </div>
    </header>
  )
}
