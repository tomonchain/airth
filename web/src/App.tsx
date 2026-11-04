import { useEffect, useMemo, useState } from "react"
import { AgentList, type Last } from "@/components/agent-list"
import { Details } from "@/components/details"
import { Rail, type Page } from "@/components/rail"
import { TopBar } from "@/components/topbar"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Business, type Item, type Realm } from "@/lib/api"
import { Conversation } from "@/screens/conversation"
import { Calendar } from "@/screens/calendar"
import { Company } from "@/screens/company"
import { Finance } from "@/screens/finance"
import { Home } from "@/screens/home"
import { Routines } from "@/screens/routines"
import { Tasks } from "@/screens/tasks"
import { Inbox } from "@/screens/inbox"

function route(): string | null {
  const m = location.hash.match(/^#\/b\/([^/?&]+)/)
  return m ? decodeURIComponent(m[1]) : null
}

type Data = { agents: Agent[]; items: Item[]; business: Business | null }

export default function App() {
  const [hash, setHash] = useState(route)
  useEffect(() => {
    const on = () => setHash(route())
    addEventListener("hashchange", on)
    return () => removeEventListener("hashchange", on)
  }, [])
  const realms = usePoll(() => api<Realm[]>("realms"), 15000, [])
  const realm = hash ?? realms.data?.find((r) => r.default)?.name ?? realms.data?.[0]?.name ?? null

  const data = usePoll<Data>(
    realm
      ? async () => {
          const [team, queue, hq] = await Promise.all([
            api<{ agents: Agent[] }>(`realms/${enc(realm)}/team`),
            api<{ items: Item[] }>(`realms/${enc(realm)}/queue`),
            api<{ businesses: Business[] }>("hq"),
          ])
          return { agents: team.agents, items: queue.items, business: hq.businesses.find((b) => b.name === realm) ?? null }
        }
      : null,
    4000,
    [realm],
  )

  const [page, setPage] = useState<Page>("home")
  const [current, setCurrent] = useState<string | null>(null)
  const [tabs, setTabs] = useState<string[]>([])
  const [details, setDetails] = useState(true)
  const agents = data.data?.agents ?? []
  const items = data.data?.items ?? []

  useEffect(() => { setCurrent(null); setTabs([]) }, [realm])
  useEffect(() => {
    if (page !== "agents" || current || !agents.length) return
    const first = agents.find((a) => a.waiting > 0) ?? agents[0]
    open(first.name)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agents.length, current, page])

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

  // Each agent's latest line, for the list: a pending item first, else its
  // latest run's sentence.
  const last = useMemo(() => {
    const out: Record<string, Last> = {}
    for (const x of data.data?.business?.activity ?? []) if (!out[x.agent]) out[x.agent] = { text: x.sentence, at: x.at }
    for (const it of [...items].sort((a, b) => b.created_at.localeCompare(a.created_at))) {
      if (!it.agent) continue
      const l = out[it.agent]
      if (!l || (l.at ?? "") < it.created_at) out[it.agent] = { text: it.text?.split("\n")[0] ?? it.title, at: it.created_at }
    }
    return out
  }, [data.data, items])

  const agent = agents.find((a) => a.name === current) ?? null

  if (!realm) {
    return <div className="grid h-svh place-items-center text-sm text-muted-foreground">{realms.error ? `Airth is not answering: ${realms.error}` : "Loading…"}</div>
  }

  return (
    <div className="flex h-svh flex-col overflow-hidden bg-background">
      <TopBar
        realms={realms.data ?? []} realm={realm} tabs={tabs} current={page === "agents" ? current : null} agents={agents}
        onTab={open} onClose={close} onNew={() => setPage("agents")} details={details} onDetails={() => setDetails(!details)}
      />
      <div className="flex min-h-0 flex-1">
        <Rail page={page} onPage={setPage} needs={items.length} live={agents.some((a) => a.state === "working")} />
        <main className="mr-2 mb-2 flex min-w-0 flex-1 overflow-hidden rounded-xl border bg-surface shadow-sm">
          {page === "home" ? (
            <Home realm={realm} agents={data.data?.agents ?? null} items={items} business={data.data?.business ?? null}
              at={data.at} error={data.error} onAgent={open} onNeeds={() => setPage("needs")} />
          ) : page === "routines" ? <Routines realm={realm} agents={agents} onAgent={open} />
          : page === "tasks" ? <Tasks realm={realm} agents={agents} onAgent={open} />
          : page === "calendar" ? <Calendar realm={realm} agents={agents} onAgent={open} />
          : page === "finance" ? <Finance realm={realm} business={data.data?.business ?? null} agents={agents} />
          : page === "company" ? <Company realm={realm} description={data.data?.business?.description ?? null} />
          : (
          <>
          {page === "needs" ? (
            <Inbox realm={realm} items={items} agents={agents} onDone={data.refresh} onAgent={open} />
          ) : (
          <>
          <AgentList agents={agents} items={items} last={last} selected={current} onOpen={open} />
          {agent ? (
            <>
              <Conversation key={agent.name} realm={realm} agent={agent} onDone={data.refresh} />
              {details && <Details agent={agent} />}
            </>
          ) : (
            <div className="grid flex-1 place-items-center text-sm text-muted-foreground">{data.error ?? "Choose an agent."}</div>
          )}
          </>
          )}
          </>
          )}
        </main>
      </div>
    </div>
  )
}
