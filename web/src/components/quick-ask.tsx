import { useEffect, useRef, useState } from "react"
import { ArrowUp, Loader } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { MicButton } from "@/components/mic"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { called, type Agent } from "@/lib/api"
import { sendOrder } from "@/lib/orders"

const ROUTE = "route"

/** Ask from anywhere: one order, to the founder's interlocutor (decision 120)
    or the agent named, without leaving the page. It is a task like any
    other: what follows comes in its conversation and in Needs you. */
export function QuickAsk({ open, onOpenChange, realm, agents, onTask }: {
  open: boolean; onOpenChange: (o: boolean) => void; realm: string; agents: Agent[]; onTask: (id: string) => void
}) {
  const [text, setText] = useState("")
  const [who, setWho] = useState(ROUTE)
  const [busy, setBusy] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  const lead = agents.find((a) => a.dispatch)
  useEffect(() => { if (open) requestAnimationFrame(() => box.current?.focus()) }, [open])

  const toLabel = (v: string) => {
    if (v !== ROUTE) { const a = agents.find((x) => x.name === v); return a ? called(a) : v }
    return lead ? called(lead) : "The agent it is for"
  }

  async function send() {
    const t = text.trim()
    if (!t || busy) return
    setBusy(true)
    try {
      const out = await sendOrder(realm, t, who)
      const a = agents.find((x) => x.name === out.agent)
      toast.success(`Sent to ${a ? called(a) : out.agent}`, { action: { label: "Open", onClick: () => onTask(out.id) } })
      setText("")
      onOpenChange(false)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const to = who === ROUTE ? lead : agents.find((a) => a.name === who)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false} className="top-[14%] translate-y-0 gap-0 p-0 sm:max-w-xl max-sm:top-[max(env(safe-area-inset-top),12px)]">
        <DialogTitle className="sr-only">Ask the team</DialogTitle>
        <DialogDescription className="sr-only">An order, sent to your interlocutor or the agent you choose.</DialogDescription>
        <div className="p-3 pb-2">
          <textarea ref={box} value={text} onChange={(e) => setText(e.target.value)} rows={3} disabled={busy}
            placeholder={lead ? `Ask ${lead.first_name ?? lead.title} for anything…` : "Ask the team for anything…"} aria-label="What to ask"
            onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); send() } }}
            className="w-full resize-none bg-transparent px-1 text-[15px] leading-relaxed outline-none placeholder:text-muted-foreground" />
        </div>
        <div className="flex items-center gap-2 border-t px-3 py-2">
          <Select value={who} onValueChange={(v) => setWho(String(v ?? ROUTE))}>
            <SelectTrigger size="sm" className="h-8 max-w-56 gap-1.5 text-xs" aria-label="Who it is for">
              {to && <Character agent={to} size={16} still />}
              <SelectValue>{(v: string) => `To: ${toLabel(v)}`}</SelectValue>
            </SelectTrigger>
            <SelectContent className="min-w-64">
              <SelectItem value={ROUTE}>{lead ? `${called(lead)} · passes it on` : "The agent it is for"}</SelectItem>
              {agents.filter((a) => a !== lead).map((a) => <SelectItem key={a.name} value={a.name}>{called(a)}</SelectItem>)}
            </SelectContent>
          </Select>
          <span className="flex-1" />
          <span className="text-[11px] text-muted-foreground max-sm:hidden">⌘↵ to send</span>
          <MicButton value={text} onChange={setText} />
          <Button size="icon-sm" className="rounded-full" disabled={busy || !text.trim()} onClick={send} aria-label="Send">
            {busy ? <Loader className="animate-spin" /> : <ArrowUp />}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
