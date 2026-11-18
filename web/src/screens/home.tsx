import { useEffect, useMemo, useRef, useState } from "react"
import { ArrowUp, CalendarClock, Flag, FolderKanban, Inbox, Lightbulb, Loader, Mail as MailIcon, MessageSquare, Plus, Sparkles, Users } from "lucide-react"
import { toast } from "sonner"
import { MicButton } from "@/components/mic"
import { Button } from "@/components/ui/button"
import { usePoll } from "@/hooks/use-poll"
import { api, called, enc, type Agent, type Calendar, type Company, type Item, type Project, type Tasks } from "@/lib/api"
import { Dither } from "@/components/dither"
import { greeting, moment } from "@/lib/greeting"
import { glance } from "@/screens/project"
import { sendOrder } from "@/lib/orders"
import { inThread, projectLines } from "@/lib/project-threads"
import { recent, type Mailbox } from "@/lib/mail"
import { Conversation } from "@/screens/conversation"
import { MailThread, type MailLinks } from "@/screens/mail"
import type { Page } from "@/components/rail"

/** What a suggestion does: fill the box with words to send, or open a page
    or a project. */
type Suggestion = { key: string; icon: typeof Inbox; label: string; text?: string; page?: Page; project?: number }

const EVERY = 6000

/** The Home is a conversation with the founder's interlocutor (decision
    120), as a chat app's: on a new day, a greeting, one line to ask that
    grows as it is written, and under it one suggestion at a time, drawn
    from what Airth knows; once asked, the conversation itself. Whatever is
    asked goes on as a task or a project (a plan). What waits and what is
    under way are the tabs' and the rail's; the figures are the Overview
    page's. */
/** The button beside the logo on a phone (Conversation, then New request
    once it is open); a glass pill at the top right on a computer. */
const SLOT = "glass absolute top-2 right-3 z-10 h-8 rounded-full text-muted-foreground max-md:fixed max-md:top-[calc(env(safe-area-inset-top)+8px)] max-md:right-auto max-md:left-16 max-md:z-40 max-md:size-11 max-md:p-0 max-md:text-foreground"

/** New request, the conversation, or the business's mail (decision 152). */
export type HomeMode = "start" | "thread" | "mail"

export function Home({ realm, agents, items, tasks, onTasks, onAgent, onTask, onPage, onProject, mode, onMode, mail }: {
  realm: string; agents: Agent[] | null; items: Item[]; tasks: Tasks | null
  /** The thread shown: the app's, so the address keeps it. */
  mode: HomeMode; onMode: (m: HomeMode) => void
  onTasks: () => void; onAgent: (name: string) => void; onTask: (id: string) => void; onPage: (p: Page) => void; onProject: (id: number) => void
  mail: MailLinks
}) {
  const team = agents ?? []
  const lead = team.find((a) => a.dispatch) ?? null
  const owner = usePoll<{ name: string }>(() => api("owner"), 600000, []).data
  const company = usePoll<Company>(() => api(`realms/${enc(realm)}/company`), 300000, [realm]).data
  const cal = usePoll<Calendar>(() => api(`realms/${enc(realm)}/calendar`), 300000, [realm]).data
  const projects = usePoll<{ projects: Project[] }>(() => api(`realms/${enc(realm)}/projects`), 60000, [realm], realm).data?.projects
  const first = owner?.name?.trim().split(/\s+/)[0] || null

  // Chat opens on New request, every time (the founder's choice); the
  // conversation is one click away, under Conversation. A project's tasks
  // are in its own thread (decision 147), not here.
  const leadTasks = useMemo(() => Object.values(tasks ?? {}).flat().filter((t) => lead && t.agent === lead.name && inThread(t, null)).map((t) => t.id).sort(), [tasks, lead])
  const setMode = onMode
  const shown = lead ? mode : "start"
  // What was asked on the Home goes to the conversation at once, which
  // sends it: the founder sees it there, the agent typing under it.
  const [opening, setOpening] = useState<{ text: string; at: number } | null>(null)
  // The mail thread's line, beside the projects' (decision 152).
  const box = usePoll<Mailbox>(lead ? () => api(`realms/${enc(realm)}/mail`) : null, 60000, [realm, lead?.name], realm).data
  const mailLine = lead ? { count: recent(box?.mails ?? [], new Date()), onOpen: () => setMode("mail") } : undefined

  if (shown === "mail" && lead) {
    return (
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
        <Button size="sm" variant="ghost" aria-label="New request" onClick={() => setMode("start")}
          className={SLOT}>
          <Plus className="size-4 max-md:size-5" /><span className="max-md:hidden">New request</span>
        </Button>
        <MailThread realm={realm} lead={lead} agents={team} links={mail} onConversation={() => setMode("thread")}
          aside={<ProjectLines tasks={tasks} projects={projects} onProject={onProject} />} />
      </div>
    )
  }
  if (shown === "thread" && lead) {
    return (
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
        {/* On a phone it sits beside the logo, in the row above the conversation. */}
        <Button size="sm" variant="ghost" aria-label="New request" onClick={() => setMode("start")}
          className={SLOT}>
          <Plus className="size-4 max-md:size-5" /><span className="max-md:hidden">New request</span>
        </Button>
        <Conversation realm={realm} agent={lead} agents={team} onAgent={onAgent} tasks={tasks} onDone={onTasks} top project={null} opening={opening ?? undefined}
          aside={<ProjectLines tasks={tasks} projects={projects} onProject={onProject} mail={mailLine} />} />
      </div>
    )
  }
  return (
    <div className="relative min-w-0 flex-1 overflow-y-auto">
      <Light />
      {lead && leadTasks.length > 0 && (
        // On a phone, beside the logo: the slot New request takes in the conversation.
        <Button size="sm" variant="ghost" aria-label="Conversation" onClick={() => setMode("thread")}
          className={SLOT}>
          <MessageSquare className="size-4 max-md:size-5" /><span className="max-md:hidden">Conversation</span>
        </Button>
      )}
      {/* On a phone the greeting and the box sit in the lower half, within
          the thumb's reach, clear of the tab bar. */}
      <div className="relative mx-auto flex min-h-full w-full max-w-2xl flex-col justify-center px-4 pt-14 pb-32 max-md:min-h-[calc(100%-var(--dock))] max-md:justify-end max-md:pb-[12vh] md:px-6 md:pb-24">
        <ProjectLines tasks={tasks} projects={projects} onProject={onProject} mail={mailLine} />
        <Start realm={realm} lead={lead} first={first} suggestions={suggest(items, team, company, cal, projects)} onPage={onPage} onProject={onProject}
          onAsk={lead ? (text) => { setOpening({ text, at: Date.now() }); setMode("thread") } : undefined}
          onSent={(id) => { onTasks(); onTask(id) }} />
      </div>
    </div>
  )
}

/** Over the box, one quiet line per project that moved this week: it
    opens the project's thread (decision 147); and the line to the mail's
    thread, with the week's mail (decision 152). */
function ProjectLines({ tasks, projects, onProject, mail }: {
  tasks: Tasks | null; projects: Project[] | undefined; onProject: (id: number) => void; mail?: { count: number; onOpen: () => void }
}) {
  const lines = projectLines(tasks, projects ?? [], new Date())
  if (!lines.length && !mail) return null
  return (
    <div className="mb-2 flex flex-wrap justify-center gap-1.5">
      {mail && (
        <button onClick={mail.onOpen}
          className="glass inline-flex max-w-full items-center gap-1.5 rounded-full px-3 py-1 text-xs text-muted-foreground hover:text-foreground">
          <MailIcon className="size-3.5 shrink-0" />
          <span className="truncate">{mail.count > 0 && `${mail.count} ${mail.count > 1 ? "mails" : "mail"} this week in `}<span className="font-medium text-foreground/80">Mail</span></span>
          <span aria-hidden>·</span><span className="shrink-0">open</span>
        </button>
      )}
      {lines.slice(0, 3).map((l) => (
        <button key={l.id} onClick={() => onProject(l.id)}
          className="glass inline-flex max-w-full items-center gap-1.5 rounded-full px-3 py-1 text-xs text-muted-foreground hover:text-foreground">
          <FolderKanban className="size-3.5 shrink-0" />
          <span className="truncate">{l.count} {l.count > 1 ? "requests" : "request"} in <span className="font-medium text-foreground/80">{l.name}</span></span>
          <span aria-hidden>·</span><span className="shrink-0">open</span>
        </button>
      ))}
    </div>
  )
}

/** The time, again each minute: the greeting and its background follow
    the day while the page stays open. */
function useNow(): Date {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 60 * 1000)
    return () => clearInterval(t)
  }, [])
  return now
}

/** Behind the greeting, the moment's light in grey dots. */
function Light() {
  return <Dither moment={moment(useNow())} />
}

/** The greeting, the one line to ask, and the suggestion under it. */
function Start({ realm, lead, first, suggestions, onPage, onProject, onAsk, onSent }: {
  realm: string; lead: Agent | null; first: string | null; suggestions: Suggestion[]; onPage: (p: Page) => void; onProject: (id: number) => void
  /** With a lead agent: the conversation sends it. */
  onAsk?: (text: string) => void
  /** Without one: routed here, the task's id once sent. */
  onSent: (id: string) => void
}) {
  const now = useNow()
  const [text, setText] = useState("")
  const [busy, setBusy] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  useEffect(() => { if (matchMedia("(min-width: 768px)").matches) box.current?.focus() }, [])
  // One line that grows with what is written, up to a few.
  useEffect(() => {
    const el = box.current
    if (!el) return
    el.style.height = "auto"
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`
  }, [text])

  async function send() {
    const t = text.trim()
    if (!t || busy) return
    if (onAsk) { onAsk(t); return }
    setBusy(true)
    try {
      const out = await sendOrder(realm, t, "route")
      setText("")
      toast.success("Sent", { description: "It goes on in Projects." })
      onSent(out.id)
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  function use(s: Suggestion) {
    if (s.project !== undefined) { onProject(s.project); return }
    if (s.page) { onPage(s.page); return }
    setText(s.text ?? s.label)
    requestAnimationFrame(() => { const el = box.current; if (el) { el.focus(); el.selectionStart = el.selectionEnd = el.value.length } })
  }

  return (
    <div>
      <h1 className="text-center text-[26px] leading-tight font-medium tracking-tight text-balance md:text-[30px]">
        {greeting(now, first)}
      </h1>
      <div className="mt-8 flex items-end gap-1.5 rounded-[26px] bg-bubble py-1.5 pr-1.5 pl-5">
        <textarea ref={box} value={text} onChange={(e) => setText(e.target.value)} rows={1} readOnly={busy}
          placeholder={lead ? `Ask ${lead.first_name ?? lead.title}` : "Ask the team"} aria-label="What to ask"
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing && matchMedia("(min-width: 768px)").matches) { e.preventDefault(); send() } }}
          className="max-h-[200px] min-h-10 flex-1 resize-none bg-transparent py-2 text-[16px] leading-6 outline-none placeholder:text-muted-foreground" />
        <MicButton value={text} onChange={setText} />
        <Button size="icon" className="size-10 shrink-0 rounded-full" disabled={busy || !text.trim()} onClick={send} aria-label="Send">
          {busy ? <Loader className="animate-spin" /> : <ArrowUp />}
        </Button>
      </div>
      <Suggestion list={suggestions} onUse={use} />
    </div>
  )
}

/** One suggestion at a time, in a quiet line, the next coming in every few
    seconds; held still while the pointer is on it. */
function Suggestion({ list, onUse }: { list: Suggestion[]; onUse: (s: Suggestion) => void }) {
  const [at, setAt] = useState(0)
  const [held, setHeld] = useState(false)
  useEffect(() => {
    if (held || list.length < 2) return
    const t = setInterval(() => setAt((n) => (n + 1) % list.length), EVERY)
    return () => clearInterval(t)
  }, [held, list.length])
  if (!list.length) return null
  const s = list[at % list.length]
  return (
    <button key={s.key} onClick={() => onUse(s)} onMouseEnter={() => setHeld(true)} onMouseLeave={() => setHeld(false)}
      className="mt-5 flex w-full items-center gap-3 rounded-xl px-5 py-2 text-left text-[15px] text-muted-foreground fade-up hover:text-foreground">
      <s.icon className="size-4 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{s.label}</span>
    </button>
  )
}

const DAY = 86400000
const inDays = (iso: string) => Math.ceil((new Date(`${iso}T12:00:00`).getTime() - Date.now()) / DAY)

/** What to suggest, from what Airth knows, the most pressing first: what
    waits for the founder, a project late, stuck or near its milestone
    (decision 129), an agent's idea, a date coming, the goal's next
    milestone, then what any business may want. Written by code: nothing
    here is a model's guess. */
function suggest(items: Item[], agents: Agent[], company: Company | null | undefined, cal: Calendar | null | undefined, projects: Project[] | undefined): Suggestion[] {
  const out: Suggestion[] = []
  const plan = items.find((i) => i.action === "airth.dispatch")
  if (plan) out.push({ key: `plan${plan.id}`, icon: Users, label: `Review the project: ${plan.title}`, page: "needs" })
  const waiting = items.filter((i) => i.kind !== "idea" && i.action !== "airth.dispatch").length
  if (waiting) out.push({ key: "needs", icon: Inbox, label: `Go through what waits for you (${waiting})`, page: "needs" })
  for (const p of (projects ?? []).filter((p) => p.status === "active")) {
    const g = glance(p)
    const soon = g.next?.due && inDays(g.next.due) <= 14
    if (g.stuck) out.push({ key: `stuck${p.id}`, icon: FolderKanban, label: `${p.name}: ${g.stuck} task${g.stuck > 1 ? "s" : ""} stuck`, project: p.id })
    else if (g.next && g.when && (g.when.late || soon)) out.push({ key: `next${p.id}`, icon: Flag, label: `${p.name}: ${g.next.title}, ${g.when.text.split(" · ")[1]}`, project: p.id })
  }
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
