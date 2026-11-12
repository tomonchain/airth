import { useEffect, useMemo, useRef, useState } from "react"
import { BookmarkPlus, Check, ChevronRight, CircleAlert, CornerDownRight, RotateCw, ShieldQuestion, Square, X } from "lucide-react"
import { toast } from "sonner"
import { Shimmer } from "@/components/beautifui/shimmer"
import { StreamText } from "@/components/beautifui/stream-text"
import { ChangePanel, type Change, type Review } from "@/components/change-panel"
import { Character } from "@/components/character"
import { KeepButton } from "@/components/keep-decision"
import { NeedCard } from "@/components/need-card"
import { Rich } from "@/components/rich"
import { Button } from "@/components/ui/button"
import { api, enc, type Agent, type Item } from "@/lib/api"
import { stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

type Block =
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: Record<string, unknown> }
  | { type: "tool_result"; tool_use_id: string; content: unknown; is_error?: boolean }
type Message = { role: "user" | "assistant"; content: Block[] }
export type RunView = {
  live: boolean
  elsewhere?: boolean
  version: number
  unchanged?: boolean
  state: { id: string; objective: string; status: "running" | "completed" | "stopped" | "failed"; report: string | null; resume: string | null; messages: Message[]; updated_at: string; change?: Change | null }
  title?: string | null
  checks_required?: boolean
  delivery?: Review["delivery"]
  config?: Review["config"]
  pending: { id: number; tool: string; capability: string; preview: string; warnings: string[]; grantable: boolean; grant_label: string | null } | null
}

/** When a task started: its id is the moment, as `20261104T132157.834Z`. */
export function startOf(id: string) {
  const m = id.match(/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})(\.\d+)?Z/)
  return m ? `${m[1]}-${m[2]}-${m[3]}T${m[4]}:${m[5]}:${m[6]}${m[7] ?? ""}Z` : ""
}

/** A run's view. While it is driven here, the page waits on the server for
    its next change, and hears the model's words as it writes them; that
    waiting also keeps it watched, so its questions come here (decision 75).
    A run driven elsewhere is looked at every second and a half. */
export function useRun(realm: string, id: string, active: boolean) {
  const [view, setView] = useState<RunView | null>(null)
  const [draft, setDraft] = useState("")
  const version = useRef(0)
  const [tick, setTick] = useState(0)
  useEffect(() => {
    let on = true
    let timer: ReturnType<typeof setTimeout>
    let seq = -1
    const base = `realms/${enc(realm)}/runs/${enc(id)}`
    const load = async () => {
      // A hidden tab asks nothing; it looks again once shown.
      if (document.visibilityState !== "visible") { timer = setTimeout(load, 1500); return }
      try {
        const v = await api<RunView>(`${base}?since=${version.current}`)
        if (!on) return
        if (!v.unchanged) { version.current = v.version; setView(v); setDraft("") }
        if (v.live && !v.elsewhere) follow()
        else timer = setTimeout(load, v.live || active ? 1500 : 20000)
      } catch {
        if (on) timer = setTimeout(load, 5000)
      }
    }
    const follow = async () => {
      if (document.visibilityState !== "visible") { timer = setTimeout(load, 1500); return }
      try {
        const n = await api<{ here: boolean; version: number; draft_seq: number; draft: string }>(`${base}/live?version=${version.current}&draft=${seq}`)
        if (!on) return
        if (!n.here || n.version !== version.current) { load(); return }
        seq = n.draft_seq
        setDraft(n.draft)
        follow()
      } catch {
        if (on) timer = setTimeout(load, 1500)
      }
    }
    load()
    return () => { on = false; clearTimeout(timer) }
  }, [realm, id, active, tick])
  return { view, draft, refresh: () => { version.current = 0; setTick((t) => t + 1) } }
}

// When the agent began working on what it is doing now: kept per task, so
// leaving the page and coming back does not start the count again.
function since(id: string, live: boolean, marker: number) {
  const key = `airth-working:${id}`
  try {
    const kept = JSON.parse(sessionStorage.getItem(key) ?? "null") as { at: number; marker: number } | null
    if (!live) { sessionStorage.removeItem(key); return null }
    if (kept && kept.marker === marker) return kept.at
    // The first stretch began when the task did; a later one, now.
    const at = marker === 1 && startOf(id) ? Date.parse(startOf(id)) : Date.now()
    sessionStorage.setItem(key, JSON.stringify({ at, marker }))
    return at
  } catch { return Date.now() }
}

function Elapsed({ from }: { from: number }) {
  const [now, setNow] = useState(Date.now())
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), 100); return () => clearInterval(t) }, [])
  const s = Math.max(0, (now - from) / 1000)
  return <span className="font-mono text-xs text-muted-foreground tabular-nums">{s < 60 ? `${s.toFixed(1)}s` : `${Math.floor(s / 60)}m ${Math.floor(s % 60)}s`}</span>
}

type Step = { name: string; input: Record<string, unknown>; error?: boolean; done: boolean }
type Part =
  | { kind: "founder"; text: string }
  | { kind: "agent"; text: string }
  | { kind: "steps"; steps: Step[] }
  | { kind: "note"; text: string }

// The task's messages as chat parts: the founder's words, the agent's words,
// and what it did in between, grouped.
function parts(view: RunView): Part[] {
  // A task a plan passed on carries a note for its agent: not shown.
  const out: Part[] = [{ kind: "founder", text: view.state.objective.split("\n\n(Passed on by ")[0] }]
  const byId = new Map<string, Step>()
  view.state.messages.forEach((m, i) => {
    for (const b of m.content) {
      if (b.type === "text") {
        const text = b.text.trim()
        if (!text) continue
        if (m.role === "assistant") out.push({ kind: "agent", text })
        // The agent's job and the order, which the founder wrote above; what
        // the founder wrote after, before the agent answered, is theirs.
        else if (i === 0 && b === m.content.find((x) => x.type === "text")) continue
        else if (text.startsWith("The founder allowed this")) out.push({ kind: "note", text: "You allowed it." })
        else if (text.startsWith("The tasks of your plan ended")) out.push({ kind: "note", text: "The project's tasks ended." })
        else if (text.startsWith("[Airth runtime]") || text === "Go on with the order.") out.push({ kind: "note", text: "Picked up again." })
        else out.push({ kind: "founder", text })
      } else if (b.type === "tool_use") {
        const step = { name: b.name, input: b.input, done: false }
        byId.set(b.id, step)
        const last = out[out.length - 1]
        if (last?.kind === "steps") last.steps.push(step)
        else out.push({ kind: "steps", steps: [step] })
      } else if (b.type === "tool_result") {
        const s = byId.get(b.tool_use_id)
        if (s) { s.done = true; s.error = !!b.is_error }
      }
    }
  })
  return out
}

const TOOL: Record<string, string> = {
  read_file: "Read", list_directory: "Listed", write_file: "Wrote", edit_file: "Edited", execute_command: "Ran",
  update_plan: "Updated its plan", use_skill: "Used the skill", remember: "Proposed a decision", http_request: "Called", search_files: "Searched",
}
function stepLine(s: Step) {
  const verb = TOOL[s.name] ?? (s.name.startsWith("mcp__") ? s.name.split("__").slice(1).join(" · ") : s.name.replace(/_/g, " "))
  const arg = (s.input.path ?? s.input.command ?? s.input.url ?? s.input.name ?? s.input.query) as string | undefined
  return { verb, arg: typeof arg === "string" ? arg : undefined }
}

// A turn of the agent's, as shown: what it did (its notes on the way, its
// steps, what the founder allowed), folded into one line, then its answer.
type Shown =
  | { kind: "founder"; text: string; at: number }
  | { kind: "work"; items: Part[]; at: number; live: boolean }
  | { kind: "answer"; text: string; at: number }

function turns(ps: Part[], live: boolean): Shown[] {
  const out: Shown[] = []
  let work: { p: Part; at: number }[] = []
  const flush = (end: boolean) => {
    if (!work.length) return
    // The turn's last words are its answer, once it is over; while the
    // agent works they may be a note before its next step.
    const last = work[work.length - 1]
    const answer = last.p.kind === "agent" && !(end && live) ? work.pop()! : null
    if (work.length) out.push({ kind: "work", items: work.map((w) => w.p), at: work[0].at, live: end && live })
    if (answer && answer.p.kind === "agent") out.push({ kind: "answer", text: answer.p.text, at: answer.at })
    work = []
  }
  ps.forEach((p, at) => {
    if (p.kind === "founder") { flush(false); out.push({ kind: "founder", text: p.text, at }) }
    else work.push({ p, at })
  })
  flush(true)
  return out
}

function Work({ items, live, saying, tail }: { items: Part[]; live: boolean; saying?: string; tail?: React.ReactNode }) {
  const [open, setOpen] = useState(false)
  const steps = items.flatMap((p) => (p.kind === "steps" ? p.steps : []))
  const now = (() => {
    if (saying) return saying.split("\n")[0].slice(0, 80)
    const step = steps.find((x) => !x.done)
    if (step) { const l = stepLine(step); return `${l.verb}${l.arg ? ` ${l.arg.length > 48 ? `${l.arg.slice(0, 48)}…` : l.arg}` : ""}` }
    const said = [...items].reverse().find((p) => p.kind === "agent")
    return said?.kind === "agent" ? said.text.split("\n")[0].slice(0, 80) : "Thinking"
  })()
  return (
    <div className="md:max-w-[85%]">
      <button onClick={() => setOpen(!open)} className="inline-flex max-w-full items-center gap-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:text-foreground">
        <ChevronRight className={cn("size-3.5 shrink-0 transition-transform", open && "rotate-90")} />
        {live ? <Shimmer className="truncate">{`${now.replace(/[\s.…:]+$/, "")}…`}</Shimmer> : <span>Worked{steps.length ? ` · ${steps.length} step${steps.length === 1 ? "" : "s"}` : ""}</span>}
      </button>
      {tail}
      {open && (
        <ol className="mt-1 ml-1.5 space-y-1.5 border-l pl-3 fade-up">
          {items.map((p, i) =>
            p.kind === "agent" ? <li key={i} className="text-xs leading-relaxed whitespace-pre-wrap text-muted-foreground">{p.text}</li>
            : p.kind === "note" ? <li key={i} className="text-xs text-muted-foreground italic">{p.text}</li>
            : p.kind === "steps" ? p.steps.map((s, k) => {
              const l = stepLine(s)
              return (
                <li key={`${i}.${k}`} className="flex items-start gap-2 text-xs">
                  {/* A step that failed is part of the work: the agent went on. */}
                  {s.error ? <X className="mt-0.5 size-3 shrink-0 text-muted-foreground" /> : s.done ? <Check className="mt-0.5 size-3 shrink-0 text-emerald-500" /> : <span className="mt-1 size-2 shrink-0 animate-pulse rounded-full bg-sky-500" />}
                  <span className="min-w-0"><span className="text-foreground/80">{l.verb}</span>{l.arg && <span className="ml-1.5 font-mono break-all text-muted-foreground">{l.arg.length > 140 ? `${l.arg.slice(0, 140)}…` : l.arg}</span>}{s.error && <span className="ml-1.5 text-muted-foreground">· did not work</span>}</span>
                </li>
              )
            }) : null,
          )}
        </ol>
      )}
    </div>
  )
}

function Bubble({ side, children, last, agent, time, aside }: { side: "left" | "right"; children: React.ReactNode; last?: boolean; agent: Agent; time?: string; aside?: React.ReactNode }) {
  if (side === "right") {
    return (
      <div className={cn("flex flex-col items-end", last ? "mb-4" : "mb-1.5")}>
        <div className="flex max-w-[88%] items-start gap-1 md:max-w-[75%]">
          {aside}
          <div className="min-w-0 rounded-2xl rounded-br-md bg-me px-3.5 py-2 text-[13.5px] leading-relaxed whitespace-pre-wrap text-me-foreground">{children}</div>
        </div>
        {time && last && <div className="mt-1 pr-1 text-[11px] text-muted-foreground">{time}</div>}
      </div>
    )
  }
  return (
    <div className={cn("flex items-end gap-2.5", last ? "mb-4" : "mb-1.5")}>
      <span className="hidden w-8 shrink-0 md:block">{last && <Character agent={agent} size={32} still />}</span>
      <div className="flex min-w-0 flex-1 items-start gap-1">{children}{aside}</div>
    </div>
  )
}

/** One task with an agent, as a chat: the founder's messages on the right,
    the agent's words on the left, its steps folded in between, and its
    question to the founder when it has one. */
/** A task one agent passed to another (decision 120), as the thread shows it. */
export type Passed = { id: string; agent: string; title: string; order: string; status?: string }

export function TaskThread({ realm, id, agent, items, fresh, echoes = [], onEcho, onChange, from, passed = [], onAgent }: {
  realm: string; id: string; agent: Agent; items: Item[]; fresh: boolean; echoes?: string[]; onEcho?: (text: string) => void; onChange: () => void
  /** Who passed this task on, when another agent did. */
  from?: string
  /** The tasks this one passed on to the team. */
  passed?: Passed[]
  onAgent?: (name: string) => void
}) {
  const { view, draft, refresh } = useRun(realm, id, fresh || echoes.length > 0)
  // Words the founder already read as they were written are not written
  // out again once they are in the run.
  const read = useRef("")
  if (draft) read.current = draft
  const [busy, setBusy] = useState(false)
  const shown = useRef<number | null>(null)
  const ps = useMemo(() => (view ? parts(view) : []), [view])
  const shownTurns = useMemo(() => turns(ps, !!view?.live), [ps, view?.live])
  // What the agent is writing: a sentence is most often a note before its
  // next step, said on the work line; longer, it is an answer, written out
  // as a bubble. A note in a bubble would fold away a moment later and the
  // thread would jump each time.
  const long = !!draft && (draft.length > 160 || draft.trim().includes("\n"))
  const short = draft && !long ? draft : undefined
  // The messages the founder just sent, shown by the conversation until the
  // run has them: looked for among its latest words only.
  useEffect(() => {
    if (!echoes.length) return
    const said = ps.filter((p) => p.kind === "founder").slice(-(echoes.length + 1)).map((p) => p.text)
    for (const e of echoes) if (said.includes(e.trim())) onEcho?.(e)
  }, [ps, echoes, onEcho])
  if (!view) return null
  if (shown.current === null) shown.current = fresh ? 1 : ps.length
  const live = view.live
  const status = view.state.status
  const working = since(id, live, view.state.messages.filter((m) => m.role === "user").length)
  // Stopped on a question it asked in Needs you (decision 64): its item.
  const asked = !live && status === "stopped" ? view.state.report?.match(/\(item (\d+)\)/)?.[1] : undefined
  const askedItem = asked ? items.find((i) => String(i.id) === asked) : undefined
  const question = askedItem?.status === "pending" ? askedItem : undefined

  async function answerLive(approve: boolean, forRun = false) {
    if (!view?.pending) return
    setBusy(true)
    try { await api(`realms/${enc(realm)}/runs/${enc(id)}/approval`, { approval_id: view.pending.id, approve, for_run: forRun }); refresh() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  async function answerStopped(approve: boolean) {
    if (!question) return
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/queue/${question.id}`, { answer: approve ? "approve" : "reject", text: "", revision: question.revision, seconds: 0 })
      if (approve) {
        // Go on now, with the message the service would send at its turn.
        const v = await api<RunView>(`realms/${enc(realm)}/runs/${enc(id)}`)
        await api(`realms/${enc(realm)}/runs/${enc(id)}/messages`, { message: v.state.resume ?? "Go on with the order." })
      }
      refresh(); onChange()
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  // Pick a stopped task up again: with what the founder allowed, else as
  // the runtime resumes any stopped run.
  async function goOn() {
    setBusy(true)
    try {
      if (view?.state.resume) await api(`realms/${enc(realm)}/runs/${enc(id)}/messages`, { message: view.state.resume })
      else await api(`realms/${enc(realm)}/runs/${enc(id)}/resume`, {})
      refresh(); onChange()
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  async function stop() {
    try { await api(`realms/${enc(realm)}/runs/${enc(id)}/stop`, {}); refresh() } catch (e) { toast.error((e as Error).message) }
  }
  const timer = <span className="ml-2 inline-flex items-center gap-1.5 align-middle">{working && <Elapsed from={working} />}<StopButton onStop={stop} /></span>

  return (
    <div>
      {shownTurns.map((t, i) => {
        const next = shownTurns[i + 1]
        const isNew = t.at >= (shown.current ?? 0)
        // Passed on by another agent: its brief, not the founder's words.
        if (t.kind === "founder" && t.at === 0 && from) {
          return (
            <div key={t.at} className={cn("mb-4", isNew && "fade-up")}>
              <div className="rounded-2xl bg-bubble px-4 py-3">
                <div className="mb-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                  <CornerDownRight className="size-3.5" /> <b className="font-medium text-foreground">{from}</b> passed this on to <b className="font-medium text-foreground">{agent.title}</b>
                  <span className="ml-auto">{stamp(startOf(id))}</span>
                </div>
                <p className="text-[13.5px] leading-relaxed whitespace-pre-wrap">{t.text}</p>
              </div>
            </div>
          )
        }
        if (t.kind === "founder") {
          return (
            <div key={t.at} className={cn("group/msg", isNew && "fade-up")}>
              <Bubble side="right" agent={agent} last={next?.kind !== "founder"} time={t.at === 0 ? stamp(startOf(id)) : undefined}
                aside={<KeepButton realm={realm} text={t.text} className="mt-1.5 shrink-0" />}>{t.text}</Bubble>
            </div>
          )
        }
        const last = (!next || next.kind === "founder") && !(i === shownTurns.length - 1 && (live || question))
        return (
          <div key={t.at} className={cn("group/msg", isNew && "fade-up")}>
            <Bubble side="left" agent={agent} last={last} aside={t.kind === "answer" ? <KeepButton realm={realm} text={t.text} className="mt-1.5 shrink-0" /> : undefined}>
              {t.kind === "work" ? <Work items={t.items} live={t.live && !long} saying={t.live ? short : undefined} tail={t.live && !long && !view.pending ? timer : undefined} /> : (
                <div className="rounded-2xl rounded-bl-md bg-bubble px-4 py-2.5 md:max-w-[85%] text-[13.5px] leading-relaxed">
                  <AgentText text={t.text} stream={isNew && !read.current.includes(t.text)} />
                </div>
              )}
            </Bubble>
          </div>
        )
      })}
      {/* The agent proposes a decision for the whole team (decision 83):
          kept only if the founder says so. */}
      {view.pending?.tool === "remember" && (() => {
        const [decision, replaced] = view.pending.preview.split("\n\nReplaces: ")
        return (
          <Bubble side="left" agent={agent} last>
            <div className="rounded-2xl rounded-bl-md border border-sky-400/40 md:max-w-[85%] bg-bubble px-4 py-3 fade-up">
              <div className="mb-2 flex items-center gap-1.5 text-xs font-medium text-sky-600 dark:text-sky-300"><BookmarkPlus className="size-3.5" /> {agent.title} proposes to keep this for the whole team</div>
              <p className="text-[13.5px] leading-relaxed whitespace-pre-wrap">{decision}</p>
              {replaced && <p className="mt-2 text-xs text-muted-foreground"><span className="font-medium">In place of:</span> <span className="line-through decoration-muted-foreground/50">{replaced}</span></p>}
              <p className="mt-2 text-xs text-muted-foreground">Every agent and routine is told it from now on. Keep only what you decided, not ideas still open.</p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button size="sm" disabled={busy} onClick={() => answerLive(true)}><Check /> Keep</Button>
                <Button size="sm" variant="ghost" className="text-muted-foreground" disabled={busy} onClick={() => answerLive(false)}>Not now</Button>
              </div>
            </div>
          </Bubble>
        )
      })()}
      {((view.pending && view.pending.tool !== "remember") || question) && (
        <Bubble side="left" agent={agent} last>
          <div className="rounded-2xl rounded-bl-md border border-amber-400/40 md:max-w-[85%] bg-bubble px-4 py-3 fade-up">
            <div className="mb-2 flex items-center gap-1.5 text-xs font-medium text-amber-600 dark:text-amber-300"><ShieldQuestion className="size-3.5" /> {agent.title} asks before it goes on</div>
            <pre className="max-h-40 overflow-auto rounded-lg bg-muted/60 px-3 py-2 font-mono text-[12px] leading-relaxed whitespace-pre-wrap">{view.pending?.preview ?? question?.text ?? question?.title}</pre>
            {view.pending?.warnings.map((w, i) => <p key={i} className="mt-2 flex gap-1.5 text-xs text-amber-600 dark:text-amber-300"><CircleAlert className="mt-0.5 size-3.5 shrink-0" /> {w}</p>)}
            <div className="mt-3 flex flex-wrap gap-2">
              <Button size="sm" disabled={busy} onClick={() => (view.pending ? answerLive(true) : answerStopped(true))}><Check /> Allow</Button>
              {view.pending?.grantable && <Button size="sm" variant="outline" disabled={busy} onClick={() => answerLive(true, true)}>{view.pending.grant_label ?? "Allow for this task"}</Button>}
              <Button size="sm" variant="ghost" className="text-muted-foreground" disabled={busy} onClick={() => (view.pending ? answerLive(false) : answerStopped(false))}><X /> Deny</Button>
            </div>
          </div>
        </Bubble>
      )}
      {live && !view.pending && long && (
        <Bubble side="left" agent={{ ...agent, state: "working" }} last>
          <div className="writing rounded-2xl md:max-w-[85%] rounded-bl-md bg-bubble px-4 py-2.5 text-[13.5px] leading-relaxed">
            {/* A list item or a line just begun shows once it has words. */}
            <Rich text={draft.replace(/\n\s*(?:[-*•]|\d+[.)])?\s*$/, "").trimEnd()} />
          </div>
          <div className="mt-1 flex items-center gap-2 pl-1">
            {working && <Elapsed from={working} />}
            <button onClick={stop} className="grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Stop" title="Stop"><Square className="size-3 fill-current" /></button>
          </div>
        </Bubble>
      )}
      {live && !view.pending && !long && shownTurns.at(-1)?.kind !== "work" && (
        <Bubble side="left" agent={{ ...agent, state: "working" }} last>
          <div className="fade-up"><Work items={[]} live saying={short} tail={timer} /></div>
        </Bubble>
      )}
      {/* Its plan for the team, waiting for the founder (decision 120):
          approved, edited or rejected here as in Needs you. */}
      {items.filter((i) => i.run === id && i.action === "airth.dispatch" && i.status === "pending").map((plan) => (
        <div key={plan.id} className="mb-4 md:ml-10 md:max-w-[85%] fade-up">
          <NeedCard realm={realm} it={plan} onDone={() => { refresh(); onChange() }} />
        </div>
      ))}
      {/* What this task passed on to the team, and where each part is. */}
      {passed.length > 0 && (
        <div className="mb-4 rounded-2xl bg-bubble px-4 py-3 fade-up">
          <div className="mb-2 text-xs font-medium text-muted-foreground">The project's tasks</div>
          <ul className="space-y-1">
            {passed.map((p) => (
              <li key={p.id}>
                <button onClick={() => onAgent?.(p.agent)} className="flex w-full items-start gap-2 rounded-lg px-2 py-1.5 text-left text-[13px] hover:bg-accent">
                  <CornerDownRight className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1"><b className="font-medium">{p.title}</b> <span className="text-muted-foreground">{p.order.split("\n")[0]}</span></span>
                  <span className={cn("shrink-0 rounded-md px-1.5 py-0.5 text-[11px]",
                    p.status === "completed" ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
                    : p.status === "failed" ? "bg-red-500/10 text-red-500"
                    : p.status === "running" ? "bg-sky-500/10 text-sky-600 dark:text-sky-300"
                    : "bg-muted text-muted-foreground")}>{p.status === "completed" ? "done" : p.status ?? "waiting"}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {/* A task of a business that works in worktrees has a change from its
          start: shown only once it holds something to review. */}
      {view.state.change && (view.state.change.files.length > 0 || view.state.change.head !== view.state.change.base || view.state.change.status !== "open") && (
        <ChangePanel realm={realm} id={id} title={view.title ?? view.state.objective} onChange={() => { refresh(); onChange() }}
          r={{ live, change: view.state.change, checks_required: !!view.checks_required, delivery: view.delivery ?? "adopt", config: view.config ?? { files: [], agents: [] } }}
          onAsk={async (text) => { await api(`realms/${enc(realm)}/runs/${enc(id)}/messages`, { message: text }); refresh(); onChange() }} />
      )}
      {/* A passing failure, about to be tried again (decision 81). */}
      {live && view.state.report?.startsWith("Trying again") && (() => {
        const [, when, why] = view.state.report.match(/^(Trying again in [^:]+): ([\s\S]*)$/) ?? []
        return <p className="mb-4 md:ml-10 text-xs text-amber-600 dark:text-amber-400">{failure(why ?? null)} {when}.</p>
      })()}
      {!live && status === "failed" && (
        <div className="mb-4 md:ml-10 flex flex-wrap items-center gap-2 text-xs">
          <span className="text-red-500">{failure(view.state.report)}</span>
          <Button size="xs" variant="outline" disabled={busy} onClick={goOn}><RotateCw /> Try again</Button>
        </div>
      )}
      {!live && status === "stopped" && !question && (
        <div className="mb-4 md:ml-10 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span>
            {askedItem && askedItem.status !== "rejected" ? `It paused to ask you; you allowed it in Needs you.`
              : askedItem ? "It paused to ask you; you said no, so it stopped."
              : /^stopped by/i.test(view.state.report ?? "") || !view.state.report ? "You stopped it." : `Stopped: ${view.state.report}`}
          </span>
          {!(askedItem && askedItem.status === "rejected") && <Button size="xs" variant="outline" disabled={busy} onClick={goOn}>Go on</Button>}
        </div>
      )}
    </div>
  )
}

/** Why a task failed, in a line: a model's error is long and technical. */
export function failure(report: string | null) {
  if (!report) return "It failed."
  if (/^model error/i.test(report)) {
    if (/\b(429|50[0-4]|rate.?limit|overloaded|529)\b/i.test(report)) return "The model is busy right now."
    if (/\b(401|403|invalid.*key|authentication)\b/i.test(report)) return "The model refused its key: check it in Settings → Keys."
    if (/\b(402|credit|balance|quota)\b/i.test(report)) return "The model's account is out of credit."
    if (/not answering|timed? ?out|connection|broke off/i.test(report)) return "The model stopped answering."
    return "The model failed to answer."
  }
  return report.split("\n")[0]
}

function StopButton({ onStop }: { onStop: () => void }) {
  return <button onClick={onStop} className="grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Stop" title="Stop"><Square className="size-3 fill-current" /></button>
}

/** An agent's words: streamed as they arrive, then shown with links, lists,
    bold and italic. */
function AgentText({ text, stream }: { text: string; stream: boolean }) {
  const [done, setDone] = useState(!stream)
  if (done) return <Rich text={text} />
  const plain = text.replace(/\*\*|__|`/g, "").replace(/^\s*#{1,6}\s+/gm, "")
  return <span className="whitespace-pre-wrap"><StreamText text={plain} caret={false} onDone={() => setDone(true)} /></span>
}
