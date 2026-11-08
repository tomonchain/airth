// What each model provider costs, and how long its money lasts (decision
// 107): the ring in the rail, and the panel it opens.
import { useState } from "react"
import { Popover } from "@base-ui/react/popover"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { usePoll } from "@/hooks/use-poll"
import { api } from "@/lib/api"
import { cn } from "@/lib/utils"

type Level = "ok" | "warn" | "low" | "unknown"
export type Provider = {
  provider: string; name: string; month_usd: number; daily_usd: number
  budget: { monthly_usd: number; reset_day: number } | null
  since_reset_usd: number | null; resets_on: string | null
  balance: { amount: number; currency: string } | null
  left_usd: number | null; days_left: number | null; used: number | null; level: Level
}
export type Spend = { providers: Provider[]; month_usd: number }

const RANK: Record<Level, number> = { low: 3, warn: 2, ok: 1, unknown: 0 }
const TONE: Record<Level, string> = { low: "text-red-500", warn: "text-amber-500", ok: "text-emerald-500", unknown: "text-muted-foreground" }
const BAR: Record<Level, string> = { low: "bg-red-500", warn: "bg-amber-500", ok: "bg-emerald-500", unknown: "bg-foreground/25" }

const money = (n: number) => `$${n < 10 ? n.toFixed(2) : n.toFixed(0)}`
const days = (d: number | null) => d === null ? null : d < 1 ? "less than a day" : `~${Math.floor(d)} day${Math.floor(d) === 1 ? "" : "s"}`
const date = (iso: string) => new Date(iso + "T12:00:00").toLocaleDateString(undefined, { day: "numeric", month: "short" })

/** The spending, read every five minutes. */
export function useSpend() {
  return usePoll(() => api<Spend>("spend"), 300_000, [])
}

/** The provider closest to running out: it sets the ring. */
function worst(data: Spend | null): Provider | null {
  if (!data?.providers.length) return null
  return [...data.providers].sort((a, b) => RANK[b.level] - RANK[a.level] || (a.days_left ?? Infinity) - (b.days_left ?? Infinity))[0]
}

/** How full the ring is: the budget spent, or how near a balance is to
    running out (a month or more left draws nothing). */
function fill(p: Provider | null): number {
  if (!p) return 0
  if (p.used !== null) return p.used
  if (p.days_left !== null) return Math.min(1, Math.max(0, 1 - p.days_left / 30))
  return p.level === "low" ? 1 : 0
}

export function SpendRing({ data, live, size = 32 }: { data: Spend | null; live: boolean; size?: number }) {
  const p = worst(data)
  const r = 13, c = 2 * Math.PI * r
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden className={TONE[p?.level ?? "unknown"]}>
      <circle cx="16" cy="16" r={r} fill="none" stroke="currentColor" strokeOpacity={0.2} strokeWidth="3" />
      <circle cx="16" cy="16" r={r} fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"
        strokeDasharray={`${c * fill(p)} ${c}`} transform="rotate(-90 16 16)" className="transition-[stroke-dasharray] duration-500" />
      <circle cx="16" cy="16" r="4" className={cn(live ? "animate-pulse fill-emerald-400" : "fill-muted-foreground/60")} />
    </svg>
  )
}

function Line({ p }: { p: Provider }) {
  if (p.balance) {
    const lasts = p.balance.currency === "USD" ? days(p.days_left) : null
    return <>{p.balance.currency === "USD" ? money(p.balance.amount) : `${p.balance.amount.toFixed(2)} ${p.balance.currency}`} left{lasts && <> · {lasts}</>}</>
  }
  if (p.budget && p.since_reset_usd !== null) {
    return <>{money(p.since_reset_usd)} of {money(p.budget.monthly_usd)}{p.resets_on && <> · resets {date(p.resets_on)}</>}{days(p.days_left) && <> · {days(p.days_left)}</>}</>
  }
  return <>Paid by use: no balance from this provider, and no budget yet</>
}

function BudgetForm({ p, onDone }: { p: Provider; onDone: () => void }) {
  const [amount, setAmount] = useState(p.budget ? String(p.budget.monthly_usd) : "")
  const [day, setDay] = useState(String(p.budget?.reset_day ?? 1))
  const [error, setError] = useState<string | null>(null)
  async function save(remove = false) {
    try {
      await (remove
        ? api(`spend/budgets/${p.provider}`, undefined, "DELETE")
        : api(`spend/budgets/${p.provider}`, { monthly_usd: Number(amount), reset_day: Number(day) }, "PUT"))
      onDone()
    } catch (e) { setError((e as Error).message) }
  }
  return (
    <form className="mt-2 space-y-2" onSubmit={(e) => { e.preventDefault(); save() }}>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <label className="flex items-center gap-1">$<Input id={`budget-${p.provider}`} inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} className="h-7 w-20" placeholder="50" /> a month</label>
        <label className="flex items-center gap-1">from day <Input id={`reset-${p.provider}`} inputMode="numeric" value={day} onChange={(e) => setDay(e.target.value)} className="h-7 w-12" /></label>
      </div>
      {error && <p className="text-xs text-red-500">{error}</p>}
      <div className="flex gap-1.5">
        <Button type="submit" size="sm" className="h-7">Save</Button>
        {p.budget && <Button type="button" size="sm" variant="ghost" className="h-7" onClick={() => save(true)}>Remove budget</Button>}
      </div>
    </form>
  )
}

/** Each provider: what is left and for how long, its budget, and the month. */
export function SpendPanel({ data, error, onChanged, onUsage }: { data: Spend | null; error: string | null; onChanged: () => void; onUsage?: () => void }) {
  const [editing, setEditing] = useState<string | null>(null)
  if (!data) return <p className="p-1 text-sm text-muted-foreground">{error ? `Airth is not answering: ${error}` : "Loading…"}</p>
  return (
    <div className="space-y-3">
      <div className="flex items-baseline justify-between">
        <h2 className="text-sm font-semibold">Models and spending</h2>
        <span className="text-xs text-muted-foreground tabular-nums">{money(data.month_usd)} this month</span>
      </div>
      {!data.providers.length && <p className="text-sm text-muted-foreground">No model call this month yet.</p>}
      <ul className="space-y-2">
        {data.providers.map((p) => (
          <li key={p.provider} className="rounded-xl bg-bubble px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span className={cn("size-2 shrink-0 rounded-full", BAR[p.level])} />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{p.name}</span>
              <span className="text-xs text-muted-foreground tabular-nums">{money(p.month_usd)} this month</span>
            </div>
            <p className={cn("mt-1 text-xs", p.level === "unknown" ? "text-muted-foreground" : "text-foreground/80")}><Line p={p} /></p>
            {p.used !== null && (
              <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-foreground/10">
                <div className={cn("h-full rounded-full", BAR[p.level])} style={{ width: `${Math.round(p.used * 100)}%` }} />
              </div>
            )}
            {!p.balance && (editing === p.provider
              ? <BudgetForm p={p} onDone={() => { setEditing(null); onChanged() }} />
              : <button className="mt-1 text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline" onClick={() => setEditing(p.provider)}>{p.budget ? "Change budget" : "Set a monthly budget"}</button>)}
          </li>
        ))}
      </ul>
      <p className="text-[11px] leading-snug text-muted-foreground">
        Spending is estimated from list prices; a balance is the provider's own figure, read hourly. "Days" is what is left at the last seven days' rate.
        {onUsage && <> <button className="underline underline-offset-2 hover:text-foreground" onClick={onUsage}>See Usage</button></>}
      </p>
    </div>
  )
}

/** The rail's ring, which opens the panel. */
export function SpendIndicator({ live, onUsage }: { live: boolean; onUsage: () => void }) {
  const { data, error, refresh } = useSpend()
  const [open, setOpen] = useState(false)
  const p = worst(data)
  const label = p ? `${p.name}: ${p.level === "unknown" ? `${money(p.month_usd)} this month` : days(p.days_left) ?? "nothing left"}` : "Models and spending"
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger className="mb-1 grid size-10 place-items-center rounded-xl hover:bg-accent" aria-label={`${label}${live ? "; an agent is working" : ""}`} title={label}>
        <SpendRing data={data} live={live} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner side="right" align="end" sideOffset={10}>
          <Popover.Popup className="w-80 rounded-2xl border bg-popover p-4 text-popover-foreground shadow-lg outline-none data-[ending-style]:opacity-0 data-[starting-style]:opacity-0 transition-opacity">
            <SpendPanel data={data} error={error} onChanged={refresh} onUsage={() => { setOpen(false); onUsage() }} />
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  )
}
