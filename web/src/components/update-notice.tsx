import { useState } from "react"
import { ArrowUpCircle, Loader, TriangleAlert } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { usePoll } from "@/hooks/use-poll"
import { api } from "@/lib/api"

type Info = {
  version: string
  update: { available: { short: string; changes: string[]; count: number; release: boolean } | null; phase: string; step: string | null; error: string | null }
}

/** A newer Airth, prepared and checked in the background (the classic
    interface's notice): shown in the title bar, installed on a restart. */
export function UpdateNotice({ working }: { working: number }) {
  const { data, refresh } = usePoll<Info>(() => api("info"), 60000, [])
  const [sure, setSure] = useState(false)
  const [restarting, setRestarting] = useState(false)
  const u = data?.update
  const a = u?.available
  if (restarting) return <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground"><Loader className="size-3.5 animate-spin" /> Restarting Airth…</span>
  if (!a || !u || !["ready", "preparing", "checking", "failed"].includes(u.phase)) return null

  async function restart() {
    // Tasks in progress stop with the restart; they can be resumed.
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

  const what = `${a.count} change${a.count === 1 ? "" : "s"}`
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<button className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-xs font-medium hover:bg-accent" />}>
        {u.phase === "ready" ? <><ArrowUpCircle className="size-4 text-sky-500" /> Update ready</>
          : u.phase === "failed" ? <><TriangleAlert className="size-4 text-amber-500" /> Update not installed</>
          : <><Loader className="size-3.5 animate-spin text-muted-foreground" /> Preparing an update</>}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80 p-3">
        <div className="text-sm font-medium">{u.phase === "failed" ? `Update ${a.short} not installed` : u.phase === "ready" ? "A new version of Airth" : `Preparing ${a.short}`}</div>
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
            {sure && <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">{working} task{working === 1 ? " is" : "s are"} in progress: they stop with the restart and can be resumed.</p>}
            <Button className="mt-3 w-full" size="sm" onClick={restart}>{sure ? "Restart anyway" : "Restart to update"}</Button>
          </>
        )}
        {u.phase === "failed" && <Button className="mt-3 w-full" size="sm" variant="outline" onClick={() => api("update/check", {}).then(refresh, (e: Error) => toast.error(e.message))}>Try again</Button>}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
