import { useState } from "react"
import { ArrowLeft, PenLine } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { api, DEPARTMENTS, enc, type Agent } from "@/lib/api"

type Template = { name: string; title: string; department: string; desc: string; job: string }

/** Jobs to start from, by business function (the classic interface's). The
    founder edits them before hiring. */
const TEMPLATES: Template[] = [
  { name: "writer", title: "Writer", department: "marketing", desc: "Articles, posts and threads in the business's voice.", job:
`Write the business's articles and posts, in its voice (see the profile).

Each time:
1. Pick one subject that serves the strategy: news worth explaining, or a lasting notion your readers need.
2. Research it from reliable sources. Note every source you use.
3. Write it: facts and numbers first, plain words, no hype.
4. Prepare it where it is published, as a draft. Never publish without the owner's approval.

In your report: the subject and why, the sources, and what the owner must decide.` },
  { name: "analyst", title: "Analyst", department: "operations", desc: "Watches markets, competitors or metrics, and reports.", job:
`Watch what matters to the business (markets, competitors, its own numbers) and report what changed.

Each time:
1. Read the sources you were given, and nothing you were not.
2. Keep what changes the picture for the business; leave out the noise.
3. Say what it means and what you would do about it.

In your report: the few things that matter, with their source, and one recommendation.` },
  { name: "accountant", title: "Accountant", department: "finance", desc: "Reads the bank and the invoices. Never pays.", job:
`Keep the business's books readable for the owner.

Each time:
1. Read what came in and what went out since the last time.
2. Match it to invoices and subscriptions; flag what you cannot match.
3. Never move money, pay, or change anything at the bank.

In your report: money in, money out, what the team cost, and what looks wrong.` },
  { name: "support", title: "Support", department: "support", desc: "Answers customers within the rules you set.", job:
`Answer the business's customers, within the rules in the strategy.

Each time:
1. Read the new messages.
2. Draft an answer for each: clear, polite, short.
3. Anything about money (refunds, prices, discounts) or a complaint waits for the owner.

In your report: what came in, what you answered, what waits for the owner.` },
  { name: "sales", title: "Sales", department: "sales", desc: "Finds leads and drafts outreach for you to send.", job:
`Find people the business could serve, and prepare the first message to them.

Each time:
1. Look for leads that fit the strategy's target.
2. For each, note why they fit and where you found them.
3. Draft a short, personal first message. Never send it: the owner does.

In your report: the leads, why each one, and the drafts.` },
]

const OWN: Template = { name: "", title: "", department: "", desc: "Write its job from scratch.", job: "" }

// What a template's agent looks like before it exists.
const face = (t: Template): Agent => ({ name: t.name || "new", title: t.title || "New agent", department: t.department || null, state: "asleep" } as Agent)

/** Hire an agent (decision 36): start from a job, change it, and it joins
    the team as a file in agents/. It works once a routine or a task names it. */
export function HireAgent({ realm, taken, open, onOpenChange, onHired }: {
  realm: string; taken: string[]; open: boolean; onOpenChange: (open: boolean) => void; onHired: (name: string) => void
}) {
  const [t, setT] = useState<Template | null>(null)
  const [busy, setBusy] = useState(false)
  const [was, setWas] = useState(open)
  if (open !== was) { setWas(open); if (open) setT(null) }

  const pick = (x: Template) => {
    // A second writer gets a name of its own.
    let name = x.name
    for (let i = 2; name && taken.includes(name); i++) name = `${x.name}-${i}`
    setT({ ...x, name })
  }
  const clash = !!t && taken.includes(t.name)
  const valid = !!t && /^[a-z0-9][a-z0-9_-]*$/.test(t.name) && !clash && !!t.title.trim() && !!t.department && !!t.job.trim()

  async function hire() {
    if (!t || !valid) return
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/agents/${enc(t.name)}`, { job: t.job, title: t.title.trim(), department: t.department }, "PUT")
      toast.success(`${t.title} joined the team. Give it a routine, or a task, for it to work.`)
      onOpenChange(false)
      onHired(t.name)
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[88svh] flex-col gap-0 p-0 sm:max-w-[640px]">
        <DialogHeader className="border-b p-5">
          <div className="flex items-center gap-3">
            {t && <Button size="icon-sm" variant="ghost" aria-label="Back" onClick={() => setT(null)}><ArrowLeft /></Button>}
            {t && <Character agent={face(t)} size={40} />}
            <div>
              <DialogTitle>{t ? (t.title ? `Hire a ${t.title.toLowerCase()}` : "Hire an agent") : "Hire an agent"}</DialogTitle>
              <DialogDescription>{t ? `Saved in agents/${t.name || "…"}.md. It reads the strategy and the profile too.` : "Start from a job and make it yours, or write one."}</DialogDescription>
            </div>
          </div>
        </DialogHeader>

        {!t ? (
          <div className="grid gap-2 overflow-y-auto p-5 sm:grid-cols-2">
            {TEMPLATES.map((x) => (
              <button key={x.name} onClick={() => pick(x)} className="flex items-center gap-3 rounded-xl border p-3 text-left transition-colors hover:bg-accent/60">
                <Character agent={face(x)} size={36} still />
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{x.title}</span>
                  <span className="block text-xs text-muted-foreground">{x.desc}</span>
                </span>
              </button>
            ))}
            <button onClick={() => pick(OWN)} className="flex items-center gap-3 rounded-xl border border-dashed p-3 text-left transition-colors hover:bg-accent/60">
              <span className="grid size-9 place-items-center rounded-full bg-muted"><PenLine className="size-4 text-muted-foreground" /></span>
              <span className="min-w-0">
                <span className="block text-sm font-medium">Your own</span>
                <span className="block text-xs text-muted-foreground">{OWN.desc}</span>
              </span>
            </button>
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
            <div className="grid grid-cols-3 gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="hire-name">Name</Label>
                <Input id="hire-name" value={t.name} spellCheck={false} autoComplete="off" placeholder="writer" className="font-mono" aria-invalid={clash}
                  onChange={(e) => setT({ ...t, name: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-") })} />
                {clash && <p className="text-xs text-red-500">Already in the team.</p>}
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="hire-title">Title</Label>
                <Input id="hire-title" value={t.title} placeholder="Writer" onChange={(e) => setT({ ...t, title: e.target.value })} />
              </div>
              <div className="space-y-1.5">
                <Label>Department</Label>
                <Select value={t.department} onValueChange={(v) => setT({ ...t, department: String(v ?? "") })}>
                  <SelectTrigger className="w-full capitalize"><SelectValue placeholder="Choose" /></SelectTrigger>
                  <SelectContent>{DEPARTMENTS.map((d) => <SelectItem key={d} value={d} className="capitalize">{d}</SelectItem>)}</SelectContent>
                </Select>
              </div>
            </div>
            <div className="flex min-h-0 flex-1 flex-col space-y-1.5">
              <Label htmlFor="hire-job">Job</Label>
              <Textarea id="hire-job" value={t.job} onChange={(e) => setT({ ...t, job: e.target.value })} autoFocus={!t.job}
                placeholder="What it is for and how it works, in your words." className="min-h-64 flex-1 font-mono text-[12.5px] leading-relaxed" />
            </div>
          </div>
        )}

        {t && (
          <DialogFooter className="m-0 flex-row justify-end rounded-b-xl border-t bg-muted/30 p-4">
            <Button variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
            <Button disabled={busy || !valid} onClick={hire}>{busy ? "Hiring…" : "Hire"}</Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  )
}
