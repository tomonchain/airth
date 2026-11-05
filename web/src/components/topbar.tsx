import { Building, ChevronsUpDown, PanelRight, Plus, Sparkles, X } from "lucide-react"
import { Character } from "@/components/character"
import { UpdateNotice } from "@/components/update-notice"
import { mac, titleBar } from "@/lib/shell"
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
  const others = realms.filter((r) => r.name !== realm && !r.archived).reduce((n, r) => n + (r.needs_approval || 0), 0)
  return (
    <header onMouseDown={titleBar} className="flex h-12 shrink-0 items-center gap-2 pr-3 select-none">
      {/* In the Mac app the window's traffic lights sit here. */}
      <div className={cn("flex w-[404px] shrink-0 items-center", mac ? "pl-[78px]" : "pl-4")}>
        <DropdownMenu>
          {/* In the app, as small as the window's buttons beside it. */}
          <DropdownMenuTrigger render={<button className={cn("flex items-center rounded-lg font-semibold hover:bg-accent", mac ? "mt-[5px] h-6 gap-1.5 px-1.5 text-[12.5px]" : "gap-2 px-2 py-1 text-[13.5px]")} />}>
            <span className={cn("rounded-full bg-[radial-gradient(circle_at_35%_30%,#d9f99d,#22c55e_55%,#14532d)] shadow-inner", mac ? "size-3.5" : "size-5")} />
            <span className="capitalize">{realm}</span>
            {/* Another business waits for the founder: said here, wherever they are. */}
            {others > 0 && <span className="grid h-4 min-w-4 place-items-center rounded-full bg-red-500 px-1 text-[10px] font-semibold text-white" title={`${others} waiting in your other businesses`}>{others}</span>}
            <ChevronsUpDown className={cn("text-muted-foreground", mac ? "size-3" : "size-3.5")} />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            {/* A label only holds inside a group: without one, opening the menu threw. */}
            <DropdownMenuGroup>
            <DropdownMenuLabel>Businesses</DropdownMenuLabel>
            {realms.filter((r) => !r.archived).map((r) => (
              <DropdownMenuItem key={r.name} onClick={() => (location.hash = `#/b/${encodeURIComponent(r.name)}`)} className="capitalize">
                <span className="flex-1">{r.name}</span>
                {r.needs_approval > 0 && <span className="grid h-4 min-w-4 place-items-center rounded-full bg-red-500 px-1 text-[10px] font-semibold text-white">{r.needs_approval}</span>}
                {r.live > 0 && <span className="size-1.5 rounded-full bg-emerald-500" title="An agent is working" />}
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
