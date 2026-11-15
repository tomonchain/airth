import { useState } from "react"
import { toast } from "sonner"
import { BookOpen, Brain, Download, Gauge, Mail, MessageSquare, Pencil, Repeat, ShieldCheck, Wrench } from "lucide-react"
import { Back } from "@/components/page"
import { Character } from "@/components/character"
import { EditAgent } from "@/components/edit-agent"
import { Rich } from "@/components/rich"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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

const TABS = [
  { id: "chat", label: "Conversation", icon: MessageSquare },
  { id: "overview", label: "Overview", icon: Gauge },
  { id: "rights", label: "Rights", icon: ShieldCheck },
  { id: "tools", label: "Tools", icon: Wrench },
  { id: "routines", label: "Routines", icon: Repeat },
  { id: "memory", label: "Memory", icon: Brain },
  { id: "mail", label: "Mail", icon: Mail },
] as const
type Tab = (typeof TABS)[number]["id"]

/** An agent's page (decision 140): who it is and what it may do, in tabs,
    with its package to take to another business. */
export function AgentPage({ realm, agent, agents, departments, tasks, narrow, onAgent, onDone, onSaved, onGone, onBack }: {
  realm: string; agent: Agent; agents: Agent[]; departments: Department[]; tasks: Tasks | null; narrow: boolean
  onAgent: (name: string) => void; onDone: () => void; onSaved: () => void; onGone: () => void; onBack: () => void
}) {
  const [tab, setTab] = useState<Tab>("chat")
  const [editing, setEditing] = useState(false)

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

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="shrink-0 border-b">
        <div className={cn("flex items-center gap-3 px-4 pt-3 pb-2 md:px-6", narrow && "pl-[68px]")}>
          {narrow && <Back label="All agents" onClick={onBack} />}
          <Character agent={agent} size={narrow ? 28 : 40} still={narrow} />
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-[15px] font-semibold md:text-base">{called(agent)}</h1>
            <p className="truncate text-xs text-muted-foreground">{departmentName(agent.department)} · {STATE[agent.state]}</p>
          </div>
          <Button variant="ghost" size="sm" onClick={exportAgent} aria-label="Export"><Download /><span className="max-md:hidden">Export</span></Button>
          <Button variant="outline" size="sm" onClick={() => setEditing(true)} aria-label="Edit"><Pencil /><span className="max-md:hidden">Edit</span></Button>
        </div>
        <nav className="flex gap-0.5 px-2 md:px-4" role="tablist">
          {TABS.map(({ id, label, icon: Icon }) => (
            <button key={id} role="tab" aria-selected={tab === id} aria-label={label} onClick={() => setTab(id)}
              className={cn("relative flex h-10 min-w-0 flex-1 items-center justify-center gap-1.5 rounded-t-md px-2 text-sm text-muted-foreground transition-colors hover:text-foreground md:flex-none md:px-3",
                tab === id && "text-foreground after:absolute after:inset-x-2 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground")}>
              <Icon className="size-4 shrink-0" /><span className="max-md:hidden">{label}</span>
            </button>
          ))}
        </nav>
      </header>
      <EditAgent key={agent.name} realm={realm} agent={agent} departments={departments} open={editing} onOpenChange={setEditing} onSaved={onSaved} onGone={onGone} />
      {tab === "chat" ? (
        <Conversation key={agent.name} realm={realm} agent={agent} agents={agents} onAgent={onAgent} tasks={tasks} onDone={onDone} />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-3xl space-y-6 px-4 py-6 md:px-6">
            {tab === "overview" && <Overview realm={realm} agent={agent} onSaved={onSaved} onEdit={() => setEditing(true)} />}
            {tab === "rights" && <Rights realm={realm} agent={agent} />}
            {tab === "tools" && <Tools key={agent.name} realm={realm} agent={agent} onSaved={onSaved} />}
            {tab === "routines" && <Routines agent={agent} />}
            {tab === "memory" && <Memory realm={realm} agent={agent} onSaved={onSaved} />}
            {tab === "mail" && <AgentMail realm={realm} agent={agent} />}
          </div>
        </div>
      )}
    </div>
  )
}

function Title({ children, note }: { children: React.ReactNode; note?: React.ReactNode }) {
  return (
    <div className="mb-2.5 flex items-baseline justify-between gap-3">
      <h2 className="text-sm font-semibold">{children}</h2>
      {note && <span className="text-xs text-muted-foreground">{note}</span>}
    </div>
  )
}

function Stat({ label, value, tone }: { label: string; value: React.ReactNode; tone?: "warn" | "bad" }) {
  return (
    <div className="rounded-xl bg-bubble px-3.5 py-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cn("mt-1 text-lg font-semibold tabular-nums", tone === "warn" && "text-amber-500", tone === "bad" && "text-red-500")}>{value}</div>
    </div>
  )
}

type Budgets = { agents: { agent: string; cap: number | null; spent: number }[] }

function Overview({ realm, agent, onSaved, onEdit }: { realm: string; agent: Agent; onSaved: () => void; onEdit: () => void }) {
  const budgets = usePoll<Budgets>(() => api(`realms/${enc(realm)}/budgets`), 60000, [realm])
  const month = budgets.data?.agents.find((b) => b.agent === agent.name)
  const share = month?.cap ? month.spent / month.cap : 0
  const last = agent.routines.map((r) => r.last?.at ?? "").sort().at(-1)
  return (
    <>
      <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
        <Stat label="Last 30 days" value={usd(agent.cost_usd) || "$0"} />
        <Stat label="This month" value={month ? `${usd(month.spent) || "$0"}${month.cap ? ` / ${usd(month.cap)}` : ""}` : "…"} tone={share >= 1 ? "bad" : share >= 0.8 ? "warn" : undefined} />
        <Stat label="Waiting for you" value={agent.waiting} />
        <Stat label="Last ran" value={last ? ago(last) : "—"} />
      </div>
      {agent.look_saved === false && (
        <p className="text-xs text-muted-foreground">Its look comes from its name for now. <button className="underline underline-offset-2 hover:text-foreground" onClick={async () => {
          try { await api(`realms/${enc(realm)}/agents/${enc(agent.name)}/look`, {}); toast.success("Look kept."); onSaved() } catch (e) { toast.error((e as Error).message) }
        }}>Keep this look</button></p>
      )}
      <section>
        <Title note={<button onClick={onEdit} className="underline underline-offset-2 hover:text-foreground">Edit</button>}>Job</Title>
        <div className="rounded-xl bg-bubble px-4 py-3.5 text-sm leading-relaxed">
          {agent.body ? <Rich text={agent.body} /> : <span className="text-muted-foreground">No job written yet.</span>}
        </div>
      </section>
    </>
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

const flip = (l: string[], n: string) => (l.includes(n) ? l.filter((x) => x !== n) : [...l, n])

/** What its tasks may use (decision 64): nothing else. */
function Tools({ realm, agent, onSaved }: { realm: string; agent: Agent; onSaved: () => void }) {
  const tools = useTools(realm)
  const [integrations, setIntegrations] = useState<string[]>(agent.integrations ?? [])
  const [skills, setSkills] = useState<string[]>(agent.skills ?? [])
  const [repos, setRepos] = useState<string[]>(agent.repos ?? [])
  const [busy, setBusy] = useState(false)
  const same = (a: string[], b: string[]) => a.length === b.length && a.every((x) => b.includes(x))
  const changed = !same(integrations, agent.integrations ?? []) || !same(skills, agent.skills ?? []) || !same(repos, agent.repos ?? [])
  async function save() {
    setBusy(true)
    try {
      // The job as stored now, not as this page last saw it: it may have
      // just been edited, and this write sends it back whole.
      const stored = (await api<{ agents: { name: string; job: string }[] }>(`realms/${enc(realm)}/agents`)).agents.find((a) => a.name === agent.name)
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, { job: stored?.job ?? agent.body ?? agent.job ?? "", integrations, skills, repos }, "PUT")
      toast.success(`${called(agent)}'s tools are saved.`)
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  const group = (title: string, hint: string, names: { name: string; hint?: string }[], on: string[], toggle: (n: string) => void, mark?: string) => names.length > 0 && (
    <section>
      <Title note={hint}>{title}</Title>
      <div className="flex flex-wrap gap-2">
        {names.map(({ name, hint }) => <Toggle key={name} label={name} hint={hint} mark={mark} on={on.includes(name)} onClick={() => toggle(name)} />)}
      </div>
    </section>
  )
  if (!tools.data) return <p className="text-sm text-muted-foreground">Loading…</p>
  const none = !tools.data.integrations.length && !tools.data.servers.length && !tools.data.skills.length && !tools.data.repos.length
  return (
    <>
      <p className="text-sm text-muted-foreground">Its tasks use only what is on. With nothing on, it reads the public web and no more.</p>
      {none && <p className="text-sm text-muted-foreground">This business has no connector or skill yet: connect them in the Marketplace.</p>}
      {group("Connectors", "Services it calls", tools.data.integrations.map((name) => ({ name })), integrations, (n) => setIntegrations((l) => flip(l, n)))}
      {group("MCP servers", "Tools from a server", tools.data.servers, skills, (n) => setSkills((l) => flip(l, n)))}
      {group("Skills", "Know-how it can load", tools.data.skills.map((name) => ({ name })), skills, (n) => setSkills((l) => flip(l, n)), "✦")}
      {group("Code", "Each task on its own branch", tools.data.repos.map((name) => ({ name })), repos, (n) => setRepos((l) => flip(l, n)))}
      <div className="flex justify-end gap-2">
        {changed && <Button variant="ghost" onClick={() => { setIntegrations(agent.integrations ?? []); setSkills(agent.skills ?? []); setRepos(agent.repos ?? []) }}>Undo</Button>}
        <Button disabled={!changed || busy} onClick={save}>{busy ? "Saving…" : "Save"}</Button>
      </div>
    </>
  )
}

function Toggle({ label, hint, mark: glyph, on, onClick }: { label: string; hint?: string; mark?: string; on: boolean; onClick: () => void }) {
  const mark = LOGOS[label]
  return (
    <button type="button" onClick={onClick} title={hint} aria-pressed={on}
      className={cn("inline-flex h-9 items-center gap-2 rounded-full border px-3 text-sm transition-colors",
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

