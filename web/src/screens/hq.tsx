import { ChevronRight, CircleDollarSign, Clock3, Hand } from "lucide-react"
import { Loading, Page, Panel } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, type Business } from "@/lib/api"
import { usd } from "@/lib/time"
import { cn } from "@/lib/utils"

type Hq = { headline: string; needs: number; businesses: (Business & { first: { title: string; agent: string | null } | null })[] }

const HEALTH: Record<string, string> = {
  healthy: "bg-emerald-500", watch: "bg-amber-500", "at risk": "bg-red-500", "not started": "bg-zinc-400",
}

/** HQ (decision 53): every business, what needs the founder in each, their
    time and cost this week. It aggregates; it never reads one business's
    content into another. */
export function Hq({ onOpen }: { onOpen: (realm: string) => void }) {
  const { data } = usePoll<Hq>(() => api("hq"), 15000, [])
  const time = data?.businesses.map((b) => b.time).filter(Boolean)
  const cost = data?.businesses.reduce((a, b) => a + (b.cost_usd || 0), 0) ?? 0
  return (
    <Page wide eyebrow="HQ" title="All businesses" lede={data?.headline}>
      {!data ? <Loading /> : (
        <>
          <div className="grid grid-cols-3 gap-2.5">
            <Panel className="px-3.5 py-3"><div className="flex items-center gap-1.5 text-xs text-muted-foreground"><Hand className="size-3.5" /> Need you</div><div className={cn("mt-1 text-xl font-semibold", data.needs > 0 && "text-amber-500")}>{data.needs}</div></Panel>
            <Panel className="px-3.5 py-3"><div className="flex items-center gap-1.5 text-xs text-muted-foreground"><Clock3 className="size-3.5" /> Your time this week</div><div className="mt-1 truncate text-xl font-semibold">{time?.length ? time.join(" + ") : "–"}</div></Panel>
            <Panel className="px-3.5 py-3"><div className="flex items-center gap-1.5 text-xs text-muted-foreground"><CircleDollarSign className="size-3.5" /> Spent this week</div><div className="mt-1 text-xl font-semibold">{usd(cost) || "$0"}</div></Panel>
          </div>
          <div className="space-y-2.5">
            {data.businesses.map((b) => (
              <button key={b.name} onClick={() => onOpen(b.name)} className="block w-full rounded-xl border bg-bubble/40 p-4 text-left transition-colors hover:bg-accent/50">
                <div className="flex items-center gap-3">
                  <span className={cn("size-2.5 shrink-0 rounded-full", HEALTH[b.health.state] ?? "bg-zinc-400")} title={b.health.state} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2"><span className="text-base font-semibold capitalize">{b.name}</span><span className="truncate text-xs text-muted-foreground">{b.description}</span></div>
                    <div className="text-sm text-foreground/80">{b.headline}</div>
                  </div>
                  <div className="text-right text-xs text-muted-foreground">
                    {b.needs > 0 ? <div className="font-medium text-amber-500">{b.needs} need you</div> : <div>Nothing for you</div>}
                    <div>{[b.time, usd(b.cost_usd) || "$0"].filter(Boolean).join(" · ")} this week</div>
                  </div>
                  <ChevronRight className="size-4 text-muted-foreground" />
                </div>
                <p className="mt-2 pl-5.5 text-xs text-muted-foreground"><span className="capitalize">{b.health.state}</span>. {b.health.sentence}</p>
                {b.proof && (
                  <p className="mt-1 flex items-center gap-2 pl-5.5 text-xs" title={b.proof.sentence}>
                    <span className="flex gap-0.5" aria-hidden>{Array.from({ length: b.proof.of }, (_, i) => <span key={i} className={cn("h-1.5 w-4 rounded-full", i < b.proof!.in_a_row ? "bg-emerald-500" : "bg-muted")} />)}</span>
                    <span className="text-muted-foreground">On its own: {b.proof.sentence}</span>
                  </p>
                )}
                {b.first && <p className="mt-1 truncate pl-5.5 text-xs">First for you: <span className="text-foreground/80">{b.first.title}</span></p>}
                {b.vitals.length > 0 && (
                  <div className="mt-3 grid grid-cols-2 gap-2 pl-5.5 sm:grid-cols-4">
                    {b.vitals.map((v) => (
                      <div key={v.label} className="rounded-lg border bg-surface px-2.5 py-1.5" title={v.note}>
                        <div className="truncate text-[11px] text-muted-foreground">{v.label}</div>
                        <div className={cn("text-sm font-medium", v.out && "text-amber-500")}>{v.value}</div>
                      </div>
                    ))}
                  </div>
                )}
              </button>
            ))}
          </div>
        </>
      )}
    </Page>
  )
}
