import { Bot, BookOpen, Check, GitBranch, Hand, Play, Sparkles, Zap, X, type LucideIcon } from "lucide-react"
import { Character } from "@/components/character"
import { Loading, Page, Panel } from "@/components/page"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Agent, type Routine } from "@/lib/api"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"

// The six blocks (CLAUDE.md): what each looks like in a routine's chain.
const BLOCK: Record<string, { icon: LucideIcon; label: string }> = {
  read: { icon: BookOpen, label: "Read" },
  ai: { icon: Sparkles, label: "AI" },
  condition: { icon: GitBranch, label: "If" },
  approval: { icon: Hand, label: "You approve" },
  act: { icon: Zap, label: "Act" },
}

function trigger(r: Routine) {
  const t = r.graph.trigger
  if (t.schedule) return t.schedule
  if (t.telegram) return `on ${t.telegram}`
  if (t.webhook) return "on a webhook"
  return "by hand"
}

/** Each routine as its chain of blocks, when it runs, how it went, what it
    costs. Editing the graph stays in the classic interface for now. */
export function Routines({ realm, agents, onAgent }: { realm: string; agents: Agent[]; onAgent: (name: string) => void }) {
  const { data } = usePoll<{ routines: Routine[] }>(() => api(`realms/${enc(realm)}/graphs`), 15000, [realm])
  return (
    <Page title="Routines" lede={data ? `${data.routines.length} routines, the agents' recurring work.` : undefined}>
      {!data ? <Loading /> : (
        <div className="space-y-2.5">
          {data.routines.map((r) => {
            const a = agents.find((x) => x.name === r.graph.agent)
            const last = r.runs[0]
            return (
              <Panel key={r.name} className="px-4 py-3.5">
                <div className="flex items-center gap-3">
                  {a ? <button onClick={() => onAgent(a.name)} aria-label={a.title}><Character agent={a} size={30} still /></button> : <Bot className="size-6" />}
                  <div className="min-w-0 flex-1">
                    <div className="font-mono text-[13px] font-medium">{r.name}</div>
                    <div className="text-xs text-muted-foreground">{r.agent_title} · {trigger(r)}{r.next_at ? ` · next ${new Date(r.next_at).toLocaleString("en-GB", { weekday: "short", hour: "2-digit", minute: "2-digit" })}` : ""}</div>
                  </div>
                  <div className="text-right text-xs text-muted-foreground">
                    {last ? (
                      <span className={cn("inline-flex items-center gap-1", last.ok ? "text-emerald-500" : "text-red-500")}>
                        {last.ok ? <Check className="size-3.5" /> : <X className="size-3.5" />} {ago(last.at)}
                      </span>
                    ) : "Never ran"}
                    <div className="tabular-nums">{usd(r.cost_usd) || "$0"} this week</div>
                  </div>
                </div>
                <div className="mt-3 flex flex-wrap items-center gap-1">
                  {r.graph.trigger.schedule || r.graph.trigger.telegram ? (
                    <span className="inline-flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground"><Play className="size-3" /> {trigger(r)}</span>
                  ) : null}
                  {r.graph.blocks.map((b) => {
                    const k = BLOCK[b.type] ?? BLOCK.act
                    return (
                      <span key={b.id} className="flex items-center gap-1">
                        <span className="text-muted-foreground/50">→</span>
                        <span className={cn("inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px]", b.type === "approval" && "border-amber-400/40 text-amber-600 dark:text-amber-300")}>
                          <k.icon className="size-3" /> {b.id}
                        </span>
                      </span>
                    )
                  })}
                </div>
                {last?.error && <p className="mt-2 line-clamp-2 font-mono text-[11px] text-red-500">{last.error}</p>}
              </Panel>
            )
          })}
        </div>
      )}
    </Page>
  )
}
