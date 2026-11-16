import { useState } from "react"
import { Popover } from "@base-ui/react/popover"
import { ArrowUpCircle, Loader, TriangleAlert } from "lucide-react"
import { toast } from "sonner"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { usePoll } from "@/hooks/use-poll"
import { api } from "@/lib/api"

type Info = {
  version: string
  update: { available: { short: string; changes: string[]; count: number; release: boolean } | null; phase: string; step: string | null; error: string | null }
}

/** A newer Airth, prepared and checked in the background, installed on a
    restart: its state, and the restart. `working`: tasks in progress, which
    the restart stops (they can be resumed), so it asks once more. */
export function useUpdate(working: number) {
  const { data, refresh } = usePoll<Info>(() => api("info"), 60000, [])
  const [sure, setSure] = useState(false)
  const [restarting, setRestarting] = useState(false)
  const u = data?.update
  const a = u?.available
  const shown = restarting || (!!a && !!u && ["ready", "preparing", "checking", "failed"].includes(u.phase))

  async function restart() {
    if (working && !sure) { setSure(true); return }
    const before = data!.version
    try { await api("update/restart", {}) } catch (e) { toast.error((e as Error).message); return }
    setRestarting(true)
    for (let i = 0; i < 300; i++) {
      await new Promise((r) => setTimeout(r, 1000))
      try { if ((await api<Info>("info")).version !== before) { location.reload(); return } } catch { /* still restarting */ }
    }
    setRestarting(false)
    toast.error("Airth did not come back. Open it again from the Mac's menu bar.")
  }
  const retry = () => api("update/check", {}).then(refresh, (e: Error) => toast.error(e.message))
  const label = restarting ? "Restarting Airth…" : u?.phase === "ready" ? "Update ready" : u?.phase === "failed" ? "Update not installed" : "Preparing an update"
  return { u, a, shown, ready: !restarting && u?.phase === "ready", restarting, sure, working, restart, retry, label }
}

export type Update = ReturnType<typeof useUpdate>

/** The update's icon: ready, failed, or on its way. */
export function UpdateIcon({ up, className }: { up: Update; className?: string }) {
  if (up.ready) return <ArrowUpCircle className={cn("text-sky-500", className)} />
  if (!up.restarting && up.u?.phase === "failed") return <TriangleAlert className={cn("text-amber-500", className)} />
  return <Loader className={cn("animate-spin text-muted-foreground", className)} />
}

/** What the update holds and the button that installs it. */
export function UpdatePanel({ up, phone }: { up: Update; phone?: boolean }) {
  const { u, a } = up
  if (up.restarting) return <p className="text-sm text-muted-foreground">Restarting Airth…</p>
  if (!u || !a) return null
  const what = `${a.count} change${a.count === 1 ? "" : "s"}`
  return (
    <div className={cn(phone && "text-base [&_.text-xs]:text-sm")}>
      <div className={cn("font-medium", phone ? "text-base" : "text-sm")}>{u.phase === "failed" ? `Update ${a.short} not installed` : u.phase === "ready" ? "A new version of Airth" : `Preparing ${a.short}`}</div>
      <div className="mt-0.5 text-xs text-muted-foreground">
        {u.phase === "failed" ? (a.release ? "It could not be downloaded or verified." : "Its build or tests failed.")
          : u.phase === "ready" ? `${a.short} · ${what} · ${a.release ? "signature verified" : "tests passed"}`
          : `${u.step ?? "starting"}…`}
      </div>
      {u.phase === "failed" && u.error && <pre className="mt-2 max-h-32 overflow-auto rounded-md bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">{u.error}</pre>}
      {u.phase !== "failed" && a.changes.length > 0 && (
        <ul className="mt-2 max-h-40 list-disc space-y-0.5 overflow-y-auto pl-4 text-xs">{a.changes.slice(0, 8).map((c) => <li key={c}>{c}</li>)}</ul>
      )}
      {u.phase === "ready" && (
        <>
          {up.sure && <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">{up.working} task{up.working === 1 ? " is" : "s are"} in progress: they stop with the restart and can be resumed.</p>}
          <Button className={cn("mt-3 w-full", phone && "h-11 text-base")} size="sm" onClick={up.restart}>{up.sure ? "Restart anyway" : "Restart to update"}</Button>
        </>
      )}
      {u.phase === "failed" && <Button className={cn("mt-3 w-full", phone && "h-11 text-base")} size="sm" variant="outline" onClick={up.retry}>Try again</Button>}
    </div>
  )
}

/** On a computer: a button at the rail's foot, above the spending ring,
    shown only while an update is on its way, ready or failed. */
export function RailUpdate({ working }: { working: number }) {
  const up = useUpdate(working)
  if (!up.shown) return null
  return (
    <Popover.Root>
      <Popover.Trigger className="grid size-10 place-items-center rounded-xl hover:bg-accent" aria-label={up.label} title={up.label}>
        <UpdateIcon up={up} className="size-[19px]" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner side="right" align="end" sideOffset={10}>
          <Popover.Popup className="w-80 rounded-2xl border bg-popover p-4 text-popover-foreground shadow-lg outline-none data-[ending-style]:opacity-0 data-[starting-style]:opacity-0 transition-opacity">
            <UpdatePanel up={up} />
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  )
}

/** On a phone: a pill floating at the top right, as before (the founder's
    choice: only the computer's moved to the rail). */
export function PhoneUpdate({ working }: { working: number }) {
  const up = useUpdate(working)
  if (!up.shown) return null
  if (up.restarting) return <span className="inline-flex h-11 items-center gap-1.5 px-4 text-sm text-muted-foreground"><Loader className="size-3.5 animate-spin" /> Restarting Airth…</span>
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<button className="inline-flex h-11 items-center gap-1.5 rounded-full px-4 text-sm font-medium hover:bg-accent" />}>
        <UpdateIcon up={up} className="size-4" /> {up.label}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-[min(24rem,calc(100vw-24px))] p-4">
        <UpdatePanel up={up} phone />
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
