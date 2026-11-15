import { useState } from "react"
import { toast } from "sonner"
import { Input } from "@/components/ui/input"
import { ExternalLink, Landmark } from "lucide-react"
import { ImportStatement } from "@/components/import-statement"
import { Character } from "@/components/character"
import { Empty, Figure, Loading, Page, Panel, Section, Seg } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Business, type Calendar, type Company, type Routine } from "@/lib/api"
import { usd } from "@/lib/time"
import { cn } from "@/lib/utils"

/** The business's money: cash and runway from the bank (read-only, decision
    54), from the statements the founder imports (decision 123) or entered
    by hand (decision 68), what the agents cost, and the tax and filing
    dates its profile implies. Never a total across businesses. */
export function Finance({ realm, business, agents, onImported }: { realm: string; business: Business | null; agents: Agent[]; onImported?: () => void }) {
  const cal = usePoll<Calendar>(() => api(`realms/${enc(realm)}/calendar`), 60000, [realm])
  const routines = usePoll<{ routines: Routine[] }>(() => api(`realms/${enc(realm)}/graphs`), 60000, [realm])
  const company = usePoll<Company>(() => api(`realms/${enc(realm)}/company`), 60000, [realm])
  const f = business?.finance
  const p = company.data?.profile
  const months = f?.runway_months
  const spend = agents.reduce((s, a) => s + a.cost_usd, 0)
  const max = Math.max(...agents.map((a) => a.cost_usd), 0.0001)
  const [imported, setImported] = useState(0)

  return (
    <Page eyebrow={p?.legal_name ?? realm} title="Finance" lede={f?.runway ? `Runway ${f.runway}.` : "What the business has, spends and owes."}>
      <Section title="Cash" note={<span className="flex items-center gap-2">{f?.source}{f && !f.unknown && <ImportStatement realm={realm} onDone={() => { setImported((n) => n + 1); onImported?.() }} small />}</span>}>
        {!business ? <Loading /> : f && !f.unknown ? (
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            <Figure label="Cash" value={f.cash} />
            <Figure label="In this month" value={f.in} />
            <Figure label="Out this month" value={f.out} />
            <Figure label="Runway" value={f.runway} tone={months == null ? undefined : months < 6 ? "bad" : months < 12 ? "warn" : undefined} />
          </div>
        ) : (
          <Empty icon={Landmark} title={f?.unknown ?? "No bank figures yet"}>
            Export a statement from Revolut (Statements, Transaction statement, CSV) and import it here, each week or month: a transaction already in counts once. Airth never moves money.
            <span className="mt-3 block"><ImportStatement realm={realm} onDone={() => { setImported((n) => n + 1); onImported?.() }} /></span>
          </Empty>
        )}
      </Section>

      <Spending key={imported} realm={realm} />

      <Section title="What the agents cost" note="This week">
        <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-3">
          <Figure label="Models" value={usd(business?.cost_usd ?? spend) || "$0"} />
          <Figure label="Your time" value={business?.time ?? "–"} />
          <Figure label="Per approved item" value={approvedCost(business)} />
        </div>
        <Panel className="mt-2.5 divide-y">
          {agents.map((a) => (
            <div key={a.name} className="flex items-center gap-3 px-3.5 py-2.5">
              <Character agent={a} size={24} still />
              <span className="w-32 shrink-0 truncate text-sm">{a.title}</span>
              <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                <span className="block h-full rounded-full bg-foreground/60" style={{ width: `${(a.cost_usd / max) * 100}%` }} />
              </span>
              <span className="w-16 text-right text-sm tabular-nums">{usd(a.cost_usd) || "$0"}</span>
            </div>
          ))}
        </Panel>
        {!!routines.data?.routines.some((r) => r.unpriced) && (
          <p className="mt-2 text-xs text-muted-foreground">Some calls used a model with no price set: their cost is not counted.</p>
        )}
      </Section>

      <Budgets realm={realm} agents={agents} />

      <Section title="Tax and filings" note={cal.data ? "Not legal advice: check with your accountant" : undefined}>
        {!cal.data ? <Loading /> : (
          <Panel className="divide-y">
            {cal.data.deadlines.items.map((d, i) => (
              <div key={i} className="flex items-center gap-3 px-3.5 py-3">
                <div className={cn("w-14 shrink-0 text-center", d.days_left <= 30 && !d.filed && "text-amber-500")}>
                  <div className="text-base font-semibold tabular-nums">{d.days_left}</div>
                  <div className="text-[11px] text-muted-foreground">days</div>
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium">{d.title}</div>
                  <div className="text-xs text-muted-foreground">{d.authority} · due {d.due} · period to {d.period_end}</div>
                </div>
                {d.filed ? <span className="text-xs text-emerald-500">Filed</span> : (
                  <a href={d.source} target="_blank" rel="noreferrer" className="text-muted-foreground hover:text-foreground" aria-label="The rule"><ExternalLink className="size-4" /></a>
                )}
              </div>
            ))}
          </Panel>
        )}
      </Section>

      <Section title="Accounts">
        <Panel className="grid grid-cols-2 gap-x-6 gap-y-3 px-4 py-3.5 text-sm sm:grid-cols-3">
          <Fact label="Bank" value={p?.bank} />
          <Fact label="Currency" value={p?.currency} />
          <Fact label="Fiscal year ends" value={p?.fiscal_year_end ? monthDay(p.fiscal_year_end) : null} />
          <Fact label="VAT number" value={p?.vat_number} />
          <Fact label="Tax regime" value={p?.tax_regime} />
          <Fact label="Incorporated" value={p?.incorporated_on} />
        </Panel>
      </Section>
    </Page>
  )
}

type Spend = { currency: string | null; days: { date: string; agents: number; bank: number | null }[] }
type Bucket = { label: string; title: string; from: string; agents: number; bank: number | null }

/** A month's cap and spend (decision 139): the business's, or an agent's. */
type Budget = { cap: number | null; spent: number }
type Budgets = Budget & { month: string; agents: (Budget & { agent: string })[] }

/** The month's caps on model cost (decision 139): the business's, over
    everything, and each agent's, over its tasks. Past one, no task starts
    and scheduled routines wait until the next month. */
function Budgets({ realm, agents }: { realm: string; agents: Agent[] }) {
  const { data, refresh } = usePoll<Budgets>(() => api(`realms/${enc(realm)}/budgets`), 60000, [realm])
  const month = data && new Date(`${data.month}-01T00:00:00Z`).toLocaleDateString(undefined, { month: "long", timeZone: "UTC" })
  return (
    <Section title="Monthly budget" note={month && `${month} · list prices`}>
      {!data ? <Loading /> : (
        <>
          <Panel className="divide-y">
            <BudgetRow key={`business:${data.cap ?? ""}`} realm={realm} label="Whole business" budget={data} onSaved={refresh} />
            {data.agents.map((b) => {
              const a = agents.find((x) => x.name === b.agent)
              return <BudgetRow key={`${b.agent}:${b.cap ?? ""}`} realm={realm} agent={b.agent} who={a} label={a?.title ?? b.agent} budget={b} onSaved={refresh} />
            })}
          </Panel>
          <p className="mt-2 text-xs text-muted-foreground">
            Past a budget, no task starts and scheduled routines wait until the 1st; you are told once in Needs you. Leave empty for no cap.
          </p>
        </>
      )}
    </Section>
  )
}

function BudgetRow({ realm, agent, who, label, budget, onSaved }: {
  realm: string; agent?: string; who?: Agent; label: string; budget: Budget; onSaved: () => void
}) {
  const [value, setValue] = useState(budget.cap == null ? "" : String(budget.cap))
  const [busy, setBusy] = useState(false)
  const share = budget.cap ? budget.spent / budget.cap : 0
  const changed = value.trim() !== (budget.cap == null ? "" : String(budget.cap))
  async function save() {
    if (!changed) return
    const cap = value.trim() === "" ? null : Number(value)
    if (cap !== null && !(Number.isFinite(cap) && cap > 0 && cap <= 100000)) { toast.error("A budget is a number of dollars above 0 and at most 100000."); return }
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/budgets`, { agent: agent ?? null, cap }, "PUT")
      toast.success(cap === null ? `${label}: no monthly cap.` : `${label}: ${usd(cap)} a month.`)
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  return (
    <div className="flex items-center gap-3 px-3.5 py-2.5">
      {who ? <Character agent={who} size={24} still /> : <Landmark className="size-6 shrink-0 p-0.5 text-muted-foreground" />}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span className={cn("truncate text-sm", !agent && "font-medium")}>{label}</span>
          <span className={cn("shrink-0 text-xs tabular-nums text-muted-foreground", share >= 1 && "text-red-500", share >= 0.8 && share < 1 && "text-amber-500")}>
            {usd(budget.spent) || "$0"}{budget.cap ? ` of ${usd(budget.cap)}` : ""}
          </span>
        </div>
        {!!budget.cap && (
          <span className="mt-1.5 block h-1.5 overflow-hidden rounded-full bg-muted">
            <span className={cn("block h-full rounded-full bg-foreground/60", share >= 0.8 && "bg-amber-500", share >= 1 && "bg-red-500")} style={{ width: `${Math.min(share, 1) * 100}%` }} />
          </span>
        )}
      </div>
      <div className="relative w-24 shrink-0">
        <span className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-sm text-muted-foreground">$</span>
        <Input value={value} onChange={(e) => setValue(e.target.value)} onBlur={save} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur() }}
          inputMode="decimal" placeholder="No cap" disabled={busy} aria-label={`${label}'s monthly budget in dollars`} className="h-8 pl-6 text-right tabular-nums" />
      </div>
    </div>
  )
}

/** What the business spent, day by day or week by week: out of the bank
    when it is connected (in its currency), and what the agents' model calls
    cost (in dollars). Two currencies are never added up. */
function Spending({ realm }: { realm: string }) {
  const [days, setDays] = useState(30)
  const [by, setBy] = useState<"day" | "week">("day")
  const [what, setWhat] = useState<"bank" | "agents">("bank")
  const { data } = usePoll<Spend>(() => api(`realms/${enc(realm)}/spend?days=${days}`), 300000, [realm, days])
  const bank = !!data?.currency
  const shown = bank ? what : "agents"
  const buckets = data ? group(data.days, by) : []
  const value = (b: Bucket) => (shown === "bank" ? b.bank ?? 0 : b.agents)
  const total = buckets.reduce((s, b) => s + value(b), 0)
  const money = (n: number) => (shown === "bank" ? amount(n, data?.currency ?? "") : usd(n) || "$0")
  return (
    <Section title="Spending" note={data ? `${money(total)} in ${days} days · ${money(total / Math.max(buckets.length, 1))} a ${by} on average` : undefined}>
      <div className="mb-2.5 flex flex-wrap items-center gap-2">
        {bank && <Seg label="What" value={what} options={[["bank", `Out of the bank (${data!.currency})`], ["agents", "Agents ($)"]]} onPick={setWhat} />}
        <Seg label="By" value={by} options={[["day", "Daily"], ["week", "Weekly"]]} onPick={setBy} />
        <Seg label="Period" value={days} options={[[30, "30 days"], [90, "90 days"]]} onPick={setDays} />
      </div>
      {!data ? <Loading /> : (
        <Panel className="px-4 pt-4 pb-2">
          <Bars buckets={buckets} value={value} money={money} color={shown === "bank" ? "bg-rose-500/80" : "bg-sky-500"} />
        </Panel>
      )}
      {data && !bank && <p className="mt-2 text-xs text-muted-foreground">What the agents cost. Import a bank statement to see what goes out of it each day.</p>}
    </Section>
  )
}

/** Days, or Monday-to-Sunday weeks, oldest first. */
function group(days: Spend["days"], by: "day" | "week"): Bucket[] {
  const label = (d: string) => new Date(`${d}T12:00:00`).toLocaleDateString("en-GB", { day: "numeric", month: "short" })
  if (by === "day") return days.map((d) => ({ label: label(d.date), title: label(d.date), from: d.date, agents: d.agents, bank: d.bank }))
  const weeks: Bucket[] = []
  for (const d of days) {
    const date = new Date(`${d.date}T12:00:00`)
    const monday = new Date(date)
    monday.setDate(date.getDate() - ((date.getDay() + 6) % 7))
    const key = monday.toISOString().slice(0, 10)
    const last = weeks[weeks.length - 1]
    if (last?.from === key) {
      last.agents += d.agents
      if (d.bank !== null) last.bank = (last.bank ?? 0) + d.bank
    } else weeks.push({ label: label(key), title: `Week of ${label(key)}`, from: key, agents: d.agents, bank: d.bank })
  }
  return weeks
}

function Bars({ buckets, value, money, color }: { buckets: Bucket[]; value: (b: Bucket) => number; money: (n: number) => string; color: string }) {
  const max = Math.max(Number.MIN_VALUE, ...buckets.map(value))
  const every = Math.ceil(buckets.length / 7)
  // The last one is named too, unless the one before it is too close.
  const named = (i: number) => i % every === 0 || (i === buckets.length - 1 && i % every >= every / 2)
  return (
    <div className="flex h-44 items-end gap-[3px]">
      {buckets.map((b, i) => (
        <div key={b.from} className="group flex h-full min-w-0 flex-1 flex-col justify-end" title={`${b.title}\n${money(value(b))}`}>
          <div className={cn("w-full rounded-t-sm group-hover:opacity-80", color)} style={{ height: `${(100 * value(b)) / max}%`, minHeight: value(b) ? 2 : 0 }} />
          <div className="mt-1 h-4 text-center text-[10px] whitespace-nowrap text-muted-foreground">{named(i) ? b.label : ""}</div>
        </div>
      ))}
    </div>
  )
}

/** An amount in the bank's currency, as the HQ shows it. */
function amount(n: number, currency: string) {
  return new Intl.NumberFormat("en-GB", { style: "currency", currency: currency || "USD", maximumFractionDigits: 0 }).format(n)
}

function approvedCost(b: Business | null) {
  const approved = b?.vitals.filter((v) => v.note === "Approved this week").reduce((s, v) => s + (parseInt(v.value) || 0), 0) ?? 0
  return approved && b ? usd(b.cost_usd / approved) : "–"
}

const monthDay = (md: string) => {
  const [m, d] = md.split("-").map(Number)
  return new Date(2000, m - 1, d).toLocaleDateString("en-GB", { day: "numeric", month: "long" })
}

export function Fact({ label, value }: { label: string; value?: string | null }) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cn("mt-0.5", !value && "text-muted-foreground/60")}>{value ?? "Not set"}</div>
    </div>
  )
}
