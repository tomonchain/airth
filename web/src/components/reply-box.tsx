import { useEffect, useRef, useState } from "react"
import { ArrowUp, Loader } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { api, enc, type Item } from "@/lib/api"
import { sendOrder } from "@/lib/orders"
import { cn } from "@/lib/utils"

/** The founder's answer in words, under what an agent sent up (decision
    127). Where it goes depends on the item: a routine's draft is drafted
    again with it; a task's question goes on with it; anything else is a new
    task for the agent, about the item. "Keep it as a lesson" also writes it
    into the agent's file, which its tasks and routines read from then on. */
export function ReplyBox({ realm, it, redo, onDone }: {
  realm: string; it: Item; redo: (text: string) => Promise<void>; onDone: () => void
}) {
  const [text, setText] = useState("")
  const [lesson, setLesson] = useState(false)
  const [busy, setBusy] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const el = box.current
    if (!el) return
    el.style.height = "auto"
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`
  }, [text])

  const draft = it.kind === "approval" && !!it.routine && it.editable
  const task = !!it.run && !it.routine
  const says = draft ? "It drafts this again with your words." : task ? "Its task goes on with your words." : "A new task for the agent, about this."

  async function send() {
    const t = text.trim()
    if (!t || busy || !it.agent) return
    setBusy(true)
    try {
      if (lesson) await api(`realms/${enc(realm)}/agents/${enc(it.agent)}/lessons`, { text: t })
      if (draft) await redo(t)
      else if (task) {
        await api(`realms/${enc(realm)}/runs/${enc(it.run!)}/messages`, { message: t })
        toast.success("Sent to its task.")
      } else {
        await sendOrder(realm, `About "${it.title}":\n\n${t}`, it.agent)
        toast.success("A task is open with the agent.")
      }
      if (lesson && !draft) toast.success("Kept as a lesson: its next work reads it.")
      setText(""); setLesson(false)
      onDone()
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  if (!it.agent) return null
  return (
    <div className="mt-4">
      <div className="flex items-end gap-1.5 rounded-[22px] border bg-surface py-1 pr-1 pl-4 shadow-[0_2px_10px_-4px_rgb(0_0_0/0.08)] focus-within:border-ring">
        <textarea ref={box} value={text} onChange={(e) => setText(e.target.value)} rows={1} disabled={busy}
          placeholder="Reply to the agent…" aria-label="Reply to the agent"
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); send() } }}
          className="max-h-40 min-h-9 flex-1 resize-none bg-transparent py-1.5 text-sm leading-6 outline-none placeholder:text-muted-foreground" />
        <Button size="icon-sm" className="mb-0.5 size-8 shrink-0 rounded-full" disabled={busy || !text.trim()} onClick={send} aria-label="Send the reply">
          {busy ? <Loader className="animate-spin" /> : <ArrowUp />}
        </Button>
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 px-2 text-xs text-muted-foreground">
        <span>{says}</span>
        <label className={cn("ml-auto flex cursor-pointer items-center gap-1.5", lesson && "text-foreground")}>
          <input type="checkbox" checked={lesson} onChange={(e) => setLesson(e.target.checked)} className="size-3.5 accent-current" />
          Keep it as a lesson
        </label>
      </div>
    </div>
  )
}
