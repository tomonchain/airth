import { useState } from "react"
import { Search } from "lucide-react"
import { Character } from "@/components/character"
import type { Agent, Item } from "@/lib/api"
import { stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

export type Last = { text: string; at: string | null }

/** The agents as conversations: the one with something for the founder
    first, then the latest to have worked. */
export function AgentList({ agents, items, last, selected, onOpen }: {
  agents: Agent[]; items: Item[]; last: Record<string, Last>; selected: string | null; onOpen: (name: string) => void
}) {
  const [q, setQ] = useState("")
  const waiting = (a: Agent) => items.filter((i) => i.agent === a.name).length
  const shown = agents
    .filter((a) => !q || `${a.title} ${a.department} ${a.job}`.toLowerCase().includes(q.toLowerCase()))
    .sort((a, b) => Number(waiting(b) > 0) - Number(waiting(a) > 0) || (last[b.name]?.at ?? "").localeCompare(last[a.name]?.at ?? ""))
  return (
    <div className="flex w-[340px] shrink-0 flex-col border-r">
      <div className="p-3">
        <label className="flex h-9 items-center gap-2 rounded-lg bg-muted px-3 text-sm text-muted-foreground focus-within:ring-2 focus-within:ring-ring/40">
          <Search className="size-4" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search" className="flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
        </label>
      </div>
      <ul className="flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
        {shown.map((a) => {
          const n = waiting(a)
          const l = last[a.name]
          return (
            <li key={a.name}>
              <button
                onClick={() => onOpen(a.name)}
                className={cn("flex w-full items-center gap-3 rounded-xl px-2.5 py-2.5 text-left transition-colors hover:bg-accent/70", selected === a.name && "bg-accent")}
              >
                <Character agent={a} size={40} still={selected !== a.name && a.state !== "working"} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[13.5px] font-medium">{a.title}</span>
                    {a.department && <span className="shrink-0 rounded-md border px-1.5 text-[11px] text-muted-foreground capitalize">{a.department}</span>}
                    <span className="ml-auto shrink-0 text-xs text-muted-foreground">{stamp(l?.at)}</span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-2">
                    <span className={cn("truncate text-sm text-muted-foreground", a.state === "working" && "text-emerald-500")}>
                      {a.state === "working" ? "Working…" : l?.text ?? "No messages yet"}
                    </span>
                    {n > 0 && <span className="ml-auto grid h-5 min-w-5 shrink-0 place-items-center rounded-full bg-amber-400 px-1.5 text-[11px] font-semibold text-black">{n}</span>}
                  </div>
                </div>
              </button>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
