import { useState } from "react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { api, enc } from "@/lib/api"

const ISOLATION: [string, string][] = [
  ["auto", "Automatic: a worktree per task for git repositories"],
  ["none", "None: work directly in the folder"],
  ["worktree", "A worktree per task"],
]

/** Creates a business: its name, what it does (its profile, which every
    agent reads), its folder and how tasks work in it. Returns its name. */
export async function createBusiness(f: { name: string; what: string; root: string; isolation?: string }): Promise<string> {
  const what = f.what.trim()
  const r = await api<{ name: string }>("realms", {
    name: f.name.trim(), description: what.split("\n")[0].slice(0, 200), root: f.root.trim(),
    isolation: f.isolation && f.isolation !== "auto" ? f.isolation : null,
  })
  if (what) await api(`realms/${enc(r.name)}/profile`, { text: what }, "PUT")
  return r.name
}

export const slug = (v: string) => v.toLowerCase().replace(/[^a-z0-9_-]/g, "-")

/** A new business, from the business menu. */
export function NewBusiness({ open, onOpenChange, onCreated }: { open: boolean; onOpenChange: (open: boolean) => void; onCreated: (name: string) => void }) {
  const empty = { name: "", what: "", root: "", isolation: "auto" }
  const [f, setF] = useState(empty)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [was, setWas] = useState(open)
  if (open !== was) { setWas(open); if (open) { setF(empty); setError(null) } }

  async function create() {
    setBusy(true)
    setError(null)
    try {
      const name = await createBusiness(f)
      toast.success(`${name} is ready.`)
      onOpenChange(false)
      onCreated(name)
    } catch (e) { setError((e as Error).message) }
    finally { setBusy(false) }
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>New business</DialogTitle>
          <DialogDescription>Its own files, code, agents, connectors and rules. Nothing crosses between businesses.</DialogDescription>
        </DialogHeader>
        <div className="space-y-1.5">
          <Label htmlFor="nb-name">Name</Label>
          <Input id="nb-name" value={f.name} autoFocus spellCheck={false} autoComplete="off" placeholder="my-shop" className="font-mono" onChange={(e) => setF({ ...f, name: slug(e.target.value) })} />
          <p className="text-xs text-muted-foreground">Lowercase letters, digits and dashes.</p>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="nb-what">What it does</Label>
          <Textarea id="nb-what" rows={3} value={f.what} onChange={(e) => setF({ ...f, what: e.target.value })}
            placeholder="What the business does, for whom, and how it sounds." />
          <p className="text-xs text-muted-foreground">Every agent reads it. It is kept in business/profile.md.</p>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="nb-root">Folder</Label>
          <Input id="nb-root" value={f.root} spellCheck={false} placeholder="Empty: a private folder Airth creates, or e.g. ~/code/my-shop" className="font-mono" onChange={(e) => setF({ ...f, root: e.target.value })} />
          <p className="text-xs text-muted-foreground">Where its files and code are. Other businesses can never read it.</p>
        </div>
        <div className="space-y-1.5">
          <Label>How tasks work in it</Label>
          <Select value={f.isolation} onValueChange={(v) => setF({ ...f, isolation: String(v ?? "auto") })}>
            <SelectTrigger className="w-full"><SelectValue>{(v: string) => ISOLATION.find(([k]) => k === v)?.[1] ?? v}</SelectValue></SelectTrigger>
            <SelectContent>{ISOLATION.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
          </Select>
        </div>
        {error && <p className="rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300">{error}</p>}
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button disabled={busy || !/^[a-z0-9][a-z0-9_-]*$/.test(f.name)} onClick={create}>{busy ? "Creating…" : "Create the business"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
