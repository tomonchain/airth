import { useRef, useState } from "react"
import { ExternalLink, FileUp, Landmark } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Character } from "@/components/character"
import { Empty, Figure, Loading, Page, Panel, Section, Seg } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, raw, type Agent, type Business, type Calendar, type Company, type Routine } from "@/lib/api"
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

type Imported = { added: number; known: number; not_completed: number; from: string; to: string }

/** A Revolut statement, as exported in CSV, read into the business's
    figures. */
function ImportStatement({ realm, onDone, small }: { realm: string; onDone: () => void; small?: boolean }) {
  const input = useRef<HTMLInputElement>(null)
  const [busy, setBusy] = useState(false)
  async function send(file: File) {
    setBusy(true)
    try {
      const r: Imported = await (await raw(`realms/${enc(realm)}/finance/statement`, { method: "POST", body: file })).json()
      const added = r.added === 1 ? "1 new transaction" : `${r.added} new transactions`
      toast.success(`Statement read: ${added}`, {
        description: `${r.from} to ${r.to}${r.known ? ` · ${r.known} already in` : ""}${r.not_completed ? ` · ${r.not_completed} not completed, left out` : ""}`,
      })
      onDone()
    } catch (e) {
      toast.error("Statement not read", { description: (e as Error).message })
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <input ref={input} type="file" accept=".csv,text/csv" hidden
        onChange={(e) => { const f = e.target.files?.[0]; if (f) send(f); e.target.value = "" }} />
      {small ? (
        <button className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground disabled:opacity-50" disabled={busy} onClick={() => input.current?.click()}>
          <FileUp className="size-3.5" /> Import statement
        </button>
      ) : (
        <Button variant="outline" size="sm" disabled={busy} onClick={() => input.current?.click()}>
          <FileUp /> {busy ? "Reading…" : "Import a Revolut statement (CSV)"}
        </Button>
      )}
    </>
  )
}

type Spend = { currency: string | null; days: { date: string; agents: number; bank: number | null }[] }
type Bucket = { label: string; title: string; from: string; agents: number; bank: number | null }

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
