import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react"
import { PenLine } from "lucide-react"
import { QuickAsk } from "@/components/quick-ask"
import { AgentList, type Last } from "@/components/agent-list"
import { AgentPage } from "@/components/agent-page"
import { MAIN, Rail, TabBar, type Page } from "@/components/rail"
import { PhoneMenu, TopBar } from "@/components/topbar"
import { useNarrow } from "@/hooks/use-narrow"
import { useSwipe } from "@/hooks/use-swipe"
import { cn } from "@/lib/utils"
import { isStep, parse, write } from "@/lib/address"
import { usePoll } from "@/hooks/use-poll"
import { useChanges } from "@/hooks/use-changes"
import { api, enc, named, onPhone, paired, type Agent, type Business, type Department, type Item, type Realm, type Task, type Tasks } from "@/lib/api"
import { failure } from "@/components/task-thread"
import { Home, type HomeMode } from "@/screens/home"
import { Overview } from "@/screens/overview"
import { Inbox } from "@/screens/inbox"
import type { Section as SettingsSection } from "@/screens/settings"
import { PageBoundary } from "@/components/error-boundary"
import { Pair } from "@/components/pair"
import { DeviceAsks } from "@/components/device-asks"
import { Palette } from "@/components/palette"
import { FileLinks } from "@/components/markdown"

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

function route(): string | null {
  return parse(location.hash).realm
}

// The page and what is open in it, after the business in the address
// (#/b/acme/tasks/20261116-0912, #/b/acme/documents/docs/plan.md): a reload
// comes back to them, a link opens them, and Back moves between them.
function opened(): { page: Page; sub: string | null } {
  const a = parse(location.hash)
  return { page: a.page, sub: a.sub }
}

// An address that names a page, not a business alone (which its menu
// writes, and which keeps the page shown).
const PAGED = /^#\/b\/[^/]+\/./

// What the address says is open, for each page that can open something.
const startOf = (page: Page, sub: string | null) => ({
  thread: page === "home" && sub === "conversation",
  // The business's mail, a thread of Chat (decision 152).
  mail: page === "home" && sub === "mail",
  agent: page === "agents" ? sub : null,
  task: page === "tasks" && sub && !sub.startsWith("project/") ? sub : null,
  project: page === "tasks" && sub?.startsWith("project/") && Number.isInteger(Number(sub.slice(8))) ? Number(sub.slice(8)) : null,
  doc: page === "documents" ? sub : null,
  section: page === "settings" ? sub : null,
  item: page === "needs" && sub && Number.isInteger(Number(sub)) ? Number(sub) : null,
})

type Data = { agents: Agent[]; departments: Department[]; broken: string[]; items: Item[]; tasks: Tasks | null }

export default function App() {
  const narrow = useNarrow()
  // What the address opens on the first load.
  const [first] = useState(() => { const o = opened(); return startOf(o.page, o.sub) })
  const [hash, setHash] = useState(route)
  const [welcome, setWelcome] = useState(() => location.hash === "#/welcome")
  const [founding, setFounding] = useState(false)
  const [task, setTask] = useState<string | null>(first.task)
  const [palette, setPalette] = useState(false)
  // Ask from anywhere: the order goes, the page stays.
  const [asking, setAsking] = useState(false)
  // Pages opened on something from another page: a file, a settings section.
  const [doc, setDoc] = useState<{ path: string | null; at: number } | null>(first.doc ? { path: first.doc, at: 0 } : null)
  const [setting, setSetting] = useState<{ section: SettingsSection | null; at: number } | null>(first.section ? { section: first.section as SettingsSection, at: 0 } : null)
  // What is open in a page, as the page reports it, for the address.
  const [openDoc, setOpenDoc] = useState<string | null>(first.doc)
  const [openSection, setOpenSection] = useState<string | null>(first.section)
  const [openProjectId, setOpenProjectId] = useState<number | null>(first.project)
  const [needsStart, setNeedsStart] = useState<{ id: number | null; at: number } | null>(first.item != null ? { id: first.item, at: 0 } : null)
  const [openItem, setOpenItem] = useState<number | null>(first.item)
  const [prefill, setPrefill] = useState<{ text: string; at: number } | null>(null)
  // A project opened from the Home or the Overview (decision 129).
  const [project, setProject] = useState<{ id: number | null; at: number } | null>(first.project != null ? { id: first.project, at: 0 } : null)
  const openProject = (id: number) => { setTask(null); setProject({ id, at: Date.now() }); setPage("tasks") }
  // Chat: New request, or the conversation. Opening Chat from the rail,
  // the tab bar or ⌘K starts on New request.
  const [homeMode, setHomeMode] = useState<HomeMode>(first.thread ? "thread" : first.mail ? "mail" : "start")
  const go = (p: Page) => { if (p === "home") setHomeMode("start"); setPage(p) }
  const ask = (text = "") => { setTask(null); setPage("tasks"); setPrefill({ text, at: Date.now() }) }

  // ⌘K goes anywhere; ⌘J, or ⌘⇧O (the classic interface's key), asks the team.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return
      const k = e.key.toLowerCase()
      if (k === "k" && !e.shiftKey) { e.preventDefault(); setPalette((o) => !o) }
      if ((k === "j" && !e.shiftKey) || (k === "o" && e.shiftKey)) { e.preventDefault(); setAsking(true) }
    }
    addEventListener("keydown", key)
    return () => removeEventListener("keydown", key)
  }, [])

  // Back, Forward, a link or an address typed: open what it says, without
  // writing it back as a new step.
  const restoring = useRef(false)
  // The address last read or written: a browser may tell the same move twice
  // (hashchange and popstate), and it is read once.
  const known = useRef(location.hash)
  useEffect(() => {
    const on = () => {
      if (location.hash === known.current) return
      known.current = location.hash
      setHash(route()); setWelcome(location.hash === "#/welcome")
      if (!PAGED.test(location.hash)) return
      restoring.current = true
      // If what it opens is already open, nothing renders to clear the mark.
      setTimeout(() => { restoring.current = false }, 500)
      const to = opened()
      restore(to.page, to.sub)
    }
    addEventListener("hashchange", on)
    addEventListener("popstate", on)
    return () => { removeEventListener("hashchange", on); removeEventListener("popstate", on) }
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
  // Runs at work in every business: a restart to update stops them all.
  const running = (realms.data ?? []).reduce((n, r) => n + (r.archived ? 0 : r.live || 0), 0)
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
    // The server says when the business changed; this is only the fallback.
    15000,
    [realm],
    realm,
  )
  useChanges(realm)
  // The business's summary (Home, the last lines) is computed over every
  // business: asked less often than what the agents are doing.
  const hq = usePoll<{ businesses: Business[] }>(realm ? () => api("hq") : null, 15000, [realm])
  const business = hq.data?.businesses.find((b) => b.name === realm) ?? null

  const [page, setPage] = useState<Page>(() => opened().page)
  // On a phone, a sideways swipe moves between the bar's pages, and the
  // page comes in from the side the finger went.
  const [mainEl, setMainEl] = useState<HTMLElement | null>(null)
  const [slide, setSlide] = useState<{ from: "left" | "right"; at: number } | null>(null)
  const onSwipe = useCallback((to: "left" | "right") => {
    const order = MAIN.map(([p]) => p)
    const at = order.indexOf(page)
    const next = at < 0 ? undefined : order[at + (to === "left" ? 1 : -1)]
    if (!next) return
    // Off for a frame, then on: a swipe during the last one's slide plays its own.
    setSlide(null)
    requestAnimationFrame(() => setSlide({ from: to === "left" ? "right" : "left", at: Date.now() }))
    setPage(next)
  }, [page])
  useSwipe(mainEl, narrow, onSwipe)
  const [current, setCurrent] = useState<string | null>(first.agent)
  const [tabs, setTabs] = useState<string[]>(first.agent ? [first.agent] : [])
  // An agent opens on its conversation, as a chat does; its other tabs hold
  // its overview, rights, tools, routines, memory and mail (decision 140).
  const agents = data.data?.agents ?? []
  // An agent's idea is told in its conversation, not in Needs you (decision 119).
  const items = (data.data?.items ?? []).filter((i) => i.kind !== "idea")

  // Open what an address says (Back, a link, another business's page).
  function restore(to: Page, sub: string | null) {
    const s = startOf(to, sub)
    const at = Date.now()
    setPage(to)
    if (to === "home") setHomeMode(s.thread ? "thread" : s.mail ? "mail" : "start")
    if (to === "agents") { setCurrent(s.agent); if (s.agent) setTabs((t) => (t.includes(s.agent!) ? t : [...t, s.agent!])) }
    if (to === "tasks") {
      setTask(s.task)
      setOpenProjectId(s.project)
      if (!s.task) setProject({ id: s.project, at })
    }
    if (to === "documents") { setOpenDoc(s.doc); setDoc({ path: s.doc, at }) }
    if (to === "settings") { setOpenSection(s.section); setSetting({ section: s.section as SettingsSection | null, at }) }
    if (to === "needs") { setOpenItem(s.item); setNeedsStart({ id: s.item, at }) }
  }

  // Another business: nothing of the last one stays open, unless the
  // address names what to open in this one. Not on the first render, which
  // opens what the address says.
  const shownRealm = useRef(realm)
  useEffect(() => {
    if (shownRealm.current === realm) return
    shownRealm.current = realm
    setCurrent(null); setTabs([]); setTask(null); setHomeMode("start")
    setOpenDoc(null); setDoc(null); setOpenSection(null); setSetting(null)
    setOpenProjectId(null); setProject(null); setOpenItem(null); setNeedsStart(null)
    const here = parse(location.hash)
    if (here.realm === realm && PAGED.test(location.hash)) restore(here.page, here.sub)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [realm])
  // The address follows the page and what is open in it. A new page, or
  // opening and closing something, is a step Back comes back to; the first
  // load, what Back or a link just opened, an agent opened for the founder,
  // and moving from one open thing to the next only correct the address.
  const settled = useRef(false)
  const quiet = useRef(false)
  const sub = page === "home" ? (homeMode === "thread" ? "conversation" : homeMode === "mail" ? "mail" : null)
    : page === "agents" ? current
    : page === "tasks" ? task ?? (openProjectId != null ? `project/${openProjectId}` : null)
    : page === "documents" ? openDoc
    : page === "settings" ? openSection
    : page === "needs" ? (openItem != null ? String(openItem) : null)
    : null
  useEffect(() => {
    if (!realm) return
    // Another kind of address (the welcome guide, a pairing link): left as is.
    if (location.hash && !location.hash.startsWith("#/b/")) return
    const here = parse(location.hash)
    // Another business was just asked for (its menu writes the address): it
    // is written once that business is open.
    if (here.realm && here.realm !== realm) return
    // A business alone (#/b/acme, from its menu): completed, not a new step.
    const bare = !PAGED.test(location.hash)
    const to = { realm, page, sub }
    const at = write(to)
    const replace = !settled.current || bare || restoring.current || quiet.current || !isStep(here, to)
    settled.current = true; restoring.current = false; quiet.current = false
    known.current = at
    if (location.hash === at) return
    if (replace) history.replaceState(null, "", at)
    else history.pushState(null, "", at)
  }, [realm, page, sub])
  useEffect(() => {
    // On a phone the list comes first: the founder picks.
    if (narrow || page !== "agents" || current || !agents.length) return
    const first = agents.find((a) => a.waiting > 0) ?? agents[0]
    quiet.current = true
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

  // A new business, from the business menu: the same guide, from its
  // business step, with its register and its connectors (decision 126).
  if (founding) {
    return (
      <Suspense fallback={null}>
        <Welcome fresh realms={realms.data ?? []} onCancel={() => setFounding(false)}
          onDone={(r) => { setFounding(false); realms.refresh(); if (r) { location.hash = `#/b/${encodeURIComponent(r)}`; setPage("home") } }} />
      </Suspense>
    )
  }

  if (!realm) {
    return <div className="grid h-svh place-items-center text-sm text-muted-foreground">{realms.error ? `Airth is not answering: ${realms.error}` : "Loading…"}</div>
  }

  // A workspace file an agent links to opens in Documents (decision 146).
  const openFile = (path: string) => { setDoc({ path, at: Date.now() }); setPage("documents") }
  return (
    <FileLinks.Provider value={openFile}>
    <div className="relative flex h-svh flex-col overflow-hidden bg-background pt-[env(safe-area-inset-top)] max-md:bg-surface">
      {narrow ? (
        <PhoneMenu server={server} realms={realms.data ?? []} realm={realm} working={running} onNewBusiness={() => setFounding(true)} onHq={() => setPage("hq")} />
      ) : (
        <TopBar
          server={server} realms={realms.data ?? []} realm={realm} onAgents={page === "agents"} tabs={tabs} current={page === "agents" ? current : null} agents={agents}
          onTab={open} onClose={close} onNewBusiness={() => setFounding(true)} onHq={() => setPage("hq")}
        />
      )}
      <QuickAsk open={asking} onOpenChange={setAsking} realm={realm} agents={agents} onTask={(id) => { setTask(id); setPage("tasks") }} />
      {/* On a phone, Ask floats above the tabs, but not where a box to write in is already the page's own. */}
      {narrow && page !== "home" && page !== "needs" && page !== "tasks" && page !== "agents" && (
        <button onClick={() => setAsking(true)} aria-label="Ask the team"
          className="fixed right-4 bottom-[var(--dock)] z-40 grid size-13 place-items-center rounded-full bg-foreground text-background shadow-lg active:scale-95">
          <PenLine className="size-5" />
        </button>
      )}
      <Palette open={palette} onOpenChange={setPalette} realm={realm} realms={realms.data ?? []} agents={agents}
        onPage={go} onAgent={open} onTask={(id) => { setTask(id); setPage("tasks") }} onAsk={() => setAsking(true)} />
      <DeviceAsks />
      <div className="flex min-h-0 flex-1">
        {!narrow && <Rail page={page} onPage={go} needs={items.length} live={agents.some((a) => a.state === "working")} working={running} />}
        <main ref={setMainEl} onAnimationEnd={(e) => e.target === e.currentTarget && setSlide(null)}
          className={cn("flex min-w-0 flex-1 overflow-hidden bg-surface md:mr-2 md:mb-2 md:rounded-xl md:border md:shadow-sm",
            slide && "animate-in fade-in-0 duration-200 ease-out", slide?.from === "right" && "slide-in-from-right-10", slide?.from === "left" && "slide-in-from-left-10")}>
          <PageBoundary page={page}>
          <Suspense fallback={<div className="flex-1" />}>
          {page === "home" ? (
            <Home realm={realm} agents={data.data?.agents ?? null} items={items} tasks={data.data?.tasks ?? null}
              onTasks={data.refresh} onAgent={open} onTask={(id) => { setTask(id); setPage("tasks") }} onPage={setPage} onProject={openProject} mode={homeMode} onMode={setHomeMode}
              mail={{
                onTask: (id) => { setTask(id); setPage("tasks") },
                onItem: (id) => { setOpenItem(id); setNeedsStart({ id, at: Date.now() }); setPage("needs") },
                onSettings: () => { setSetting({ section: "mailboxes", at: Date.now() }); setPage("settings") },
              }} />
          ) : page === "overview" ? (
            <Overview realm={realm} agents={data.data?.agents ?? null} items={items} business={business} tasks={data.data?.tasks ?? null}
              onAgent={open} onNeeds={() => setPage("needs")}
              onTask={(id) => { setTask(id); setPage("tasks") }} onPage={setPage} onImported={hq.refresh} onProject={openProject} />
          ) : page === "routines" ? <Routines realm={realm} agents={agents} items={items} onAgent={open}
            businesses={(realms.data ?? []).filter((r) => !r.archived || r.name === realm).map((r) => ({ name: r.name, title: named(r.name, r.title) }))} />
          : page === "tasks" ? <Tasks realm={realm} agents={agents} items={items} open={task} onOpen={setTask} onAgent={open} prefill={prefill} tasks={data.data?.tasks ?? null} onTasks={data.refresh} startProject={project} onProject={(id) => { setOpenProjectId(id); if (id == null) setProject(null) }} />
          : page === "calendar" ? <Calendar realm={realm} agents={agents} onAgent={open} />
          : page === "documents" ? <Documents realm={realm} onAsk={ask} start={doc} onOpen={(p) => { setOpenDoc(p); setDoc((d) => ({ path: p, at: d?.at ?? 0 })) }} />
          : page === "usage" ? <Usage realm={realm} />
          : page === "hq" ? <Hq shared={hq.data as Parameters<typeof Hq>[0]["shared"]} onOpen={(r) => { location.hash = `#/b/${encodeURIComponent(r)}`; setPage("home") }} />
          : page === "finance" ? <Finance realm={realm} business={business} agents={agents} onImported={hq.refresh} />
          : page === "settings" ? <Settings realm={realm} start={setting} onSection={(x) => { setOpenSection(x); setSetting((d) => ({ section: x, at: d?.at ?? 0 })) }} />
          : page === "marketplace" ? <Marketplace realm={realm} />
          : page === "company" ? <Company realm={realm} description={business?.description ?? null}
              onFile={openFile} onRepos={() => { setSetting({ section: "repositories", at: Date.now() }); setPage("settings") }} />
          : (
          <>
          {page === "needs" ? (
            <Inbox realm={realm} items={items} agents={agents} onDone={data.refresh} onAgent={open} onTask={(id) => { setTask(id); setPage("tasks") }} start={needsStart} onOpen={(id) => { setOpenItem(id); setNeedsStart((d) => ({ id, at: d?.at ?? 0 })) }} />
          ) : (
          <>
          {!(narrow && agent) && <AgentList realm={realm} agents={agents} departments={data.data?.departments ?? []} broken={data.data?.broken ?? []} items={items} last={last} selected={current} onOpen={open} onHired={(name) => { data.refresh(); open(name) }} />}
          {agent ? (
            <AgentPage key={agent.name} realm={realm} agent={agent} agents={agents} departments={data.data?.departments ?? []} tasks={data.data?.tasks ?? null} narrow={narrow}
              onAgent={open} onDone={data.refresh} onSaved={data.refresh} onGone={() => { close(agent.name); data.refresh() }} onBack={() => setCurrent(null)} />
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
      {narrow && <TabBar page={page} onPage={(p) => { go(p); if (p === "agents") setCurrent(null) }} needs={items.length} />}
    </div>
    </FileLinks.Provider>
  )
}
