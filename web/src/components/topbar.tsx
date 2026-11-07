import { Building, ChevronsUpDown, PanelRight, Plus, Sparkles, X } from "lucide-react"
import { Character } from "@/components/character"
import { UpdateNotice } from "@/components/update-notice"
import { titleBar } from "@/lib/shell"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import type { Agent, Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

/** The window's top: the open conversations as tabs (on the Agents page),
    the update notice, the details toggle and, at the far right, the
    business. In the Mac app it is the title bar. */
export function TopBar({ realms, realm, onAgents, tabs, current, agents, onTab, onClose, onNewBusiness, onHq, details, onDetails }: {
  realms: Realm[]; realm: string; onAgents: boolean; tabs: string[]; current: string | null; agents: Agent[]
  onTab: (name: string) => void; onClose: (name: string) => void; onNewBusiness: () => void; onHq: () => void; details: boolean; onDetails: () => void
}) {
  const others = realms.filter((r) => r.name !== realm && !r.archived).reduce((n, r) => n + (r.needs_approval || 0), 0)
  const closed = agents.filter((a) => !tabs.includes(a.name))
  return (
    <header onMouseDown={titleBar} className="flex h-12 shrink-0 items-center gap-2 pr-3 select-none max-md:pl-3">
      {/* Room for the Mac window's buttons, then the tabs from the column's edge. */}
      <div className="w-[calc(var(--side)+56px)] shrink-0 max-md:hidden" />
      <nav className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto max-md:invisible">
        {onAgents && tabs.map((t) => {
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
        {/* Another agent in a tab: the ones not open yet. */}
        {onAgents && closed.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" className="shrink-0 text-muted-foreground" aria-label="Open an agent" />}><Plus /></DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              {closed.map((a) => (
                <DropdownMenuItem key={a.name} onClick={() => onTab(a.name)}>
                  <Character agent={a} size={18} still /> <span className="truncate">{a.title}</span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </nav>
      <UpdateNotice working={agents.filter((a) => a.state === "working").length} />
      {onAgents && <Button variant="ghost" size="icon-sm" className={cn("text-muted-foreground max-md:hidden", details && "bg-accent text-foreground")} aria-label="Details" title="Details" onClick={onDetails}><PanelRight /></Button>}
      <DropdownMenu>
        <DropdownMenuTrigger render={<button className="flex h-8 items-center gap-2 rounded-lg border bg-background/60 pr-2 pl-1.5 text-[13.5px] font-semibold hover:bg-accent" />}>
          <span className="size-5 rounded-full bg-[radial-gradient(circle_at_35%_30%,#d9f99d,#22c55e_55%,#14532d)] shadow-inner" />
          <span className="capitalize">{realm}</span>
          {/* Another business waits for the founder: said here, wherever they are. */}
          {others > 0 && <span className="grid h-4 min-w-4 place-items-center rounded-full bg-red-500 px-1 text-[10px] font-semibold text-white" title={`${others} waiting in your other businesses`}>{others}</span>}
          <ChevronsUpDown className="size-3.5 text-muted-foreground" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
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
    </header>
  )
}
