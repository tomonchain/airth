import { ExternalLink, Moon, Sun } from "lucide-react"
import { useTheme } from "@/hooks/use-theme"
import { Button } from "@/components/ui/button"
import type { Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

export function AppSidebar({ realms, current, needs }: { realms: Realm[]; current: string | null; needs: Record<string, number> }) {
  const { theme, setTheme } = useTheme()
  const dark = theme === "dark"
  return (
    <aside className="flex w-60 shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground">
      <div className="flex h-14 items-center gap-2.5 px-4">
        <span className="grid size-7 place-items-center rounded-lg bg-foreground text-[13px] font-semibold text-background">A</span>
        <span className="text-sm font-semibold tracking-tight">Airth</span>
      </div>
      <p className="px-4 pt-2 pb-1.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Businesses</p>
      <nav className="flex flex-col gap-0.5 px-2">
        {realms.filter((r) => !r.archived).map((r) => (
          <a
            key={r.name}
            href={`#/b/${encodeURIComponent(r.name)}`}
            className={cn(
              "group flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-sm transition-colors hover:bg-sidebar-accent",
              current === r.name && "bg-sidebar-accent font-medium",
            )}
          >
            <span className="grid size-6 place-items-center rounded-md bg-muted text-xs font-semibold uppercase">{r.name[0]}</span>
            <span className="flex-1 truncate capitalize">{r.name}</span>
            {r.live > 0 && <span className="size-1.5 animate-pulse rounded-full bg-emerald-400" title="Working now" />}
            {(needs[r.name] ?? 0) > 0 && (
              <span className="rounded-full bg-amber-400/15 px-1.5 text-[11px] font-medium tabular-nums text-amber-600 dark:text-amber-300">{needs[r.name]}</span>
            )}
          </a>
        ))}
      </nav>
      <div className="mt-auto flex items-center gap-1 border-t p-2">
        <a href="/classic" className="flex flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-xs text-muted-foreground hover:bg-sidebar-accent hover:text-foreground">
          <ExternalLink className="size-3.5" /> Classic interface
        </a>
        <Button variant="ghost" size="icon-sm" aria-label={dark ? "Light mode" : "Dark mode"} onClick={() => setTheme(dark ? "light" : "dark")}>
          {dark ? <Sun /> : <Moon />}
        </Button>
      </div>
    </aside>
  )
}
