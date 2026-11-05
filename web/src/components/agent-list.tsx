import { useEffect, useState } from "react"
import { Pin, PinOff, Plus, Search, TriangleAlert } from "lucide-react"
import { Character } from "@/components/character"
import { HireAgent } from "@/components/hire-agent"
import { DEPARTMENTS, departmentKey, departmentName, type Agent, type Item } from "@/lib/api"
import { stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

export type Last = { text: string; at: string | null }

/** The agents as conversations: the one with something for the founder
    first, then the latest to have worked. */
export function AgentList({ realm, agents, broken, items, last, selected, onOpen, onHired }: {
  realm: string; agents: Agent[]; broken: string[]; items: Item[]; last: Record<string, Last>; selected: string | null; onOpen: (name: string) => void; onHired: (name: string) => void
}) {
  const [q, setQ] = useState("")
  const [hiring, setHiring] = useState(false)
  // Pinned agents stay on top; kept in this browser, per business.
  const key = `airth-pins:${realm}`
  const [pins, setPins] = useState<string[]>(() => { try { return JSON.parse(localStorage.getItem(key) ?? "[]") } catch { return [] } })
  useEffect(() => { try { setPins(JSON.parse(localStorage.getItem(key) ?? "[]")) } catch { setPins([]) } }, [key])
  const togglePin = (name: string) => setPins((p) => {
    const next = p.includes(name) ? p.filter((n) => n !== name) : [...p, name]
    try { localStorage.setItem(key, JSON.stringify(next)) } catch { /* private window */ }
    return next
  })
  const waiting = (a: Agent) => items.filter((i) => i.agent === a.name).length
  const shown = agents
    .filter((a) => !q || `${a.title} ${a.department} ${a.job}`.toLowerCase().includes(q.toLowerCase()))
    .sort((a, b) => Number(waiting(b) > 0) - Number(waiting(a) > 0) || (last[b.name]?.at ?? "").localeCompare(last[a.name]?.at ?? ""))
  // By department: the glossary's seven in order, then the business's own,
  // then none; pinned agents on top.
  const own = [...new Set(agents.map((a) => departmentKey(a.department)))]
    .filter((d) => d && !(DEPARTMENTS as readonly string[]).includes(d)).sort()
  const groups = [
    { label: "Pinned", list: shown.filter((a) => pins.includes(a.name)) },
    ...[...DEPARTMENTS, ...own, ""].map((d) => ({
      label: departmentName(d || null),
      list: shown.filter((a) => !pins.includes(a.name) && departmentKey(a.department) === d),
    })),
  ].filter((g) => g.list.length)
  return (
    <div className="flex w-[280px] shrink-0 flex-col border-r">
      <div className="p-3">
        <label className="flex h-9 items-center gap-2 rounded-lg bg-muted px-3 text-sm text-muted-foreground focus-within:ring-2 focus-within:ring-ring/40">
          <Search className="size-4" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search" className="flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
        </label>
      </div>
      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {groups.map((g) => (
          <section key={g.label} className="mb-1">
            <h2 className="px-2.5 pt-2 pb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{g.label}</h2>
            <ul className="space-y-0.5">
              {g.list.map((a) => {
                const n = waiting(a)
                const l = last[a.name]
                return (
                  <li key={a.name} className="group/row relative">
                    <button
                      onClick={() => onOpen(a.name)}
                      className={cn("flex w-full items-center gap-3 rounded-xl px-2.5 py-2.5 text-left transition-colors hover:bg-accent/70", selected === a.name && "bg-accent")}
                    >
                      <Character agent={a} size={36} still={selected !== a.name && a.state !== "working"} />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate text-[13.5px] font-medium">{a.title}</span>
                          {pins.includes(a.name) && <Pin className="size-3 shrink-0 text-muted-foreground" />}
                          <span className="ml-auto shrink-0 text-xs text-muted-foreground">{stamp(l?.at)}</span>
                        </div>
                        <div className="mt-0.5 flex items-center gap-2">
                          <span className={cn("truncate text-sm text-muted-foreground", a.state === "working" && "text-emerald-500")}>
                            {a.state === "working" ? "Working…" : l?.text ?? "No messages yet"}
                          </span>
                          {n > 0 && <span className="ml-auto grid h-5 min-w-5 shrink-0 place-items-center rounded-full bg-red-500 px-1.5 text-[11px] font-semibold text-white">{n}</span>}
                        </div>
                      </div>
                    </button>
                    <button onClick={() => togglePin(a.name)} aria-label={pins.includes(a.name) ? `Unpin ${a.title}` : `Pin ${a.title}`} title={pins.includes(a.name) ? "Unpin" : "Pin to the top"}
                      className="absolute top-2 right-2 hidden size-6 place-items-center rounded-md bg-background/80 text-muted-foreground group-hover/row:grid hover:text-foreground">
                      {pins.includes(a.name) ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
                    </button>
                  </li>
                )
              })}
            </ul>
          </section>
        ))}
        {!groups.length && <p className="px-3 py-4 text-sm text-muted-foreground">{q ? "No agent matches." : "No agent yet."}</p>}
        {/* A file Airth cannot read (an agent may write them): said, not hidden, so the rest of the team stays. */}
        {broken.map((b) => (
          <p key={b} className="mx-1 mt-2 flex gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 px-2.5 py-2 text-xs text-amber-700 dark:text-amber-300">
            <TriangleAlert className="mt-px size-3.5 shrink-0" />
            <span><span className="font-medium">Not shown:</span> {b}. Fix the file in Documents.</span>
          </p>
        ))}
      </div>
      <div className="border-t p-2">
        <button onClick={() => setHiring(true)} className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground">
          <Plus className="size-4" /> Hire an agent
        </button>
      </div>
      <HireAgent realm={realm} taken={agents.map((a) => a.name)} departments={own} open={hiring} onOpenChange={setHiring} onHired={onHired} />
    </div>
  )
}
