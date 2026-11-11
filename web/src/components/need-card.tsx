import { useEffect, useState } from "react"
import { AlertTriangle, ArrowRight, CalendarClock, Check, ExternalLink, FileText, Hand, LifeBuoy, Lightbulb, Loader, Pencil, RotateCcw, Send, ShieldAlert, Sparkles, Timer, Users, X, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { StreamText } from "@/components/beautifui/stream-text"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Kbd } from "@/components/ui/kbd"
import { Textarea } from "@/components/ui/textarea"
import { api, enc, type Item } from "@/lib/api"
import { cn } from "@/lib/utils"

const KIND: Record<Item["kind"], { label: string; icon: LucideIcon; tone: string }> = {
  incident: { label: "Incident", icon: AlertTriangle, tone: "text-red-500 bg-red-500/10" },
  approval: { label: "To approve", icon: Send, tone: "text-sky-500 bg-sky-500/10" },
  prepared: { label: "You do it", icon: Hand, tone: "text-violet-500 bg-violet-500/10" },
  delayed: { label: "Goes out unless you cancel", icon: Timer, tone: "text-amber-500 bg-amber-500/10" },
  deadline: { label: "Deadline", icon: CalendarClock, tone: "text-amber-500 bg-amber-500/10" },
  report: { label: "Report", icon: FileText, tone: "text-muted-foreground bg-muted" },
  idea: { label: "Idea", icon: Lightbulb, tone: "text-amber-600 bg-amber-500/10 dark:text-amber-300" },
}

export function kindOf(it: Item) {
  if (it.action === "airth.dispatch") return { label: "Plan for the team", icon: Users, tone: "text-sky-500 bg-sky-500/10" }
  if (it.flag === "blocking") return { label: "Held by the check", icon: ShieldAlert, tone: "text-red-500 bg-red-500/10" }
  return KIND[it.kind] ?? KIND.report
}

/** One thing an agent sent up, with the answers it allows. The server says
    what an answer does; this only asks. As a card in a thread, or as the
    page of Needs you, where A, E and R answer from the keyboard. */
export function NeedCard({ realm, it, onDone, page, still, stream, onTask }: { realm: string; it: Item; onDone: () => void; page?: boolean; still?: boolean; stream?: boolean; onTask?: (id: string) => void }) {
  const [mode, setMode] = useState<"edit" | "reject" | "confirm" | "redo" | null>(null)
  const [ask, setAsk] = useState("")
  const [text, setText] = useState(it.text ?? "")
  const [why, setWhy] = useState("")
  const [busy, setBusy] = useState(false)
  const [shown] = useState(Date.now())
  const k = kindOf(it)
  const blocked = it.flag === "blocking"
  // A routine's draft can be drafted again; a task's proposed call cannot.
  const redoable = it.kind === "approval" && !!it.routine && it.editable

  async function answer(kind: string, body = "", extra: object = {}) {
    setBusy(true)
    try {
      const out = await api<{ report?: string; item: { status: string } }>(`realms/${enc(realm)}/queue/${it.id}`, {
        answer: kind, text: body, revision: it.revision, seconds: Math.round((Date.now() - shown) / 1000), ...extra,
      })
      toast.success(kind === "edit" ? "Edited. It still waits for your approval." : out.report ?? `Done: ${out.item.status}`)
      setMode(null)
      onDone()
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  // A new version of the draft: the agent drafts again with what to change.
  // It takes a model call, so it says it is working until the new one is in.
  async function redo() {
    if (!ask.trim()) return
    setBusy(true)
    try {
      const out = await api<{ report: string }>(`realms/${enc(realm)}/queue/${it.id}`, {
        answer: "redo", text: ask, revision: it.revision, seconds: Math.round((Date.now() - shown) / 1000),
      }, undefined, 180000)
      toast.success(out.report)
      setMode(null)
      setAsk("")
      onDone()
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  // Something the founder must do themself (a filing, a prepared action):
  // a task with the agent that fits, to guide them through it. It does not
  // answer the item: Filed or Done stays the founder's, once it is done.
  const [helping, setHelping] = useState(false)
  async function help() {
    setHelping(true)
    try {
      const what = [it.title, it.text && it.text !== it.title ? it.text : null, it.source?.link ? `Official page: ${it.source.link}` : null].filter(Boolean).join("\n")
      const text = `Help me do this myself, step by step:\n\n${what}\n\nTell me where to go, what I need at hand, and what to enter, using what you know of the business (its legal identity, its figures). Ask me for anything you don't have. Don't file or submit anything yourself: I do it.`
      const r = await api<{ agent: string | null; error?: string }>(`realms/${enc(realm)}/orders/route`, { text })
      if (!r.agent) throw new Error(r.error ?? "No agent fits this: ask from Tasks, choosing one.")
      const out = await api<{ id: string }>(`realms/${enc(realm)}/orders`, { text, agent: r.agent, watched: true })
      toast.success("A task is open with the agent: it guides you there.")
      onTask?.(out.id)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setHelping(false)
    }
  }

  // An idea the founder takes up (decision 119): a task with the agent who
  // had it, then the idea is marked useful.
  const [going, setGoing] = useState(false)
  async function go() {
    if (!it.agent) return
    setGoing(true)
    try {
      const text = `Go ahead with your idea: ${it.title}\n\n${it.text ?? ""}${it.angle ? `\n\nWhy now: ${it.angle}` : ""}\n\nPropose the first step; anything that goes out still comes to me first.`
      const out = await api<{ id: string }>(`realms/${enc(realm)}/orders`, { text, agent: it.agent, watched: true })
      await api(`realms/${enc(realm)}/queue/${it.id}`, { answer: "approve", text: "", revision: it.revision, seconds: Math.round((Date.now() - shown) / 1000) })
      toast.success("A task is open with the agent.")
      onDone()
      onTask?.(out.id)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setGoing(false)
    }
  }

  // The main answer A gives, when one is safe without a second look: never
  // an override, a cancellation or a retry the founder must check first.
  const key =
    it.retry || blocked || it.kind === "delayed" ? null
    : "approve"
  useEffect(() => {
    if (!page || still) return
    const on = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || busy) return
      if (/^(INPUT|TEXTAREA|SELECT)$/.test((e.target as HTMLElement)?.tagName ?? "")) {
        if (e.key === "Escape") setMode(null)
        return
      }
      const k = e.key.toLowerCase()
      if (k === "a" && key && !mode) { e.preventDefault(); answer(key) }
      else if (k === "e" && it.editable && !mode) { e.preventDefault(); setMode("edit") }
      else if (k === "n" && redoable && !mode) { e.preventDefault(); setMode("redo") }
      else if (k === "r" && (it.kind === "approval" || it.kind === "delayed") && !mode) { e.preventDefault(); setMode("reject") }
      else if (k === "escape") setMode(null)
    }
    addEventListener("keydown", on)
    return () => removeEventListener("keydown", on)
  })
  const hint = (k: string) => (page ? <Kbd className="ml-1 h-4 min-w-4 bg-current/10 max-md:hidden px-1 text-[10px] font-normal text-current opacity-50 shadow-none">{k}</Kbd> : null)

  const main =
    it.retry === "clear" ? <Button size="sm" disabled={busy} onClick={() => answer("retry")}><RotateCcw /> Retry</Button>
    : it.retry === "ambiguous" ? <Button size="sm" variant="outline" disabled={busy} onClick={() => setMode("confirm")}><RotateCcw /> Retry…</Button>
    : it.kind === "incident" || it.kind === "report" ? <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Seen{hint("A")}</Button>
    : it.kind === "idea" ? <Button size="sm" disabled={busy || going} onClick={go}>{going ? <Loader className="animate-spin" /> : <ArrowRight />} Go ahead</Button>
    : it.kind === "prepared" ? <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Mark done{hint("A")}</Button>
    : it.kind === "deadline" ? <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Filed{hint("A")}</Button>
    : it.kind === "delayed" ? <Button size="sm" variant="destructive" disabled={busy} onClick={() => setMode("reject")}><X /> Cancel it{hint("R")}</Button>
    : blocked ? <Button size="sm" variant="destructive" disabled={busy} onClick={() => answer("override")}><ShieldAlert /> Override</Button>
    : <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Approve{hint("A")}</Button>

  return (
    <article className={page ? "" : "rounded-2xl rounded-bl-md bg-bubble px-4 py-3.5"}>
      {/* In a conversation, a draft to approve needs no label: its buttons
          say it, and the thread gives its time. Other kinds keep a short
          one, so an incident or a deadline is not read as a draft. */}
      {!page && !(it.kind === "approval" && !blocked && it.action !== "airth.dispatch") && <header className="mb-2 flex items-center gap-2 text-xs">
        <span className={cn("inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 font-medium", k.tone)}>
          <k.icon className="size-3.5" /> {k.label}
        </span>
      </header>}

      {!page && !(it.kind === "approval" && it.text) && <h3 className="text-[13.5px] leading-snug font-medium">{it.title}</h3>}

      {it.flag_reason && (
        <div className={cn("mt-2.5 flex gap-2 rounded-lg px-2.5 py-2 text-xs leading-relaxed",
          blocked ? "bg-red-500/10 text-red-700 dark:text-red-200" : "bg-amber-500/10 text-amber-800 dark:text-amber-100")}>
          <ShieldAlert className="mt-0.5 size-3.5 shrink-0" />
          <span><b className="font-medium">The check:</b> {it.flag_reason}</span>
        </div>
      )}

      {/* What the draft answers or is written from, to judge it here. */}
      {page && it.kind === "approval" && (it.source?.text || it.source?.title) && (
        <figure className="mt-4 rounded-xl border px-4 py-3 text-[13px] leading-relaxed text-muted-foreground">
          <figcaption className="mb-1 flex items-center gap-2 text-[11px] font-medium tracking-wide uppercase">
            From the source
            {it.source.link && <a href={it.source.link} target="_blank" rel="noreferrer" className="ml-auto inline-flex items-center gap-1 normal-case tracking-normal hover:text-foreground"><ExternalLink className="size-3" /> Open</a>}
          </figcaption>
          {it.source.title && <p className="font-medium text-foreground">{it.source.title}</p>}
          {it.source.text && it.source.text !== it.source.title && <p className="mt-0.5 line-clamp-6 whitespace-pre-wrap">{it.source.text}</p>}
        </figure>
      )}

      {mode === "edit" ? (
        <div className="mt-3">
          <Textarea value={text} onChange={(e) => setText(e.target.value)} rows={page ? 9 : 6} className="text-sm leading-relaxed" autoFocus />
          {it.action === "airth.dispatch"
            ? <p className="mt-1 text-xs text-muted-foreground">One step a line: <code>- agent: task</code>, or <code>- agent (after other): task</code> to wait for another's result. Remove a line to drop a step.</p>
            : <p className="mt-1 text-right text-xs tabular-nums text-muted-foreground">{text.length} characters</p>}
        </div>
      ) : it.text && it.kind === "approval" ? (
        page ? (
          <div className="mt-4 rounded-xl bg-bubble px-4 py-3.5">
            <p className="text-sm leading-relaxed whitespace-pre-wrap">{it.text}</p>
            <p className="mt-2 flex gap-3 text-xs text-muted-foreground">
              {it.angle && <span className="min-w-0 flex-1"><b className="font-medium">Why:</b> {it.angle}</span>}
              <span className="ml-auto shrink-0 tabular-nums">{it.text.length} characters</span>
            </p>
          </div>
        ) : <p className="mt-1 text-[13.5px] leading-relaxed whitespace-pre-wrap">{stream ? <StreamText text={it.text} caret={false} /> : it.text}</p>
      ) : it.kind === "idea" && it.text ? (
        <>
          <p className="mt-1 text-[13.5px] leading-relaxed whitespace-pre-wrap">{stream ? <StreamText text={it.text} caret={false} /> : it.text}</p>
          {it.angle && <p className="mt-2 text-xs text-muted-foreground"><b className="font-medium">Why now:</b> {it.angle}</p>}
        </>
      ) : page && it.text && it.text !== it.title ? (
        <pre className="mt-4 max-h-96 overflow-auto rounded-xl bg-bubble px-4 py-3.5 font-sans text-sm leading-relaxed whitespace-pre-wrap">{it.text}</pre>
      ) : it.note ? (
        <p className="mt-2 text-sm text-muted-foreground">{it.note}</p>
      ) : null}

      {!page && it.source?.link && (
        <a href={it.source.link} target="_blank" rel="noreferrer" className="mt-2 inline-flex max-w-full items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <ExternalLink className="size-3 shrink-0" /> <span className="truncate">{it.source.title ?? it.source.link}</span>
        </a>
      )}

      {mode === "redo" && (
        <Input className="mt-3" placeholder="What should change? Shorter, funnier, another angle…" value={ask} onChange={(e) => setAsk(e.target.value)} autoFocus
          disabled={busy} onKeyDown={(e) => e.key === "Enter" && redo()} />
      )}
      {mode === "reject" && (
        <Input className="mt-3" placeholder={it.kind === "idea" ? "Why not? Your team hears it (optional)" : it.routine ? "Why? The next drafts hear it (optional)" : "Why? (optional)"} value={why} onChange={(e) => setWhy(e.target.value)} autoFocus
          onKeyDown={(e) => e.key === "Enter" && answer("reject", why)} />
      )}
      {mode === "confirm" && (
        <p className="mt-3 text-xs text-muted-foreground">Check on the platform that nothing was created, then retry.</p>
      )}

      {page && it.note && it.kind === "approval" && <p className="mt-3 text-xs text-muted-foreground">{it.note}</p>}

      <footer className={cn("mt-3 flex flex-wrap items-center gap-1", page && "mt-5 gap-2 border-t pt-4")}>
        {mode === "edit" ? (
          <>
            <Button size="sm" disabled={busy} onClick={() => answer("edit", text)}><Check /> Save the edit</Button>
            <Button size="sm" variant="ghost" onClick={() => setMode(null)}>Cancel</Button>
          </>
        ) : mode === "redo" ? (
          <>
            <Button size="sm" disabled={busy || !ask.trim()} onClick={redo}>{busy ? <><Loader className="animate-spin" /> Drafting again…</> : <><Sparkles /> Draft again</>}</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setMode(null)}>Cancel</Button>
          </>
        ) : mode === "reject" ? (
          <>
            <Button size="sm" variant="destructive" disabled={busy} onClick={() => answer("reject", why)}><X /> {it.kind === "delayed" ? "Cancel it" : it.kind === "idea" ? "Not for me" : "Reject"}</Button>
            <Button size="sm" variant="ghost" onClick={() => setMode(null)}>Keep it</Button>
          </>
        ) : mode === "confirm" ? (
          <>
            <Button size="sm" disabled={busy} onClick={() => answer("retry", "", { checked: true })}><RotateCcw /> Nothing was created: retry</Button>
            <Button size="sm" variant="ghost" onClick={() => setMode(null)}>Back</Button>
          </>
        ) : (
          <>
            {main}
            {it.retry === "ambiguous" && <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Seen</Button>}
            {/* In a conversation the other answers stay quiet, beside Approve. */}
            {it.editable && <Button size={page ? "sm" : "xs"} variant={page ? "outline" : "ghost"} className={cn(!page && "text-muted-foreground")} onClick={() => setMode("edit")}><Pencil /> Edit{hint("E")}</Button>}
            {redoable && <Button size={page ? "sm" : "xs"} variant={page ? "outline" : "ghost"} className={cn(!page && "text-muted-foreground")} onClick={() => setMode("redo")}><Sparkles /> Redo{hint("N")}</Button>}
            {it.kind === "approval" && <Button size={page ? "sm" : "xs"} variant="ghost" className="text-muted-foreground" aria-label="Reject" title="Reject" onClick={() => setMode("reject")}><X /> <span className={cn(!page && "max-sm:hidden")}>Reject</span>{hint("R")}</Button>}
            {it.kind === "deadline" && !it.overdue && <Button size="sm" variant="ghost" onClick={() => answer("reject", "seen")}>Seen</Button>}
            {it.kind === "idea" && (
              <>
                <Button size="sm" variant="outline" disabled={busy} onClick={() => answer("approve")}><Check /> Useful</Button>
                <Button size="sm" variant="ghost" className="text-muted-foreground" disabled={busy} onClick={() => setMode("reject")}>Not for me</Button>
              </>
            )}
            {onTask && (it.kind === "deadline" || it.kind === "prepared") && (
              <Button size="sm" variant="outline" disabled={helping} onClick={help}>{helping ? <Loader className="animate-spin" /> : <LifeBuoy />} Get help</Button>
            )}
          </>
        )}
      </footer>
    </article>
  )
}
