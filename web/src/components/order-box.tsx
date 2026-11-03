import { useState } from "react"
import { ArrowUp } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { api, enc } from "@/lib/api"

/** An order to one agent (decision 64). It runs unwatched: what commits the
    business, or needs a permission, comes back to Needs you. */
export function OrderBox({ realm, agent, title, onSent }: { realm: string; agent: string; title: string; onSent: () => void }) {
  const [text, setText] = useState("")
  const [busy, setBusy] = useState(false)
  async function send() {
    if (!text.trim()) return
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/orders`, { text, agent })
      toast.success(`${title} is on it. What needs you comes back here.`)
      setText("")
      onSent()
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <form
      onSubmit={(e) => { e.preventDefault(); send() }}
      className="flex items-end gap-2 rounded-xl border bg-background/60 p-2 shadow-xs focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30"
    >
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send() } }}
        rows={1}
        placeholder={`Ask ${title} to…`}
        className="max-h-32 min-h-8 flex-1 resize-none bg-transparent px-1.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground"
      />
      <Button type="submit" size="icon-sm" disabled={busy || !text.trim()} aria-label="Send the order"><ArrowUp /></Button>
    </form>
  )
}
