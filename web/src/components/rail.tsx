import { Bot, Building2, CalendarClock, House, Inbox, ListChecks, Moon, Settings, Sun, Wallet, Workflow, type LucideIcon } from "lucide-react"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useTheme } from "@/hooks/use-theme"
import { cn } from "@/lib/utils"

export type Page = "home" | "agents" | "needs" | "routines" | "tasks" | "calendar" | "finance" | "company" | "settings"

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
        <span className="absolute -top-0.5 -right-0.5 grid h-4 min-w-4 place-items-center rounded-full bg-amber-400 px-1 text-[10px] font-semibold text-black">{badge}</span>
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

/** The app's rail: the pages of a business. Pages not moved yet open in the
    classic interface. */
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
      <span className="my-1.5 h-px w-6 bg-border" />
      <RailButton icon={Wallet} label="Finance" active={page === "finance"} onClick={() => onPage("finance")} />
      <RailButton icon={Building2} label="Company" active={page === "company"} onClick={() => onPage("company")} />
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
