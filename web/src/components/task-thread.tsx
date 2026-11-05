import { useEffect, useMemo, useRef, useState } from "react"
import { Check, ChevronRight, CircleAlert, ShieldQuestion, Square, Wrench, X } from "lucide-react"
import { toast } from "sonner"
import { Shimmer } from "@/components/beautifui/shimmer"
import { StreamText } from "@/components/beautifui/stream-text"
import { ChangePanel, type Change, type Review } from "@/components/change-panel"
import { Character } from "@/components/character"
import { Rich } from "@/components/rich"
import { Typing } from "@/components/typing"
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
  const out: Part[] = [{ kind: "founder", text: view.state.objective }]
  const byId = new Map<string, Step>()
  view.state.messages.forEach((m, i) => {
    for (const b of m.content) {
      if (b.type === "text") {
        const text = b.text.trim()
        if (!text) continue
        if (m.role === "assistant") out.push({ kind: "agent", text })
        else if (i === 0) continue // the agent's job and the order, which the founder wrote above
        else if (text.startsWith("The founder allowed this")) out.push({ kind: "note", text: "You allowed it." })
        else if (text.startsWith("[Airth runtime]")) out.push({ kind: "note", text: "Picked up again." })
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
  update_plan: "Updated its plan", use_skill: "Used the skill", http_request: "Called", search_files: "Searched",
}
function stepLine(s: Step) {
  const verb = TOOL[s.name] ?? (s.name.startsWith("mcp__") ? s.name.split("__").slice(1).join(" · ") : s.name.replace(/_/g, " "))
  const arg = (s.input.path ?? s.input.command ?? s.input.url ?? s.input.name ?? s.input.query) as string | undefined
  return { verb, arg: typeof arg === "string" ? arg : undefined }
}

function Steps({ steps, live }: { steps: Step[]; live: boolean }) {
  const [open, setOpen] = useState(false)
  const errors = steps.filter((s) => s.error).length
  const running = live && steps.some((s) => !s.done)
  return (
    <div className="max-w-[85%]">
      <button onClick={() => setOpen(!open)} className="inline-flex items-center gap-1.5 rounded-full border bg-bubble/50 px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground">
        <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
        <Wrench className="size-3" />
        {running ? <Shimmer>{(() => { const l = stepLine(steps[steps.length - 1]); return `${l.verb}${l.arg ? ` ${l.arg.length > 48 ? `${l.arg.slice(0, 48)}…` : l.arg}` : ""}…` })()}</Shimmer> : `${steps.length} step${steps.length === 1 ? "" : "s"}`}
        {errors > 0 && <span className="text-red-500">· {errors} failed</span>}
      </button>
      {open && (
        <ol className="mt-1.5 ml-3 space-y-1 border-l pl-3 fade-up">
          {steps.map((s, i) => {
            const l = stepLine(s)
            return (
              <li key={i} className="flex items-start gap-2 text-xs">
                {s.error ? <X className="mt-0.5 size-3 shrink-0 text-red-500" /> : s.done ? <Check className="mt-0.5 size-3 shrink-0 text-emerald-500" /> : <span className="mt-1 size-2 shrink-0 animate-pulse rounded-full bg-sky-500" />}
                <span className="min-w-0"><span className="text-foreground/80">{l.verb}</span>{l.arg && <span className="ml-1.5 font-mono break-all text-muted-foreground">{l.arg.length > 140 ? `${l.arg.slice(0, 140)}…` : l.arg}</span>}</span>
              </li>
            )
          })}
        </ol>
      )}
    </div>
  )
}

function Bubble({ side, children, last, agent, time }: { side: "left" | "right"; children: React.ReactNode; last?: boolean; agent: Agent; time?: string }) {
  if (side === "right") {
    return (
      <div className={cn("flex flex-col items-end", last ? "mb-4" : "mb-1.5")}>
        <div className="max-w-[75%] rounded-2xl rounded-br-md bg-me px-3.5 py-2 text-[13.5px] leading-relaxed whitespace-pre-wrap text-me-foreground">{children}</div>
        {time && last && <div className="mt-1 pr-1 text-[11px] text-muted-foreground">{time}</div>}
      </div>
    )
  }
  return (
    <div className={cn("flex items-end gap-2.5", last ? "mb-4" : "mb-1.5")}>
      <span className="w-8 shrink-0">{last && <Character agent={agent} size={32} still />}</span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  )
}

/** One task with an agent, as a chat: the founder's messages on the right,
    the agent's words on the left, its steps folded in between, and its
    question to the founder when it has one. */
export function TaskThread({ realm, id, agent, items, fresh, echo, onEcho, onChange }: {
  realm: string; id: string; agent: Agent; items: Item[]; fresh: boolean; echo?: string; onEcho?: () => void; onChange: () => void
}) {
  const { view, draft, refresh } = useRun(realm, id, fresh || !!echo)
  // Words the founder already read as they were written are not written
  // out again once they are in the run.
  const read = useRef("")
  if (draft) read.current = draft
  const [busy, setBusy] = useState(false)
  const shown = useRef<number | null>(null)
  const ps = useMemo(() => (view ? parts(view) : []), [view])
  // The message the founder just sent, shown by the conversation until the
  // run has it.
  useEffect(() => {
    const said = ps.filter((p) => p.kind === "founder").at(-1)
    if (echo && said?.kind === "founder" && said.text === echo.trim()) onEcho?.()
  }, [ps, echo, onEcho])
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

  return (
    <div>
      {ps.map((p, i) => {
        const nextSide = ps[i + 1] ? (ps[i + 1].kind === "founder" ? "right" : ps[i + 1].kind === "note" ? "none" : "left") : "end"
        const isNew = i >= (shown.current ?? 0)
        if (p.kind === "note") return <p key={i} className="my-3 text-center text-[11px] text-muted-foreground">{p.text}</p>
        if (p.kind === "founder") {
          return <div key={i} className={cn(isNew && "fade-up")}><Bubble side="right" agent={agent} last={nextSide !== "right"} time={i === 0 ? stamp(startOf(id)) : undefined}>{p.text}</Bubble></div>
        }
        const last = nextSide !== "left" && !(i === ps.length - 1 && (live || question))
        return (
          <div key={i} className={cn(isNew && "fade-up")}>
            <Bubble side="left" agent={agent} last={last}>
              {p.kind === "steps" ? <Steps steps={p.steps} live={live} /> : (
                <div className="max-w-[85%] rounded-2xl rounded-bl-md bg-bubble px-4 py-2.5 text-[13.5px] leading-relaxed">
                  <AgentText text={p.text} stream={isNew && !read.current.includes(p.text)} />
                </div>
              )}
            </Bubble>
          </div>
        )
      })}
      {(view.pending || question) && (
        <Bubble side="left" agent={agent} last>
          <div className="max-w-[85%] rounded-2xl rounded-bl-md border border-amber-400/40 bg-bubble px-4 py-3 fade-up">
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
      {live && !view.pending && draft && (
        <Bubble side="left" agent={{ ...agent, state: "working" }} last>
          <div className="writing max-w-[85%] rounded-2xl rounded-bl-md bg-bubble px-4 py-2.5 text-[13.5px] leading-relaxed">
            {/* A list item or a line just begun shows once it has words. */}
            <Rich text={draft.replace(/\n\s*(?:[-*•]|\d+[.)])?\s*$/, "").trimEnd()} />
          </div>
          <div className="mt-1 flex items-center gap-2 pl-1">
            {working && <Elapsed from={working} />}
            <button onClick={stop} className="grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Stop" title="Stop"><Square className="size-3 fill-current" /></button>
          </div>
        </Bubble>
      )}
      {live && !view.pending && !draft && working && (
        <Bubble side="left" agent={{ ...agent, state: "working" }} last>
          <div className="inline-flex items-center gap-3 rounded-2xl rounded-bl-md bg-bubble px-4 py-3 fade-up">
            <Typing />
            <span className="text-xs text-muted-foreground">{doing(ps)}</span>
            <Elapsed from={working} />
            <button onClick={stop} className="grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Stop" title="Stop"><Square className="size-3 fill-current" /></button>
          </div>
        </Bubble>
      )}
      {view.state.change && (
        <ChangePanel realm={realm} id={id} title={view.title ?? view.state.objective} onChange={() => { refresh(); onChange() }}
          r={{ live, change: view.state.change, checks_required: !!view.checks_required, delivery: view.delivery ?? "adopt", config: view.config ?? { files: [], agents: [] } }}
          onAsk={async (text) => { await api(`realms/${enc(realm)}/runs/${enc(id)}/messages`, { message: text }); refresh(); onChange() }} />
      )}
      {!live && status === "failed" && <p className="mb-4 ml-10 text-xs text-red-500">{view.state.report ?? "It failed."}</p>}
      {!live && status === "stopped" && !question && (
        <div className="mb-4 ml-10 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
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

/** What the agent is at, in a few words: its step in progress, else thinking. */
function doing(ps: Part[]) {
  const last = ps[ps.length - 1]
  if (last?.kind === "steps") {
    const step = last.steps.find((x) => !x.done)
    if (step) {
      const l = stepLine(step)
      return l.arg ? `${l.verb} ${l.arg.length > 40 ? `${l.arg.slice(0, 40)}…` : l.arg}` : l.verb
    }
  }
  return "Thinking"
}

/** An agent's words: streamed as they arrive, then shown with links, lists,
    bold and italic. */
function AgentText({ text, stream }: { text: string; stream: boolean }) {
  const [done, setDone] = useState(!stream)
  if (done) return <Rich text={text} />
  const plain = text.replace(/\*\*|__|`/g, "").replace(/^\s*#{1,6}\s+/gm, "")
  return <span className="whitespace-pre-wrap"><StreamText text={plain} caret={false} onDone={() => setDone(true)} /></span>
}
