import { useState } from "react"
import { Pencil } from "lucide-react"
import { Character } from "@/components/character"
import { EditAgent } from "@/components/edit-agent"
import { Button } from "@/components/ui/button"
import type { Agent } from "@/lib/api"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"

const STATE: Record<Agent["state"], string> = {
  needs: "Waiting for you", incident: "Stopped on an incident", working: "Working now", idle: "Between runs", asleep: "Nothing scheduled",
}

/** The agent's card: who it is, what it costs, its routines. */
export function Details({ realm, agent, onSaved }: { realm: string; agent: Agent; onSaved: () => void }) {
  const [editing, setEditing] = useState(false)
  return (
    <aside className="relative w-80 shrink-0 overflow-y-auto border-l px-5 py-6">
      <Button variant="outline" size="sm" className="absolute top-4 right-4" onClick={() => setEditing(true)}><Pencil /> Edit</Button>
      <EditAgent key={agent.name} realm={realm} agent={agent} open={editing} onOpenChange={setEditing} onSaved={onSaved} />
      <div className="flex flex-col items-center text-center">
        <Character agent={agent} size={84} />
        <h2 className="mt-4 text-lg font-semibold">{agent.title}</h2>
        <p className="text-sm text-muted-foreground capitalize">{agent.department}</p>
        <span className="mt-2 rounded-full border px-2.5 py-0.5 text-xs text-muted-foreground">{STATE[agent.state]}</span>
      </div>
      {agent.job && <p className="mt-5 text-sm leading-relaxed text-foreground/80">{agent.job}</p>}
      <dl className="mt-5 grid grid-cols-2 gap-2">
        <div className="rounded-xl bg-muted px-3 py-2.5"><dt className="text-xs text-muted-foreground">This week</dt><dd className="mt-0.5 font-semibold tabular-nums">{usd(agent.cost_usd) || "$0"}</dd></div>
        <div className="rounded-xl bg-muted px-3 py-2.5"><dt className="text-xs text-muted-foreground">For you</dt><dd className="mt-0.5 font-semibold tabular-nums">{agent.waiting}</dd></div>
      </dl>
      <h3 className="mt-6 mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Routines</h3>
      {agent.routines.length ? (
        <ul className="space-y-1">
          {agent.routines.map((r) => (
            <li key={r.name} className="flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-sm hover:bg-accent/60">
              <span className={cn("size-1.5 rounded-full", r.last ? (r.last.ok ? "bg-emerald-400" : "bg-red-500") : "bg-zinc-500")} />
              <span className="flex-1 truncate font-mono text-[13px]">{r.name}</span>
              <span className="text-xs text-muted-foreground">{r.schedule ?? "on demand"}</span>
            </li>
          ))}
        </ul>
      ) : <p className="text-sm text-muted-foreground">No routine yet.</p>}
      {agent.routines.some((r) => r.last) && (
        <p className="mt-3 text-xs text-muted-foreground">Last ran {ago(agent.routines.map((r) => r.last?.at ?? "").sort().at(-1))}.</p>
      )}
    </aside>
  )
}
