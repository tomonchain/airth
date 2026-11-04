import { CalendarClock } from "lucide-react"
import { Empty, Loading, Page, Section } from "@/components/page"
import { Character } from "@/components/character"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Calendar as Data } from "@/lib/api"
import { cn } from "@/lib/utils"

const STATE: Record<string, string> = {
  ran: "text-emerald-500", failed: "text-red-500", "not run": "text-muted-foreground line-through", next: "text-sky-500", planned: "text-muted-foreground",
}

/** The week ahead: when each routine runs, and the legal deadlines. */
export function Calendar({ realm, agents, onAgent }: { realm: string; agents: Agent[]; onAgent: (name: string) => void }) {
  const { data } = usePoll<Data>(() => api(`realms/${enc(realm)}/calendar`), 30000, [realm])
  return (
    <Page title="Calendar" lede={data?.title} wide>
      {!data ? <Loading /> : (
        <>
          <div className="grid grid-cols-7 gap-2">
            {data.days.map((d) => (
              <div key={d.day + d.weekday} className={cn("min-h-56 rounded-xl border bg-bubble/40 p-2", d.today && "border-sky-500/50")}>
                <div className="mb-2 flex items-baseline justify-between px-1">
                  <span className="text-xs text-muted-foreground">{d.weekday}</span>
                  <span className={cn("text-base font-semibold tabular-nums", d.today && "text-sky-500")}>{d.day}</span>
                </div>
                <div className="space-y-1">
                  {d.deadlines.map((x, i) => (
                    <div key={i} className="rounded-md bg-amber-500/15 px-1.5 py-1 text-[11px] leading-tight text-amber-700 dark:text-amber-300">{x.title}</div>
                  ))}
                  {d.runs.map((r, i) => {
                    const a = agents.find((x) => x.name === r.agent)
                    return (
                      <button key={i} onClick={() => onAgent(r.agent)} className="flex w-full items-center gap-1.5 rounded-md px-1 py-1 text-left hover:bg-accent/60">
                        {a && <Character agent={a} size={16} still />}
                        <span className="min-w-0 flex-1">
                          <span className="block truncate font-mono text-[11px]">{r.routine}</span>
                          <span className={cn("block text-[10.5px]", STATE[r.state])}>{r.time} · {r.state}</span>
                        </span>
                      </button>
                    )
                  })}
                </div>
              </div>
            ))}
          </div>
          <Section title="Legal deadlines" note={data.deadlines.note}>
            {data.deadlines.items.length ? (
              <div className="space-y-1.5">
                {data.deadlines.items.map((d, i) => (
                  <div key={i} className="flex items-center gap-3 rounded-xl border bg-bubble/40 px-3.5 py-2.5 text-sm">
                    <span className="w-28 shrink-0 text-muted-foreground">{d.due}</span>
                    <span className="flex-1 font-medium">{d.title}</span>
                    <span className="text-xs text-muted-foreground">{d.authority} · in {d.days_left} days</span>
                  </div>
                ))}
              </div>
            ) : <Empty icon={CalendarClock} title="No deadline in the next twelve months" />}
          </Section>
        </>
      )}
    </Page>
  )
}
