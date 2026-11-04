import { Inbox as InboxIcon } from "lucide-react"
import { Character } from "@/components/character"
import { NeedCard } from "@/components/need-card"
import type { Agent, Item } from "@/lib/api"

// Most pressing first: what broke, what the check holds, what is due.
export function rank(it: Item) {
  if (it.kind === "incident") return 0
  if (it.flag === "blocking" || (it.kind === "deadline" && it.overdue)) return 1
  return { deadline: 2, approval: 3, prepared: 4, delayed: 5, report: 6, incident: 0 }[it.kind] ?? 6
}

/** Everything the team sent up, in one place, most pressing first. */
export function Inbox({ realm, items, agents, onDone, onAgent }: {
  realm: string; items: Item[]; agents: Agent[]; onDone: () => void; onAgent: (name: string) => void
}) {
  const sorted = [...items].sort((a, b) => rank(a) - rank(b) || a.created_at.localeCompare(b.created_at))
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl px-6 py-8">
        <h1 className="text-2xl font-semibold tracking-tight">Needs you</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          {sorted.length ? `${sorted.length} things the team sent up, most pressing first.` : "Nothing waits for you."}
        </p>
        {!sorted.length && (
          <div className="mt-24 flex flex-col items-center text-muted-foreground"><InboxIcon className="size-10 opacity-40" /></div>
        )}
        <div className="mt-6 space-y-6">
          {sorted.map((it) => {
            const a = agents.find((x) => x.name === it.agent)
            return (
              <div key={`${it.id}-${it.revision}`} className="flex gap-3">
                {a ? (
                  <button onClick={() => onAgent(a.name)} aria-label={`Open ${a.title}`} className="self-start pt-7"><Character agent={a} size={36} still /></button>
                ) : <span className="w-9 shrink-0" />}
                <div className="min-w-0 flex-1">
                  {a && <button onClick={() => onAgent(a.name)} className="mb-1.5 text-sm font-medium hover:underline">{a.title}</button>}
                  <NeedCard realm={realm} it={it} onDone={onDone} />
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
