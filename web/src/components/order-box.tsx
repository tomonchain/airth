import { useState } from "react"
import { ArrowUp, Plus } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { api, enc } from "@/lib/api"

/** An order to one agent (decision 64). It runs unwatched: what commits the
    business, or needs a permission, comes back as a message to answer. */
export function OrderBox({ realm, agent, title, onSent }: { realm: string; agent: string; title: string; onSent: (text: string) => void }) {
  const [text, setText] = useState("")
  const [busy, setBusy] = useState(false)
  async function send() {
    const t = text.trim()
    if (!t) return
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/orders`, { text: t, agent })
      setText("")
      onSent(t)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <form
      onSubmit={(e) => { e.preventDefault(); send() }}
      className="flex items-end gap-2 rounded-2xl border bg-bubble/60 p-2 shadow-sm focus-within:border-ring/50"
    >
      <Button type="button" variant="ghost" size="icon" className="rounded-full text-muted-foreground" aria-label="Attach" disabled><Plus /></Button>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send() } }}
        rows={1}
        placeholder={`Write to ${title}`}
        className="max-h-40 min-h-8 flex-1 resize-none bg-transparent px-1 py-1.5 text-[13.5px] outline-none placeholder:text-muted-foreground"
      />
      <Button type="submit" size="icon" className="rounded-full" disabled={busy || !text.trim()} aria-label="Send"><ArrowUp /></Button>
    </form>
  )
}
