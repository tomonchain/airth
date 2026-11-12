import { useState } from "react"
import { Bot, Building2, CalendarClock, ChartColumn, Ellipsis, FolderKanban, FolderOpen, Inbox, MessageCircle, Moon, Settings, Store, Sun, type LucideIcon, Wallet, Workflow, LayoutDashboard } from "lucide-react"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { SpendIndicator, SpendPanel, SpendRing, useSpend } from "@/components/spend"
import { useTheme } from "@/hooks/use-theme"
import { cn } from "@/lib/utils"

export type Page = "home" | "overview" | "agents" | "needs" | "routines" | "tasks" | "calendar" | "documents" | "finance" | "usage" | "hq" | "company" | "settings" | "marketplace"

function RailButton({ icon: Icon, label, active, badge, onClick, href, soon }: {
  icon: LucideIcon; label: string; active?: boolean; badge?: number; onClick?: () => void; href?: string; soon?: boolean
}) {
  const cls = cn(
    "relative grid size-10 place-items-center rounded-xl text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
    active && "bg-accent text-foreground",
    soon && "cursor-default opacity-40 hover:bg-transparent hover:text-muted-foreground",
  )
  const inner = (
    <>
      <Icon className="size-[19px]" strokeWidth={1.8} />
      {!!badge && (
        <span className="absolute -top-0.5 -right-0.5 grid h-4 min-w-4 place-items-center rounded-full bg-red-500 px-1 text-[10px] font-semibold text-white ring-2 ring-background">{badge}</span>
      )}
    </>
  )
  return (
    <Tooltip>
      <TooltipTrigger render={href ? <a href={href} target="_blank" rel="noreferrer" className={cls} aria-label={label} /> : <button className={cls} aria-label={label} onClick={soon ? undefined : onClick} aria-disabled={soon || undefined} />}>
        {inner}
      </TooltipTrigger>
      <TooltipContent side="right">{soon ? `${label}: not moved here yet` : label}</TooltipContent>
    </Tooltip>
  )
}

// The founder talks, answers what waits, follows the projects: those three
// first; then the team, then the business, in groups (decisions 128, 130).
// Nothing left Airth (decision 42): only where it is found moved.
export const MAIN: [Page, string, LucideIcon][] = [["home", "Chat", MessageCircle], ["needs", "Needs you", Inbox], ["tasks", "Projects", FolderKanban]]
export const GROUPS: { title: string; pages: [Page, string, LucideIcon][] }[] = [
  { title: "Team", pages: [["agents", "Team", Bot], ["routines", "Routines", Workflow], ["calendar", "Calendar", CalendarClock]] },
  { title: "Business", pages: [["overview", "Overview", LayoutDashboard], ["finance", "Finance", Wallet], ["documents", "Documents", FolderOpen], ["company", "Business", Building2], ["marketplace", "Marketplace", Store]] },
  { title: "Airth", pages: [["usage", "Usage", ChartColumn], ["settings", "Settings", Settings]] },
]
export const MORE = GROUPS.flatMap((g) => g.pages)

/** The app's rail: the three pages the founder lives in, then the team's
    and the business's, a group each; Settings at its foot. */
export function Rail({ page, onPage, needs, live }: { page: Page; onPage: (p: Page) => void; needs: number; live: boolean }) {
  const { theme, setTheme } = useTheme()
  return (
    <nav className="flex w-16 shrink-0 flex-col items-center gap-1.5 overflow-y-auto pt-1 pb-3">
      {MAIN.map(([p, label, Icon]) => <RailButton key={p} icon={Icon} label={label} active={page === p} badge={p === "needs" ? needs : 0} onClick={() => onPage(p)} />)}
      {/* Usage is the spending ring's; Settings, the foot's. */}
      {GROUPS.slice(0, 2).map((g) => (
        <div key={g.title} role="group" aria-label={g.title} className="flex flex-col items-center gap-1.5">
          <span className="my-1.5 h-px w-6 bg-foreground/10" />
          {g.pages.map(([p, label, Icon]) => <RailButton key={p} icon={Icon} label={label} active={page === p} onClick={() => onPage(p)} />)}
        </div>
      ))}
      <div className="mt-auto flex flex-col items-center gap-1.5">
        <SpendIndicator live={live} onUsage={() => onPage("usage")} />
        <RailButton icon={theme === "dark" ? Sun : Moon} label={theme === "dark" ? "Light mode" : "Dark mode"} onClick={() => setTheme(theme === "dark" ? "light" : "dark")} />
        <RailButton icon={Settings} label="Settings" active={page === "settings"} onClick={() => onPage("settings")} />
      </div>
    </nav>
  )
}

/** The rail on a phone: a bar at the bottom, within the thumb's reach. */
export function TabBar({ page, onPage, needs }: { page: Page; onPage: (p: Page) => void; needs: number }) {
  const { theme, setTheme } = useTheme()
  const [more, setMore] = useState(false)
  const [spending, setSpending] = useState(false)
  const spend = useSpend()
  const tabs: [Page | "more", string, LucideIcon][] = [...MAIN, ["more", "More", Ellipsis]]
  // The tab the page is under: its own, or More; none for the HQ.
  const at = MAIN.findIndex(([p]) => p === page)
  const open = at >= 0 ? at : MORE.some(([p]) => p === page) ? tabs.length - 1 : -1
  const known = open >= 0
  return (
    <nav className="pointer-events-none absolute inset-x-0 bottom-0 z-40 px-3 pb-[max(env(safe-area-inset-bottom),10px)]">
      <div className="glass pointer-events-auto relative mx-auto grid h-[62px] max-w-md grid-cols-4 rounded-[31px] p-1">
        {/* One quarter of the bar, moved under the open tab. */}
        <span aria-hidden className={cn("glass-lens absolute inset-y-1 left-1 w-[calc((100%-8px)/4)] rounded-[27px]", !known && "opacity-0")}
          style={{ transform: `translateX(${open * 100}%)` }} />
        {tabs.map(([p, label, Icon], i) => {
          const active = known && i === open
          const badge = p === "needs" ? needs : 0
          return (
            <button key={label} onClick={() => p === "more" ? setMore(true) : onPage(p)} aria-current={active ? "page" : undefined}
              className={cn("relative flex flex-col items-center justify-center gap-0.5 rounded-[27px] text-[10px] font-medium text-foreground/60 transition-colors active:scale-95", active && "text-foreground")}>
              <Icon className="size-[22px]" strokeWidth={active ? 2.2 : 1.8} />
              {label}
              {badge > 0 && <span className="absolute top-1 left-[calc(50%+5px)] grid h-4 min-w-4 place-items-center rounded-full bg-red-500 px-1 text-[10px] font-semibold text-white">{badge}</span>}
            </button>
          )
        })}
      </div>
      <Sheet open={more} onOpenChange={(o) => { setMore(o); if (!o) setSpending(false) }}>
        <SheetContent side="bottom" className="glass rounded-[28px] border px-3 pt-4 pb-3 data-[side=bottom]:inset-x-3 data-[side=bottom]:bottom-[max(env(safe-area-inset-bottom),10px)]">
          <SheetTitle className="px-2">More</SheetTitle>
          {/* What the models cost, as the rail's ring shows it on a computer. */}
          {spending ? (
            <div className="max-h-[60svh] overflow-y-auto px-1">
              <SpendPanel data={spend.data} error={spend.error} onChanged={spend.refresh} onUsage={() => { onPage("usage"); setMore(false); setSpending(false) }} />
              <button className="mt-3 text-sm text-muted-foreground" onClick={() => setSpending(false)}>Back</button>
            </div>
          ) : (
            <button onClick={() => setSpending(true)} className="flex items-center gap-3 rounded-xl px-2 py-2 text-left hover:bg-accent">
              <SpendRing data={spend.data} live={false} size={28} />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">Models and spending</span>
                <span className="block text-xs text-muted-foreground">{spend.data ? `$${spend.data.month_usd.toFixed(2)} this month` : "Loading…"}</span>
              </span>
            </button>
          )}
          {!spending && <div className="max-h-[62svh] overflow-y-auto">
            {GROUPS.map((g, i) => (
              <section key={g.title} className="mt-2">
                <h3 className="px-2 pb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{g.title}</h3>
                <div className="grid grid-cols-4 gap-1">
                  {g.pages.map(([p, label, Icon]) => (
                    <button key={p} onClick={() => { onPage(p); setMore(false) }}
                      className={cn("flex flex-col items-center gap-1.5 rounded-xl py-3 text-xs text-muted-foreground hover:bg-accent", page === p && "bg-accent text-foreground")}>
                      <Icon className="size-[22px]" strokeWidth={1.8} /> {label}
                    </button>
                  ))}
                  {i === GROUPS.length - 1 && (
                    <button onClick={() => setTheme(theme === "dark" ? "light" : "dark")} className="flex flex-col items-center gap-1.5 rounded-xl py-3 text-xs text-muted-foreground hover:bg-accent">
                      {theme === "dark" ? <Sun className="size-[22px]" strokeWidth={1.8} /> : <Moon className="size-[22px]" strokeWidth={1.8} />} {theme === "dark" ? "Light" : "Dark"}
                    </button>
                  )}
                </div>
              </section>
            ))}
          </div>}
        </SheetContent>
      </Sheet>
    </nav>
  )
}
