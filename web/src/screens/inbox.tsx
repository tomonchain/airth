import { useEffect, useMemo, useRef, useState } from "react"
import { ChevronDown, ChevronUp, CircleCheckBig, ExternalLink, ListChecks, MessageSquare } from "lucide-react"
import { Back, SideHead, SideSearch } from "@/components/page"
import { Character } from "@/components/character"
import { kindOf, NeedCard } from "@/components/need-card"
import { Button, buttonVariants } from "@/components/ui/button"
import { Kbd } from "@/components/ui/kbd"
import { toast } from "sonner"
import { useNarrow } from "@/hooks/use-narrow"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Item } from "@/lib/api"
import { ago, day, left, stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

// What needs the founder's decision first, what the check holds and what
// is overdue at its head; what broke last, folded by kind (decision 131).
export function rank(it: Item) {
  if (it.flag === "blocking" || (it.kind === "deadline" && it.overdue)) return 0
  return { approval: 1, deadline: 2, prepared: 3, delayed: 4, report: 5, idea: 6, incident: 7 }[it.kind] ?? 5
}

const SECTIONS = [
  { label: "To decide", ranks: [0, 1, 2] },
  { label: "To do yourself", ranks: [3] },
  { label: "Going out", ranks: [4] },
  { label: "To read", ranks: [5] },
  { label: "Problems", ranks: [7] },
]

// Incidents that say the same thing (a routine late again, the same act
// failing) are one line: the newest stands for the others.
const same = (a: Item, b: Item) => a.kind === "incident" && b.kind === "incident" && a.title === b.title && a.routine === b.routine
const seen = (realm: string, it: Item) => api(`realms/${enc(realm)}/queue/${it.id}`, { answer: "approve", text: "", revision: it.revision, seconds: 0 })

/** Mark incidents seen, all at once; how many were. */
async function seeAll(realm: string, list: Item[]) {
  let done = 0
  for (const it of list) {
    try { await seen(realm, it); done++ } catch (e) { toast.error((e as Error).message) }
  }
  if (done) toast.success(`${done} marked seen.`)
  return done
}

/** A routine's name as a person says it: "daily-posts" is "Daily posts". */
const named = (routine: string) => {
  const s = routine.replace(/[-_]+/g, " ").trim()
  return s.charAt(0).toUpperCase() + s.slice(1)
}

/** The items of a section in runs: same routine, same day, one header each,
    so a morning's drafts read as one batch and an old batch can go at once. */
function batches(list: Item[]) {
  const out: { key: string; label: string | null; items: Item[] }[] = []
  for (const it of list) {
    const label = it.routine ? `${named(it.routine)} · ${day(it.created_at)}` : null
    const key = label ?? `item-${it.id}`
    const last = out[out.length - 1]
    if (label && last?.key === key) last.items.push(it)
    else out.push({ key, label, items: [it] })
  }
  return out
}

/** A batch's header: its routine and day, how many, and a way to let the
    whole batch go when the moment passed (an old morning's drafts). */
function Batch({ realm, label, items, onDone }: { realm: string; label: string; items: Item[]; onDone: () => void }) {
  const [sure, setSure] = useState(false)
  const [busy, setBusy] = useState(false)
  const drafts = items.filter((i) => i.kind === "approval")
  async function rejectAll() {
    setBusy(true)
    let done = 0
    for (const it of drafts) {
      try {
        await api(`realms/${enc(realm)}/queue/${it.id}`, { answer: "reject", text: "let go with its batch", revision: it.revision, seconds: 0 })
        done++
      } catch (e) {
        toast.error((e as Error).message)
      }
    }
    setBusy(false)
    setSure(false)
    if (done) toast.success(`${done} rejected.`)
    onDone()
  }
  return (
    <div className="flex items-center gap-2 px-2.5 pt-2 pb-1 text-[11.5px] text-muted-foreground">
      <span className="truncate font-medium text-foreground/80">{label}</span>
      <span className="tabular-nums">{items.length}</span>
      {drafts.length > 1 && (sure ? (
        <span className="ml-auto flex shrink-0 items-center gap-1">
          <Button size="xs" variant="destructive" disabled={busy} onClick={rejectAll}>Reject {drafts.length}</Button>
          <Button size="xs" variant="ghost" disabled={busy} onClick={() => setSure(false)}>Keep</Button>
        </span>
      ) : (
        <Button size="xs" variant="ghost" className="ml-auto shrink-0 text-muted-foreground" onClick={() => setSure(true)}>Reject all</Button>
      ))}
    </div>
  )
}

const soon = (it: Item) => !!it.expires_at && Date.parse(it.expires_at) - Date.now() < 3 * 3600 * 1000

const preview = (it: Item) => (it.kind === "approval" && it.text ? it.text : it.title).replace(/\s+/g, " ")

/** Needs you: what the team sent up, as a mailbox. The list by urgency on
    the left, the open item in full on the right; answering opens the next.
    J and K move, A, E and R answer. */
export function Inbox({ realm, items, agents, onDone, onAgent, onTask }: {
  realm: string; items: Item[]; agents: Agent[]; onDone: () => void; onAgent: (name: string) => void; onTask: (id: string) => void
}) {
  const [q, setQ] = useState("")
  const sorted = useMemo(
    () => [...items].sort((a, b) => rank(a) - rank(b)
      || a.created_at.slice(0, 10).localeCompare(b.created_at.slice(0, 10))
      || (a.routine ?? "").localeCompare(b.routine ?? "")
      || a.created_at.localeCompare(b.created_at))
      .filter((it) => !q || `${preview(it)} ${it.agent} ${it.routine}`.toLowerCase().includes(q.toLowerCase()))
      // An incident like a newer one is folded into it.
      .filter((it, _, all) => !all.some((o) => o !== it && same(o, it) && o.created_at > it.created_at)),
    [items, q],
  )
  // The incidents each line stands for, itself included.
  const like = (it: Item) => items.filter((o) => o === it || same(o, it))
  const [sweeping, setSweeping] = useState(false)
  const [sel, setSel] = useState<number | null>(null)
  const pos = sorted.findIndex((i) => i.id === sel)
  // On a phone the list comes first, and an item opens when picked.
  const narrow = useNarrow()
  const open = pos >= 0 ? sorted[pos] : narrow ? null : sorted[0] ?? null
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
      <div className={cn("flex w-(--side) shrink-0 flex-col border-r max-md:w-full max-md:border-r-0", open && "max-md:hidden")}>
        <SideHead title="Needs you" note={items.length} />
        <SideSearch value={q} onChange={setQ} />
        <div className="flex-1 overflow-y-auto px-2 pb-3">
          <Drafts realm={realm} />
          {narrow && !sorted.length && <p className="px-2.5 py-8 text-center text-sm text-muted-foreground">All clear: nothing waits for you.</p>}
          {SECTIONS.map((s) => {
            const list = sorted.filter((i) => s.ranks.includes(rank(i)))
            if (!list.length) return null
            return (
              <section key={s.label} className="mt-2">
                <h2 className="flex items-center gap-2 px-2.5 pt-1 pb-1.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
                  <span className="flex-1">{s.label}</span>
                  {/* What broke goes in one gesture, once read. */}
                  {s.label === "Problems" && (sweeping ? (
                    <span className="flex items-center gap-1 normal-case tracking-normal">
                      <Button size="xs" onClick={async () => { await seeAll(realm, items.filter((i) => i.kind === "incident")); setSweeping(false); onDone() }}>Mark all seen</Button>
                      <Button size="xs" variant="ghost" onClick={() => setSweeping(false)}>Keep</Button>
                    </span>
                  ) : <Button size="xs" variant="ghost" className="h-5 px-1.5 normal-case tracking-normal" onClick={() => setSweeping(true)}>Seen all</Button>)}
                  <span className="tabular-nums">{items.filter((i) => s.ranks.includes(rank(i))).length}</span>
                </h2>
                {batches(list).map((b) => (
                <div key={b.key}>
                {/* A routine's batch has a header; incidents are already one line a kind. */}
                {b.label && b.items.length > 1 && s.label !== "Problems" && <Batch realm={realm} label={b.label} items={b.items} onDone={onDone} />}
                <ul className="space-y-0.5">
                  {b.items.map((it) => {
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
                              {like(it).length > 1 && <span className="shrink-0 rounded bg-foreground/[0.07] px-1 text-[10.5px] font-medium tabular-nums">×{like(it).length}</span>}
                              <span className={cn("inline-flex shrink-0 items-center gap-1 rounded px-1 text-[10.5px] font-medium", k.tone)}><k.icon className="size-3" />{k.label}</span>
                              <span className={cn("ml-auto shrink-0 text-[11px] text-muted-foreground", soon(it) && "text-amber-600 dark:text-amber-400")}
                                title={it.expires_at ? `Leaves Needs you unanswered ${ago(it.expires_at).replace(" ago", "")}` : undefined}>
                                {soon(it) ? left(it.expires_at) : stamp(it.created_at)}
                              </span>
                            </span>
                            <span className="mt-0.5 line-clamp-2 text-[12.5px] leading-snug text-muted-foreground">{preview(it)}</span>
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
                </div>
                ))}
              </section>
            )
          })}
        </div>
      </div>

      {!open ? (
        <div className="flex flex-1 flex-col items-center justify-center text-center max-md:hidden">
          <CircleCheckBig className="size-10 text-emerald-500/80" />
          <p className="mt-4 text-base font-medium">All clear</p>
          <p className="mt-1 text-sm text-muted-foreground">Nothing waits for you. What the team sends up lands here.</p>
        </div>
      ) : (
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-12 shrink-0 items-center gap-1 border-b px-4 max-md:px-3">
            <Back label="All items" onClick={() => setSel(null)} />
            <Button variant="ghost" size="icon-sm" disabled={at <= 0} onClick={() => setSel(sorted[at - 1].id)} aria-label="Previous (K)"><ChevronUp /></Button>
            <Button variant="ghost" size="icon-sm" disabled={at >= sorted.length - 1} onClick={() => setSel(sorted[at + 1].id)} aria-label="Next (J)"><ChevronDown /></Button>
            <span className="ml-1 text-xs text-muted-foreground tabular-nums">{at + 1} of {sorted.length}</span>
            {open.expires_at && <span className={cn("ml-2 text-xs text-muted-foreground", soon(open) && "text-amber-600 dark:text-amber-400")}>· {left(open.expires_at)}, then it leaves unanswered</span>}
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
                <div className="mx-auto max-w-2xl px-4 py-5 md:px-8 md:py-7">
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
              <div className="mx-auto max-w-2xl px-4 py-5 md:px-8 md:py-7">
                <Header it={open} agent={agent(open.agent)} onAgent={onAgent} onTask={onTask} />
                {like(open).length > 1 && (
                  <div className="mt-3 flex items-center gap-2 rounded-lg bg-bubble px-3 py-2 text-xs text-muted-foreground">
                    <span className="flex-1">The same happened {like(open).length} times; this is the latest.</span>
                    <Button size="xs" variant="ghost" onClick={async () => { await seeAll(realm, like(open)); const n = sorted[at + 1] ?? sorted[at - 1]; setAfter(n?.id ?? null); onDone() }}>Seen all {like(open).length}</Button>
                  </div>
                )}
                <NeedCard
                  key={`${open.id}-${open.revision}`} realm={realm} it={open} page onTask={onTask}
                  onDone={() => { const n = sorted[at + 1] ?? sorted[at - 1]; setAfter(n?.id ?? null); onDone() }}
                />
                {open.routine && open.run && <Origin key={open.id} realm={realm} it={open} />}
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

type Step = { block: string; outcome: string; items_in: number; items_out: number; started_at: string; finished_at: string | null; cost_usd: number | null }

/** Where an item comes from (the next interface's panel): the routine's run
    that sent it up, step by step, and what that run cost. */
function Origin({ realm, it }: { realm: string; it: Item }) {
  const [blocks, setBlocks] = useState<Step[] | null>(null)
  useEffect(() => {
    api<{ blocks: Step[] }>(`realms/${enc(realm)}/queue/${it.id}/run`).then((r) => setBlocks(r.blocks), () => setBlocks([]))
  }, [realm, it.id])
  const secs = (b: Step) => (b.finished_at ? Math.max(0, Math.round((Date.parse(b.finished_at) - Date.parse(b.started_at)) / 1000)) : 0)
  return (
    <section className="mt-8 rounded-xl bg-bubble p-4">
      <h3 className="text-xs font-medium text-muted-foreground">Where it comes from</h3>
      {!blocks ? <p className="mt-2 text-xs text-muted-foreground">Loading…</p> : !blocks.length ? (
        <p className="mt-2 text-xs text-muted-foreground">No step was recorded for this run.</p>
      ) : (
        <>
          <p className="mt-1 text-xs"><span className="font-mono">{it.routine}</span> · started {stamp(blocks[0].started_at)}</p>
          <ol className="mt-2.5 space-y-1">
            {blocks.map((b, i) => (
              <li key={i} className="flex items-center gap-2 text-xs">
                <span className={cn("size-1.5 rounded-full", b.outcome === "failed" ? "bg-red-500" : b.outcome === "waiting" ? "bg-amber-500" : "bg-emerald-500")} />
                <span className="w-24 truncate font-mono">{b.block}</span>
                <span className="text-muted-foreground">{b.outcome === "waiting" ? `${b.items_in} for you` : b.outcome === "failed" ? "failed" : `${b.items_in} → ${b.items_out} · ${secs(b)} s`}</span>
              </li>
            ))}
          </ol>
          <p className="mt-2.5 text-xs text-muted-foreground">Run cost <span className="font-medium text-foreground">{((c) => (c && c < 0.01 ? "<$0.01" : `$${c.toFixed(2)}`))(blocks.reduce((a, b) => a + (b.cost_usd ?? 0), 0))}</span></p>
        </>
      )}
    </section>
  )
}

type Draft = { id: number; pipeline: string; kind: string; text: string; source: { link?: string; title?: string } | null; revision: number; created_at: string }

/** Drafts from the fixed sequences of business/pipelines/ (the classic
    Steering page's): approve, edit or reject each. */
function Drafts({ realm }: { realm: string }) {
  const { data, refresh } = usePoll<{ drafts: Draft[] }>(() => api(`realms/${enc(realm)}/drafts?status=pending`), 15000, [realm])
  const [editing, setEditing] = useState<{ id: number; text: string } | null>(null)
  const shown = useRef(new Map<number, number>())
  if (!data?.drafts.length) return null
  async function decide(d: Draft, body: object) {
    const since = shown.current.get(d.id) ?? Date.now()
    try { await api(`realms/${enc(realm)}/drafts/${d.id}`, { ...body, revision: d.revision, seconds: Math.round((Date.now() - since) / 1000) }); setEditing(null); refresh() }
    catch (e) { toast.error((e as Error).message) }
  }
  return (
    <section className="mt-2">
      <h2 className="flex items-center justify-between px-2.5 pt-1 pb-1.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Drafts <span className="tabular-nums">{data.drafts.length}</span></h2>
      <div className="space-y-2 px-1">
        {data.drafts.map((d) => {
          if (!shown.current.has(d.id)) shown.current.set(d.id, Date.now())
          const link = /^https?:\/\//i.test(d.source?.link ?? "") ? d.source!.link! : null
          return (
            <div key={d.id} className="rounded-xl bg-bubble p-3">
              <div className="text-[11px] text-muted-foreground">{d.kind} · {d.pipeline} · {stamp(d.created_at)}</div>
              {editing?.id === d.id
                ? <textarea value={editing.text} onChange={(e) => setEditing({ id: d.id, text: e.target.value })} rows={5} className="mt-1.5 w-full rounded-md bg-bubble p-2 text-[12.5px] outline-none" />
                : <p className="mt-1 text-[12.5px] leading-snug whitespace-pre-wrap">{d.text}</p>}
              {link && <a href={link} target="_blank" rel="noreferrer" className="mt-1 block truncate text-[11px] text-sky-500 hover:underline">{d.source?.title || link}</a>}
              <div className="mt-2 flex gap-1.5">
                {editing?.id === d.id ? (
                  <>
                    <Button size="xs" onClick={() => decide(d, { action: "edit", text: editing.text })}>Save</Button>
                    <Button size="xs" variant="ghost" onClick={() => setEditing(null)}>Cancel</Button>
                  </>
                ) : (
                  <>
                    <Button size="xs" onClick={() => decide(d, { action: "approve" })}>Approve</Button>
                    <Button size="xs" variant="outline" onClick={() => setEditing({ id: d.id, text: d.text })}>Edit</Button>
                    <Button size="xs" variant="ghost" onClick={() => decide(d, { action: "reject", reason: "" })}>Reject</Button>
                  </>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </section>
  )
}
