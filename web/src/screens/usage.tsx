import { useState } from "react"
import { Figure, Loading, Page, Panel, Section } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { cn } from "@/lib/utils"

type Cost = { input: number; cache_read: number; cache_write: number; output: number }
type Tokens = { input: number; output: number; cache_read: number; cache_write: number; calls: number; peak_input: number; cost: Cost; unpriced: number }
type Share = Tokens & { name: string }
type Report = {
  days: number; window: Tokens; today: Tokens; month: Tokens
  by_day: (Tokens & { date: string })[]; by_realm: Share[]; by_model: Share[]; by_purpose: Share[]; by_source: Share[]; by_routine: Share[]
  top_runs: (Tokens & { realm: string; id: string; objective: string; scheduled: boolean; compactions: number })[]
  pipelines?: { pipeline: string; calls: number; input_tokens: number; output_tokens: number; cost_usd: number; unpriced: number }[]
  agents?: Record<string, string>
}

/** 1234 → 1.2k: token counts at a glance. */
function tok(n: number) {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)}B`
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 1 : 2)}M`
  if (n >= 1e3) return `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}k`
  return String(n)
}
function usd(x: number) {
  if (!x) return "$0"
  if (x < 0.01) return "<$0.01"
  return `$${x >= 100 ? Math.round(x).toLocaleString() : x.toFixed(2)}`
}
const total = (t: Tokens) => t.input + t.output
const costOf = (t: Tokens) => t.cost.input + t.cost.cache_read + t.cost.cache_write + t.cost.output
const fresh = (t: Tokens) => Math.max(0, t.input - t.cache_read - t.cache_write)
const cached = (t: Tokens) => (t.input ? Math.round((100 * t.cache_read) / t.input) : 0)

// What was billed, in the order the chart stacks it, bottom first.
const KINDS: { key: keyof Cost; label: string; tokens: (t: Tokens) => number; color: string }[] = [
  { key: "input", label: "Fresh input", tokens: fresh, color: "bg-sky-500" },
  { key: "cache_write", label: "Cache writes", tokens: (t) => t.cache_write, color: "bg-violet-500" },
  { key: "cache_read", label: "Cache reads", tokens: (t) => t.cache_read, color: "bg-emerald-500" },
  { key: "output", label: "Output", tokens: (t) => t.output, color: "bg-amber-500" },
]
const PURPOSES: Record<string, [string, string]> = {
  work: ["Run steps", "The model working on the request: each step re-reads the whole conversation."],
  compaction: ["Compaction", "Summarizing a conversation that grew too long."],
  warm: ["Keeping the cache warm", "Refreshing the prompt cache while tools wait for your approval."],
  triage: ["Triage", "Deciding whether a request fits the fast model."],
  title: ["Change titles", "Naming a change before it is delivered."],
}
const SOURCES: Record<string, string> = { chats: "Asked by you", objectives: "Routines" }

/** Shares summed under another name: each routine's agent, say. */
function merge(items: Share[], keyOf: (s: Share) => string): Share[] {
  const out = new Map<string, Share>()
  for (const s of items) {
    const k = keyOf(s)
    const m = out.get(k) ?? { name: k, input: 0, output: 0, cache_read: 0, cache_write: 0, calls: 0, peak_input: 0, unpriced: 0, cost: { input: 0, cache_read: 0, cache_write: 0, output: 0 } }
    for (const f of ["input", "output", "cache_read", "cache_write", "calls", "unpriced"] as const) m[f] += s[f]
    for (const f of Object.keys(m.cost) as (keyof Cost)[]) m.cost[f] += s.cost[f]
    out.set(k, m)
  }
  return [...out.values()]
}

function Seg<T extends string | number>({ label, value, options, onPick }: { label: string; value: T; options: [T, string][]; onPick: (v: T) => void }) {
  return (
    <div role="radiogroup" aria-label={label} className="flex rounded-lg border p-0.5">
      {options.map(([v, l]) => (
        <button key={String(v)} role="radio" aria-checked={value === v} onClick={() => onPick(v)}
          className={cn("rounded-md px-2.5 py-1 text-xs transition-colors", value === v ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")}>{l}</button>
      ))}
    </div>
  )
}

/** What running the business costs (the classic Usage page): what its tasks
    and routines exchanged with the model, per day, by agent, routine, model
    and purpose, and the costliest tasks. Estimates from public list prices. */
export function Usage({ realm }: { realm: string }) {
  const [days, setDays] = useState(30)
  const [scope, setScope] = useState<"business" | "all">("business")
  const [show, setShow] = useState<"cost" | "tokens">("cost")
  const one = scope === "business"
  const { data: r } = usePoll<Report>(() => api(`usage?days=${days}${one ? `&realm=${enc(realm)}` : ""}`), 300000, [realm, days, scope])
  const priced = !!r && r.window.unpriced < r.window.calls
  const cost = priced && show === "cost"
  const weight = (t: Tokens) => (cost ? costOf(t) : total(t))
  const fmt = (n: number) => (cost ? usd(n) : tok(n))
  const amount = (t: Tokens) => (priced ? usd(costOf(t)) : tok(total(t)))
  const detail = (t: Tokens) => `${tok(total(t))} tokens · ${t.calls} call${t.calls === 1 ? "" : "s"}`
  const part = (t: Tokens, k: (typeof KINDS)[number]) => (cost ? t.cost[k.key] : k.tokens(t))

  return (
    <Page wide title="Usage" lede={`What running ${one ? realm : "your businesses"} costs: what its tasks and routines exchanged with the model. Estimates from public list prices; input read from the cache costs a tenth of fresh input.`}>
      <div className="-mt-3 flex flex-wrap items-center gap-2">
        {priced && <Seg label="Show" value={show} options={[["cost", "Cost"], ["tokens", "Tokens"]]} onPick={setShow} />}
        <Seg label="Period" value={days} options={[[7, "7 days"], [30, "30 days"], [90, "90 days"]]} onPick={setDays} />
        <Seg label="Scope" value={scope} options={[["business", realm.charAt(0).toUpperCase() + realm.slice(1)], ["all", "All businesses"]]} onPick={setScope} />
      </div>
      {!r ? <Loading /> : (
        <>
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            <Figure label={`Last ${r.days} days`} value={amount(r.window)} note={detail(r.window)} />
            <Figure label="Today" value={amount(r.today)} note={detail(r.today)} />
            <Figure label="This month" value={amount(r.month)} note={detail(r.month)} />
            <Figure label="From cache" value={`${cached(r.window)}%`} note={`${tok(r.window.cache_read)} of ${tok(r.window.input)} input tokens`} />
          </div>

          {!r.window.calls ? <Panel className="px-4 py-3 text-sm text-muted-foreground">No task called a model in this period.</Panel> : (
            <>
              <Section title="Per day" note={<span className="flex flex-wrap gap-3">{KINDS.map((k) => <span key={k.key} className="inline-flex items-center gap-1.5"><i className={cn("size-2 rounded-sm", k.color)} />{k.label}</span>)}</span>}>
                <Panel className="px-4 pt-4 pb-2">
                  <Chart r={r} weight={weight} part={part} priced={priced} />
                </Panel>
              </Section>

              <div className="grid gap-6 md:grid-cols-2">
                {one ? (
                  <>
                    <Shares title="By agent" items={merge(r.by_routine, (s) => (!s.name ? "Asked by you" : r.agents?.[s.name] ?? "No agent"))} weight={weight} fmt={fmt} all={weight(r.window)} />
                    <Shares title="By routine" items={r.by_routine.map((s) => ({ ...s, name: s.name || "Asked by you" }))} weight={weight} fmt={fmt} all={weight(r.window)} />
                  </>
                ) : (
                  <>
                    <Shares title="By business" items={r.by_realm} weight={weight} fmt={fmt} all={weight(r.window)} />
                    <Shares title="By model" items={r.by_model.map((s) => ({ ...s, name: s.name === "unrecorded" ? "Before tracking" : s.name }))} weight={weight} fmt={fmt} all={weight(r.window)} />
                  </>
                )}
                <Section title={cost ? "Where the cost goes" : "Where the tokens go"}>
                  <Panel className="space-y-3 p-4">
                    {KINDS.map((k) => {
                      const sum = KINDS.reduce((a, x) => a + part(r.window, x), 0) || 1
                      return <Bar key={k.key} name={k.label} value={fmt(part(r.window, k))} pct={(100 * part(r.window, k)) / sum} color={k.color} />
                    })}
                  </Panel>
                </Section>
                {one
                  ? <Shares title="By model" items={r.by_model.map((s) => ({ ...s, name: s.name === "unrecorded" ? "Before tracking" : s.name }))} weight={weight} fmt={fmt} all={weight(r.window)} />
                  : <Shares title="Why the model was called" items={r.by_purpose.map((s) => ({ ...s, name: PURPOSES[s.name]?.[0] ?? s.name }))} weight={weight} fmt={fmt} all={weight(r.window)} />}
                {one && <Shares title="Why the model was called" items={r.by_purpose.map((s) => ({ ...s, name: PURPOSES[s.name]?.[0] ?? s.name }))} weight={weight} fmt={fmt} all={weight(r.window)} />}
                {one && r.by_source.length > 1 && <Shares title="Asked by you, and routines" items={r.by_source.map((s) => ({ ...s, name: SOURCES[s.name] ?? s.name }))} weight={weight} fmt={fmt} all={weight(r.window)} />}
              </div>
            </>
          )}

          {!!r.pipelines?.length && (
            <Section title="Fixed sequences">
              <Panel className="divide-y">
                {r.pipelines.map((p) => (
                  <div key={p.pipeline} className="flex items-center gap-3 px-4 py-2.5">
                    <div className="min-w-0 flex-1">
                      <div className="font-mono text-[13px]">{p.pipeline}</div>
                      <div className="text-xs text-muted-foreground">{p.calls} model call{p.calls === 1 ? "" : "s"} · {tok(p.input_tokens + p.output_tokens)} tokens{p.unpriced ? ` · ${p.unpriced} without a known price` : ""}</div>
                    </div>
                    <span className="text-sm font-medium tabular-nums">{usd(p.cost_usd)}</span>
                  </div>
                ))}
              </Panel>
            </Section>
          )}

          {r.top_runs.length > 0 && (
            <Section title={priced ? "Costliest tasks" : "Hungriest tasks"}>
              <Panel className="divide-y">
                {r.top_runs.map((t) => (
                  <div key={`${t.realm}${t.id}`} className="flex items-center gap-3 px-4 py-2.5">
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm">{t.objective || "(no objective)"}</div>
                      <div className="text-xs text-muted-foreground">
                        {[!one && t.realm, t.scheduled && "routine", `${t.calls} calls`, `${tok(Math.round(t.input / Math.max(t.calls, 1)))} avg context`, `${tok(t.peak_input)} peak`, `${cached(t)}% cached`,
                          t.compactions ? `${t.compactions} compaction${t.compactions === 1 ? "" : "s"}` : null].filter(Boolean).join(" · ")}
                      </div>
                    </div>
                    <div className="text-right">
                      <div className="text-sm font-medium tabular-nums">{fmt(weight(t))}</div>
                      <div className="text-xs text-muted-foreground">{cost ? `${tok(total(t))} tokens` : priced ? usd(costOf(t)) : ""}</div>
                    </div>
                  </div>
                ))}
              </Panel>
            </Section>
          )}

          {r.window.unpriced > 0 && (
            <p className="text-xs text-muted-foreground">{r.window.unpriced} of {r.window.calls} calls used a model without a known price and are left out of costs. Add its prices, in USD per million tokens, to prices.json in Airth's home.</p>
          )}
        </>
      )}
    </Page>
  )
}

function Chart({ r, weight, part, priced }: { r: Report; weight: (t: Tokens) => number; part: (t: Tokens, k: (typeof KINDS)[number]) => number; priced: boolean }) {
  const max = Math.max(Number.MIN_VALUE, ...r.by_day.map(weight))
  const every = Math.ceil(r.by_day.length / 7)
  return (
    <div className="flex h-44 items-end gap-[3px]">
      {r.by_day.map((d, i) => {
        const date = new Date(`${d.date}T12:00:00`)
        const label = date.toLocaleDateString("en-GB", { day: "numeric", month: "short" })
        return (
          <div key={d.date} className="group flex h-full min-w-0 flex-1 flex-col justify-end"
            title={`${label}\n${priced ? `${usd(costOf(d))} · ` : ""}${tok(total(d))} tokens · ${d.calls} calls`}>
            <div className="flex w-full flex-col-reverse overflow-hidden rounded-t-sm group-hover:opacity-80" style={{ height: `${(100 * weight(d)) / max}%` }}>
              {KINDS.map((k) => <span key={k.key} className={k.color} style={{ height: `${weight(d) ? (100 * part(d, k)) / weight(d) : 0}%` }} />)}
            </div>
            <div className="mt-1 h-4 text-center text-[10px] whitespace-nowrap text-muted-foreground">{i % every === 0 || i === r.by_day.length - 1 ? label : ""}</div>
          </div>
        )
      })}
    </div>
  )
}

function Bar({ name, value, pct, color = "bg-sky-500" }: { name: string; value: string; pct: number; color?: string }) {
  return (
    <div>
      <div className="flex items-baseline justify-between gap-3 text-sm"><span className="truncate">{name}</span><span className="shrink-0 text-xs text-muted-foreground tabular-nums">{value} · {Math.round(pct)}%</span></div>
      <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-muted"><span className={cn("block h-full rounded-full", color)} style={{ width: `${Math.max(pct, 1)}%` }} /></div>
    </div>
  )
}

function Shares({ title, items, weight, fmt, all }: { title: string; items: Share[]; weight: (t: Tokens) => number; fmt: (n: number) => string; all: number }) {
  const sorted = [...items].sort((a, b) => weight(b) - weight(a))
  return (
    <Section title={title}>
      <Panel className="space-y-3 p-4">
        {sorted.length ? sorted.map((s) => <Bar key={s.name} name={s.name} value={fmt(weight(s))} pct={(100 * weight(s)) / (all || 1)} />)
          : <p className="text-sm text-muted-foreground">Nothing.</p>}
      </Panel>
    </Section>
  )
}
