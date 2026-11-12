import { useEffect, useMemo, useRef, useState } from "react"
import { ArrowRight, ArrowUp, CalendarClock, Flag, Inbox, LayoutDashboard, Lightbulb, Loader, MessageSquare, Plus, Sparkles, Users } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { MicButton } from "@/components/mic"
import { Button } from "@/components/ui/button"
import { usePoll } from "@/hooks/use-poll"
import { api, called, enc, type Agent, type Business, type Calendar, type Company, type Item, type Tasks } from "@/lib/api"
import { sendOrder } from "@/lib/orders"
import { cn } from "@/lib/utils"
import { Conversation } from "@/screens/conversation"
import type { Page } from "@/components/rail"

/** What a suggestion does: fill the box with words to send, or open a page. */
type Suggestion = { key: string; icon: typeof Inbox; label: string; text?: string; page?: Page }

const SHOWN = 4
const EVERY = 8000

/** The Home is a conversation with the founder's interlocutor (decision
    120), as a chat app's: on a new day, a greeting, one box to ask, and a few
    suggestions drawn from what Airth knows; once asked, the conversation
    itself. Whatever is asked goes on as a task or a project (a plan). The
    figures are one line at the top; the rest is the Overview page. */
export function Home({ realm, agents, items, business, tasks, onTasks, onAgent, onTask, onPage }: {
  realm: string; agents: Agent[] | null; items: Item[]; business: Business | null; tasks: Tasks | null
  onTasks: () => void; onAgent: (name: string) => void; onTask: (id: string) => void; onPage: (p: Page) => void
}) {
  const team = agents ?? []
  const lead = team.find((a) => a.dispatch) ?? null
  const owner = usePoll<{ name: string }>(() => api("owner"), 600000, []).data
  const company = usePoll<Company>(() => api(`realms/${enc(realm)}/company`), 300000, [realm]).data
  const cal = usePoll<Calendar>(() => api(`realms/${enc(realm)}/calendar`), 300000, [realm]).data
  const first = owner?.name?.trim().split(/\s+/)[0] || null

  // The conversation opens when the interlocutor already has today's task;
  // a new day starts on the greeting. The founder switches either way.
  const leadTasks = useMemo(() => Object.values(tasks ?? {}).flat().filter((t) => lead && t.agent === lead.name).map((t) => t.id).sort(), [tasks, lead])
  const today = new Date().toISOString().slice(0, 10).replace(/-/g, "")
  const [mode, setMode] = useState<"start" | "thread" | null>(null)
  useEffect(() => setMode(null), [realm])
  const shown = lead && (mode ?? (leadTasks[leadTasks.length - 1]?.startsWith(today) ? "thread" : "start")) === "thread" ? "thread" : "start"

  const status = <StatusLine items={items} tasks={tasks} business={business} onPage={onPage}
    action={shown === "thread"
      ? <Button size="sm" variant="ghost" className="h-7 text-muted-foreground" onClick={() => setMode("start")}><Plus /> New request</Button>
      : lead && leadTasks.length > 0 && <Button size="sm" variant="ghost" className="h-7 text-muted-foreground" onClick={() => setMode("thread")}><MessageSquare /> Conversation</Button>} />

  if (shown === "thread" && lead) {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="border-b px-4 py-2 md:px-6">{status}</div>
        <Conversation realm={realm} agent={lead} agents={team} onAgent={onAgent} tasks={tasks} onDone={onTasks} />
      </div>
    )
  }
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex min-h-full w-full max-w-2xl flex-col px-4 pt-4 pb-28 md:px-6 md:pb-10">
        {status}
        <Start realm={realm} lead={lead} first={first} suggestions={suggest(items, team, company, cal)} onPage={onPage}
          onSent={(id) => { onTasks(); if (lead) setMode("thread"); else onTask(id) }} />
      </div>
    </div>
  )
}

/** The figures that matter, in one line: each opens its page. */
function StatusLine({ items, tasks, business, onPage, action }: {
  items: Item[]; tasks: Tasks | null; business: Business | null; onPage: (p: Page) => void; action?: React.ReactNode
}) {
  const waiting = items.length
  const running = (tasks?.running.length ?? 0) + (tasks?.waiting.length ?? 0)
  const f = business?.finance
  const bits: [string, Page, boolean][] = [
    [waiting ? `${waiting} waiting for you` : "Nothing waits for you", "needs", waiting > 0],
    [running ? `${running} in progress` : "Nothing in progress", "tasks", false],
    ...(f && !f.unknown && f.runway ? [[`Runway ${f.runway}`, "finance", false] as [string, Page, boolean]] : []),
  ]
  return (
    <div className="flex flex-wrap items-center gap-x-1 gap-y-1 text-xs text-muted-foreground">
      {bits.map(([label, page, hot], i) => (
        <span key={page} className="flex items-center gap-1">
          {i > 0 && <span aria-hidden>·</span>}
          <button onClick={() => onPage(page)} className={cn("rounded px-1 py-0.5 hover:bg-accent hover:text-foreground", hot && "font-medium text-amber-600 dark:text-amber-400")}>{label}</button>
        </span>
      ))}
      <button onClick={() => onPage("overview")} className="ml-1 flex items-center gap-1 rounded px-1 py-0.5 hover:bg-accent hover:text-foreground"><LayoutDashboard className="size-3.5" /> Overview</button>
      <span className="flex-1" />
      {action}
    </div>
  )
}

/** The greeting, the one box and the suggestions under it. */
function Start({ realm, lead, first, suggestions, onPage, onSent }: {
  realm: string; lead: Agent | null; first: string | null; suggestions: Suggestion[]; onPage: (p: Page) => void; onSent: (id: string) => void
}) {
  const [text, setText] = useState("")
  const [busy, setBusy] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  useEffect(() => { if (matchMedia("(min-width: 768px)").matches) box.current?.focus() }, [])

  async function send() {
    const t = text.trim()
    if (!t || busy) return
    setBusy(true)
    try {
      const out = await sendOrder(realm, t, lead?.name ?? "route")
      setText("")
      if (!lead) toast.success("Sent", { description: "It goes on in Tasks." })
      onSent(out.id)
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  function use(s: Suggestion) {
    if (s.page) { onPage(s.page); return }
    setText(s.text ?? s.label)
    requestAnimationFrame(() => { const el = box.current; if (el) { el.focus(); el.selectionStart = el.selectionEnd = el.value.length } })
  }

  return (
    <div className="flex flex-1 flex-col justify-center py-10">
      <div className="flex flex-col items-center text-center">
        {lead && <Character agent={lead} size={56} />}
        <h1 className="mt-4 text-[28px] leading-tight font-semibold tracking-tight text-balance md:text-[32px]">
          What shall we do today{first ? `, ${first}` : ""}?
        </h1>
        {lead && <p className="mt-2 text-sm text-muted-foreground">{called(lead)} takes it, and passes on what is another's.</p>}
      </div>
      <div className="mt-7 rounded-3xl border bg-surface p-3 shadow-sm focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/20">
        <textarea ref={box} value={text} onChange={(e) => setText(e.target.value)} rows={3} disabled={busy}
          placeholder="Ask for anything: a post, a plan, a figure, a fix…" aria-label="What to ask"
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing && matchMedia("(min-width: 768px)").matches) { e.preventDefault(); send() } }}
          className="w-full resize-none bg-transparent px-1.5 pt-1 text-[16px] leading-relaxed outline-none placeholder:text-muted-foreground" />
        <div className="flex items-center justify-end gap-2">
          <MicButton value={text} onChange={setText} />
          <Button size="icon" className="rounded-full" disabled={busy || !text.trim()} onClick={send} aria-label="Send">
            {busy ? <Loader className="animate-spin" /> : <ArrowUp />}
          </Button>
        </div>
      </div>
      <Suggestions list={suggestions} onUse={use} />
    </div>
  )
}

/** A few suggestions at a time, the next coming in every few seconds; held
    still while the pointer is on them. */
function Suggestions({ list, onUse }: { list: Suggestion[]; onUse: (s: Suggestion) => void }) {
  const [at, setAt] = useState(0)
  const [held, setHeld] = useState(false)
  useEffect(() => {
    if (held || list.length <= SHOWN) return
    const t = setInterval(() => setAt((n) => (n + 1) % list.length), EVERY)
    return () => clearInterval(t)
  }, [held, list.length])
  if (!list.length) return null
  const some = list.length <= SHOWN ? list : Array.from({ length: SHOWN }, (_, i) => list[(at + i) % list.length])
  return (
    <div className="mt-4 grid gap-2 sm:grid-cols-2" onMouseEnter={() => setHeld(true)} onMouseLeave={() => setHeld(false)}>
      {some.map((s) => (
        <button key={s.key} onClick={() => onUse(s)}
          className="group flex items-center gap-2.5 rounded-2xl border px-3.5 py-3 text-left text-sm fade-up transition-colors hover:bg-accent/60">
          <s.icon className="size-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 leading-snug">{s.label}</span>
          {s.page && <ArrowRight className="size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />}
        </button>
      ))}
    </div>
  )
}

const DAY = 86400000
const inDays = (iso: string) => Math.ceil((new Date(`${iso}T12:00:00`).getTime() - Date.now()) / DAY)

/** What to suggest, from what Airth knows, the most pressing first: what
    waits for the founder, an agent's idea, a date coming, the goal's next
    milestone, then what any business may want. Written by code: nothing
    here is a model's guess. */
function suggest(items: Item[], agents: Agent[], company: Company | null | undefined, cal: Calendar | null | undefined): Suggestion[] {
  const out: Suggestion[] = []
  const plan = items.find((i) => i.action === "airth.dispatch")
  if (plan) out.push({ key: `plan${plan.id}`, icon: Users, label: `Review the plan: ${plan.title}`, page: "needs" })
  const waiting = items.filter((i) => i.kind !== "idea" && i.action !== "airth.dispatch").length
  if (waiting) out.push({ key: "needs", icon: Inbox, label: `Go through what waits for you (${waiting})`, page: "needs" })
  for (const idea of items.filter((i) => i.kind === "idea").slice(0, 2)) {
    const a = agents.find((x) => x.name === idea.agent)
    out.push({ key: `idea${idea.id}`, icon: Lightbulb, label: idea.title, text: `Go ahead with ${a ? `${called(a)}'s idea` : "this idea"}: ${idea.title}` })
  }
  for (const d of (cal?.deadlines.items ?? []).filter((d) => !d.filed && d.days_left >= 0 && d.days_left <= 45).slice(0, 2)) {
    out.push({ key: `due${d.title}${d.due}`, icon: CalendarClock, label: `${d.title}, due in ${d.days_left} days`, text: `Help me prepare the ${d.title} (${d.authority}), due ${d.due}.` })
  }
  const raw = company?.goal?.milestones as unknown
  const milestones = Array.isArray(raw) ? (raw as string[]) : typeof raw === "string" ? raw.replace(/^\[|\]$/g, "").split(",") : []
  const next = milestones.map((m) => m.trim().match(/^(.*?)\s+by\s+(\d{4}-\d{2}-\d{2})$/)).find((m) => m && inDays(m[2]) >= 0)
  if (next) out.push({ key: "milestone", icon: Flag, label: `${next[1]}, in ${inDays(next[2])} days`, text: `Plan how we reach this milestone: ${next[1]} by ${next[2]}.` })
  out.push(
    { key: "week", icon: Sparkles, label: "What did the team do this week?", text: "What did the team do this week, and what is left to do?" },
    { key: "posts", icon: Sparkles, label: "Plan next week's posts", text: "Plan next week's posts with me: the subjects, and who writes what." },
  )
  return out
}
