import { useEffect, useRef, useState } from "react"
import { toast } from "sonner"
import { BookOpen, Brain, ChevronLeft, ChevronRight, Download, Mail, Repeat, Settings2, ShieldCheck, UserRound, Wrench } from "lucide-react"
import { Back } from "@/components/page"
import { Character } from "@/components/character"
import { DepartmentField } from "@/components/department-field"
import { Rich } from "@/components/rich"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { useTools } from "@/hooks/use-tools"
import { api, called, departmentName, enc, type Agent, type Department, type Tasks } from "@/lib/api"
import { LOGOS } from "@/lib/logos"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"
import { Conversation } from "@/screens/conversation"
import { MailboxRow, type Box } from "@/screens/settings-mail"

const STATE: Record<Agent["state"], string> = {
  needs: "Waiting for you", incident: "Stopped on an incident", working: "Working now", idle: "Between runs", asleep: "Nothing scheduled",
}

const SECTIONS = [
  { id: "profile", label: "Profile", hint: "Who it is and its job", icon: UserRound },
  { id: "rights", label: "Rights", hint: "What it may do alone", icon: ShieldCheck },
  { id: "tools", label: "Tools", hint: "Connectors and skills", icon: Wrench },
  { id: "routines", label: "Routines", hint: "Its recurring work", icon: Repeat },
  { id: "memory", label: "Memory", hint: "Your lessons", icon: Brain },
  { id: "mail", label: "Mail", hint: "Its mailbox", icon: Mail },
] as const
type Section = (typeof SECTIONS)[number]["id"]

type Form = { firstName: string; title: string; department: string; job: string }
/** Profile drafts not saved yet, by business and agent: leaving the page
    or switching agent keeps them, until saved or undone. */
const drafts = new Map<string, Form>()
const draftKey = (realm: string, agent: string) => `${realm}/${agent}`
// Closing or reloading the window while any draft waits asks first,
// wherever in Airth the founder is; leaving a page within Airth keeps it.
window.addEventListener("beforeunload", (e) => { if (drafts.size) e.preventDefault() })

/** An agent: its conversation first, as a chat is; Edit opens what it is
    and what it may do, section by section (decision 140). */
export function AgentPage({ realm, agent, agents, departments, tasks, narrow, onAgent, onDone, onSaved, onGone, onBack }: {
  realm: string; agent: Agent; agents: Agent[]; departments: Department[]; tasks: Tasks | null; narrow: boolean
  onAgent: (name: string) => void; onDone: () => void; onSaved: () => void; onGone: () => void; onBack: () => void
}) {
  const [editing, setEditing] = useState(false)
  if (editing) return <AgentEditor realm={realm} agent={agent} departments={departments} narrow={narrow} onClose={() => setEditing(false)} onSaved={onSaved} onGone={onGone} />
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className={cn("flex h-14 shrink-0 items-center gap-2 border-b px-3 md:px-5", narrow && "h-[60px] pl-[68px]")}>
        {narrow && <Back label="All agents" onClick={onBack} />}
        {/* The head opens the agent, as a contact's does in a chat. */}
        <button onClick={() => setEditing(true)} className="-ml-1.5 flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-1.5 py-1 text-left transition-colors hover:bg-accent/60">
          <Character agent={agent} size={32} still={narrow} />
          <span className="min-w-0">
            <span className="block truncate text-[15px] leading-tight font-semibold">{called(agent)}</span>
            <span className="block truncate text-xs text-muted-foreground">{departmentName(agent.department)} · {STATE[agent.state]}</span>
          </span>
        </button>
        <Button variant="outline" size="sm" className="relative" onClick={() => setEditing(true)}>
          <Settings2 />Edit
          {drafts.has(draftKey(realm, agent.name)) && <span className="absolute -top-1 -right-1 size-2.5 rounded-full border-2 border-background bg-amber-500" aria-label="unsaved changes" />}
        </Button>
      </header>
      <Conversation key={agent.name} realm={realm} agent={agent} agents={agents} onAgent={onAgent} tasks={tasks} onDone={onDone} />
    </div>
  )
}

/** The agent's edit page: one section at a time, a side list on a wide
    screen, a row of icons on a phone. The profile keeps what is typed
    while another section is open; leaving with it unsaved asks first. */
function AgentEditor({ realm, agent, departments, narrow, onClose, onSaved, onGone }: {
  realm: string; agent: Agent; departments: Department[]; narrow: boolean; onClose: () => void; onSaved: () => void; onGone: () => void
}) {
  const [section, setSection] = useState<Section>("profile")
  const [dirty, setDirty] = useState(false)
  const [asking, setAsking] = useState(false)
  const scroller = useRef<HTMLDivElement>(null)
  const leave = () => (dirty ? setAsking(true) : onClose())

  // Escape goes back to the conversation, unless the founder is typing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null
      if (e.key !== "Escape" || e.defaultPrevented || t?.closest("input, textarea, select, [contenteditable], [role=dialog]")) return
      leave()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  })

  function go(s: Section) {
    setSection(s)
    scroller.current?.scrollTo({ top: 0 })
  }

  async function exportAgent() {
    try {
      const pkg = await api<object>(`realms/${enc(realm)}/agents/${enc(agent.name)}/export`)
      const url = URL.createObjectURL(new Blob([JSON.stringify(pkg, null, 2)], { type: "application/json" }))
      const a = Object.assign(document.createElement("a"), { href: url, download: `${agent.name}.airth-agent.json` })
      a.click()
      URL.revokeObjectURL(url)
      toast.success(`${called(agent)} exported: its file, routines and prompts, without this business's mailbox, budget or lessons.`)
    } catch (e) { toast.error((e as Error).message) }
  }

  const current = SECTIONS.find((s) => s.id === section)!
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className={cn("flex h-14 shrink-0 items-center gap-2 border-b px-3 md:px-5", narrow && "h-[60px] pl-[68px]")}>
        <Button variant="ghost" size="sm" className="-ml-1.5 shrink-0 px-2" onClick={leave} aria-label="Back to the conversation">
          <ChevronLeft /><span className="max-md:hidden">Conversation</span>
        </Button>
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <Character agent={agent} size={28} still />
          <div className="min-w-0">
            <div className="truncate text-[15px] leading-tight font-semibold">{called(agent)}</div>
            <div className="truncate text-xs text-muted-foreground">Settings · {departmentName(agent.department)}</div>
          </div>
        </div>
        <Button variant="ghost" size="sm" onClick={exportAgent} aria-label="Export"><Download /><span className="max-md:hidden">Export</span></Button>
      </header>
      {narrow && (
        <nav className="grid shrink-0 grid-cols-6 border-b px-1" aria-label="Sections">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button key={id} onClick={() => go(id)} aria-current={section === id ? "page" : undefined}
              className={cn("relative flex flex-col items-center gap-1 py-2 text-[11px] text-muted-foreground transition-colors",
                section === id && "text-foreground after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground")}>
              <Icon className="size-[18px]" />{label}
            </button>
          ))}
        </nav>
      )}
      <div className="flex min-h-0 flex-1">
        {!narrow && (
          <nav className="w-60 shrink-0 space-y-0.5 overflow-y-auto border-r p-3" aria-label="Sections">
            {SECTIONS.map(({ id, label, hint, icon: Icon }) => (
              <button key={id} onClick={() => go(id)} aria-current={section === id ? "page" : undefined}
                className={cn("group flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors",
                  section === id ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/50 hover:text-foreground")}>
                <Icon className="size-4 shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium">{label}{id === "profile" && dirty && <span className="ml-1.5 inline-block size-1.5 rounded-full bg-amber-500 align-middle" aria-label="unsaved" />}</span>
                  <span className="block truncate text-xs text-muted-foreground">{hint}</span>
                </span>
                <ChevronRight className={cn("size-3.5 shrink-0 opacity-0 transition-opacity", section === id && "opacity-60")} />
              </button>
            ))}
          </nav>
        )}
        <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-3xl space-y-6 px-4 pt-6 pb-32 md:px-8 md:pb-10">
            {!narrow && <h2 className="text-lg font-semibold">{current.label}</h2>}
            {/* The profile stays mounted, so what is typed survives a look at another section. */}
            <div className={cn("space-y-6", section !== "profile" && "hidden")}>
              <Profile realm={realm} agent={agent} departments={departments} onSaved={onSaved} onGone={onGone} onDirty={setDirty} />
            </div>
            {section === "rights" && <Rights realm={realm} agent={agent} />}
            {section === "tools" && <Tools key={agent.name} realm={realm} agent={agent} onSaved={onSaved} />}
            {section === "routines" && <Routines agent={agent} />}
            {section === "memory" && <Memory realm={realm} agent={agent} onSaved={onSaved} />}
            {section === "mail" && <AgentMail realm={realm} agent={agent} />}
          </div>
        </div>
      </div>
      <Dialog open={asking} onOpenChange={setAsking}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Leave without saving?</DialogTitle>
            <DialogDescription>The changes to {called(agent)}'s profile are not saved yet.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setAsking(false)}>Keep editing</Button>
            <Button variant="destructive" onClick={() => { drafts.delete(draftKey(realm, agent.name)); setAsking(false); onClose() }}>Discard</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function Title({ children, note }: { children: React.ReactNode; note?: React.ReactNode }) {
  return (
    <div className="mb-2.5 flex items-baseline justify-between gap-3">
      <h3 className="text-sm font-semibold">{children}</h3>
      {note && <span className="text-xs text-muted-foreground">{note}</span>}
    </div>
  )
}

function Stat({ label, value, tone }: { label: string; value: React.ReactNode; tone?: "warn" | "bad" }) {
  return (
    <div className="rounded-xl bg-bubble px-3.5 py-2.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cn("mt-0.5 text-base font-semibold tabular-nums", tone === "warn" && "text-amber-500", tone === "bad" && "text-red-500")}>{value}</div>
    </div>
  )
}

type Budgets = { agents: { agent: string; cap: number | null; spent: number }[] }

/** Who it is and its job, written in its file in the business folder; only
    the founder edits them, runs cannot. Letting it go is here too. */
function Profile({ realm, agent, departments, onSaved, onGone, onDirty }: {
  realm: string; agent: Agent; departments: Department[]; onSaved: () => void; onGone: () => void; onDirty: (dirty: boolean) => void
}) {
  const budgets = usePoll<Budgets>(() => api(`realms/${enc(realm)}/budgets`), 60000, [realm])
  const month = budgets.data?.agents.find((b) => b.agent === agent.name)
  const share = month?.cap ? month.spent / month.cap : 0
  const last = agent.routines.map((r) => r.last?.at ?? "").sort().at(-1)

  const saved: Form = { firstName: agent.first_name ?? "", title: agent.title, department: agent.department ?? "", job: agent.body || agent.job || "" }
  const draft = draftKey(realm, agent.name)
  const [form, setForm] = useState<Form>(() => drafts.get(draft) ?? saved)
  const [busy, setBusy] = useState(false)
  const [leaving, setLeaving] = useState(false)
  // The server trims what it stores, so spaces alone change nothing.
  const dirty = form.firstName.trim() !== saved.firstName || form.title.trim() !== saved.title || form.department !== saved.department || form.job.trim() !== saved.job.trim()
  useEffect(() => onDirty(dirty), [dirty, onDirty])
  useEffect(() => { if (dirty) drafts.set(draft, form); else drafts.delete(draft) }, [dirty, form, draft])
  // What was saved elsewhere comes in, unless the founder is mid-edit.
  const key = JSON.stringify(saved)
  useEffect(() => { if (!dirty) setForm(saved) }, [key]) // eslint-disable-line react-hooks/exhaustive-deps
  const set = (k: keyof Form) => (v: string) => setForm((f) => ({ ...f, [k]: v }))
  const valid = form.title.trim() && form.job.trim() && form.department

  async function save() {
    setBusy(true)
    try {
      const sent: Form = { firstName: form.firstName.trim(), title: form.title.trim(), department: form.department, job: form.job.trim() }
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, { title: sent.title, first_name: sent.firstName, department: sent.department, job: sent.job }, "PUT")
      setForm(sent)
      drafts.delete(draft)
      toast.success(`${sent.firstName || sent.title} is saved.`)
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }

  // Its file leaves agents/; its past tasks stay. The server refuses while
  // a routine still names it, and says which.
  async function letGo() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, undefined, "DELETE")
      drafts.delete(draft)
      toast.success(`${called(agent)} left the team.`)
      onGone()
    } catch (e) {
      toast.error((e as Error).message)
      setLeaving(false)
    } finally { setBusy(false) }
  }

  return (
    <>
      <section className="space-y-4">
        <div className="flex items-center gap-4">
          <Character agent={{ ...agent, department: form.department || null, title: form.title }} size={56} />
          <div className="min-w-0 text-sm">
            <div className="font-medium">{form.firstName.trim() || form.title || "—"}</div>
            <div className="text-muted-foreground">{departmentName(form.department || null)}</div>
            {agent.look_saved === false && (
              <button className="mt-0.5 text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground" onClick={async () => {
                try { await api(`realms/${enc(realm)}/agents/${enc(agent.name)}/look`, {}); toast.success("Look kept."); onSaved() } catch (e) { toast.error((e as Error).message) }
              }}>Its look comes from its name. Keep this look</button>
            )}
          </div>
        </div>
        <div className="grid gap-3 sm:grid-cols-3">
          <div className="space-y-1.5">
            <Label htmlFor="agent-first-name">First name</Label>
            <Input id="agent-first-name" value={form.firstName} placeholder="Optional" maxLength={30} onChange={(e) => set("firstName")(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="agent-title">Title</Label>
            <Input id="agent-title" value={form.title} onChange={(e) => set("title")(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="agent-department">Department</Label>
            <DepartmentField id="agent-department" value={form.department} departments={departments} onChange={set("department")} />
          </div>
        </div>
      </section>

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Stat label="Last 30 days" value={usd(agent.cost_usd) || "$0"} />
        <Stat label="This month" value={month ? `${usd(month.spent) || "$0"}${month.cap ? ` / ${usd(month.cap)}` : ""}` : "…"} tone={share >= 1 ? "bad" : share >= 0.8 ? "warn" : undefined} />
        <Stat label="Waiting for you" value={agent.waiting} />
        <Stat label="Last ran" value={last ? ago(last) : "—"} />
      </div>

      <AgentModel realm={realm} agent={agent} onSaved={onSaved} />

      <section className="space-y-1.5">
        <Label htmlFor="agent-job">Job</Label>
        <p className="text-xs text-muted-foreground">Its mission, how it works and what it reports, in Markdown. It reads this on every task.</p>
        <Textarea id="agent-job" value={form.job} onChange={(e) => set("job")(e.target.value)} className="field-sizing-content min-h-72 font-mono text-[12.5px] leading-relaxed" />
        {!dirty && form.job && (
          <details className="group rounded-xl bg-bubble px-4 py-3 text-sm">
            <summary className="cursor-pointer text-xs text-muted-foreground select-none">As it reads</summary>
            <div className="mt-2 leading-relaxed"><Rich text={form.job} /></div>
          </details>
        )}
      </section>

      {dirty && (
        <div className="sticky bottom-4 z-10 flex items-center gap-2 rounded-xl border bg-background/95 px-4 py-2.5 shadow-lg backdrop-blur max-md:bottom-24">
          <span className="flex-1 text-sm text-muted-foreground">Unsaved changes</span>
          <Button variant="ghost" size="sm" onClick={() => setForm(saved)}>Undo</Button>
          <Button size="sm" disabled={busy || !valid} onClick={save}>{busy ? "Saving…" : "Save"}</Button>
        </div>
      )}

      <section className="rounded-xl border border-red-500/25 px-4 py-3.5">
        <Title>Let {called(agent)} go</Title>
        <p className="text-sm text-muted-foreground">Its file leaves the team; its past tasks and conversation stay. Refused while a routine still names it.</p>
        <div className="mt-3 flex gap-2">
          {leaving ? (
            <>
              <Button variant="destructive" size="sm" disabled={busy} onClick={letGo}>Let {called(agent)} go</Button>
              <Button variant="ghost" size="sm" onClick={() => setLeaving(false)}>Keep</Button>
            </>
          ) : <Button variant="outline" size="sm" className="text-red-500 hover:text-red-500" onClick={() => setLeaving(true)}>Let go…</Button>}
        </div>
      </section>
    </>
  )
}

type Providers = { providers: { id: string; name: string; configured: boolean; models: string[] }[] }
const BUSINESS = "-business"

/** The model its tasks use (decision 141): the business's, or one of the
    models a configured provider offers. Saved as soon as it is picked. */
function AgentModel({ realm, agent, onSaved }: { realm: string; agent: Agent; onSaved: () => void }) {
  const options = usePoll<Providers>(() => api(`realms/${enc(realm)}/models`), 600000, [realm])
  const [busy, setBusy] = useState(false)
  const current = agent.model ?? BUSINESS
  const ready = (options.data?.providers ?? []).filter((p) => p.configured && p.models.length)
  async function pick(v: string) {
    if (v === current) return
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, { job: agent.body || agent.job || "", model: v === BUSINESS ? "" : v }, "PUT")
      toast.success(v === BUSINESS ? `${called(agent)} uses the business's model.` : `${called(agent)} now works with ${v.split("/")[1]}.`)
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  return (
    <section className="flex items-center gap-4 max-sm:flex-col max-sm:items-start max-sm:gap-1.5">
      <div className="min-w-0 flex-1">
        <Label>Model</Label>
        <p className="text-xs text-muted-foreground">What its tasks and agent routines think with. A model picked in a conversation still wins there.</p>
      </div>
      <Select value={current} disabled={busy} onValueChange={(v) => v && pick(String(v))}>
        <SelectTrigger className="w-64 max-sm:w-full" aria-label="Model"><SelectValue>{(v: string) => (v === BUSINESS ? "The business's model" : v.split("/")[1] ?? v)}</SelectValue></SelectTrigger>
        <SelectContent>
          <SelectItem value={BUSINESS}>The business's model</SelectItem>
          {/* One no longer offered stays shown, so it can be changed. */}
          {current !== BUSINESS && !ready.some((p) => p.models.some((m) => `${p.id}/${m}` === current)) && <SelectItem value={current}>{current}</SelectItem>}
          {ready.map((p) => (
            <SelectGroup key={p.id}>
              <SelectLabel>{p.name}</SelectLabel>
              {p.models.map((m) => <SelectItem key={m} value={`${p.id}/${m}`}>{m}</SelectItem>)}
            </SelectGroup>
          ))}
        </SelectContent>
      </Select>
    </section>
  )
}

type Level = { action: string; class: "read" | "reversible" | "commits"; ceiling: number; start: number; classed: boolean; level: number }

const LEVELS = [
  { n: 0, name: "Prepare", hint: "It prepares; you do it." },
  { n: 1, name: "Propose", hint: "It asks in Needs you first." },
  { n: 2, name: "Act with delay", hint: "It acts after a window you can cancel." },
  { n: 3, name: "Act", hint: "It acts, then reports." },
]

const CLASS: Record<Level["class"], string> = { read: "Reads", reversible: "Reversible", commits: "Commits" }

/** What it may do alone, action by action (decision 96): only the founder
    raises a level, never past the action's ceiling; an incident lowers it. */
function Rights({ realm, agent }: { realm: string; agent: Agent }) {
  const levels = usePoll<{ actions: Level[] }>(() => api(`realms/${enc(realm)}/agents/${enc(agent.name)}/levels`), 60000, [realm, agent.name])
  const [busy, setBusy] = useState<string | null>(null)
  async function set(action: string, level: number) {
    setBusy(action)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}/levels/${enc(action)}`, { level }, "PUT")
      toast.success(`${called(agent)} takes ${action} at level ${level}: ${LEVELS[level].name.toLowerCase()}.`)
      levels.refresh()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(null) }
  }
  const groups = new Map<string, Level[]>()
  for (const a of levels.data?.actions ?? []) {
    const k = a.action.split(".")[0]
    groups.set(k, [...(groups.get(k) ?? []), a])
  }
  return (
    <>
      <div className="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
        {LEVELS.map((l) => (
          <div key={l.n} className="rounded-lg border px-2.5 py-2"><span className="font-medium">{l.n} · {l.name}</span><p className="mt-0.5 text-muted-foreground">{l.hint}</p></div>
        ))}
      </div>
      {!levels.data ? <p className="text-sm text-muted-foreground">Loading…</p> : !levels.data.actions.length ? (
        <p className="text-sm text-muted-foreground">No action is declared in this business yet. Connect a service in the Marketplace.</p>
      ) : [...groups].map(([group, actions]) => (
        <section key={group}>
          <Title>{group === "airth" ? "Airth" : group.charAt(0).toUpperCase() + group.slice(1)}</Title>
          <div className="divide-y rounded-xl bg-bubble">
            {actions.map((a) => (
              <div key={a.action} className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3.5 py-2.5">
                <div className="min-w-0 flex-1 basis-48">
                  <div className="truncate font-mono text-[13px]">{a.action.slice(group.length + 1)}</div>
                  <div className="text-xs text-muted-foreground">{CLASS[a.class]}{!a.classed && " · not classed yet"} · up to {a.ceiling}</div>
                </div>
                <div className="flex shrink-0 rounded-lg border p-0.5" role="radiogroup" aria-label={`${a.action} level`}>
                  {LEVELS.map((l) => (
                    <button key={l.n} role="radio" aria-checked={a.level === l.n} title={l.n > a.ceiling ? `Never above ${a.ceiling} for this action` : l.name}
                      disabled={l.n > a.ceiling || busy === a.action} onClick={() => a.level !== l.n && set(a.action, l.n)}
                      className={cn("h-8 w-9 rounded-md text-sm tabular-nums transition-colors disabled:opacity-30",
                        a.level === l.n ? "bg-foreground text-background" : "text-muted-foreground hover:bg-accent")}>
                      {l.n}
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      ))}
      <p className="text-xs text-muted-foreground">Content from outside (a post, a page, a mail) still waits for your approval unless you raised the level. Payments, contracts, merging and deploying never go above 1.</p>
    </>
  )
}

/** What its tasks may use (decision 64): nothing else. Each switch is
    saved as it flips, one at a time. */
function Tools({ realm, agent, onSaved }: { realm: string; agent: Agent; onSaved: () => void }) {
  const tools = useTools(realm)
  const [on, setOn] = useState({ integrations: agent.integrations ?? [], skills: agent.skills ?? [], repos: agent.repos ?? [] })
  const [busy, setBusy] = useState<string | null>(null)
  async function flip(kind: keyof typeof on, name: string) {
    const before = on
    const next = { ...on, [kind]: on[kind].includes(name) ? on[kind].filter((x) => x !== name) : [...on[kind], name] }
    setOn(next)
    setBusy(name)
    try {
      // The job as stored now, not as this page last saw it: this write
      // sends it back whole.
      const stored = (await api<{ agents: { name: string; job: string }[] }>(`realms/${enc(realm)}/agents`)).agents.find((a) => a.name === agent.name)
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, { job: stored?.job ?? agent.body ?? agent.job ?? "", ...next }, "PUT")
      toast.success(next[kind].includes(name) ? `${called(agent)} can use ${name}.` : `${called(agent)} no longer uses ${name}.`)
      onSaved()
    } catch (e) {
      setOn(before)
      toast.error((e as Error).message)
    } finally { setBusy(null) }
  }
  const group = (title: string, hint: string, names: { name: string; hint?: string }[], kind: keyof typeof on, mark?: string) => names.length > 0 && (
    <section>
      <Title note={hint}>{title}</Title>
      <div className="flex flex-wrap gap-2">
        {names.map(({ name, hint }) => <Toggle key={name} label={name} hint={hint} mark={mark} on={on[kind].includes(name)} disabled={busy !== null} onClick={() => flip(kind, name)} />)}
      </div>
    </section>
  )
  if (!tools.data) return <p className="text-sm text-muted-foreground">Loading…</p>
  const none = !tools.data.integrations.length && !tools.data.servers.length && !tools.data.skills.length && !tools.data.repos.length
  return (
    <>
      <p className="text-sm text-muted-foreground">Its tasks use only what is on. With nothing on, it reads the public web and no more.</p>
      {none && <p className="text-sm text-muted-foreground">This business has no connector or skill yet: connect them in the Marketplace.</p>}
      {group("Connectors", "Services it calls", tools.data.integrations.map((name) => ({ name })), "integrations")}
      {group("MCP servers", "Tools from a server", tools.data.servers, "skills")}
      {group("Skills", "Know-how it can load", tools.data.skills.map((name) => ({ name })), "skills", "✦")}
      {group("Code", "Each task on its own branch", tools.data.repos.map((name) => ({ name })), "repos")}
    </>
  )
}

function Toggle({ label, hint, mark: glyph, on, disabled, onClick }: { label: string; hint?: string; mark?: string; on: boolean; disabled?: boolean; onClick: () => void }) {
  const mark = LOGOS[label]
  return (
    <button type="button" onClick={onClick} title={hint} aria-pressed={on} disabled={disabled}
      className={cn("inline-flex h-9 items-center gap-2 rounded-full border px-3 text-sm transition-colors disabled:cursor-wait",
        on ? "border-sky-500/60 bg-sky-500/10 text-foreground" : "text-muted-foreground hover:text-foreground")}>
      {mark ? <svg viewBox="0 0 24 24" className="size-4" style={{ color: mark[0] ?? "currentColor" }}><path fill="currentColor" d={mark[1]} /></svg>
        : glyph ? <span className="text-xs">{glyph}</span> : null}
      {label}
    </button>
  )
}

function Routines({ agent }: { agent: Agent }) {
  if (!agent.routines.length) return <p className="text-sm text-muted-foreground">No routine yet. Make one on the Routines page, or install a team from the Marketplace.</p>
  return (
    <div className="divide-y rounded-xl bg-bubble">
      {agent.routines.map((r) => (
        <div key={r.name} className="flex items-center gap-3 px-3.5 py-3">
          <span className={cn("size-2 shrink-0 rounded-full", r.last ? (r.last.ok ? "bg-emerald-400" : "bg-red-500") : "bg-zinc-400")} />
          <span className="min-w-0 flex-1 truncate font-mono text-[13px]">{r.name}</span>
          <span className="shrink-0 text-xs text-muted-foreground">{r.schedule ?? "on demand"}{r.last && ` · ${r.last.ok ? "ran" : "failed"} ${ago(r.last.at)}`}</span>
        </div>
      ))}
    </div>
  )
}

/** The founder's lessons (decision 127): kept in its file, read by its
    tasks and its routines. They stay with this business. */
function Memory({ realm, agent, onSaved }: { realm: string; agent: Agent; onSaved: () => void }) {
  const [text, setText] = useState("")
  const [busy, setBusy] = useState(false)
  async function add() {
    if (!text.trim()) return
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}/lessons`, { text: text.trim() })
      setText("")
      toast.success("Kept: it reads this on every task and routine.")
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  const lessons = agent.lessons ?? []
  return (
    <>
      <section>
        <Title note={`${lessons.length} kept`}>Lessons from you</Title>
        {lessons.length ? (
          <ul className="divide-y rounded-xl bg-bubble">
            {lessons.map((l, i) => <li key={i} className="flex gap-2.5 px-3.5 py-2.5 text-sm"><BookOpen className="mt-0.5 size-4 shrink-0 text-muted-foreground" />{l}</li>)}
          </ul>
        ) : <p className="text-sm text-muted-foreground">None yet. What you tell it here, it keeps.</p>}
      </section>
      <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); add() }}>
        <Input value={text} onChange={(e) => setText(e.target.value)} placeholder="A lesson, e.g. never promise a delivery date" className="h-10 flex-1" />
        <Button type="submit" className="h-10" disabled={busy || !text.trim()}>Keep</Button>
      </form>
      <p className="text-xs text-muted-foreground">Lessons stay with this business: an exported agent leaves them behind.</p>
    </>
  )
}

function AgentMail({ realm, agent }: { realm: string; agent: Agent }) {
  const boxes = usePoll<{ mailboxes: Box[] }>(() => api(`realms/${enc(realm)}/mailboxes`), 60000, [realm])
  const box = boxes.data?.mailboxes.find((b) => b.agent === agent.name)
  return (
    <>
      <p className="text-sm text-muted-foreground">Its own address on the business's domain. What it receives is read as data, never as orders; every mail it sends waits for your approval, signed as prepared by an AI agent, with a copy to archive@.</p>
      {!boxes.data ? <p className="text-sm text-muted-foreground">Loading…</p> : box ? (
        <div className="rounded-xl bg-bubble"><MailboxRow key={`${box.agent}:${box.address ?? ""}`} realm={realm} box={box} onSaved={boxes.refresh} /></div>
      ) : <p className="text-sm text-muted-foreground">Not found.</p>}
    </>
  )
}

