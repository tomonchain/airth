import { useEffect, useMemo, useState } from "react"
import { Activity as ActivityIcon, ChevronRight, CircleDollarSign, Clock3, Hand, Loader, X } from "lucide-react"
import { AgentAvatar, deptHue } from "@/components/agent-avatar"
import { NeedCard } from "@/components/need-card"
import { OrderBox } from "@/components/order-box"
import { RunTimeline } from "@/components/run-timeline"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Activity, type Agent, type AgentState, type Business, type Item, type Run } from "@/lib/api"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"

const GROUPS: { state: AgentState; label: string }[] = [
  { state: "needs", label: "Waiting for you" },
  { state: "incident", label: "Stopped on an incident" },
  { state: "working", label: "Working now" },
  { state: "idle", label: "Between runs" },
  { state: "asleep", label: "Nothing scheduled" },
]

// Most pressing first: what broke, what the check holds, what is due.
function rank(it: Item) {
  if (it.kind === "incident") return 0
  if (it.flag === "blocking" || (it.kind === "deadline" && it.overdue)) return 1
  return { deadline: 2, approval: 3, prepared: 4, delayed: 5, report: 6, incident: 0 }[it.kind] ?? 6
}

type Data = { team: Agent[]; items: Item[]; business: Business | null }

type Peek = { agent: string } | { inbox: true } | null

/** A business at work: its agents by what they are doing, live, and what
    their work sends up to the founder. */
export function Floor({ realm }: { realm: string }) {
  const { data, at, refresh, error } = usePoll<Data>(
    async () => {
      const [team, queue, hq] = await Promise.all([
        api<{ agents: Agent[] }>(`realms/${enc(realm)}/team`),
        api<{ items: Item[] }>(`realms/${enc(realm)}/queue`),
        api<{ businesses: Business[] }>("hq"),
      ])
      return { team: team.agents, items: queue.items, business: hq.businesses.find((b) => b.name === realm) ?? null }
    },
    4000,
    [realm],
  )
  const [peek, setPeek] = useState<Peek>(null)
  useEffect(() => setPeek(null), [realm])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setPeek(null)
    addEventListener("keydown", onKey)
    return () => removeEventListener("keydown", onKey)
  }, [])

  const items = useMemo(() => [...(data?.items ?? [])].sort((a, b) => rank(a) - rank(b) || a.created_at.localeCompare(b.created_at)), [data])
  const b = data?.business
  const working = data?.team.filter((a) => a.state === "working").length ?? 0

  return (
    <div className="flex min-w-0 flex-1">
      <ScrollArea className="h-svh min-w-0 flex-1">
        <div className="mx-auto max-w-4xl px-8 pt-10 pb-24">
          <header>
            <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{b?.description ?? realm}</p>
            <h1 className="mt-1.5 text-3xl font-semibold tracking-tight capitalize">{realm}</h1>
            {b ? <p className="mt-2 text-lg text-foreground/80">{b.headline}</p> : <Skeleton className="mt-3 h-6 w-80" />}
          </header>

          <div className="mt-6 grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Stat
              icon={Hand} label="Need you" value={items.length} tone={items.length ? "amber" : undefined}
              onClick={items.length ? () => setPeek({ inbox: true }) : undefined} active={!!peek && "inbox" in peek}
            />
            <Stat icon={Loader} label="Working now" value={working} tone={working ? "emerald" : undefined} spin={working > 0} />
            <Stat icon={Clock3} label="Your time this week" value={b?.time ?? "–"} />
            <Stat icon={CircleDollarSign} label="Spent this week" value={b ? usd(b.cost_usd) || "$0" : "–"} />
          </div>

          <div className="mt-10 mb-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold">Team</h2>
            <LiveTag at={at} error={error} />
          </div>

          {!data ? (
            <div className="space-y-2">{[0, 1, 2].map((i) => <Skeleton key={i} className="h-[72px] rounded-xl" />)}</div>
          ) : (
            <div className="space-y-6">
              {GROUPS.map((g) => {
                const agents = data.team.filter((a) => a.state === g.state)
                if (!agents.length) return null
                return (
                  <section key={g.state}>
                    <h3 className="mb-2 flex items-center gap-2 text-xs font-medium text-muted-foreground">
                      {g.label} <span className="tabular-nums opacity-60">{agents.length}</span>
                    </h3>
                    <ul className="overflow-hidden rounded-xl border bg-card shadow-xs">
                      {agents.map((a) => (
                        <AgentRow
                          key={a.name} agent={a} activity={b?.activity.find((x) => x.agent === a.name)}
                          selected={!!peek && "agent" in peek && peek.agent === a.name}
                          onOpen={() => setPeek({ agent: a.name })}
                        />
                      ))}
                    </ul>
                  </section>
                )
              })}
            </div>
          )}

          {!!b?.activity.length && (
            <section className="mt-10">
              <h2 className="mb-3 flex items-center gap-2 text-sm font-semibold"><ActivityIcon className="size-4 text-muted-foreground" /> What happened</h2>
              <ol className="space-y-0.5">
                {b.activity.map((x, i) => {
                  const agent = data?.team.find((a) => a.name === x.agent)
                  return (
                    <li key={i}>
                      <button onClick={() => setPeek({ agent: x.agent })} className="flex w-full items-start gap-3 rounded-lg px-2 py-2 text-left hover:bg-muted/60">
                        <span className="mt-1.5 size-2 shrink-0 rounded-full" style={{ background: deptHue(agent?.department ?? null) }} />
                        <span className="min-w-0 flex-1 text-sm">
                          <b className="font-medium">{x.agent_title}</b> <span className="text-foreground/75">{lower(x.sentence)}</span>
                        </span>
                        <span className="shrink-0 font-mono text-[11px] text-muted-foreground">{x.routine}</span>
                        <span className="w-14 shrink-0 text-right text-xs text-muted-foreground">{ago(x.at)}</span>
                      </button>
                    </li>
                  )
                })}
              </ol>
            </section>
          )}
        </div>
      </ScrollArea>

      {peek && data && (
        <aside className="flex h-svh w-[460px] shrink-0 flex-col border-l bg-sidebar/40 animate-in fade-in slide-in-from-right-4 duration-200">
          {"inbox" in peek ? (
            <Inbox realm={realm} items={items} team={data.team} onClose={() => setPeek(null)} onDone={refresh} onAgent={(a) => setPeek({ agent: a })} />
          ) : (
            (() => {
              const agent = data.team.find((a) => a.name === peek.agent)
              return agent ? (
                <AgentPeek key={agent.name} realm={realm} agent={agent} items={items.filter((i) => i.agent === agent.name)}
                  activity={b?.activity.filter((x) => x.agent === agent.name) ?? []} onClose={() => setPeek(null)} onDone={refresh} />
              ) : null
            })()
          )}
        </aside>
      )}
    </div>
  )
}

const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1)

function Stat({ icon: Icon, label, value, tone, spin, onClick, active }: {
  icon: typeof Hand; label: string; value: string | number; tone?: "amber" | "emerald"; spin?: boolean; onClick?: () => void; active?: boolean
}) {
  const Tag = onClick ? "button" : "div"
  return (
    <Tag
      onClick={onClick}
      className={cn(
        "rounded-xl border bg-card p-3.5 text-left shadow-xs transition-colors",
        onClick && "hover:border-foreground/20",
        active && "border-amber-400/60 ring-3 ring-amber-400/15",
      )}
    >
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Icon className={cn("size-3.5", tone === "amber" && "text-amber-500", tone === "emerald" && "text-emerald-500", spin && "animate-spin [animation-duration:2.5s]")} />
        {label}
      </div>
      <div className="mt-1.5 text-2xl font-semibold tracking-tight tabular-nums">{value}</div>
    </Tag>
  )
}

function LiveTag({ at, error }: { at: number; error: string | null }) {
  const [, tick] = useState(0)
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 1000)
    return () => clearInterval(t)
  }, [])
  if (error) return <span className="text-xs text-red-500">Airth is not answering: {error}</span>
  const s = at ? Math.round((Date.now() - at) / 1000) : null
  return (
    <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
      <span className="relative flex size-2">
        <span className="absolute inset-0 animate-ping rounded-full bg-emerald-400/60" />
        <span className="relative size-2 rounded-full bg-emerald-400" />
      </span>
      Live{s != null && s > 2 ? ` · ${s}s ago` : ""}
    </span>
  )
}

function AgentRow({ agent, activity, selected, onOpen }: { agent: Agent; activity?: Activity; selected: boolean; onOpen: () => void }) {
  const next = agent.routines.find((r) => r.schedule)
  return (
    <li className="border-b last:border-b-0">
      <button
        onClick={onOpen}
        className={cn("group flex w-full items-center gap-3.5 px-4 py-3.5 text-left transition-colors hover:bg-muted/50", selected && "bg-muted/70")}
      >
        <AgentAvatar agent={agent} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">{agent.title}</span>
            <span className="text-xs text-muted-foreground capitalize">{agent.department}</span>
          </div>
          {agent.state === "working" ? (
            <div className="mt-1.5 flex items-center gap-2">
              <span className="relative h-1 w-28 overflow-hidden rounded-full bg-emerald-500/15">
                <span className="absolute inset-y-0 w-1/3 animate-[slide_1.4s_ease-in-out_infinite] rounded-full bg-emerald-400" />
              </span>
              <span className="text-xs text-emerald-600 dark:text-emerald-300">Running {next?.name ?? "a mission"}…</span>
            </div>
          ) : (
            <p className="mt-0.5 truncate text-[13px] text-muted-foreground">
              {activity ? <>{activity.sentence} <span className="opacity-70">· {ago(activity.at)}</span></> : agent.job}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {next?.schedule && <span className="hidden font-mono text-[11px] text-muted-foreground md:inline">{next.schedule}</span>}
          {agent.waiting > 0 && (
            <Badge className="border-amber-400/30 bg-amber-400/15 text-amber-700 dark:text-amber-300" variant="outline">{agent.waiting} for you</Badge>
          )}
          <ChevronRight className="size-4 text-muted-foreground/50 transition-transform group-hover:translate-x-0.5" />
        </div>
      </button>
    </li>
  )
}

function PeekHeader({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  return (
    <div className="flex items-start gap-3 border-b px-5 py-4">
      <div className="min-w-0 flex-1">{children}</div>
      <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close"><X /></Button>
    </div>
  )
}

function AgentPeek({ realm, agent, items, activity, onClose, onDone }: {
  realm: string; agent: Agent; items: Item[]; activity: Activity[]; onClose: () => void; onDone: () => void
}) {
  const runId = items.find((i) => i.run)?.id
  const [run, setRun] = useState<Run | null>(null)
  useEffect(() => {
    setRun(null)
    if (runId != null) api<Run>(`realms/${enc(realm)}/queue/${runId}/run`).then(setRun).catch(() => setRun(null))
  }, [realm, runId])

  return (
    <>
      <PeekHeader onClose={onClose}>
        <div className="flex items-center gap-3">
          <AgentAvatar agent={agent} size="lg" />
          <div className="min-w-0">
            <h2 className="text-lg font-semibold tracking-tight">{agent.title}</h2>
            <p className="text-xs text-muted-foreground"><span className="capitalize">{agent.department}</span> · {usd(agent.cost_usd) || "$0"} this week</p>
          </div>
        </div>
        {agent.job && <p className="mt-3 text-[13px] leading-relaxed text-muted-foreground">{agent.job}</p>}
      </PeekHeader>

      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-7 px-5 py-5">
          {items.length > 0 && (
            <section>
              <SectionTitle>Waiting for you · {items.length}</SectionTitle>
              <div className="space-y-3">{items.map((it) => <NeedCard key={`${it.id}-${it.revision}`} realm={realm} it={it} onDone={onDone} />)}</div>
            </section>
          )}

          {run ? (
            <section>
              <SectionTitle>Latest run</SectionTitle>
              <div className="rounded-xl border bg-card p-4"><RunTimeline run={run} /></div>
            </section>
          ) : activity.length > 0 ? (
            <section>
              <SectionTitle>Recent runs</SectionTitle>
              <ul className="space-y-2">
                {activity.map((x, i) => (
                  <li key={i} className="rounded-lg border bg-card px-3 py-2.5 text-sm">
                    {x.sentence}
                    <div className="mt-1 flex justify-between text-xs text-muted-foreground"><span className="font-mono">{x.routine}</span><span>{ago(x.at)}</span></div>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          <section>
            <SectionTitle>Routines</SectionTitle>
            {agent.routines.length ? (
              <ul className="divide-y rounded-xl border bg-card">
                {agent.routines.map((r) => (
                  <li key={r.name} className="flex items-center gap-3 px-3.5 py-2.5 text-sm">
                    <span className={cn("size-1.5 rounded-full", r.last ? (r.last.ok ? "bg-emerald-400" : "bg-red-500") : "bg-zinc-400")} />
                    <span className="flex-1 font-mono text-[13px]">{r.name}</span>
                    <span className="text-xs text-muted-foreground">{r.schedule ?? "on demand"}</span>
                    <span className="w-16 text-right text-xs text-muted-foreground">{r.last ? ago(r.last.at) : "never"}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-sm text-muted-foreground">No routine yet. Give it an order below, or a routine in Routines.</p>
            )}
          </section>
        </div>
      </ScrollArea>

      <div className="border-t p-3">
        <OrderBox realm={realm} agent={agent.name} title={agent.title} onSent={onDone} />
      </div>
    </>
  )
}

function Inbox({ realm, items, team, onClose, onDone, onAgent }: {
  realm: string; items: Item[]; team: Agent[]; onClose: () => void; onDone: () => void; onAgent: (a: string) => void
}) {
  return (
    <>
      <PeekHeader onClose={onClose}>
        <h2 className="text-lg font-semibold tracking-tight">Needs you</h2>
        <p className="text-xs text-muted-foreground">{items.length ? `${items.length} things the team sent up, most pressing first.` : "Nothing waits for you."}</p>
      </PeekHeader>
      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-3 px-5 py-5">
          {items.map((it) => {
            const a = team.find((x) => x.name === it.agent)
            return (
              <div key={`${it.id}-${it.revision}`}>
                {a && (
                  <button onClick={() => onAgent(a.name)} className="mb-1.5 flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground">
                    <AgentAvatar agent={a} size="sm" /> {a.title}
                  </button>
                )}
                <NeedCard realm={realm} it={it} onDone={onDone} />
              </div>
            )
          })}
        </div>
      </ScrollArea>
    </>
  )
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="mb-2.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">{children}</h3>
}
