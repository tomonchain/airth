import { useState } from "react"
import { BookmarkPlus } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Textarea } from "@/components/ui/textarea"
import { api, enc } from "@/lib/api"
import { cn } from "@/lib/utils"

type Memory = { id: string; text: string }

/** Keep something said in a chat for the whole team (decision 83): only
    what the founder chooses. A brainstorm stays in its chat; a decision
    that changes an earlier one replaces it. */
export function KeepButton({ realm, text, className }: { realm: string; text: string; className?: string }) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState("")
  const [replaces, setReplaces] = useState("")
  const [memory, setMemory] = useState<Memory[] | null>(null)
  const [busy, setBusy] = useState(false)
  async function start() {
    // As plain words: the decision is read by models and by the founder.
    setDraft(text.replace(/\*\*|__|`/g, "").trim().slice(0, 1000)); setReplaces(""); setOpen(true)
    try { setMemory((await api<{ memory: Memory[] }>(`realms/${enc(realm)}`)).memory) } catch { setMemory([]) }
  }
  async function keep() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/memory`, { text: draft.trim(), replaces: replaces || undefined })
      toast.success(replaces ? "Kept, in place of the earlier decision." : "Kept for the whole team.")
      setOpen(false)
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  return (
    <>
      <button onClick={start} title="Keep as a decision: every agent and routine is told it" aria-label="Keep as a decision"
        className={cn("grid size-6 place-items-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/msg:opacity-100 hover:bg-accent hover:text-foreground", className)}>
        <BookmarkPlus className="size-3.5" />
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-[520px]">
          <DialogHeader>
            <DialogTitle>Keep as a decision</DialogTitle>
            <DialogDescription>Every agent and every routine of this business is told it from now on. Write it as the decision, not the discussion.</DialogDescription>
          </DialogHeader>
          <Textarea value={draft} maxLength={1000} rows={4} onChange={(e) => setDraft(e.target.value)} />
          {!!memory?.length && (
            <label className="space-y-1.5 text-sm">
              <span className="text-xs text-muted-foreground">Does it change an earlier decision?</span>
              <select value={replaces} onChange={(e) => setReplaces(e.target.value)} className="h-9 w-full rounded-lg border bg-background px-2 text-sm">
                <option value="">No, it is a new one</option>
                {memory.map((m) => <option key={m.id} value={m.id}>Replaces: {m.text.length > 90 ? `${m.text.slice(0, 90)}…` : m.text}</option>)}
              </select>
            </label>
          )}
          <DialogFooter>
            <Button variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
            <Button disabled={busy || !draft.trim()} onClick={keep}>Keep</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
