import { useEffect, useMemo, useState } from "react"
import { Building2, ListChecks, Moon, PanelRight, PenLine, Search, Sparkles, Sun, type LucideIcon } from "lucide-react"
import { Character } from "@/components/character"
import { MAIN, MORE, type Page } from "@/components/rail"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { useTheme } from "@/hooks/use-theme"
import { api, called, enc, type Agent, type Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

type Entry = { group: string; label: string; icon?: LucideIcon; agent?: Agent; keys?: string; run: () => void }
type Run = { id: string; title: string | null; objective: string; archived: boolean; live: boolean }

export const MOD = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl "

const PAGES: [Page, string, LucideIcon][] = [...MAIN, ...MORE, ["hq", "HQ: all businesses", Building2]]

/** ⌘K (the classic interface's palette): go anywhere, open an agent or a
    task, switch business, and the few actions worth a key. */
export function Palette({ open, onOpenChange, realm, realms, agents, onPage, onAgent, onTask, onAsk, onDetails }: {
  open: boolean; onOpenChange: (o: boolean) => void; realm: string; realms: Realm[]; agents: Agent[]
  onPage: (p: Page) => void; onAgent: (name: string) => void; onTask: (id: string) => void; onAsk: () => void; onDetails: () => void
}) {
  const { theme, setTheme } = useTheme()
  const [q, setQ] = useState("")
  const [i, setI] = useState(0)
  const [runs, setRuns] = useState<Run[]>([])
  useEffect(() => {
    if (!open) return
    setQ(""); setI(0)
    api<Run[]>(`realms/${enc(realm)}/runs`).then((r) => setRuns(r.filter((x) => !x.archived).slice(0, 30)), () => setRuns([]))
  }, [open, realm])

  const entries = useMemo<Entry[]>(() => [
    { group: "Actions", label: "Ask the team for something", icon: PenLine, keys: `${MOD}⇧O`, run: onAsk },
    { group: "Actions", label: theme === "dark" ? "Switch to light mode" : "Switch to dark mode", icon: theme === "dark" ? Sun : Moon, run: () => setTheme(theme === "dark" ? "light" : "dark") },
    { group: "Actions", label: "Show or hide the details", icon: PanelRight, run: onDetails },
    ...PAGES.map(([p, label, icon]) => ({ group: "Go to", label, icon, run: () => onPage(p) })),
    { group: "Go to", label: "The welcome guide", icon: Sparkles, run: () => { location.hash = "#/welcome" } },
    ...agents.map((a) => ({ group: "Agents", label: called(a), agent: a, run: () => onAgent(a.name) })),
    ...runs.map((r) => ({ group: `Tasks in ${realm}`, label: r.title || r.objective.split("\n")[0] || r.id, icon: ListChecks, run: () => onTask(r.id) })),
    ...realms.filter((r) => !r.archived && r.name !== realm).map((r) => ({ group: "Switch business", label: r.name, icon: Building2, run: () => { location.hash = `#/b/${encodeURIComponent(r.name)}` } })),
  ], [agents, runs, realms, realm, theme, onAsk, onDetails, onPage, onAgent, onTask, setTheme])

  const words = q.toLowerCase().split(/\s+/).filter(Boolean)
  const hits = entries.filter((e) => words.every((w) => `${e.group} ${e.label}`.toLowerCase().includes(w))).slice(0, 40)
  const pick = (e?: Entry) => { if (!e) return; onOpenChange(false); e.run() }
  let last = ""

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="top-[16%] translate-y-0 gap-0 p-0 sm:max-w-xl" showCloseButton={false}>
        <DialogTitle className="sr-only">Go anywhere</DialogTitle>
        <div className="flex items-center gap-2 border-b px-3">
          <Search className="size-4 text-muted-foreground" />
          <input autoFocus value={q} onChange={(e) => { setQ(e.target.value); setI(0) }} placeholder="Go to a page, an agent, a task, a business…" aria-label="Search"
            className="h-12 flex-1 bg-transparent text-sm outline-none"
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setI((x) => Math.min(x + 1, hits.length - 1)) }
              if (e.key === "ArrowUp") { e.preventDefault(); setI((x) => Math.max(x - 1, 0)) }
              if (e.key === "Enter") { e.preventDefault(); pick(hits[i]) }
            }} />
          <kbd className="rounded border px-1.5 text-[11px] text-muted-foreground">esc</kbd>
        </div>
        <div className="max-h-96 overflow-y-auto p-1.5">
          {!hits.length && <p className="px-3 py-3 text-sm text-muted-foreground">Nothing matches.</p>}
          {hits.map((e, n) => {
            const head = e.group !== last
            last = e.group
            return (
              <div key={`${e.group}${e.label}${n}`}>
                {head && <div className="px-2.5 pt-2 pb-1 text-[11px] font-medium text-muted-foreground">{e.group}</div>}
                <button onClick={() => pick(e)} onMouseMove={() => setI(n)} className={cn("flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-sm", n === i && "bg-accent")}>
                  {e.agent ? <Character agent={e.agent} size={18} still /> : e.icon ? <e.icon className="size-4 text-muted-foreground" /> : null}
                  <span className="min-w-0 flex-1 truncate">{e.label}</span>
                  {e.keys && <kbd className="text-[11px] text-muted-foreground">{e.keys}</kbd>}
                </button>
              </div>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}
