import { useEffect, useState } from "react"
import { ArrowUp, Plus, X } from "lucide-react"
import { toast } from "sonner"
import { MicButton } from "@/components/mic"
import { Button } from "@/components/ui/button"
import { usePrefs } from "@/hooks/use-prefs"
import { api, enc } from "@/lib/api"

/** Writing to an agent. A message continues the agent's latest task, as a
    chat does; a new task is started on purpose. Sent from here, the founder
    watches it, so its questions come back here (decision 75). */
export function OrderBox({ realm, agent, title, task, closed, working, onSending, onSent, onFailed }: {
  realm: string; agent: string; title: string; task?: string; closed?: string | null; working: boolean
  onSending: (text: string, task: string | null) => void; onSent: (taskId: string | null) => void; onFailed: () => void
}) {
  const [text, setText] = useState("")
  const { prefs } = usePrefs()
  const [busy, setBusy] = useState(false)
  const [fresh, setFresh] = useState(false)
  useEffect(() => setFresh(false), [agent])
  const continuing = !!task && !fresh

  async function send() {
    const t = text.trim()
    if (!t || working) return
    setBusy(true)
    // The message goes into the thread now, as a chat does; the server's
    // copy replaces it when it comes back.
    setText("")
    onSending(t, continuing ? task! : null)
    try {
      // Closed since the page last looked (its change adopted elsewhere): the
      // message starts a new task rather than failing.
      const goOn = continuing && await api(`realms/${enc(realm)}/runs/${enc(task!)}/messages`, { message: t })
        .then(() => true, (e: Error) => { if (/change was (adopted|discarded)/.test(e.message)) return false; throw e })
      if (goOn) {
        onSent(null)
      } else {
        const out = await api<{ id: string }>(`realms/${enc(realm)}/orders`, { text: t, agent, watched: true })
        onSent(out.id)
        setFresh(false)
      }
    } catch (e) {
      setText(t)
      onFailed()
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div>
      <div className="mb-1.5 flex h-6 items-center gap-2 px-1 text-xs text-muted-foreground">
        {working ? <span>{title} is working. You can write when it is done, or stop it.</span>
          : continuing ? (
            <>
              <span>Continues the current task</span>
              <button onClick={() => setFresh(true)} className="inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 hover:bg-accent hover:text-foreground"><Plus className="size-3" /> New task</button>
            </>
          ) : task ? (
            <>
              <span>Starts a new task</span>
              <button onClick={() => setFresh(false)} className="inline-flex items-center gap-1 rounded-md px-1 py-0.5 hover:bg-accent hover:text-foreground"><X className="size-3" /> Keep the current one</button>
            </>
          ) : closed ? <span>The last task is closed (its change was {closed}): this starts a new one.</span>
          : <span>Starts a task: {title} asks you here before anything that needs your OK.</span>}
      </div>
      <form onSubmit={(e) => { e.preventDefault(); send() }} className="flex items-end gap-2 rounded-2xl border bg-bubble/60 p-2 shadow-sm focus-within:border-ring/50">
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            // ↵ or ⌘↵ sends, as the founder chose in Settings; the other adds a line.
            const mod = e.metaKey || e.ctrlKey
            if (e.key === "Enter" && !e.shiftKey && (prefs.send === "mod" ? mod : !mod)) { e.preventDefault(); send() }
          }}
          rows={1}
          placeholder={`Write to ${title}`}
          className="max-h-40 min-h-8 flex-1 resize-none bg-transparent px-2 py-1.5 text-[13.5px] outline-none placeholder:text-muted-foreground field-sizing-content"
        />
        <MicButton value={text} onChange={setText} className="mb-0.5" />
        <Button type="submit" size="icon" className="rounded-full" disabled={busy || working || !text.trim()} aria-label="Send"><ArrowUp /></Button>
      </form>
    </div>
  )
}
