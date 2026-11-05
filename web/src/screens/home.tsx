import { useEffect, useState } from "react"
import { Activity as ActivityIcon, ChevronRight, CircleAlert, CircleCheck, CircleDollarSign, Clock3, Hand, Loader } from "lucide-react"
import { Character } from "@/components/character"
import { Skeleton } from "@/components/ui/skeleton"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Activity, type Agent, type AgentState, type Business, type Item } from "@/lib/api"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"

const GROUPS: { state: AgentState; label: string }[] = [
  { state: "needs", label: "Waiting for you" },
  { state: "incident", label: "Stopped on an incident" },
  { state: "working", label: "Working now" },
  { state: "idle", label: "Between runs" },
  { state: "asleep", label: "Nothing scheduled" },
]

/** A business's home: what matters today, its figures, the team by what
    each agent is doing, and what happened. */
export function Home({ realm, agents, items, business, at, error, onAgent, onNeeds }: {
  realm: string; agents: Agent[] | null; items: Item[]; business: Business | null; at: number; error: string | null
  onAgent: (name: string) => void; onNeeds: () => void
}) {
  const b = business
  const working = agents?.filter((a) => a.state === "working").length ?? 0
  const week = usePoll<Steering>(() => api(`realms/${enc(realm)}/steering`), 60000, [realm]).data
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl px-8 pt-9 pb-16">
        <header>
          <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{b?.description ?? realm}</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-tight capitalize">{realm}</h1>
          {b ? <p className="mt-1.5 text-base text-foreground/80">{b.headline}</p> : <Skeleton className="mt-3 h-5 w-72" />}
        </header>

        <div className="mt-6 grid grid-cols-2 gap-2.5 sm:grid-cols-4">
          <Stat icon={Hand} label="Need you" value={items.length} tone={items.length ? "amber" : undefined} onClick={items.length ? onNeeds : undefined} />
          <Stat icon={Loader} label="Working now" value={working} tone={working ? "emerald" : undefined} spin={working > 0} />
          <Stat icon={Clock3} label="Your time this week" value={b?.time ?? "–"} />
          <Stat icon={CircleDollarSign} label="Spent this week" value={b ? usd(b.cost_usd) || "$0" : "–"} />
        </div>

        <div className="mt-9 mb-2.5 flex items-center justify-between">
          <h2 className="text-sm font-semibold">Team</h2>
          <LiveTag at={at} error={error} />
        </div>

        {!agents ? (
          <div className="space-y-2">{[0, 1, 2].map((i) => <Skeleton key={i} className="h-16 rounded-xl" />)}</div>
        ) : (
          <div className="space-y-5">
            {GROUPS.map((g) => {
              const list = agents.filter((a) => a.state === g.state)
              if (!list.length) return null
              return (
                <section key={g.state}>
                  <h3 className="mb-1.5 flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    {g.label} <span className="tabular-nums opacity-60">{list.length}</span>
                  </h3>
                  <ul className="overflow-hidden rounded-xl border bg-bubble/40">
                    {list.map((a) => (
                      <AgentRow key={a.name} agent={a} activity={b?.activity.find((x) => x.agent === a.name)} onOpen={() => onAgent(a.name)} />
                    ))}
                  </ul>
                </section>
              )
            })}
          </div>
        )}

        {!!b?.activity.length && (
          <section className="mt-9">
            <h2 className="mb-2 flex items-center gap-2 text-sm font-semibold"><ActivityIcon className="size-4 text-muted-foreground" /> What happened</h2>
            <ol className="space-y-0.5">
              {b.activity.map((x, i) => {
                const a = agents?.find((y) => y.name === x.agent)
                return (
                  <li key={i}>
                    <button onClick={() => onAgent(x.agent)} className="flex w-full items-center gap-3 rounded-lg px-2 py-1.5 text-left hover:bg-accent/60">
                      {a ? <Character agent={a} size={20} still /> : <span className="size-5" />}
                      <span className="min-w-0 flex-1 text-sm">
                        <b className="font-medium">{x.agent_title}</b> <span className="text-foreground/75">{x.sentence.charAt(0).toLowerCase() + x.sentence.slice(1)}</span>
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

        {!!week?.attention.length && (
          <section className="mt-9">
            <h2 className="mb-2 flex items-center gap-2 text-sm font-semibold"><CircleAlert className="size-4 text-red-500" /> Needs attention</h2>
            <ol className="space-y-0.5">
              {week.attention.map((x, i) => (
                <li key={i} className="flex items-center gap-3 rounded-lg px-2 py-1.5">
                  <span className="min-w-0 flex-1 truncate text-sm text-red-600 dark:text-red-400" title={x.title}>{x.title.split("\n")[0]}</span>
                  {x.at && <span className="w-14 shrink-0 text-right text-xs text-muted-foreground">{ago(x.at)}</span>}
                </li>
              ))}
            </ol>
          </section>
        )}

        {!!week?.delivered.length && (
          <section className="mt-9">
            <h2 className="mb-2 flex items-center gap-2 text-sm font-semibold"><CircleCheck className="size-4 text-muted-foreground" /> Delivered this week</h2>
            <ol className="space-y-0.5">
              {week.delivered.map((x) => {
                const a = agents?.find((y) => y.name === x.agent)
                return (
                  <li key={x.run}>
                    <button disabled={!a} onClick={() => a && onAgent(a.name)} className="flex w-full items-start gap-3 rounded-lg px-2 py-1.5 text-left enabled:hover:bg-accent/60">
                      {a ? <Character agent={a} size={20} still /> : <span className="size-5 shrink-0" />}
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm font-medium">{x.title}</span>
                        {x.report && <span className="line-clamp-1 text-xs text-muted-foreground">{x.report}</span>}
                      </span>
                      <span className="w-14 shrink-0 text-right text-xs text-muted-foreground">{ago(x.at)}</span>
                    </button>
                  </li>
                )
              })}
            </ol>
          </section>
        )}

        {week && week.week.approved + week.week.rejected > 0 && (
          <p className="mt-9 text-xs text-muted-foreground">
            Drafts this week: {[`${week.week.approved} approved`, week.week.approved && `${Math.round((100 * week.week.unedited) / week.week.approved)}% of them unedited`, `${week.week.rejected} rejected`].filter(Boolean).join(", ")}.
          </p>
        )}
      </div>
    </div>
  )
}

/** What the old Steering page showed beyond the figures: what went wrong,
    what the team finished, and how the drafts fared this week. */
type Steering = {
  attention: { kind: string; title: string; run?: string | null; at?: string | null }[]
  delivered: { run: string; title: string; agent: string | null; at: string; report: string }[]
  week: { approved: number; unedited: number; rejected: number }
}

function Stat({ icon: Icon, label, value, tone, spin, onClick }: {
  icon: typeof Hand; label: string; value: string | number; tone?: "amber" | "emerald"; spin?: boolean; onClick?: () => void
}) {
  const Tag = onClick ? "button" : "div"
  return (
    <Tag onClick={onClick} className={cn("rounded-xl border bg-bubble/40 px-3.5 py-3 text-left transition-colors", onClick && "hover:border-foreground/20")}>
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Icon className={cn("size-3.5", tone === "amber" && "text-amber-500", tone === "emerald" && "text-emerald-500", spin && "animate-spin [animation-duration:2.5s]")} />
        {label}
      </div>
      <div className="mt-1 text-xl font-semibold tracking-tight tabular-nums">{value}</div>
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
      Live{s != null && s > 5 ? ` · ${s}s ago` : ""}
    </span>
  )
}

function AgentRow({ agent, activity, onOpen }: { agent: Agent; activity?: Activity; onOpen: () => void }) {
  const next = agent.routines.find((r) => r.schedule)
  return (
    <li className="border-b last:border-b-0">
      <button onClick={onOpen} className="group flex w-full items-center gap-3 px-3.5 py-3 text-left transition-colors hover:bg-accent/50">
        <Character agent={agent} size={34} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">{agent.title}</span>
            <span className="text-xs text-muted-foreground capitalize">{agent.department}</span>
          </div>
          {agent.state === "working" ? (
            <div className="mt-1 flex items-center gap-2">
              <span className="relative h-1 w-24 overflow-hidden rounded-full bg-emerald-500/15">
                <span className="absolute inset-y-0 w-1/3 animate-[slide_1.4s_ease-in-out_infinite] rounded-full bg-emerald-400" />
              </span>
              <span className="text-xs text-emerald-600 dark:text-emerald-300">Running {next?.name ?? "a mission"}…</span>
            </div>
          ) : (
            <p className="truncate text-[13px] text-muted-foreground">
              {activity ? <>{activity.sentence} <span className="opacity-70">· {ago(activity.at)}</span></> : agent.job}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {next?.schedule && <span className="hidden font-mono text-[11px] text-muted-foreground md:inline">{next.schedule}</span>}
          {agent.waiting > 0 && (
            <span className="rounded-full bg-red-500 px-1.5 text-[11px] font-semibold text-white">{agent.waiting}</span>
          )}
          <ChevronRight className="size-4 text-muted-foreground/50 transition-transform group-hover:translate-x-0.5" />
        </div>
      </button>
    </li>
  )
}
