import { useEffect, useRef, useState } from "react"
import { ArrowUp, AtSign, Plus, Slash, X } from "lucide-react"
import { toast } from "sonner"
import { MicButton } from "@/components/mic"
import { Button } from "@/components/ui/button"
import { usePrefs } from "@/hooks/use-prefs"
import { api, called, enc, type Agent } from "@/lib/api"
import { cn } from "@/lib/utils"

/** What the founder is typing at the caret: "/" and a tool's name, or "@"
    and an agent's. */
function typing(text: string, caret: number): { kind: "/" | "@"; query: string; from: number } | null {
  const m = text.slice(0, caret).match(/(?:^|\s)([/@])([\p{L}\w.-]*)$/u)
  if (!m) return null
  return { kind: m[1] as "/" | "@", query: m[2].toLowerCase(), from: caret - m[2].length - 1 }
}

/** How an agent is mentioned: its first name, else its file name. */
const handle = (a: Agent) => (a.first_name ? a.first_name.replace(/\s+/g, "-") : a.name)

/** Writing to an agent. A message continues the agent's latest task, as a
    chat does; a new task is started on purpose. Sent from here, the founder
    watches it, so its questions come back here (decision 75). */
export function OrderBox({ realm, agent, title, task, closed, working, tools = [], others = [], onOther, onSending, onSent, onFailed }: {
  realm: string; agent: string; title: string; task?: string; closed?: string | null; working: boolean
  /** The agent's tools, offered after "/"; the business's other agents, after "@". */
  tools?: string[]; others?: Agent[]; onOther?: (name: string) => void
  onSending: (text: string, task: string | null) => void; onSent: (taskId: string | null) => void; onFailed: () => void
}) {
  const [text, setText] = useState("")
  const box = useRef<HTMLTextAreaElement>(null)
  const [at, setAt] = useState<ReturnType<typeof typing>>(null)
  const [pick, setPick] = useState(0)
  const choices: { key: string; label: string; hint: string }[] = !at ? [] : at.kind === "/"
    ? tools.filter((t) => t.toLowerCase().includes(at.query)).map((t) => ({ key: t, label: `/${t}`, hint: "a tool it may use" }))
    : others.filter((a) => `${handle(a)} ${a.name} ${a.title}`.toLowerCase().includes(at.query)).map((a) => ({ key: handle(a), label: `@${handle(a)}`, hint: called(a) }))
  const shown = choices.slice(0, 6)

  function look(value: string, caret: number) { setAt(typing(value, caret)); setPick(0) }
  function choose(key: string) {
    if (!at || !box.current) return
    const caret = box.current.selectionStart
    const next = `${text.slice(0, at.from)}${at.kind}${key} ${text.slice(caret)}`
    setText(next); setAt(null)
    const pos = at.from + key.length + 2
    requestAnimationFrame(() => { box.current?.focus(); box.current?.setSelectionRange(pos, pos) })
  }

  /** "@Ada …" at the start: the order goes to Ada, as a task of hers. */
  function addressed(t: string): { agent: Agent; rest: string } | null {
    const m = t.match(/^@([\p{L}\w.-]+)\s+([\s\S]+)$/u)
    if (!m) return null
    const who = m[1].toLowerCase()
    const a = others.find((x) => handle(x).toLowerCase() === who || x.name.toLowerCase() === who)
    return a ? { agent: a, rest: m[2].trim() } : null
  }
  const { prefs } = usePrefs()
  const [busy, setBusy] = useState(false)
  const [fresh, setFresh] = useState(false)
  useEffect(() => setFresh(false), [agent])
  // ↑ in the empty box brings back the last message, to fix and send again.
  const [last, setLast] = useState("")
  useEffect(() => setLast(""), [agent])
  // At work, a message goes to what it is doing: it reads it at its next step.
  const continuing = !!task && (!fresh || working)

  async function send() {
    const t = text.trim()
    if (!t) return
    const other = addressed(t)
    if (other) {
      setBusy(true)
      try {
        await api(`realms/${enc(realm)}/orders`, { text: other.rest, agent: other.agent.name, watched: true })
        setText(""); setLast(t)
        toast.success(`Sent to ${called(other.agent)}.`, onOther ? { action: { label: "Open", onClick: () => onOther(other.agent.name) } } : undefined)
      } catch (e) { toast.error((e as Error).message) }
      finally { setBusy(false) }
      return
    }
    setBusy(true)
    // The message goes into the thread now, as a chat does; the server's
    // copy replaces it when it comes back.
    setText("")
    setLast(t)
    onSending(t, continuing ? task! : null)
    try {
      // Closed since the page last looked (its change adopted elsewhere), or
      // the agent now works in another repository: the message starts a new
      // task rather than failing.
      const goOn = continuing && await api(`realms/${enc(realm)}/runs/${enc(task!)}/messages`, { message: t })
        .then(() => true, (e: Error) => { if (/change was (adopted|discarded)|start a new task instead/.test(e.message)) return false; throw e })
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

  async function stop() {
    try { await api(`realms/${enc(realm)}/runs/${enc(task!)}/stop`, {}) } catch (e) { toast.error((e as Error).message) }
  }

  return (
    <div>
      <div className="mb-1.5 flex min-h-6 flex-wrap items-center gap-x-2 gap-y-1 px-1 text-xs text-muted-foreground">
        {working ? <span>{title} is working. It reads what you write at its next step.<span className="hidden md:inline"> <kbd className="font-sans">Esc</kbd> stops it.</span></span>
          : continuing ? (
            <>
              <span>Continues the current task</span>
              <button onClick={() => setFresh(true)} className="inline-flex items-center gap-1 rounded-md bg-foreground/[0.05] px-1.5 py-0.5 hover:bg-accent hover:text-foreground"><Plus className="size-3" /> New task</button>
            </>
          ) : task ? (
            <>
              <span>Starts a new task</span>
              <button onClick={() => setFresh(false)} className="inline-flex items-center gap-1 rounded-md px-1 py-0.5 hover:bg-accent hover:text-foreground"><X className="size-3" /> Keep the current one</button>
            </>
          ) : closed ? <span>The last task is closed (its change was {closed}): this starts a new one.</span>
          : <span>Starts a task<span className="hidden md:inline">: {title} asks you here before anything that needs your OK</span>.</span>}
      </div>
      <form onSubmit={(e) => { e.preventDefault(); send() }} className="relative flex items-end gap-2 rounded-2xl bg-bubble p-2">
        {shown.length > 0 && (
          <div role="listbox" aria-label={at?.kind === "/" ? "Tools" : "Agents"} className="absolute bottom-full left-2 z-20 mb-1.5 w-72 overflow-hidden rounded-xl border bg-popover p-1 shadow-lg">
            {shown.map((c, i) => (
              <button key={c.key} type="button" role="option" aria-selected={i === pick} onMouseDown={(e) => { e.preventDefault(); choose(c.key) }}
                className={cn("flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[13px]", i === pick && "bg-accent")}>
                {at?.kind === "/" ? <Slash className="size-3.5 text-muted-foreground" /> : <AtSign className="size-3.5 text-muted-foreground" />}
                <span className="font-medium">{c.label}</span>
                <span className="ml-auto truncate text-xs text-muted-foreground">{c.hint}</span>
              </button>
            ))}
          </div>
        )}
        <textarea
          ref={box}
          value={text}
          onChange={(e) => { setText(e.target.value); look(e.target.value, e.target.selectionStart) }}
          onClick={(e) => look(text, e.currentTarget.selectionStart)}
          onBlur={() => setAt(null)}
          onKeyDown={(e) => {
            // The menu of tools or agents takes the arrows, ↵ and Tab first.
            if (shown.length) {
              if (e.key === "ArrowDown") { e.preventDefault(); setPick((pick + 1) % shown.length); return }
              if (e.key === "ArrowUp") { e.preventDefault(); setPick((pick - 1 + shown.length) % shown.length); return }
              if (e.key === "Enter" || e.key === "Tab") { e.preventDefault(); choose(shown[pick].key); return }
              if (e.key === "Escape") { e.preventDefault(); setAt(null); return }
            }
            // ↵ or ⌘↵ sends, as the founder chose in Settings; the other adds a line.
            const mod = e.metaKey || e.ctrlKey
            if (e.key === "Enter" && !e.shiftKey && (prefs.send === "mod" ? mod : !mod)) { e.preventDefault(); send() }
            else if (e.key === "ArrowUp" && !text && last) { e.preventDefault(); setText(last) }
            // Esc stops what the agent is doing.
            else if (e.key === "Escape" && working && task) { e.preventDefault(); stop() }
          }}
          rows={1}
          placeholder={`Write to ${title}${tools.length ? " · / a tool" : ""}${others.length ? " · @ another agent" : ""}`}
          className="max-h-40 min-h-8 flex-1 resize-none bg-transparent px-2 py-1.5 text-[13.5px] outline-none placeholder:text-muted-foreground field-sizing-content"
        />
        <MicButton value={text} onChange={setText} className="mb-0.5" />
        <Button type="submit" size="icon" className="rounded-full" disabled={busy || !text.trim()} aria-label="Send"><ArrowUp /></Button>
      </form>
    </div>
  )
}
