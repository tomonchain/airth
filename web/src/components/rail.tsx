import { useState } from "react"
import { Bot, Building2, CalendarClock, ChartColumn, Ellipsis, FolderOpen, House, Inbox, ListChecks, Moon, Settings, Store, Sun, type LucideIcon, Wallet, Workflow } from "lucide-react"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useTheme } from "@/hooks/use-theme"
import { cn } from "@/lib/utils"

export type Page = "home" | "agents" | "needs" | "routines" | "tasks" | "calendar" | "documents" | "finance" | "usage" | "hq" | "company" | "settings" | "marketplace"

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

/** The app's rail: the pages of a business. */
export function Rail({ page, onPage, needs, live }: { page: Page; onPage: (p: Page) => void; needs: number; live: boolean }) {
  const { theme, setTheme } = useTheme()
  return (
    <nav className="flex w-16 shrink-0 flex-col items-center gap-1.5 pt-1 pb-3">
      <RailButton icon={House} label="Home" active={page === "home"} onClick={() => onPage("home")} />
      <RailButton icon={Bot} label="Agents" active={page === "agents"} onClick={() => onPage("agents")} />
      <RailButton icon={Inbox} label="Needs you" active={page === "needs"} badge={needs} onClick={() => onPage("needs")} />
      <span className="my-1.5 h-px w-6 bg-border" />
      <RailButton icon={Workflow} label="Routines" active={page === "routines"} onClick={() => onPage("routines")} />
      <RailButton icon={ListChecks} label="Tasks" active={page === "tasks"} onClick={() => onPage("tasks")} />
      <RailButton icon={CalendarClock} label="Calendar" active={page === "calendar"} onClick={() => onPage("calendar")} />
      <RailButton icon={FolderOpen} label="Documents" active={page === "documents"} onClick={() => onPage("documents")} />
      <span className="my-1.5 h-px w-6 bg-border" />
      <RailButton icon={Wallet} label="Finance" active={page === "finance"} onClick={() => onPage("finance")} />
      <RailButton icon={ChartColumn} label="Usage" active={page === "usage"} onClick={() => onPage("usage")} />
      <RailButton icon={Building2} label="Company" active={page === "company"} onClick={() => onPage("company")} />
      <RailButton icon={Store} label="Marketplace" active={page === "marketplace"} onClick={() => onPage("marketplace")} />
      <div className="mt-auto flex flex-col items-center gap-1.5">
        <Tooltip>
          <TooltipTrigger render={<span className="mb-1 grid size-10 place-items-center" />}>
            <span className={cn("grid size-8 place-items-center rounded-full ring-2", live ? "ring-emerald-400/80" : "ring-zinc-500/40")}>
              <span className={cn("size-2.5 rounded-full", live ? "animate-pulse bg-emerald-400" : "bg-zinc-500")} />
            </span>
          </TooltipTrigger>
          <TooltipContent side="right">{live ? "An agent is working" : "No agent is working"}</TooltipContent>
        </Tooltip>
        <RailButton icon={theme === "dark" ? Sun : Moon} label={theme === "dark" ? "Light mode" : "Dark mode"} onClick={() => setTheme(theme === "dark" ? "light" : "dark")} />
        <RailButton icon={Settings} label="Settings" active={page === "settings"} onClick={() => onPage("settings")} />
      </div>
    </nav>
  )
}

// On a phone, the bar holds what the founder opens most; the rest is under More.
const MAIN: [Page, string, LucideIcon][] = [["home", "Home", House], ["agents", "Agents", Bot], ["needs", "Needs you", Inbox], ["tasks", "Tasks", ListChecks]]
const MORE: [Page, string, LucideIcon][] = [
  ["routines", "Routines", Workflow], ["calendar", "Calendar", CalendarClock], ["documents", "Documents", FolderOpen], ["finance", "Finance", Wallet],
  ["usage", "Usage", ChartColumn], ["company", "Company", Building2], ["marketplace", "Marketplace", Store], ["settings", "Settings", Settings],
]

/** The rail on a phone: a bar at the bottom, within the thumb's reach. */
export function TabBar({ page, onPage, needs }: { page: Page; onPage: (p: Page) => void; needs: number }) {
  const { theme, setTheme } = useTheme()
  const [more, setMore] = useState(false)
  const tabs: [Page | "more", string, LucideIcon][] = [...MAIN, ["more", "More", Ellipsis]]
  // The tab the page is under: its own, or More; none for the HQ.
  const at = MAIN.findIndex(([p]) => p === page)
  const open = at >= 0 ? at : MORE.some(([p]) => p === page) ? tabs.length - 1 : -1
  const known = open >= 0
  return (
    <nav className="pointer-events-none absolute inset-x-0 bottom-0 z-40 px-3 pb-[max(env(safe-area-inset-bottom),10px)]">
      <div className="glass pointer-events-auto relative mx-auto grid h-[62px] max-w-md grid-cols-5 rounded-[31px] p-1">
        {/* One fifth of the bar, moved under the open tab. */}
        <span aria-hidden className={cn("glass-lens absolute inset-y-1 left-1 w-[calc((100%-8px)/5)] rounded-[27px]", !known && "opacity-0")}
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
      <Sheet open={more} onOpenChange={setMore}>
        <SheetContent side="bottom" className="glass rounded-[28px] border px-3 pt-4 pb-3 data-[side=bottom]:inset-x-3 data-[side=bottom]:bottom-[max(env(safe-area-inset-bottom),10px)]">
          <SheetTitle className="px-2">More</SheetTitle>
          <div className="grid grid-cols-4 gap-1">
            {MORE.map(([p, label, Icon]) => (
              <button key={p} onClick={() => { onPage(p); setMore(false) }}
                className={cn("flex flex-col items-center gap-1.5 rounded-xl py-3 text-xs text-muted-foreground hover:bg-accent", page === p && "bg-accent text-foreground")}>
                <Icon className="size-[22px]" strokeWidth={1.8} /> {label}
              </button>
            ))}
            <button onClick={() => setTheme(theme === "dark" ? "light" : "dark")} className="flex flex-col items-center gap-1.5 rounded-xl py-3 text-xs text-muted-foreground hover:bg-accent">
              {theme === "dark" ? <Sun className="size-[22px]" strokeWidth={1.8} /> : <Moon className="size-[22px]" strokeWidth={1.8} />} {theme === "dark" ? "Light" : "Dark"}
            </button>
          </div>
        </SheetContent>
      </Sheet>
    </nav>
  )
}
