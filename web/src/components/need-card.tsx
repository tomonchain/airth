import { useEffect, useState } from "react"
import { AlertTriangle, CalendarClock, Check, ExternalLink, FileText, Hand, Pencil, RotateCcw, Send, ShieldAlert, Timer, X, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { StreamText } from "@/components/beautifui/stream-text"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Kbd } from "@/components/ui/kbd"
import { Textarea } from "@/components/ui/textarea"
import { api, enc, type Item } from "@/lib/api"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"

const KIND: Record<Item["kind"], { label: string; icon: LucideIcon; tone: string }> = {
  incident: { label: "Incident", icon: AlertTriangle, tone: "text-red-500 bg-red-500/10" },
  approval: { label: "To approve", icon: Send, tone: "text-sky-500 bg-sky-500/10" },
  prepared: { label: "You do it", icon: Hand, tone: "text-violet-500 bg-violet-500/10" },
  delayed: { label: "Goes out unless you cancel", icon: Timer, tone: "text-amber-500 bg-amber-500/10" },
  deadline: { label: "Deadline", icon: CalendarClock, tone: "text-amber-500 bg-amber-500/10" },
  report: { label: "Report", icon: FileText, tone: "text-muted-foreground bg-muted" },
}

export function kindOf(it: Item) {
  if (it.flag === "blocking") return { label: "Held by the check", icon: ShieldAlert, tone: "text-red-500 bg-red-500/10" }
  return KIND[it.kind] ?? KIND.report
}

/** One thing an agent sent up, with the answers it allows. The server says
    what an answer does; this only asks. As a card in a thread, or as the
    page of Needs you, where A, E and R answer from the keyboard. */
export function NeedCard({ realm, it, onDone, page, still, stream }: { realm: string; it: Item; onDone: () => void; page?: boolean; still?: boolean; stream?: boolean }) {
  const [mode, setMode] = useState<"edit" | "reject" | "confirm" | null>(null)
  const [text, setText] = useState(it.text ?? "")
  const [why, setWhy] = useState("")
  const [busy, setBusy] = useState(false)
  const [shown] = useState(Date.now())
  const k = kindOf(it)
  const blocked = it.flag === "blocking"

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
      else if (k === "r" && (it.kind === "approval" || it.kind === "delayed") && !mode) { e.preventDefault(); setMode("reject") }
      else if (k === "escape") setMode(null)
    }
    addEventListener("keydown", on)
    return () => removeEventListener("keydown", on)
  })
  const hint = (k: string) => (page ? <Kbd className="ml-1 opacity-70">{k}</Kbd> : null)

  const main =
    it.retry === "clear" ? <Button size="sm" disabled={busy} onClick={() => answer("retry")}><RotateCcw /> Retry</Button>
    : it.retry === "ambiguous" ? <Button size="sm" variant="outline" disabled={busy} onClick={() => setMode("confirm")}><RotateCcw /> Retry…</Button>
    : it.kind === "incident" || it.kind === "report" ? <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Seen{hint("A")}</Button>
    : it.kind === "prepared" ? <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Mark done{hint("A")}</Button>
    : it.kind === "deadline" ? <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Filed{hint("A")}</Button>
    : it.kind === "delayed" ? <Button size="sm" variant="destructive" disabled={busy} onClick={() => setMode("reject")}><X /> Cancel it{hint("R")}</Button>
    : blocked ? <Button size="sm" variant="destructive" disabled={busy} onClick={() => answer("override")}><ShieldAlert /> Override</Button>
    : <Button size="sm" disabled={busy} onClick={() => answer("approve")}><Check /> Approve{hint("A")}</Button>

  return (
    <article className={page ? "" : "rounded-2xl rounded-bl-md border border-amber-400/25 bg-bubble px-4 py-3.5"}>
      {!page && <header className="mb-2 flex items-center gap-2 text-xs">
        <span className={cn("inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 font-medium", k.tone)}>
          <k.icon className="size-3.5" /> {k.label}
        </span>
        {it.routine && <span className="font-mono text-muted-foreground">{it.routine}</span>}
        <span className="ml-auto text-muted-foreground">{ago(it.created_at)}</span>
      </header>}

      {!page && !(it.kind === "approval" && it.text) && <h3 className="text-[13.5px] leading-snug font-medium">{it.title}</h3>}

      {it.flag_reason && (
        <div className={cn("mt-2.5 flex gap-2 rounded-lg px-2.5 py-2 text-xs leading-relaxed",
          blocked ? "bg-red-500/10 text-red-700 dark:text-red-200" : "bg-amber-500/10 text-amber-800 dark:text-amber-100")}>
          <ShieldAlert className="mt-0.5 size-3.5 shrink-0" />
          <span><b className="font-medium">The check:</b> {it.flag_reason}</span>
        </div>
      )}

      {mode === "edit" ? (
        <div className="mt-3">
          <Textarea value={text} onChange={(e) => setText(e.target.value)} rows={page ? 9 : 6} className="text-sm leading-relaxed" autoFocus />
          <p className="mt-1 text-right text-xs tabular-nums text-muted-foreground">{text.length} characters</p>
        </div>
      ) : it.text && it.kind === "approval" ? (
        page ? (
          <div className="mt-4 rounded-xl border bg-bubble/60 px-4 py-3.5">
            <p className="text-sm leading-relaxed whitespace-pre-wrap">{it.text}</p>
            <p className="mt-2 text-right text-xs tabular-nums text-muted-foreground">{it.text.length} characters</p>
          </div>
        ) : <p className="mt-1 text-[13.5px] leading-relaxed whitespace-pre-wrap">{stream ? <StreamText text={it.text} caret={false} /> : it.text}</p>
      ) : page && it.text && it.text !== it.title ? (
        <pre className="mt-4 max-h-96 overflow-auto rounded-xl border bg-bubble/60 px-4 py-3.5 font-sans text-sm leading-relaxed whitespace-pre-wrap">{it.text}</pre>
      ) : it.note ? (
        <p className="mt-2 text-sm text-muted-foreground">{it.note}</p>
      ) : null}

      {!page && it.source?.link && (
        <a href={it.source.link} target="_blank" rel="noreferrer" className="mt-2 inline-flex max-w-full items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <ExternalLink className="size-3 shrink-0" /> <span className="truncate">{it.source.title ?? it.source.link}</span>
        </a>
      )}

      {mode === "reject" && (
        <Input className="mt-3" placeholder="Why? The agent learns from it (optional)" value={why} onChange={(e) => setWhy(e.target.value)} autoFocus
          onKeyDown={(e) => e.key === "Enter" && answer("reject", why)} />
      )}
      {mode === "confirm" && (
        <p className="mt-3 text-xs text-muted-foreground">Check on the platform that nothing was created, then retry.</p>
      )}

      {page && it.note && it.kind === "approval" && <p className="mt-3 text-xs text-muted-foreground">{it.note}</p>}

      <footer className={cn("mt-3 flex flex-wrap items-center gap-2", page && "mt-5 border-t pt-4")}>
        {mode === "edit" ? (
          <>
            <Button size="sm" disabled={busy} onClick={() => answer("edit", text)}><Check /> Save the edit</Button>
            <Button size="sm" variant="ghost" onClick={() => setMode(null)}>Cancel</Button>
          </>
        ) : mode === "reject" ? (
          <>
            <Button size="sm" variant="destructive" disabled={busy} onClick={() => answer("reject", why)}><X /> {it.kind === "delayed" ? "Cancel it" : "Reject"}</Button>
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
            {it.editable && <Button size="sm" variant="outline" onClick={() => setMode("edit")}><Pencil /> Edit{hint("E")}</Button>}
            {it.kind === "approval" && <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setMode("reject")}><X /> Reject{hint("R")}</Button>}
            {it.kind === "deadline" && !it.overdue && <Button size="sm" variant="ghost" onClick={() => answer("reject", "seen")}>Seen</Button>}
          </>
        )}
      </footer>
    </article>
  )
}
