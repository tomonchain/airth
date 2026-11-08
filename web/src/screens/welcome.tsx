import { useEffect, useMemo, useState } from "react"
import { ArrowLeft, ArrowRight, Check, ChevronDown, KeyRound, Loader } from "lucide-react"
import { Character } from "@/components/character"
import { createBusiness, slug } from "@/components/new-business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { api, enc, type Realm } from "@/lib/api"
import { cn } from "@/lib/utils"

// The first launch (decision 101): a model, the business, its team from the
// library, its goal. What Airth already has is not asked again.
const STEPS = ["hello", "model", "business", "team", "goal", "ready"] as const
type Step = (typeof STEPS)[number]

type TeamAgent = { name: string; title: string | null; department: string | null; accessory: string | null }
type Team = { id: string; title: string; description: string; agents: TeamAgent[]; routines: { name: string }[]; setup: string[]; installed: boolean }

// Where the legal calendar knows the rules first; any other country is
// written as its two letters, in the Company page.
const COUNTRIES: [string, string][] = [["GB", "United Kingdom"], ["FR", "France"], ["", "Elsewhere"]]

// A face that stays the same for a name, before the agent has its own look.
function look(name: string) {
  let h = 2166136261
  for (const c of name) h = Math.imul(h ^ c.charCodeAt(0), 16777619)
  return (h >>> 0).toString(16).padStart(8, "0").slice(0, 8)
}
const faceOf = (a: TeamAgent, working = false) => ({ look: look(a.name), department: a.department, accessory: a.accessory, state: working ? "working" as const : "idle" as const, title: a.title ?? a.name })

/** The first launch, one question at a time, beside the business it builds. */
export function Welcome({ realms, onDone }: { realms: Realm[]; onDone: (realm: string | null) => void }) {
  const [step, setStep] = useState<Step>("hello")
  const [ready, setReady] = useState<boolean | null>(null)
  const [realm, setRealm] = useState<string | null>(null)
  const [teams, setTeams] = useState<Team[] | null>(null)
  const [team, setTeam] = useState<string | null>(null)
  const [installed, setInstalled] = useState<Team | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [folder, setFolder] = useState(false)
  const [f, setF] = useState({ key: "", name: "", what: "", country: "GB", root: "", goal: "", by: "" })
  const set = (k: keyof typeof f) => (e: { target: { value: string } }) => setF((x) => ({ ...x, [k]: e.target.value }))
  const existing = realms.filter((r) => !r.archived)

  useEffect(() => {
    api<{ model_ready: boolean }>("setup").then((s) => setReady(s.model_ready), () => setReady(false))
  }, [])
  useEffect(() => {
    if (!realm || teams) return
    api<{ teams: Team[] }>(`realms/${enc(realm)}/library`).then((d) => setTeams(d.teams), () => setTeams([]))
  }, [realm, teams])

  // The steps this launch shows: a model already connected is not asked for.
  const shown = STEPS.filter((s) => s !== "model" || ready === false)
  const at = shown.indexOf(step)
  const go = (s: Step) => { setError(null); setStep(s) }
  const next = () => go(shown[Math.min(at + 1, shown.length - 1)])
  const back = () => go(shown[Math.max(at - 1, 0)])
  async function work(fn: () => Promise<void>) {
    setBusy(true); setError(null)
    try { await fn() } catch (e) { setError((e as Error).message) } finally { setBusy(false) }
  }
  function finish() {
    try { localStorage.setItem("airth-welcomed", "1") } catch { /* private window */ }
    onDone(realm ?? existing[0]?.name ?? null)
  }

  const id = slug(f.name.trim()).replace(/-+/g, "-").replace(/^-|-$/g, "")
  const chosen = teams?.find((t) => t.id === team) ?? null
  const crew = installed?.agents ?? chosen?.agents ?? []
  const name = f.name.trim() || realm || ""

  let title: React.ReactNode = "", lead: React.ReactNode = "", body: React.ReactNode = null, primary: React.ReactNode = null, aside: React.ReactNode = null

  if (step === "hello") {
    title = <>Your online company HQ.</>
    lead = "Give your business a staff of AI agents. They do the work; you set the goal and decide what commits. Three questions, about two minutes."
    primary = <Button size="lg" onClick={next}>Begin <ArrowRight /></Button>
    aside = existing.length > 0 && <Button variant="ghost" size="lg" onClick={() => onDone(existing[0].name)}>Skip, open {existing[0].name}</Button>
  } else if (step === "model") {
    title = "Connect a model"
    lead = <>Your agents think with a model you pay for directly, at its price. Paste an Anthropic API key, from <a className="text-sky-500 hover:underline" href="https://console.anthropic.com/settings/keys" target="_blank" rel="noreferrer">console.anthropic.com</a>.</>
    body = (
      <div className="space-y-3">
        <label className="flex h-12 items-center gap-3 rounded-xl border bg-surface px-4 focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30">
          <KeyRound className="size-4 text-muted-foreground" />
          <input type="password" autoComplete="off" spellCheck={false} value={f.key} onChange={set("key")} placeholder="sk-ant-…" aria-label="Anthropic API key" autoFocus
            className="flex-1 bg-transparent font-mono text-sm outline-none placeholder:text-muted-foreground" />
        </label>
        <p className="text-xs leading-relaxed text-muted-foreground">It stays on this Mac, and no model ever sees it. DeepSeek, OpenAI, Grok, Mistral, OpenRouter or a local model work too: skip, and choose one in Settings, Models.</p>
      </div>
    )
    aside = <Button variant="ghost" onClick={next}>Later</Button>
    primary = <Button disabled={busy || !f.key.trim()} onClick={() => work(async () => {
      const s = await api<{ model_ready: boolean }>("setup/key", { key: f.key.trim() })
      setReady(s.model_ready); setF((x) => ({ ...x, key: "" })); next()
    })}>{busy ? <><Loader className="animate-spin" /> Checking</> : <>Connect <ArrowRight /></>}</Button>
  } else if (step === "business") {
    if (realm) {
      title = `${name} has its headquarters`
      lead = "Its own folder, database, keys and team, apart from any other business."
      primary = <Button onClick={next}>Continue <ArrowRight /></Button>
    } else {
      title = "Your business"
      lead = "One company, one space. You can add others later."
      body = (
        <div className="space-y-5">
          <Labelled label="Its name" note={id && id !== f.name.trim() ? <>Its folder: <span className="font-mono">{id}</span></> : undefined}>
            <Input value={f.name} onChange={set("name")} placeholder="Northwind" autoFocus className="h-11 text-base" />
          </Labelled>
          <Labelled label="What it does" note="For whom, and how it sounds. Every agent reads it.">
            <Textarea rows={3} value={f.what} onChange={set("what")} className="text-[15px] leading-relaxed"
              placeholder="A research media on X for crypto allocators. Calm, sourced, never hype." />
          </Labelled>
          <Labelled group label="Where it is registered" note="Its legal calendar follows: filings, accounts, tax.">
            <div role="radiogroup" className="grid grid-cols-3 gap-2">
              {COUNTRIES.map(([code, label]) => (
                <button key={label} type="button" role="radio" aria-checked={f.country === code} onClick={() => setF((x) => ({ ...x, country: code }))}
                  className={cn("h-11 rounded-xl border text-sm transition-colors", f.country === code ? "border-foreground bg-foreground text-background" : "hover:bg-accent/60")}>{label}</button>
              ))}
            </div>
          </Labelled>
          {folder ? (
            <Labelled label="Its folder" note="Where its files and code already are. Empty: a private folder Airth makes.">
              <Input value={f.root} onChange={set("root")} placeholder="~/code/northwind" spellCheck={false} className="font-mono" />
            </Labelled>
          ) : (
            <button type="button" onClick={() => setFolder(true)} className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"><ChevronDown className="size-3.5" /> It already has a folder</button>
          )}
        </div>
      )
      aside = existing.length > 0 && <Button variant="ghost" onClick={() => { setRealm(existing[0].name); setF((x) => ({ ...x, name: x.name || existing[0].name })); next() }}>Use {existing[0].name}</Button>
      primary = <Button disabled={busy || !id} onClick={() => work(async () => {
        const r = await createBusiness({ name: id, what: f.what, root: f.root })
        await api(`realms/${enc(r)}/business`, { profile: { name: f.name.trim(), ...(f.country ? { country: f.country } : {}) } }, "PATCH")
        setRealm(r); next()
      })}>{busy ? <><Loader className="animate-spin" /> Creating</> : <>Create <ArrowRight /></>}</Button>
    }
  } else if (step === "team") {
    title = "Hire its team"
    lead = "A team from Airth's library: agents with a job, their routines and their prompts. You adjust them after."
    body = !teams ? <p className="text-sm text-muted-foreground"><Loader className="mr-1.5 inline size-4 animate-spin" />Opening the library…</p> : (
      <div className="space-y-2.5">
        {[...teams, null].map((t) => {
          const picked = t ? team === t.id : team === ""
          return (
            <button key={t?.id ?? "none"} type="button" onClick={() => setTeam(t?.id ?? "")} aria-pressed={picked}
              className={cn("flex w-full items-center gap-4 rounded-2xl border p-4 text-left transition-all", picked ? "border-foreground/70 bg-accent/60 shadow-sm" : "hover:bg-accent/40")}>
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-2 text-[15px] font-medium">{t ? t.title : "Start empty"}{t?.installed && <span className="text-xs font-normal text-muted-foreground">installed</span>}</span>
                <span className="mt-0.5 block text-[13px] leading-snug text-muted-foreground">{t ? t.description : "No agent yet: hire them one by one, from the Team page."}</span>
              </span>
              {t && (
                <span className="flex shrink-0 -space-x-2">
                  {t.agents.slice(0, 5).map((a) => <span key={a.name} className="rounded-full ring-2 ring-surface"><Character agent={faceOf(a)} size={30} still /></span>)}
                </span>
              )}
              <span className={cn("grid size-5 shrink-0 place-items-center rounded-full border", picked && "border-foreground bg-foreground text-background")}>{picked && <Check className="size-3" />}</span>
            </button>
          )
        })}
      </div>
    )
    primary = <Button disabled={busy || team === null || !realm} onClick={() => work(async () => {
      if (team) {
        const out = await api<{ team: Team }>(`realms/${enc(realm!)}/library/${enc(team)}/install`, {})
        setInstalled(out.team)
      }
      next()
    })}>{busy ? <><Loader className="animate-spin" /> Hiring</> : <>{team ? "Hire them" : "Continue"} <ArrowRight /></>}</Button>
  } else if (step === "goal") {
    title = "Its goal"
    lead = "What the team works toward. Every agent reads it; the weekly report measures it."
    body = (
      <div className="space-y-5">
        <Labelled label="The goal">
          <Input value={f.goal} onChange={set("goal")} placeholder="Reach 10,000 readers and 50 paying subscribers" autoFocus className="h-11 text-base" />
        </Labelled>
        <Labelled label="By" note="Optional.">
          <Input type="date" value={f.by} onChange={set("by")} className="h-11 w-48" />
        </Labelled>
      </div>
    )
    aside = <Button variant="ghost" onClick={next}>Later</Button>
    primary = <Button disabled={busy || !f.goal.trim() || !realm} onClick={() => work(async () => {
      await api(`realms/${enc(realm!)}/business`, { goal: { goal: f.goal.trim(), ...(f.by ? { by: f.by } : {}) } }, "PATCH")
      next()
    })}>Set the goal <ArrowRight /></Button>
  } else {
    title = <>{name ? `${name} is open.` : "Airth is ready."}</>
    lead = installed
      ? `${installed.agents.length} agents are on the team. Before their routines run, a few things are yours to set:`
      : "Hire agents from the Team page, or install a team from the Marketplace."
    body = installed && installed.setup.length > 0 && (
      <ol className="space-y-2.5">
        {installed.setup.slice(0, 4).map((s, n) => (
          <li key={n} className="flex gap-3 text-sm leading-relaxed">
            <span className="grid size-6 shrink-0 place-items-center rounded-full border text-xs text-muted-foreground tabular-nums">{n + 1}</span>
            <span className="text-foreground/85">{s}</span>
          </li>
        ))}
        {installed.setup.length > 4 && <li className="pl-9 text-xs text-muted-foreground">And {installed.setup.length - 4} more, on the Marketplace page.</li>}
      </ol>
    )
    primary = <Button size="lg" onClick={finish}>Open {name || "Airth"} <ArrowRight /></Button>
  }

  return (
    <div className="grid min-h-svh bg-surface lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <div className="flex min-h-svh flex-col px-6 pt-[max(env(safe-area-inset-top),24px)] pb-8 sm:px-12">
        <header className="flex items-center gap-3">
          <Ring className="size-6" />
          <span className="text-[15px] font-semibold tracking-tight">Airth</span>
          {step !== "hello" && step !== "ready" && (
            <ol className="ml-auto flex items-center gap-1.5" aria-label={`Step ${at} of ${shown.length - 2}`}>
              {shown.slice(1, -1).map((s) => {
                const n = shown.indexOf(s)
                return <li key={s} className={cn("h-1 rounded-full transition-all duration-500", n === at ? "w-7 bg-foreground" : n < at ? "w-3 bg-foreground/50" : "w-3 bg-muted")} />
              })}
            </ol>
          )}
        </header>
        <main key={`${step}${realm ? "+" : ""}`} className="mx-auto flex w-full max-w-md flex-1 flex-col justify-center py-10 fade-up">
          {step === "hello" && <Ring className="mb-8 size-14" />}
          <h1 className={cn("font-semibold tracking-tight text-balance", step === "hello" || step === "ready" ? "text-4xl sm:text-5xl" : "text-3xl")}>{title}</h1>
          <p className="mt-3 text-[15.5px] leading-relaxed text-pretty text-muted-foreground">{lead}</p>
          {body && <div className="mt-8">{body}</div>}
          {error && <p className="mt-5 rounded-lg bg-red-500/10 px-3 py-2 text-xs break-words text-red-600 dark:text-red-300">{error}</p>}
          <div className="mt-9 flex flex-wrap items-center gap-2">
            {at > 0 && step !== "ready" && <Button variant="ghost" size="icon" aria-label="Back" onClick={back}><ArrowLeft /></Button>}
            <span className="flex-1" />
            {aside}
            {primary}
          </div>
        </main>
      </div>
      <Stage step={step} name={name} what={f.what} country={f.country} crew={crew} goal={f.goal} by={f.by} />
    </div>
  )
}

/** The business as it is being built, beside the questions. */
function Stage({ step, name, what, country, crew, goal, by }: { step: Step; name: string; what: string; country: string; crew: TeamAgent[]; goal: string; by: string }) {
  const place = COUNTRIES.find(([c]) => c === country)?.[1]
  const past = (s: Step) => STEPS.indexOf(step) >= STEPS.indexOf(s)
  const when = useMemo(() => (by ? new Date(`${by}T12:00`).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" }) : null), [by])
  return (
    <aside aria-hidden className="relative hidden overflow-hidden border-l bg-background lg:block">
      <div className="absolute inset-0 bg-[radial-gradient(60%_50%_at_50%_40%,rgb(109_93_252/0.16),transparent_70%),radial-gradient(40%_40%_at_70%_60%,rgb(45_212_191/0.10),transparent_70%)]" />
      <div className="relative flex h-full items-center justify-center p-12">
        <div className="w-full max-w-sm rounded-3xl border bg-surface/90 p-7 shadow-[0_30px_80px_-30px_rgb(0_0_0/0.45)] backdrop-blur">
          <div className="flex items-center gap-3">
            <span className="size-9 shrink-0 rounded-full bg-[radial-gradient(circle_at_35%_30%,#d9f99d,#22c55e_55%,#14532d)] shadow-inner" />
            <div className="min-w-0">
              <div className={cn("truncate text-lg font-semibold tracking-tight transition-colors", !name && "text-muted-foreground/50")}>{name || "Your business"}</div>
              <div className="text-xs text-muted-foreground">{name && past("business") && place ? (country ? `Registered in ${place}` : "A company") : " "}</div>
            </div>
          </div>
          <p className={cn("mt-4 line-clamp-3 min-h-[3lh] text-[13px] leading-relaxed", what ? "text-foreground/80" : "text-muted-foreground/40")}>{what || "What it does, for whom, and how it sounds."}</p>
          <div className="mt-6 border-t pt-5">
            <div className="text-[11px] font-medium tracking-wider text-muted-foreground uppercase">Team</div>
            <div className="mt-3 flex min-h-12 flex-wrap gap-2.5">
              {crew.length ? crew.map((a, n) => (
                <span key={a.name} className="fade-up" style={{ animationDelay: `${n * 70}ms` }} title={a.title ?? a.name}>
                  <Character agent={faceOf(a, step === "ready")} size={44} still={step !== "ready"} />
                </span>
              )) : [0, 1, 2].map((n) => <span key={n} className="size-11 rounded-full border border-dashed" />)}
            </div>
          </div>
          <div className="mt-6 border-t pt-5">
            <div className="text-[11px] font-medium tracking-wider text-muted-foreground uppercase">Goal</div>
            <p className={cn("mt-2 text-[15px] font-medium tracking-tight", !goal && "text-muted-foreground/40")}>{goal || "Where the team is going"}</p>
            {when && <p className="mt-1 text-xs text-muted-foreground">By {when}</p>}
          </div>
        </div>
      </div>
    </aside>
  )
}

/** Airth's mark: the ring, in its gradient. */
function Ring({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 64" className={className} aria-hidden>
      <defs><linearGradient id="airth-ring" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#6d5dfc" /><stop offset=".55" stopColor="#3fb8ff" /><stop offset="1" stopColor="#2dd4bf" /></linearGradient></defs>
      <circle cx="32" cy="32" r="22" fill="none" stroke="url(#airth-ring)" strokeWidth="12" />
    </svg>
  )
}

/** A question: its label, its field, a note. A group of buttons is not
    wrapped in a label, which would pass clicks to its first button. */
function Labelled({ label, note, group, children }: { label: string; note?: React.ReactNode; group?: boolean; children: React.ReactNode }) {
  const Tag = group ? "div" : "label"
  return (
    <Tag className="block space-y-2" {...(group ? { role: "group", "aria-label": label } : {})}>
      <span className="block text-sm font-medium">{label}</span>
      {children}
      {note && <span className="block text-xs text-muted-foreground">{note}</span>}
    </Tag>
  )
}
