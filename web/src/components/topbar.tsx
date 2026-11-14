import { Building, ChevronsUpDown, PanelRight, PenLine, Plus, Server, Sparkles, X } from "lucide-react"
import { Character } from "@/components/character"
import { UpdateNotice } from "@/components/update-notice"
import { mac, titleBar } from "@/lib/shell"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { named, type Agent, type Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

/** The window's top: the open conversations as tabs (on the Team page),
    the update notice, the details toggle and, at the far right, the
    business. In the Mac app it is the title bar. */
export function TopBar({ server, realms, realm, onAgents, tabs, current, agents, onTab, onClose, onNewBusiness, onAsk, onHq, details, onDetails }: {
  server: string | null; realms: Realm[]; realm: string; onAgents: boolean; tabs: string[]; current: string | null; agents: Agent[]
  onTab: (name: string) => void; onClose: (name: string) => void; onNewBusiness: () => void; onAsk: () => void; onHq: () => void; details: boolean; onDetails: () => void
}) {
  const others = realms.filter((r) => r.name !== realm && !r.archived).reduce((n, r) => n + (r.needs_approval || 0), 0)
  const closed = agents.filter((a) => !tabs.includes(a.name))
  return (
    <header onMouseDown={titleBar} className="flex h-12 shrink-0 items-center gap-2 pr-3 select-none max-md:pl-3">
      {/* Room for the Mac window's buttons, then the tabs from the column's
          edge. Airth's mark and name sit over the rail in a browser, and
          just after the window's buttons in the Mac app; they open the HQ.
          On a phone, the mark alone (the founder's choice). */}
      <div className="flex w-[calc(var(--side)+56px)] shrink-0 items-center max-md:hidden">
        <button onClick={onHq} aria-label="Airth: all businesses" title="Airth: all businesses"
          className={cn("flex h-8 items-center gap-2 rounded-lg pr-2.5 pl-1 hover:bg-accent", mac ? "ml-[80px]" : "ml-4")}>
          <AirthMark id="airth-mark-desk" />
          <span className="text-[15px] font-semibold tracking-tight">Airth</span>
        </button>
      </div>
      {/* On a phone, Airth's mark at the top left; it opens the HQ, which
          the business menu alone reaches otherwise. */}
      <button onClick={onHq} aria-label="Airth: all businesses" className="grid size-8 shrink-0 place-items-center rounded-lg md:hidden">
        <AirthMark id="airth-mark-phone" />
      </button>
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
      {/* Ask from anywhere (⌘J): to the interlocutor, without leaving the page. */}
      <button onClick={onAsk} onMouseDown={(e) => e.stopPropagation()}
        className="flex h-8 w-64 shrink-0 items-center gap-2 rounded-lg bg-foreground/[0.05] px-2.5 text-[13px] text-muted-foreground hover:bg-accent hover:text-foreground max-lg:w-auto max-md:hidden">
        <PenLine className="size-3.5 shrink-0" />
        <span className="flex-1 truncate text-left max-lg:hidden">Ask the team…</span>
        <kbd className="rounded bg-muted px-1.5 font-sans text-[10.5px] max-lg:hidden">⌘J</kbd>
      </button>
      <UpdateNotice working={agents.filter((a) => a.state === "working").length} />
      {/* Which Airth this is, when the founder named it (Settings, About). */}
      {server && (
        <span className="flex h-8 max-w-32 shrink-0 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground" title="This Airth's name, set in Settings, About">
          <Server className="size-3.5 shrink-0" /> <span className="truncate">{server}</span>
        </span>
      )}
      {onAgents && <Button variant="ghost" size="icon-sm" className={cn("text-muted-foreground max-md:hidden", details && "bg-accent text-foreground")} aria-label="Details" title="Details" onClick={onDetails}><PanelRight /></Button>}
      <DropdownMenu>
        <DropdownMenuTrigger render={<button className="flex h-8 items-center gap-2 rounded-lg bg-foreground/[0.05] pr-2 pl-1.5 text-[13.5px] font-semibold hover:bg-accent" />}>
          <span className="size-5 rounded-full bg-[radial-gradient(circle_at_35%_30%,#d9f99d,#22c55e_55%,#14532d)] shadow-inner" />
          <span>{named(realm, realms.find((r) => r.name === realm)?.title)}</span>
          {/* Another business waits for the founder: said here, wherever they are. */}
          {others > 0 && <span className="grid h-4 min-w-4 place-items-center rounded-full bg-red-500 px-1 text-[10px] font-semibold text-white" title={`${others} waiting in your other businesses`}>{others}</span>}
          <ChevronsUpDown className="size-3.5 text-muted-foreground" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          {/* A label only holds inside a group: without one, opening the menu threw. */}
          <DropdownMenuGroup>
          <DropdownMenuLabel>Businesses</DropdownMenuLabel>
          {realms.filter((r) => !r.archived).map((r) => (
            <DropdownMenuItem key={r.name} onClick={() => (location.hash = `#/b/${encodeURIComponent(r.name)}`)}>
              <span className="flex-1">{named(r.name, r.title)}</span>
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

/** Airth's mark: a ring in the brand's greys. `id` names its gradient,
    one per copy on the page. */
function AirthMark({ id }: { id: string }) {
  return (
    <svg viewBox="0 0 64 64" className="size-6" aria-hidden>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" style={{ stopColor: "var(--logo-from)" }} />
          <stop offset=".55" style={{ stopColor: "var(--logo-mid)" }} />
          <stop offset="1" style={{ stopColor: "var(--logo-to)" }} />
        </linearGradient>
      </defs>
      <circle cx="32" cy="32" r="22" fill="none" stroke={`url(#${id})`} strokeWidth="12" />
    </svg>
  )
}
