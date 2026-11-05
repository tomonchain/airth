import { useEffect, useState } from "react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { DepartmentField } from "@/components/department-field"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Textarea } from "@/components/ui/textarea"
import { useTools } from "@/hooks/use-tools"
import { api, enc, type Agent, type Department } from "@/lib/api"
import { LOGOS } from "@/lib/logos"
import { cn } from "@/lib/utils"

/** Edit an agent: its title, its department and its job, written in its
    file in the business folder. Only the founder does; runs cannot. */
export function EditAgent({ realm, agent, departments, open, onOpenChange, onSaved, onGone }: {
  realm: string; agent: Agent; departments: Department[]; open: boolean; onOpenChange: (open: boolean) => void; onSaved: () => void; onGone: () => void
}) {
  const [title, setTitle] = useState(agent.title)
  const [department, setDepartment] = useState(agent.department ?? "")
  const [job, setJob] = useState(agent.body || agent.job || "")
  const [busy, setBusy] = useState(false)
  const [leaving, setLeaving] = useState(false)
  const tools = useTools(realm, open)
  const [integrations, setIntegrations] = useState<string[]>(agent.integrations ?? [])
  const [skills, setSkills] = useState<string[]>(agent.skills ?? [])
  useEffect(() => {
    if (!open) return
    setTitle(agent.title); setDepartment(agent.department ?? ""); setJob(agent.body || agent.job || "")
    setIntegrations(agent.integrations ?? []); setSkills(agent.skills ?? []); setLeaving(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  const same = (a: string[], b: string[]) => a.length === b.length && a.every((x) => b.includes(x))
  const changed = title !== agent.title || department !== (agent.department ?? "") || job !== (agent.body || agent.job || "")
    || !same(integrations, agent.integrations ?? []) || !same(skills, agent.skills ?? [])

  async function save() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, { title, department, job, integrations, skills }, "PUT")
      toast.success(`${title} is saved.`)
      onSaved()
      onOpenChange(false)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  // Its file leaves agents/; its past tasks stay. The server refuses while
  // a routine still names it, and says which.
  async function letGo() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(agent.name)}`, undefined, "DELETE")
      toast.success(`${agent.title} left the team.`)
      onOpenChange(false)
      onGone()
    } catch (e) {
      toast.error((e as Error).message)
      setLeaving(false)
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
              <Label htmlFor="agent-department">Department</Label>
              <DepartmentField id="agent-department" value={department} departments={departments} onChange={setDepartment} />
            </div>
          </div>
          <div className="space-y-2">
            <Label>What its tasks may use</Label>
            <p className="text-xs text-muted-foreground">Nothing else: without any, it only reads the public web. Connect more in the Marketplace.</p>
            {!tools.data ? <p className="text-xs text-muted-foreground">Loading…</p> : (
              <div className="flex flex-wrap gap-1.5">
                {tools.data.integrations.map((n) => <Toggle key={`i${n}`} label={n} on={integrations.includes(n)} onClick={() => setIntegrations((l) => flip(l, n))} />)}
                {tools.data.servers.map((m) => <Toggle key={`m${m.name}`} label={m.name} hint={m.description} on={skills.includes(m.name)} onClick={() => setSkills((l) => flip(l, m.name))} />)}
                {tools.data.skills.map((n) => <Toggle key={`s${n}`} label={n} skill on={skills.includes(n)} onClick={() => setSkills((l) => flip(l, n))} />)}
                {!tools.data.integrations.length && !tools.data.servers.length && !tools.data.skills.length && <span className="text-xs text-muted-foreground">This business has no connector or skill yet.</span>}
              </div>
            )}
          </div>
          <div className="flex min-h-0 flex-1 flex-col space-y-1.5">
            <Label htmlFor="agent-job">Job</Label>
            <p className="text-xs text-muted-foreground">Its mission, how it works and what it reports. The agent reads this on every task.</p>
            <Textarea id="agent-job" value={job} onChange={(e) => setJob(e.target.value)} className="min-h-72 flex-1 font-mono text-[12.5px] leading-relaxed" />
          </div>
        </div>
        <DialogFooter className="m-0 flex-row justify-end rounded-b-xl border-t bg-muted/30 p-4">
          {leaving ? (
            <span className="mr-auto flex items-center gap-1">
              <Button variant="destructive" disabled={busy} onClick={letGo}>Let {agent.title} go</Button>
              <Button variant="ghost" onClick={() => setLeaving(false)}>Keep</Button>
            </span>
          ) : <Button variant="ghost" className="mr-auto text-muted-foreground hover:text-red-500" onClick={() => setLeaving(true)}>Let go</Button>}
          <Button variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button disabled={busy || !changed || !title.trim() || !job.trim() || !department} onClick={save}>{busy ? "Saving…" : "Save"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

const flip = (l: string[], n: string) => (l.includes(n) ? l.filter((x) => x !== n) : [...l, n])

function Toggle({ label, hint, on, skill, onClick }: { label: string; hint?: string; on: boolean; skill?: boolean; onClick: () => void }) {
  const mark = LOGOS[label]
  return (
    <button type="button" onClick={onClick} title={hint} aria-pressed={on}
      className={cn("inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs transition-colors",
        on ? "border-sky-500/60 bg-sky-500/10 text-foreground" : "text-muted-foreground hover:text-foreground")}>
      {mark ? <svg viewBox="0 0 24 24" className="size-3.5" style={{ color: mark[0] ?? "currentColor" }}><path fill="currentColor" d={mark[1]} /></svg>
        : skill ? <span className="text-[10px]">✦</span> : null}
      {label}
    </button>
  )
}
