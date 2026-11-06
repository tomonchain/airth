import { useEffect, useState } from "react"
import { ArrowRight, Check, Compass, Plug, Repeat, Smartphone, Sparkles, Users, Wallet } from "lucide-react"
import { Character } from "@/components/character"
import { face, TEMPLATES, type Template } from "@/components/hire-agent"
import { createBusiness, slug } from "@/components/new-business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { api, enc, type Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

const STEPS = ["hello", "you", "model", "business", "strategy", "agent", "routine", "done"] as const
type Step = (typeof STEPS)[number]

// Each job's first routine, as the classic interface offered it.
const FIRST: Record<string, { task: string; every: string }> = {
  writer: { task: "Write and prepare this week's article.", every: "mon 08:00" },
  analyst: { task: "Watch what changed and report what matters.", every: "weekdays 08:00" },
  accountant: { task: "Sum up the week's money in and out.", every: "fri 17:00" },
  support: { task: "Answer the new customer messages.", every: "every 2h" },
  sales: { task: "Find five new leads and draft a first message to each.", every: "weekdays 10:00" },
}
const WHEN: [string, string][] = [
  ["mon 08:00", "Every Monday at 08:00"], ["weekdays 09:00", "Every weekday at 09:00"], ["daily 07:00", "Every day at 07:00"],
  ["fri 17:00", "Every Friday at 17:00"], ["weekdays 08:00", "Every weekday at 08:00"], ["weekdays 10:00", "Every weekday at 10:00"],
  ["every 2h", "Every 2 hours"], ["manual", "Only when I start it"],
]

/** The first launch, one step at a time (the classic interface's): what
    Airth is, the founder, a model, then a business with its strategy, a
    first agent and its first routine. Every step can be skipped. */
export function Welcome({ realms, onDone }: { realms: Realm[]; onDone: (realm: string | null) => void }) {
  const [i, setI] = useState(0)
  const step: Step = STEPS[i]
  const [ready, setReady] = useState<boolean | null>(null)
  const [realm, setRealm] = useState<string | null>(null)
  const [agent, setAgent] = useState<string | null>(null)
  const [t, setT] = useState<Template | null>(null)
  const [base, setBase] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [f, setF] = useState({ name: "", about: "", key: "", business: "", what: "", root: "", strategy: "", task: "", every: "mon 08:00" })
  const set = (k: keyof typeof f) => (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value })
  const existing = realms.filter((r) => !r.archived)

  useEffect(() => {
    api<{ model_ready: boolean }>("setup").then((s) => setReady(s.model_ready), () => setReady(false))
    api<{ name: string; about: string }>("owner").then((o) => setF((x) => ({ ...x, name: x.name || o.name || "", about: x.about || o.about || "" })), () => {})
  }, [])
  useEffect(() => {
    if (step !== "strategy" || !realm) return
    api<{ strategy: string }>(`realms/${enc(realm)}/steering`).then((d) => setF((x) => ({ ...x, strategy: x.strategy || d.strategy || "" })), () => {})
  }, [step, realm])

  const go = (to: Step | number) => { setError(null); setI(typeof to === "number" ? to : STEPS.indexOf(to)) }
  const next = () => go(Math.min(i + 1, STEPS.length - 1))
  async function work(fn: () => Promise<void>) {
    setBusy(true)
    setError(null)
    try { await fn() } catch (e) { setError((e as Error).message) } finally { setBusy(false) }
  }
  function finish() {
    try { localStorage.setItem("airth-welcomed", "1") } catch { /* private window */ }
    onDone(realm ?? existing[0]?.name ?? null)
  }
  // The routine step needs the agent just hired.
  const shown: Step = step === "routine" && !agent ? "done" : step

  let title = "", lead: React.ReactNode = "", body: React.ReactNode = null, primary: React.ReactNode = null, aside: React.ReactNode = null
  const skip = (label = "Skip", to?: Step) => <Button variant="ghost" onClick={() => (to ? go(to) : next())}>{label}</Button>

  if (shown === "hello") {
    title = "Run your business with a team of agents"
    lead = "Airth works on your Mac. Agents with a job do the work; you set the direction and decide what matters."
    body = (
      <div className="space-y-3">
        <Point icon={Compass} title="You steer">A strategy every agent follows, and the decisions that wait for you.</Point>
        <Point icon={Users} title="Agents work">Each has a job, its own tools and limits, and works on a schedule.</Point>
        <Point icon={Wallet} title="You see the cost">What each agent did, and what running the business costs.</Point>
        <p className="pt-1 text-xs text-muted-foreground">About five minutes. Every step can be changed later.</p>
      </div>
    )
    primary = <Button onClick={next}>Get started <ArrowRight /></Button>
  } else if (shown === "you") {
    title = "A little about you"
    lead = "Agents write and decide the way you would. This stays on this Mac."
    body = (
      <div className="space-y-4">
        <Field id="w-name" label="Name"><Input id="w-name" value={f.name} onChange={set("name")} placeholder="Your first name" autoFocus /></Field>
        <Field id="w-about" label="Who you are"><Textarea id="w-about" rows={2} value={f.about} onChange={set("about")} placeholder="e.g. Founder of a small research media. I write in French." /></Field>
      </div>
    )
    aside = skip()
    primary = <Button disabled={busy} onClick={() => work(async () => {
      if (f.name.trim() || f.about.trim()) {
        const owner = await api<Record<string, unknown> & { name: string; about: string }>("owner")
        await api("owner", { ...owner, name: f.name.trim() || owner.name, about: f.about.trim() || owner.about }, "PUT")
      }
      next()
    })}>Continue</Button>
  } else if (shown === "model") {
    if (ready) {
      title = "A model is ready"
      lead = <><Check className="inline size-4 text-emerald-500" /> Agents can think. You can change the model, or add a cheaper fast one, in Settings → Models.</>
      primary = <Button onClick={next}>Continue</Button>
    } else {
      title = "Connect a model"
      lead = <>Agents think with a model you pay for directly. Paste an Anthropic API key, from <a className="text-sky-500 hover:underline" href="https://console.anthropic.com/settings/keys" target="_blank" rel="noreferrer">console.anthropic.com</a>.</>
      body = (
        <div className="space-y-2">
          <Field id="w-key" label="Anthropic API key">
            <Input id="w-key" type="password" autoComplete="off" spellCheck={false} value={f.key} onChange={set("key")} placeholder="sk-ant-…" className="font-mono" autoFocus />
          </Field>
          <p className="text-xs text-muted-foreground">It stays on this Mac, readable by you alone; the model never sees it. Prefer DeepSeek, OpenAI, Grok, Mistral, OpenRouter or a local model? Skip, and pick one in Settings → Models.</p>
        </div>
      )
      aside = skip("Later")
      primary = <Button disabled={busy || !f.key.trim()} onClick={() => work(async () => {
        const s = await api<{ model_ready: boolean }>("setup/key", { key: f.key.trim() })
        setReady(s.model_ready)
        setF((x) => ({ ...x, key: "" }))
        next()
      })}>{busy ? "Checking…" : "Check and save"}</Button>
    }
  } else if (shown === "business") {
    if (realm) {
      title = `${realm} is ready`
      lead = "Its own folder, rules, agents and connectors, isolated from any other business."
      primary = <Button onClick={next}>Continue</Button>
    } else {
      title = "Your business"
      lead = "One business, one space: its files, code, agents and connectors. Add others later."
      body = (
        <div className="space-y-4">
          <Field id="w-biz" label="Name" note="Lowercase letters, digits and dashes.">
            <Input id="w-biz" value={f.business} onChange={(e) => setF({ ...f, business: slug(e.target.value) })} placeholder="my-shop" spellCheck={false} autoComplete="off" className="font-mono" autoFocus />
          </Field>
          <Field id="w-what" label="What it does" note="Every agent reads it. You can change it in its business/profile.md.">
            <Textarea id="w-what" rows={4} value={f.what} onChange={set("what")} placeholder={"What the business does, for whom, and how it sounds.\ne.g. A finance research media on X, for informed retail investors. Calm, sourced, never hype."} />
          </Field>
          <Field id="w-root" label="Its folder" note="Where its files and code are. Empty: a private folder Airth creates.">
            <Input id="w-root" value={f.root} onChange={set("root")} placeholder="Optional: ~/code/my-shop" spellCheck={false} className="font-mono" />
          </Field>
        </div>
      )
      aside = existing.length > 0 && <Button variant="ghost" onClick={() => { setRealm(existing[0].name); next() }}>Use {existing[0].name}</Button>
      primary = <Button disabled={busy || !/^[a-z0-9][a-z0-9_-]*$/.test(f.business)} onClick={() => work(async () => {
        setRealm(await createBusiness({ name: f.business, what: f.what, root: f.root }))
        next()
      })}>{busy ? "Creating…" : "Create the business"}</Button>
    }
  } else if (shown === "strategy") {
    title = "Set the direction"
    lead = "Every agent reads it before it works. You change it any time, on the Company page."
    body = <Textarea aria-label="Strategy" rows={8} value={f.strategy} onChange={set("strategy")} autoFocus className="text-sm leading-relaxed"
      placeholder={"Your goals, the line every agent follows, and what the team may do alone.\n\n- One article a week on the blog.\n- Explain, never hype. Facts and numbers first.\n- Ask me before publishing or spending money."} />
    aside = skip()
    primary = <Button disabled={busy || !realm} onClick={() => work(async () => {
      if (f.strategy.trim()) await api(`realms/${enc(realm!)}/strategy`, { text: f.strategy }, "PUT")
      next()
    })}>Save the strategy</Button>
  } else if (shown === "agent") {
    if (agent) {
      title = `${t?.title ?? agent} is on the team`
      lead = "It works when a routine says so, or when you give it a task. Give it a routine now."
      primary = <Button onClick={next}>Continue</Button>
    } else if (t) {
      title = t.title ? `Hire a ${t.title.toLowerCase()}` : "Hire an agent"
      lead = "Its job, in your words. Adjust it to your business."
      body = (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <Field id="w-an" label="Name"><Input id="w-an" value={t.name} onChange={(e) => setT({ ...t, name: slug(e.target.value) })} spellCheck={false} className="font-mono" /></Field>
            <Field id="w-at" label="Title"><Input id="w-at" value={t.title} onChange={(e) => setT({ ...t, title: e.target.value })} /></Field>
          </div>
          <Field id="w-job" label="Job"><Textarea id="w-job" rows={10} value={t.job} onChange={(e) => setT({ ...t, job: e.target.value })} className="font-mono text-[12.5px] leading-relaxed" /></Field>
        </div>
      )
      aside = <Button variant="ghost" onClick={() => setT(null)}>Another one</Button>
      primary = <Button disabled={busy || !realm || !/^[a-z0-9][a-z0-9_-]*$/.test(t.name) || !t.title.trim() || !t.job.trim()} onClick={() => work(async () => {
        await api(`realms/${enc(realm!)}/agents/${enc(t.name)}`, { job: t.job, title: t.title.trim(), department: t.department || "operations" }, "PUT")
        const first = FIRST[base] ?? { task: "", every: "mon 08:00" }
        setF((x) => ({ ...x, task: first.task, every: first.every }))
        setAgent(t.name)
      })}>{busy ? "Hiring…" : "Hire"}</Button>
    } else {
      title = "Your first agent"
      lead = "Pick a job to start from. You can hire more later, at the foot of the agents list."
      body = (
        <div className="grid gap-2 sm:grid-cols-2">
          {TEMPLATES.map((x) => (
            <button key={x.name} disabled={!realm} onClick={() => { setT({ ...x }); setBase(x.name) }} className="flex items-center gap-3 rounded-xl border p-3 text-left transition-colors enabled:hover:bg-accent/60 disabled:opacity-50">
              <Character agent={face(x)} size={34} still />
              <span className="min-w-0"><span className="block text-sm font-medium">{x.title}</span><span className="block text-xs text-muted-foreground">{x.desc}</span></span>
            </button>
          ))}
        </div>
      )
      aside = skip("Later", "done")
    }
  } else if (shown === "routine") {
    title = `When ${agent} works`
    lead = "What it does each time, and when. Anything it may not do alone waits for you in Needs you."
    const when = WHEN.some(([v]) => v === f.every) ? WHEN : [...WHEN, [f.every, f.every] as [string, string]]
    body = (
      <div className="space-y-4">
        <Field id="w-task" label="Each time"><Textarea id="w-task" rows={2} value={f.task} onChange={set("task")} autoFocus /></Field>
        <div className="space-y-1.5">
          <Label>When</Label>
          <Select value={f.every} onValueChange={(v) => setF({ ...f, every: String(v ?? "manual") })}>
            <SelectTrigger className="w-full"><SelectValue>{(v: string) => when.find(([k]) => k === v)?.[1] ?? v}</SelectValue></SelectTrigger>
            <SelectContent>{when.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
          </Select>
        </div>
      </div>
    )
    aside = skip("Later")
    primary = <Button disabled={busy || !f.task.trim()} onClick={() => work(async () => {
      await api(`realms/${enc(realm!)}/routines`, { id: `${agent}-routine`, objective: f.task, bot: agent, every: f.every === "manual" ? null : f.every, tools: null, allow: [] })
      next()
    })}>Create the routine</Button>
  } else {
    title = "You're set"
    lead = realm ? `${realm} is ready to run.` : "Airth is ready."
    body = (
      <div className="space-y-3">
        <Point icon={Sparkles} title="Home and Needs you">What the team is doing, and what waits for your decision.</Point>
        <Point icon={Repeat} title="Routines">Who works when. Run one now to see it work.</Point>
        <Point icon={Plug} title="Marketplace">Connect the business's services and give each agent only what it needs.</Point>
        <Point icon={Smartphone} title="Your phone">Approve from anywhere: Settings → Phone.</Point>
      </div>
    )
    primary = <Button onClick={finish}>Open {realm ?? "Airth"} <ArrowRight /></Button>
  }

  return (
    <div className="grid min-h-svh place-items-center bg-background px-4 py-10">
      <div className="w-full max-w-xl">
        <ol className="mb-6 flex items-center justify-center gap-1.5" aria-label="Steps">
          {STEPS.map((s, n) => <li key={s} aria-current={n === i ? "step" : undefined} className={cn("h-1.5 rounded-full transition-all", n === i ? "w-6 bg-foreground" : n < i ? "w-1.5 bg-foreground/50" : "w-1.5 bg-muted")} />)}
        </ol>
        <div key={`${shown}${agent ?? ""}${t?.name ?? ""}`} className="rounded-2xl border bg-surface p-7 shadow-sm fade-up">
          <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
          <p className="mt-2 text-[15px] text-foreground/80">{lead}</p>
          {body && <div className="mt-6">{body}</div>}
          {error && <p className="mt-4 rounded-lg bg-red-500/10 px-3 py-2 text-xs break-words text-red-600 dark:text-red-300">{error}</p>}
          <div className="mt-7 flex items-center gap-2">
            {i > 0 && shown !== "done" ? <Button variant="ghost" onClick={() => go(i - 1)}>Back</Button> : null}
            <span className="flex-1" />
            {aside}
            {primary}
          </div>
        </div>
      </div>
    </div>
  )
}

function Point({ icon: Icon, title, children }: { icon: React.ComponentType<{ className?: string }>; title: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3">
      <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-muted"><Icon className="size-4" /></span>
      <div><div className="text-sm font-medium">{title}</div><div className="text-sm text-muted-foreground">{children}</div></div>
    </div>
  )
}

function Field({ id, label, note, children }: { id: string; label: string; note?: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {note && <p className="text-xs text-muted-foreground">{note}</p>}
    </div>
  )
}
