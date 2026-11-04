import { ListChecks } from "lucide-react"
import { Character } from "@/components/character"
import { Empty, Loading, Page, Section } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Tasks as Data } from "@/lib/api"
import { stamp } from "@/lib/time"

const GROUPS: { key: keyof Data; label: string }[] = [
  { key: "running", label: "Running" },
  { key: "waiting", label: "Waiting for you" },
  { key: "review", label: "To review" },
  { key: "done", label: "Done" },
]

/** The one-off orders given to the agents, by where they are. */
export function Tasks({ realm, agents, onAgent }: { realm: string; agents: Agent[]; onAgent: (name: string) => void }) {
  const { data } = usePoll<Data>(() => api(`realms/${enc(realm)}/tasks`), 8000, [realm])
  const total = data ? GROUPS.reduce((s, g) => s + (data[g.key]?.length ?? 0), 0) : 0
  return (
    <Page title="Tasks" lede="One-off orders to the agents. Write to an agent to give one.">
      {!data ? <Loading /> : !total ? (
        <Empty icon={ListChecks} title="No task yet">Open an agent and write to it: the order shows here until it is done.</Empty>
      ) : GROUPS.map((g) => {
        const list = data[g.key] ?? []
        if (!list.length) return null
        return (
          <Section key={g.key} title={g.label} note={String(list.length)}>
            <div className="space-y-1.5">
              {list.map((t) => {
                const a = agents.find((x) => x.name === t.agent)
                return (
                  <button key={t.id} onClick={() => a && onAgent(a.name)} className="flex w-full items-center gap-3 rounded-xl border bg-bubble/40 px-3.5 py-2.5 text-left text-sm hover:bg-accent/50">
                    {a && <Character agent={a} size={24} still />}
                    <span className="min-w-0 flex-1 truncate">{t.order ?? t.id}</span>
                    {t.line && <span className="max-w-60 truncate text-xs text-muted-foreground">{t.line}</span>}
                    <span className="text-xs text-muted-foreground">{stamp(t.updated_at ?? t.at)}</span>
                  </button>
                )
              })}
            </div>
          </Section>
        )
      })}
    </Page>
  )
}
