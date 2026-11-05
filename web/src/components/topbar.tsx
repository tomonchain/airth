import { Building, ChevronsUpDown, PanelRight, Plus, Sparkles, X } from "lucide-react"
import { Character } from "@/components/character"
import { UpdateNotice } from "@/components/update-notice"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import type { Agent, Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

/** The window's top: the business, the open conversations as tabs, the
    details toggle. In the Mac app it is the title bar. */
export function TopBar({ realms, realm, tabs, current, agents, onTab, onClose, onNew, onNewBusiness, onHq, details, onDetails }: {
  realms: Realm[]; realm: string; tabs: string[]; current: string | null; agents: Agent[]
  onTab: (name: string) => void; onClose: (name: string) => void; onNew: () => void; onNewBusiness: () => void; onHq: () => void; details: boolean; onDetails: () => void
}) {
  return (
    <header className="flex h-12 shrink-0 items-center gap-2 pr-3">
      <div className="flex w-[404px] shrink-0 items-center pl-4">
        <DropdownMenu>
          <DropdownMenuTrigger render={<button className="flex items-center gap-2 rounded-lg px-2 py-1 text-[13.5px] font-semibold hover:bg-accent" />}>
            <span className="size-5 rounded-full bg-[radial-gradient(circle_at_35%_30%,#d9f99d,#22c55e_55%,#14532d)] shadow-inner" />
            <span className="capitalize">{realm}</span>
            <ChevronsUpDown className="size-3.5 text-muted-foreground" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            {/* A label only holds inside a group: without one, opening the menu threw. */}
            <DropdownMenuGroup>
            <DropdownMenuLabel>Businesses</DropdownMenuLabel>
            {realms.filter((r) => !r.archived).map((r) => (
              <DropdownMenuItem key={r.name} onClick={() => (location.hash = `#/b/${encodeURIComponent(r.name)}`)} className="capitalize">
                {r.name}
              </DropdownMenuItem>
            ))}
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={onHq}><Building /> HQ: all businesses</DropdownMenuItem>
            <DropdownMenuItem onClick={onNewBusiness}><Plus /> New business</DropdownMenuItem>
            <DropdownMenuItem onClick={() => (location.hash = "#/welcome")}><Sparkles /> The welcome guide</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <nav className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
        {tabs.map((t) => {
          const a = agents.find((x) => x.name === t)
          if (!a) return null
          return (
            <div key={t} className={cn("group flex h-8 shrink-0 items-center gap-1.5 rounded-lg pr-1 pl-2 text-sm", current === t ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/60")}>
              <button onClick={() => onTab(t)} className="flex items-center gap-1.5">
                <Character agent={a} size={18} still />
                <span className="max-w-40 truncate">{a.title}</span>
              </button>
              <button onClick={() => onClose(t)} aria-label={`Close ${a.title}`} className="grid size-5 place-items-center rounded opacity-0 group-hover:opacity-100 hover:bg-background/50">
                <X className="size-3" />
              </button>
            </div>
          )
        })}
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label="Open an agent" onClick={onNew}><Plus /></Button>
      </nav>
      <UpdateNotice working={agents.filter((a) => a.state === "working").length} />
      <Button variant="ghost" size="icon-sm" className={cn("text-muted-foreground", details && "bg-accent text-foreground")} aria-label="Details" onClick={onDetails}><PanelRight /></Button>
    </header>
  )
}
