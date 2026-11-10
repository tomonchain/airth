import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react"
import { PanelRight } from "lucide-react"
import { AgentList, type Last } from "@/components/agent-list"
import { Details } from "@/components/details"
import { Rail, TabBar, type Page } from "@/components/rail"
import { TopBar } from "@/components/topbar"
import { useNarrow } from "@/hooks/use-narrow"
import { usePoll } from "@/hooks/use-poll"
import { api, called, enc, named, onPhone, paired, type Agent, type Business, type Department, type Item, type Realm, type Task, type Tasks } from "@/lib/api"
import { failure } from "@/components/task-thread"
import { Conversation } from "@/screens/conversation"
import { Home } from "@/screens/home"
import { Inbox } from "@/screens/inbox"
import type { Section as SettingsSection } from "@/screens/settings"
import { PageBoundary } from "@/components/error-boundary"
import { Back } from "@/components/page"
import { Character } from "@/components/character"
import { Button } from "@/components/ui/button"
import { NewBusiness } from "@/components/new-business"
import { Pair } from "@/components/pair"
import { Palette } from "@/components/palette"

// The pages opened now and then load when first opened, so the first
// screen comes up sooner.
const Calendar = lazy(() => import("@/screens/calendar").then((m) => ({ default: m.Calendar })))
const Company = lazy(() => import("@/screens/company").then((m) => ({ default: m.Company })))
const Documents = lazy(() => import("@/screens/documents").then((m) => ({ default: m.Documents })))
const Hq = lazy(() => import("@/screens/hq").then((m) => ({ default: m.Hq })))
const Usage = lazy(() => import("@/screens/usage").then((m) => ({ default: m.Usage })))
const Finance = lazy(() => import("@/screens/finance").then((m) => ({ default: m.Finance })))
const Marketplace = lazy(() => import("@/screens/marketplace").then((m) => ({ default: m.Marketplace })))
const Routines = lazy(() => import("@/screens/routines").then((m) => ({ default: m.Routines })))
const Settings = lazy(() => import("@/screens/settings").then((m) => ({ default: m.Settings })))
const Tasks = lazy(() => import("@/screens/tasks").then((m) => ({ default: m.Tasks })))
const Welcome = lazy(() => import("@/screens/welcome").then((m) => ({ default: m.Welcome })))

const PAGES: Page[] = ["home", "agents", "needs", "routines", "tasks", "calendar", "documents", "finance", "usage", "hq", "company", "settings", "marketplace"]

function route(): string | null {
  const m = location.hash.match(/^#\/b\/([^/?&]+)/)
  return m ? decodeURIComponent(m[1]) : null
}

// The page and agent open, after the business in the address
// (#/b/acme/agents/writer): a reload comes back to them, and a link opens them.
function opened(): { page: Page; agent: string | null } {
  const m = location.hash.match(/^#\/b\/[^/?&]+\/([a-z]+)(?:\/([^/?&]+))?/)
  const page = PAGES.find((p) => p === m?.[1]) ?? "home"
  return { page, agent: page === "agents" && m?.[2] ? decodeURIComponent(m[2]) : null }
}

type Data = { agents: Agent[]; departments: Department[]; broken: string[]; items: Item[]; tasks: Tasks | null }

export default function App() {
  const narrow = useNarrow()
  const [hash, setHash] = useState(route)
  const [welcome, setWelcome] = useState(() => location.hash === "#/welcome")
  const [founding, setFounding] = useState(false)
  const [task, setTask] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  // Pages opened on something from another page: a file, a settings section.
  const [doc, setDoc] = useState<{ path: string; at: number } | null>(null)
  const [setting, setSetting] = useState<{ section: SettingsSection; at: number } | null>(null)
  const [prefill, setPrefill] = useState<{ text: string; at: number } | null>(null)
  const ask = (text = "") => { setTask(null); setPage("tasks"); setPrefill({ text, at: Date.now() }) }

  // ⌘K goes anywhere; ⌘⇧O asks the team (the classic interface's keys).
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return
      const k = e.key.toLowerCase()
      if (k === "k" && !e.shiftKey) { e.preventDefault(); setPalette((o) => !o) }
      if (k === "o" && e.shiftKey) { e.preventDefault(); ask() }
    }
    addEventListener("keydown", key)
    return () => removeEventListener("keydown", key)
  }, [])

  useEffect(() => {
    const on = () => {
      setHash(route()); setWelcome(location.hash === "#/welcome")
      // A link to a page or an agent opens it.
      const to = opened()
      if (/^#\/b\/[^/]+\/[a-z]/.test(location.hash)) {
        setPage(to.page)
        if (to.agent) { setCurrent(to.agent); setTabs((t) => (t.includes(to.agent!) ? t : [...t, to.agent!])) }
      }
    }
    addEventListener("hashchange", on)
    return () => removeEventListener("hashchange", on)
  }, [])
  const realms = usePoll(() => api<Realm[]>("realms"), 15000, [])
  const realm = hash ?? realms.data?.find((r) => r.default)?.name ?? realms.data?.[0]?.name ?? null

  // The name the founder gave this Airth (decision 113): said at the top and
  // in the tab, so they know whether they are on the server or the Mac.
  const info = usePoll<{ name: string | null }>(() => api("info"), 300000, [])
  const server = info.data?.name ?? null
  useEffect(() => {
    const again = () => info.refresh()
    addEventListener("airth:name", again)
    return () => removeEventListener("airth:name", again)
  }, [info.refresh])

  // The tab says how many things wait for the founder, in every business.
  const waiting = (realms.data ?? []).reduce((n, r) => n + (r.archived ? 0 : r.needs_approval || 0), 0)
  useEffect(() => { document.title = `${waiting ? `(${waiting}) ` : ""}${realm ? `${named(realm, realms.data?.find((r) => r.name === realm)?.title)} · ` : ""}Airth${server ? ` · ${server}` : ""}` }, [waiting, realm, realms.data, server])

  const data = usePoll<Data>(
    realm
      ? async () => {
          const [team, queue, tasks] = await Promise.all([
            api<{ agents: Agent[]; departments?: Department[]; broken?: string[]; error?: string }>(`realms/${enc(realm)}/team`),
            api<{ items: Item[] }>(`realms/${enc(realm)}/queue`),
            api<Tasks>(`realms/${enc(realm)}/tasks`).catch(() => null),
          ])
          return { agents: team.agents, departments: team.departments ?? [], broken: team.error ? [team.error] : team.broken ?? [], items: queue.items, tasks }
        }
      : null,
    4000,
    [realm],
  )
  // The business's summary (Home, the last lines) is computed over every
  // business: asked less often than what the agents are doing.
  const hq = usePoll<{ businesses: Business[] }>(realm ? () => api("hq") : null, 15000, [realm])
  const business = hq.data?.businesses.find((b) => b.name === realm) ?? null

  const [page, setPage] = useState<Page>(() => opened().page)
  const [current, setCurrent] = useState<string | null>(() => opened().agent)
  const [tabs, setTabs] = useState<string[]>(() => (opened().agent ? [opened().agent!] : []))
  // On a phone the card takes the conversation's place: closed at first.
  const [details, setDetails] = useState(!narrow)
  const agents = data.data?.agents ?? []
  const items = data.data?.items ?? []

  // Another business: its own agents' tabs. Not on the first render, which
  // opens what the address says.
  const shownRealm = useRef(realm)
  useEffect(() => {
    if (shownRealm.current === realm) return
    shownRealm.current = realm
    setCurrent(null); setTabs([]); setTask(null)
  }, [realm])
  useEffect(() => {
    if (!realm) return
    const at = `#/b/${encodeURIComponent(realm)}/${page}${page === "agents" && current ? `/${encodeURIComponent(current)}` : ""}`
    if (location.hash !== at && route() === realm) history.replaceState(null, "", at)
  }, [realm, page, current])
  useEffect(() => {
    // On a phone the list comes first: the founder picks.
    if (narrow || page !== "agents" || current || !agents.length) return
    const first = agents.find((a) => a.waiting > 0) ?? agents[0]
    open(first.name)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agents.length, current, page, narrow])

  function open(name: string) {
    setPage("agents")
    setCurrent(name)
    setTabs((t) => (t.includes(name) ? t : [...t, name]))
  }
  function close(name: string) {
    setTabs((t) => {
      const next = t.filter((x) => x !== name)
      if (current === name) setCurrent(next.at(-1) ?? null)
      return next
    })
  }

  // Each agent's latest line, for the list: whichever is newest of its
  // routines' last sentence, its conversation's last task, and what it sent
  // to Needs you.
  const last = useMemo(() => {
    const out: Record<string, Last> = {}
    const take = (agent: string, text: string, at: string | null | undefined) => {
      const l = out[agent]
      if (!l || (l.at ?? "") < (at ?? "")) out[agent] = { text, at: at ?? null }
    }
    for (const x of business?.activity ?? []) take(x.agent, x.sentence, x.at)
    // The conversation ends with the task started last (its id is its start),
    // whatever touched the others since.
    const latest: Record<string, Task> = {}
    for (const t of Object.values(data.data?.tasks ?? {}).flat()) {
      if (t.agent && (!latest[t.agent] || latest[t.agent].id < t.id)) latest[t.agent] = t
    }
    for (const [agent, t] of Object.entries(latest)) {
      const said = t.status === "failed" ? failure(t.line || null) : t.line || t.order || ""
      const line = said.replace(/[*_`#>]/g, "").split("\n").find((x) => x.trim()) ?? ""
      if (line) take(agent, line.trim(), t.at ?? t.updated_at)
    }
    for (const it of items) if (it.agent) take(it.agent, it.text?.split("\n")[0] ?? it.title, it.created_at)
    return out
  }, [data.data, business, items])

  const agent = agents.find((a) => a.name === current) ?? null

  if (onPhone && !paired()) return <Pair />

  // The first launch, with no business yet, starts with the welcome guide.
  const none = !!realms.data && !realms.data.some((r) => !r.archived)
  // Begun on the first launch, the guide stays until it is done, though the
  // business it creates comes into the list meanwhile.
  if (none && !welcome) setWelcome(true)
  if (welcome || none) {
    return (
      <Suspense fallback={null}>
        <Welcome realms={realms.data ?? []} onDone={(r) => { setWelcome(false); realms.refresh(); location.hash = r ? `#/b/${encodeURIComponent(r)}` : "" }} />
      </Suspense>
    )
  }

  if (!realm) {
    return <div className="grid h-svh place-items-center text-sm text-muted-foreground">{realms.error ? `Airth is not answering: ${realms.error}` : "Loading…"}</div>
  }

  return (
    <div className="relative flex h-svh flex-col overflow-hidden bg-background pt-[env(safe-area-inset-top)]">
      <TopBar
        server={server} realms={realms.data ?? []} realm={realm} onAgents={page === "agents"} tabs={tabs} current={page === "agents" ? current : null} agents={agents}
        onTab={open} onClose={close} onNewBusiness={() => setFounding(true)} onHq={() => setPage("hq")} details={details} onDetails={() => setDetails(!details)}
      />
      <Palette open={palette} onOpenChange={setPalette} realm={realm} realms={realms.data ?? []} agents={agents}
        onPage={setPage} onAgent={open} onTask={(id) => { setTask(id); setPage("tasks") }} onAsk={() => ask()} onDetails={() => { setDetails(!details); if (page !== "agents") setPage("agents") }} />
      <NewBusiness open={founding} onOpenChange={setFounding} onCreated={(name) => { realms.refresh(); location.hash = `#/b/${encodeURIComponent(name)}`; setPage("home") }} />
      <div className="flex min-h-0 flex-1">
        {!narrow && <Rail page={page} onPage={setPage} needs={items.length} live={agents.some((a) => a.state === "working")} />}
        <main className="flex min-w-0 flex-1 overflow-hidden bg-surface md:mr-2 md:mb-2 md:rounded-xl md:border md:shadow-sm">
          <PageBoundary page={page}>
          <Suspense fallback={<div className="flex-1" />}>
          {page === "home" ? (
            <Home realm={realm} agents={data.data?.agents ?? null} items={items} business={business}
              at={data.at} error={data.error} onAgent={open} onNeeds={() => setPage("needs")} />
          ) : page === "routines" ? <Routines realm={realm} agents={agents} onAgent={open} />
          : page === "tasks" ? <Tasks realm={realm} agents={agents} items={items} open={task} onOpen={setTask} onAgent={open} prefill={prefill} tasks={data.data?.tasks ?? null} onTasks={data.refresh} />
          : page === "calendar" ? <Calendar realm={realm} agents={agents} onAgent={open} />
          : page === "documents" ? <Documents realm={realm} onAsk={ask} start={doc} />
          : page === "usage" ? <Usage realm={realm} />
          : page === "hq" ? <Hq shared={hq.data as Parameters<typeof Hq>[0]["shared"]} onOpen={(r) => { location.hash = `#/b/${encodeURIComponent(r)}`; setPage("home") }} />
          : page === "finance" ? <Finance realm={realm} business={business} agents={agents} />
          : page === "settings" ? <Settings realm={realm} start={setting} />
          : page === "marketplace" ? <Marketplace realm={realm} />
          : page === "company" ? <Company realm={realm} description={business?.description ?? null}
              onFile={(path) => { setDoc({ path, at: Date.now() }); setPage("documents") }} onRepos={() => { setSetting({ section: "repositories", at: Date.now() }); setPage("settings") }} />
          : (
          <>
          {page === "needs" ? (
            <Inbox realm={realm} items={items} agents={agents} onDone={data.refresh} onAgent={open} onTask={(id) => { setTask(id); setPage("tasks") }} />
          ) : (
          <>
          {!(narrow && agent) && <AgentList realm={realm} agents={agents} departments={data.data?.departments ?? []} broken={data.data?.broken ?? []} items={items} last={last} selected={current} onOpen={open} onHired={(name) => { data.refresh(); open(name) }} />}
          {agent ? (
            <>
              {narrow ? (
                // One column: a head to go back to the list or see the card.
                <div className="flex min-w-0 flex-1 flex-col">
                  <div className="flex h-12 shrink-0 items-center gap-2 border-b px-3">
                    <Back label="All agents" onClick={() => { setCurrent(null); setDetails(false) }} />
                    <Character agent={agent} size={24} still />
                    <span className="min-w-0 flex-1 truncate text-[15px] font-semibold">{called(agent)}</span>
                    <Button variant="ghost" size="icon-sm" className={details ? "bg-accent" : "text-muted-foreground"} aria-label="Details" onClick={() => setDetails(!details)}><PanelRight /></Button>
                  </div>
                  {details
                    ? <Details realm={realm} agent={agent} departments={data.data?.departments ?? []} onSaved={data.refresh} onGone={() => { close(agent.name); data.refresh() }} />
                    : <Conversation key={agent.name} realm={realm} agent={agent} agents={agents} onAgent={open} tasks={data.data?.tasks ?? null} onDone={data.refresh} />}
                </div>
              ) : <Conversation key={agent.name} realm={realm} agent={agent} agents={agents} onAgent={open} tasks={data.data?.tasks ?? null} onDone={data.refresh} />}
              {!narrow && details && <Details realm={realm} agent={agent} departments={data.data?.departments ?? []} onSaved={data.refresh} onGone={() => { close(agent.name); data.refresh() }} />}
            </>
          ) : !narrow && (
            <div className="grid flex-1 place-items-center text-sm text-muted-foreground">{data.error ?? "Choose an agent."}</div>
          )}
          </>
          )}
          </>
          )}
          </Suspense>
          </PageBoundary>
        </main>
      </div>
      {narrow && <TabBar page={page} onPage={(p) => { setPage(p); if (p === "agents") setCurrent(null) }} needs={items.length} />}
    </div>
  )
}
