import { useEffect, useState } from "react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Textarea } from "@/components/ui/textarea"
import { api, DEPARTMENTS, enc, type Agent } from "@/lib/api"

/** Edit an agent: its title, its department and its job, written in its
    file in the business folder. Only the founder does; runs cannot. */
export function EditAgent({ realm, agent, open, onOpenChange, onSaved }: {
  realm: string; agent: Agent; open: boolean; onOpenChange: (open: boolean) => void; onSaved: () => void
}) {
  const [title, setTitle] = useState(agent.title)
  const [department, setDepartment] = useState(agent.department ?? "")
  const [job, setJob] = useState(agent.body || agent.job || "")
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (!open) return
    setTitle(agent.title); setDepartment(agent.department ?? ""); setJob(agent.body || agent.job || "")
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  const changed = title !== agent.title || department !== (agent.department ?? "") || job !== (agent.body || agent.job || "")

  async function save() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, { title, department, job }, "PUT")
      toast.success(`${title} is saved.`)
      onSaved()
      onOpenChange(false)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[85svh] flex-col gap-0 p-0 sm:max-w-[640px]">
        <DialogHeader className="border-b p-5">
          <div className="flex items-center gap-3">
            <Character agent={{ ...agent, department: department || null, title }} size={40} />
            <div>
              <DialogTitle>Edit {agent.title}</DialogTitle>
              <DialogDescription>Saved in agents/{agent.name}.md. Its routines and history stay.</DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-5">
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="agent-title">Title</Label>
              <Input id="agent-title" value={title} onChange={(e) => setTitle(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label>Department</Label>
              <Select value={department} onValueChange={(v) => setDepartment(String(v ?? ""))}>
                <SelectTrigger className="w-full capitalize"><SelectValue placeholder="Choose" /></SelectTrigger>
                <SelectContent>
                  {DEPARTMENTS.map((d) => <SelectItem key={d} value={d} className="capitalize">{d}</SelectItem>)}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex min-h-0 flex-1 flex-col space-y-1.5">
            <Label htmlFor="agent-job">Job</Label>
            <p className="text-xs text-muted-foreground">Its mission, how it works and what it reports. The agent reads this on every task.</p>
            <Textarea id="agent-job" value={job} onChange={(e) => setJob(e.target.value)} className="min-h-72 flex-1 font-mono text-[12.5px] leading-relaxed" />
          </div>
        </div>
        <DialogFooter className="m-0 flex-row justify-end rounded-b-xl border-t bg-muted/30 p-4">
          <Button variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button disabled={busy || !changed || !title.trim() || !job.trim() || !department} onClick={save}>{busy ? "Saving…" : "Save"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
