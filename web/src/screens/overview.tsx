import { type ReactNode } from "react"
import { Activity as ActivityIcon, ArrowRight, ChevronRight, CircleCheck, CircleDollarSign, Clock3, Flag, FolderKanban, Hand, Landmark, Loader, Sun } from "lucide-react"
import { Character } from "@/components/character"
import { Skeleton } from "@/components/ui/skeleton"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, named, type Agent, type Business, type Company, type Item, type Project, type Task, type Tasks } from "@/lib/api"
import { glance } from "@/screens/project"
import { ImportStatement } from "@/components/import-statement"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"


/** Where the business stands at a glance (decision 133): the figures,
    today in the morning brief's words, what is in progress, the goal and
    the money; the week in detail folded under them. The team, the calendar
    and what failed have their own pages. */
export function Overview({ realm, agents, items, business, tasks, onAgent, onNeeds, onTask, onPage, onImported, onProject }: {
  realm: string; agents: Agent[] | null; items: Item[]; business: Business | null; tasks: Tasks | null
  onAgent: (name: string) => void; onNeeds: () => void; onTask: (id: string) => void; onPage: (page: "finance" | "tasks" | "calendar" | "company") => void; onImported: () => void; onProject: (id: number) => void
}) {
  const b = business
  const working = agents?.filter((a) => a.state === "working").length ?? 0
  const week = usePoll<Steering>(() => api(`realms/${enc(realm)}/steering`), 60000, [realm]).data
  const today = usePoll<{ lines: string[] }>(() => api(`realms/${enc(realm)}/today`), 60000, [realm]).data
  const company = usePoll<Company>(() => api(`realms/${enc(realm)}/company`), 300000, [realm]).data
  const goal = company ? readGoal(company.goal) : null
  const goalCard = <GoalCard goal={goal} loading={!company} onOpen={() => onPage("company")} />
  const money = <MoneyCard realm={realm} business={b} onOpen={() => onPage("finance")} onImported={onImported} />
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl px-8 pt-9 pb-16 max-md:px-4 max-md:pt-6 lg:max-w-6xl">
        <header>
          <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{b?.description ?? realm}</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-tight">{named(realm, business?.title)}</h1>
          {b ? <p className="mt-1.5 text-base text-foreground/80">{b.headline}</p> : <Skeleton className="mt-3 h-5 w-72" />}
        </header>

        <div className="mt-6 grid grid-cols-2 gap-2.5 sm:grid-cols-4">
          <Stat icon={Hand} label="Need you" value={items.length} tone={items.length ? "amber" : undefined} onClick={items.length ? onNeeds : undefined} />
          <Stat icon={Loader} label="Working now" value={working} tone={working ? "emerald" : undefined} spin={working > 0} />
          <Stat icon={Clock3} label="Your time this week" value={b?.time ?? "–"} />
          <Stat icon={CircleDollarSign} label="Spent this week" value={b ? usd(b.cost_usd) || "$0" : "–"} />
        </div>

        <div className="lg:mt-2 lg:grid lg:grid-cols-[minmax(0,1fr)_20rem] lg:gap-8">
        <div className="min-w-0">
        <TodayCard lines={today?.lines ?? null} />
        <div className="mt-7 space-y-7 lg:hidden">{goalCard}{money}</div>
        <Projects realm={realm} tasks={tasks} agents={agents ?? []} onTask={onTask} onProject={onProject} onAll={() => onPage("tasks")} />


        {/* The week in detail, for when the founder wants it (decision 133). */}
        {(!!b?.activity.length || !!week?.delivered.length) && (
        <details className="group mt-9">
          <summary className="flex cursor-pointer list-none items-center gap-2 text-sm font-semibold text-muted-foreground hover:text-foreground">
            <ChevronRight className="size-4 transition-transform group-open:rotate-90" /> The week in detail
          </summary>
        {!!b?.activity.length && (
          <section className="mt-6">
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


        {!!week?.delivered.length && (
          <section className="mt-7">
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
          <p className="mt-7 text-xs text-muted-foreground">
            Drafts this week: {[`${week.week.approved} approved`, week.week.approved && `${Math.round((100 * week.week.unedited) / week.week.approved)}% of them unedited`, `${week.week.rejected} rejected`].filter(Boolean).join(", ")}.
          </p>
        )}
        </details>
        )}
        </div>
        <aside className="mt-7 hidden space-y-7 lg:block">{goalCard}{money}</aside>
        </div>
      </div>
    </div>
  )
}

/** A section's title, with a link to the page that has the rest. */
function Head({ icon: Icon, title, more, onMore }: { icon: typeof Hand; title: string; more?: string; onMore?: () => void }) {
  return (
    <div className="mb-2.5 flex items-center justify-between gap-3">
      <h2 className="flex items-center gap-2 text-sm font-semibold"><Icon className="size-4 text-muted-foreground" /> {title}</h2>
      {onMore && (
        <button onClick={onMore} className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          {more ?? "Open"} <ArrowRight className="size-3" />
        </button>
      )}
    </div>
  )
}

function Card({ children }: { children: ReactNode }) {
  return <div className="rounded-xl bg-bubble px-4 py-3.5">{children}</div>
}

/** The morning brief's words, as of now (decision 121): what waits, what
    ran, what runs today. Code wrote them, not a model. */
function TodayCard({ lines }: { lines: string[] | null }) {
  const day = new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" })
  return (
    <section className="mt-7">
      <Head icon={Sun} title={`Today · ${day}`} />
      <Card>
        {!lines ? <Skeleton className="h-12" /> : (
          <ul className="space-y-1.5 text-sm">
            {lines.map((l, i) => {
              const at = l.indexOf(": ")
              return (
                <li key={i} className="leading-snug [overflow-wrap:anywhere]">
                  {at > 0 && at < 30 ? <><span className="font-medium">{l.slice(0, at)}:</span> <span className="text-foreground/80">{l.slice(at + 2)}</span></> : <span className="text-foreground/80">{l}</span>}
                </li>
              )
            })}
          </ul>
        )}
      </Card>
    </section>
  )
}

type Milestone = { text: string; date: Date | null }
type Goal = { goal: string | null; target: string | null; by: Date | null; milestones: Milestone[] }

/** `business/goal.md`'s fields, its milestones' dates read from "… by
    YYYY-MM-DD". */
function readGoal(g: Company["goal"]): Goal | null {
  if (!g?.goal) return null
  const date = (s?: string | null) => (s && /^\d{4}-\d{2}-\d{2}$/.test(s) ? new Date(`${s}T12:00:00`) : null)
  const raw = g.milestones as unknown
  const list = Array.isArray(raw) ? (raw as string[]) : typeof raw === "string" ? raw.replace(/^\[|\]$/g, "").split(",") : []
  const milestones = list.map((m) => m.trim()).filter(Boolean).map((m) => {
    const hit = m.match(/^(.*?)\s+by\s+(\d{4}-\d{2}-\d{2})$/)
    return hit ? { text: hit[1], date: date(hit[2]) } : { text: m, date: null }
  })
  return { goal: g.goal, target: g.target ?? null, by: date(g.by), milestones }
}

const DAY = 86400000
const daysTo = (d: Date) => Math.ceil((d.getTime() - Date.now()) / DAY)
const inDays = (n: number) => (n === 0 ? "today" : n === 1 ? "tomorrow" : n > 0 ? `in ${n} days` : n === -1 ? "yesterday" : `${-n} days ago`)
const short = (d: Date) => d.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: d.getFullYear() === new Date().getFullYear() ? undefined : "numeric" })

/** The goal, its deadline, and its milestones, the next one marked. */
function GoalCard({ goal, loading, onOpen }: { goal: Goal | null; loading: boolean; onOpen: () => void }) {
  const next = goal?.milestones.find((m) => m.date && daysTo(m.date) >= 0)
  return (
    <section>
      <Head icon={Flag} title="Goal" more="Business" onMore={onOpen} />
      <Card>
        {loading ? <Skeleton className="h-16" /> : !goal ? (
          <p className="text-sm text-muted-foreground">No goal written yet: business/goal.md, with a target, a date and milestones.</p>
        ) : (
          <>
            <p className="text-sm font-medium leading-snug">{goal.goal}</p>
            {(goal.target || goal.by) && (
              <p className="mt-1 text-xs text-muted-foreground">
                {[goal.target, goal.by && `by ${short(goal.by)} (${inDays(daysTo(goal.by))})`].filter(Boolean).join(" · ")}
              </p>
            )}
            {!!goal.milestones.length && (
              <ol className="mt-3 space-y-2 border-t pt-3">
                {goal.milestones.map((m, i) => {
                  const n = m.date ? daysTo(m.date) : null
                  const passed = n !== null && n < 0
                  const isNext = m === next
                  return (
                    <li key={i} className="flex items-start gap-2.5 text-[13px]">
                      <span className={cn("mt-1.5 size-2 shrink-0 rounded-full", isNext ? "bg-amber-500" : passed ? "bg-muted-foreground/40" : "border border-muted-foreground/50")} />
                      <span className={cn("min-w-0 flex-1 leading-snug", passed && "text-muted-foreground")}>
                        {m.text}
                        {m.date && <span className={cn("block text-xs", isNext ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground")}>{short(m.date)} · {passed ? "date passed" : inDays(n!)}</span>}
                      </span>
                    </li>
                  )
                })}
              </ol>
            )}
          </>
        )}
      </Card>
    </section>
  )
}

/** Cash and runway, from the bank or the statements imported (decision
    123); without figures, the way to import a statement. */
function MoneyCard({ realm, business, onOpen, onImported }: { realm: string; business: Business | null; onOpen: () => void; onImported: () => void }) {
  const f = business?.finance
  const months = f?.runway_months
  return (
    <section>
      <Head icon={Landmark} title="Money" more="Finance" onMore={onOpen} />
      <Card>
        {!business ? <Skeleton className="h-16" /> : f && !f.unknown ? (
          <>
            <div className="flex items-end justify-between gap-3">
              <div>
                <div className="text-xs text-muted-foreground">Cash</div>
                <div className="text-xl font-semibold tracking-tight tabular-nums">{f.cash}</div>
              </div>
              <div className="text-right">
                <div className="text-xs text-muted-foreground">Runway</div>
                <div className={cn("text-xl font-semibold tracking-tight", months != null && months < 6 ? "text-red-500" : months != null && months < 12 ? "text-amber-500" : undefined)}>{f.runway}</div>
              </div>
            </div>
            <div className="mt-2.5 flex gap-4 border-t pt-2.5 text-xs text-muted-foreground">
              {f.in && <span>In this month <b className="font-medium text-foreground">{f.in}</b></span>}
              <span>Out <b className="font-medium text-foreground">{f.out}</b></span>
            </div>
            <p className="mt-1.5 text-[11px] text-muted-foreground">{f.source}</p>
          </>
        ) : (
          <>
            <p className="text-sm">{f?.unknown ?? "No figures yet"}</p>
            <p className="mt-1 mb-2.5 text-xs text-muted-foreground">Import a Revolut statement (CSV) to see cash, spending and runway.</p>
            <ImportStatement realm={realm} onDone={onImported} />
          </>
        )}
      </Card>
    </section>
  )
}


const STATUS: Record<keyof Tasks, { label: string; tone: string }> = {
  waiting: { label: "Needs your answer", tone: "text-amber-600 dark:text-amber-400" },
  review: { label: "To review", tone: "text-sky-600 dark:text-sky-400" },
  running: { label: "Working", tone: "text-emerald-600 dark:text-emerald-400" },
  done: { label: "Done", tone: "text-muted-foreground" },
}
const RANK: (keyof Tasks)[] = ["waiting", "review", "running", "done"]

/** What the team is on: the open projects first (decision 129), each with
    its progress and next milestone; then each open task not in one, a
    plan's tasks (decision 120) counted under the one that passed them on,
    the most pressing first. */
function Projects({ realm, tasks, agents, onTask, onProject, onAll }: {
  realm: string; tasks: Tasks | null; agents: Agent[]; onTask: (id: string) => void; onProject: (id: number) => void; onAll: () => void
}) {
  const projects = (usePoll<{ projects: Project[] }>(() => api(`realms/${enc(realm)}/projects`), 60000, [realm]).data?.projects ?? []).filter((p) => p.status === "active")
  const ours = new Set(projects.flatMap((p) => [p.run, ...p.tasks.map((t) => t.run)]))
  const all: (Task & { group: keyof Tasks })[] = tasks ? RANK.flatMap((g) => tasks[g].map((t) => ({ ...t, group: g }))).filter((t) => !ours.has(t.id)) : []
  const has = new Set(all.map((t) => t.id))
  const kids = (id: string) => all.filter((t) => t.from?.task === id)
  const rank = (t: Task & { group: keyof Tasks }) => Math.min(...[t, ...kids(t.id)].map((x) => RANK.indexOf(x.group)))
  const open = all
    .filter((t) => !(t.from?.task && has.has(t.from.task)))
    .filter((t) => rank(t) < RANK.indexOf("done"))
    .sort((a, b) => rank(a) - rank(b))
  return (
    <section className="mt-7">
      <Head icon={FolderKanban} title="In progress" more="Projects" onMore={onAll} />
      {!tasks ? <Skeleton className="h-20 rounded-xl" /> : !open.length && !projects.length ? (
        <Card><p className="text-sm text-muted-foreground">Nothing in progress. Ask the Chief of staff for something, and the team's tasks show here.</p></Card>
      ) : (
        <ul className="overflow-hidden rounded-xl bg-bubble">
          {projects.slice(0, 6).map((p) => {
            const g = glance(p)
            return (
              <li key={`p${p.id}`} className="border-b last:border-b-0">
                <button onClick={() => onProject(p.id)} className="flex w-full items-center gap-3 px-3.5 py-3 text-left hover:bg-accent/50">
                  <span className={cn("grid size-[26px] shrink-0 place-items-center rounded-md bg-muted text-muted-foreground", g.running && "bg-sky-500/10 text-sky-600 dark:text-sky-300")}><FolderKanban className="size-3.5" /></span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{p.name}</span>
                    <span className="mt-1 flex items-center gap-2">
                      <span className="h-1 w-28 shrink-0 overflow-hidden rounded-full bg-muted">
                        <span className="block h-full rounded-full bg-emerald-500" style={{ width: `${g.total ? (100 * g.done) / g.total : 0}%` }} />
                      </span>
                      <span className="truncate text-[11px] text-muted-foreground">
                        {g.done}/{g.total} done{g.next && ` · next: ${g.next.title}`}
                        {g.when && <span className={cn(g.when.late && "text-red-500")}>{`, ${g.when.text}`}</span>}
                        {g.stuck > 0 && <span className="text-red-500">{` · ${g.stuck} stuck`}</span>}
                      </span>
                    </span>
                  </span>
                  <ChevronRight className="size-4 shrink-0 text-muted-foreground/50" />
                </button>
              </li>
            )
          })}
          {open.slice(0, Math.max(0, 6 - projects.length)).map((t) => {
            const team = kids(t.id)
            const done = team.filter((k) => k.group === "done").length
            const a = agents.find((x) => x.name === t.agent)
            const g = RANK[rank(t)]
            return (
              <li key={t.id} className="border-b last:border-b-0">
                <button onClick={() => onTask(t.id)} className="flex w-full items-center gap-3 px-3.5 py-3 text-left hover:bg-accent/50">
                  {a ? <Character agent={a} size={26} still={g !== "running"} /> : <span className="size-[26px] shrink-0 rounded-full bg-muted" />}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm">{t.order ?? t.id}</span>
                    <span className="mt-0.5 flex items-center gap-2 text-xs">
                      <span className={STATUS[g].tone}>{STATUS[g].label}</span>
                      {a && <span className="truncate text-muted-foreground">· {a.title}</span>}
                    </span>
                    {!!team.length && (
                      <span className="mt-1.5 flex items-center gap-2">
                        <span className="h-1 w-28 overflow-hidden rounded-full bg-muted">
                          <span className="block h-full rounded-full bg-emerald-500" style={{ width: `${(100 * done) / team.length}%` }} />
                        </span>
                        <span className="text-[11px] text-muted-foreground">{done}/{team.length} of the plan done</span>
                      </span>
                    )}
                  </span>
                  <ChevronRight className="size-4 shrink-0 text-muted-foreground/50" />
                </button>
              </li>
            )
          })}
        </ul>
      )}
      {open.length + projects.length > 6 && <p className="mt-1.5 text-xs text-muted-foreground">And {open.length + projects.length - 6} more in Projects.</p>}
    </section>
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
    <Tag onClick={onClick} className={cn("rounded-xl bg-bubble px-3.5 py-3 text-left transition-colors max-md:py-2.5", onClick && "hover:bg-accent/70")}>
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Icon className={cn("size-3.5", tone === "amber" && "text-amber-500", tone === "emerald" && "text-emerald-500", spin && "animate-spin [animation-duration:2.5s]")} />
        {label}
      </div>
      <div className="mt-1 text-xl font-semibold tracking-tight tabular-nums max-md:mt-0.5 max-md:text-lg">{value}</div>
    </Tag>
  )
}


