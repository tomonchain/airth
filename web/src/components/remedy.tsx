import { useEffect, useState } from "react"
import { Loader, Play, Sparkles } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { api, enc, type Item } from "@/lib/api"
import { cn } from "@/lib/utils"

type Remedy = { title: string; message: string; budget_usd?: number }
type Proposed = { remedies: Remedy[]; ceiling: number; error?: string }

/** A stopped task's way on (decision 125): up to three remedies the model
    proposes, or the founder's own, then Resume or Cancel. A remedy is a
    message to the agent, and a larger cost budget when the work needs it,
    never above the ceiling the server sets. */
export function RemedyPanel({ realm, it, shown, onDone, onCancel, onTask }: {
  realm: string; it: Item; shown: number; onDone: () => void; onCancel: () => void; onTask?: (id: string) => void
}) {
  const [proposed, setProposed] = useState<Proposed | null>(null)
  const [picked, setPicked] = useState<number | null>(null)
  const [message, setMessage] = useState("")
  const [budget, setBudget] = useState("")
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let gone = false
    api<Proposed>(`realms/${enc(realm)}/queue/${it.id}/remedies`, {}, undefined, 90000)
      .then((p) => { if (!gone) setProposed(p) })
      .catch((e: Error) => { if (!gone) setProposed({ remedies: [], ceiling: 0, error: e.message }) })
    return () => { gone = true }
  }, [realm, it.id])

  function pick(i: number, r: Remedy) {
    setPicked(i)
    setMessage(r.message)
    setBudget(r.budget_usd ? String(r.budget_usd) : "")
  }

  async function resume() {
    setBusy(true)
    try {
      const out = await api<{ run: string }>(`realms/${enc(realm)}/queue/${it.id}/remedy`, {
        message, budget_usd: budget.trim() ? Number(budget) : null, seconds: Math.round((Date.now() - shown) / 1000),
      })
      toast.success("The task goes on.")
      onDone()
      onTask?.(out.run)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="mt-3 space-y-3">
      {!proposed ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground"><Loader className="size-4 animate-spin" /> Looking for ways to go on…</p>
      ) : proposed.remedies.length ? (
        <div className="grid gap-2">
          {proposed.remedies.map((r, i) => (
            <button key={i} onClick={() => pick(i, r)} disabled={busy}
              className={cn("rounded-xl border px-3.5 py-2.5 text-left transition-colors hover:bg-accent/50", picked === i && "border-foreground/40 bg-accent/60")}>
              <span className="flex items-center gap-2 text-sm font-medium">
                <Sparkles className="size-3.5 shrink-0 text-muted-foreground" /> {r.title}
                {r.budget_usd != null && <span className="ml-auto shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-normal tabular-nums">Budget ${r.budget_usd.toFixed(2)}</span>}
              </span>
              <span className="mt-1 block text-[13px] leading-snug text-muted-foreground [overflow-wrap:anywhere]">{r.message}</span>
            </button>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">{proposed.error ?? "No proposal."} Write how the task should go on.</p>
      )}
      <div className="space-y-2">
        <Textarea rows={3} value={message} onChange={(e) => { setMessage(e.target.value); setPicked(null) }} disabled={busy}
          placeholder="Or your own: what should the agent do now?" className="text-sm" aria-label="What the agent should do" />
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <label htmlFor={`budget-${it.id}`}>Cost budget</label>
          <span className="relative">
            <span className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2">$</span>
            <Input id={`budget-${it.id}`} inputMode="decimal" value={budget} onChange={(e) => setBudget(e.target.value.replace(/[^0-9.]/g, ""))}
              disabled={busy} placeholder="as it is" className="h-8 w-28 pl-5 text-sm" />
          </span>
          {!!proposed?.ceiling && <span>at most ${proposed.ceiling.toFixed(2)}</span>}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-1">
        <Button size="sm" disabled={busy || !message.trim()} onClick={resume}>{busy ? <Loader className="animate-spin" /> : <Play />} Resume the task</Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  )
}
