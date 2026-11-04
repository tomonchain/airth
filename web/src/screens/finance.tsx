import { ExternalLink, Landmark } from "lucide-react"
import { Character } from "@/components/character"
import { Empty, Figure, Loading, Page, Panel, Section } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Business, type Calendar, type Company, type Routine } from "@/lib/api"
import { usd } from "@/lib/time"
import { cn } from "@/lib/utils"

/** The business's money: cash and runway from the bank (read-only, decision
    54) or entered by hand (decision 68), what the agents cost, and the tax
    and filing dates its profile implies. Never a total across businesses. */
export function Finance({ realm, business, agents }: { realm: string; business: Business | null; agents: Agent[] }) {
  const cal = usePoll<Calendar>(() => api(`realms/${enc(realm)}/calendar`), 60000, [realm])
  const routines = usePoll<{ routines: Routine[] }>(() => api(`realms/${enc(realm)}/graphs`), 60000, [realm])
  const company = usePoll<Company>(() => api(`realms/${enc(realm)}/company`), 60000, [realm])
  const f = business?.finance
  const p = company.data?.profile
  const months = f?.runway_months
  const spend = agents.reduce((s, a) => s + a.cost_usd, 0)
  const max = Math.max(...agents.map((a) => a.cost_usd), 0.0001)

  return (
    <Page eyebrow={p?.legal_name ?? realm} title="Finance" lede={f?.runway ? `Runway ${f.runway}.` : "What the business has, spends and owes."}>
      <Section title="Cash" note={f?.source}>
        {!business ? <Loading /> : f && !f.unknown ? (
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            <Figure label="Cash" value={f.cash} />
            <Figure label="In this month" value={f.in} />
            <Figure label="Out this month" value={f.out} />
            <Figure label="Runway" value={f.runway} tone={months == null ? undefined : months < 6 ? "bad" : months < 12 ? "warn" : undefined} />
          </div>
        ) : (
          <Empty icon={Landmark} title={f?.unknown ?? "No bank figures yet"}>
            Connect {p?.bank ?? "the business's bank"} read-only, or write cash, spent and as_of in business/finance.md. Airth never moves money.
          </Empty>
        )}
      </Section>

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
