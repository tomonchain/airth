import { useEffect, useMemo, useState } from "react"
import { ArrowLeft, ArrowRight, Check, ChevronDown, KeyRound, Loader, Search, X } from "lucide-react"
import { Character } from "@/components/character"
import { createBusiness, ISOLATION, slug } from "@/components/new-business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { api, enc, type Realm } from "@/lib/api"
import { Connectors } from "@/screens/marketplace"
import { cn } from "@/lib/utils"

// The first launch (decision 101), and a new business from the business
// menu (decision 126): a model, the business (filled from its register when
// it has a number), its team from the library, its goal, the connectors it
// needs. What Airth already has is not asked again.
const STEPS = ["hello", "model", "business", "team", "goal", "connectors", "ready"] as const
type Step = (typeof STEPS)[number]

type TeamAgent = { name: string; title: string | null; department: string | null; accessory: string | null }
type Team = { id: string; title: string; description: string; agents: TeamAgent[]; routines: { name: string }[]; setup: string[]; connectors?: string[]; installed: boolean }

/** What a register says (decision 126), in the profile's own fields. */
type Record = {
  name: string; legal_name: string; legal_form: string | null; country: string; registration_number: string
  registered_office: string | null; incorporated_on: string | null; fiscal_year_end: string | null
  vat_number: string | null; currency: string; active: boolean; activity: string | null
}
// The fields shown for the founder to check, in the profile's words.
const FIELDS: [keyof Record, string, string][] = [
  ["legal_name", "Legal name", ""], ["legal_form", "Legal form", "ltd, sas…"], ["registration_number", "Number", ""],
  ["registered_office", "Registered office", ""], ["incorporated_on", "Incorporated", "YYYY-MM-DD"],
  ["fiscal_year_end", "Year end", "MM-DD, like 03-31"], ["vat_number", "VAT number", "None yet"],
]
const REGISTER: { [country: string]: { name: string; number: string; example: string } } = {
  GB: { name: "Companies House", number: "Company number", example: "16838911" },
  FR: { name: "the company directory", number: "SIREN or SIRET", example: "552032534" },
}

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
export function Welcome({ realms, onDone, fresh, onCancel }: { realms: Realm[]; onDone: (realm: string | null) => void; fresh?: boolean; onCancel?: () => void }) {
  const [step, setStep] = useState<Step>(fresh ? "business" : "hello")
  const [ready, setReady] = useState<boolean | null>(null)
  const [realm, setRealm] = useState<string | null>(null)
  const [teams, setTeams] = useState<Team[] | null>(null)
  const [team, setTeam] = useState<string | null>(null)
  const [installed, setInstalled] = useState<Team | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [folder, setFolder] = useState(false)
  const [rec, setRec] = useState<Record | null>(null)
  const [number, setNumber] = useState("")
  const [planned, setPlanned] = useState(false)
  const [gbKey, setGbKey] = useState<boolean | null>(null)
  const [key, setKey] = useState("")
  const [looking, setLooking] = useState(false)
  const [q, setQ] = useState("")
  const [f, setF] = useState({ key: "", name: "", what: "", country: "GB", root: "", isolation: "auto", goal: "", by: "" })
  const set = (k: keyof typeof f) => (e: { target: { value: string } }) => setF((x) => ({ ...x, [k]: e.target.value }))
  const existing = realms.filter((r) => !r.archived)

  useEffect(() => {
    api<{ model_ready: boolean }>("setup").then((s) => setReady(s.model_ready), () => setReady(false))
    api<{ gb_key: boolean }>("registers").then((r) => setGbKey(r.gb_key), () => setGbKey(false))
  }, [])

  // The company's record, from its register: it fills the name and the
  // profile, for the founder to check before anything is written.
  async function lookUp() {
    setLooking(true); setError(null)
    try {
      if (f.country === "GB" && !gbKey) {
        await api("registers/key", { key: key.trim() }, "PUT")
        setGbKey(true); setKey("")
      }
      const r = await api<Record>(`registers/${f.country}/${encodeURIComponent(number.trim())}`)
      setRec(r)
      setF((x) => ({ ...x, name: x.name.trim() ? x.name : r.name }))
    } catch (e) { setError((e as Error).message) } finally { setLooking(false) }
  }
  useEffect(() => {
    if (!realm || teams) return
    api<{ teams: Team[] }>(`realms/${enc(realm)}/library`).then((d) => setTeams(d.teams), () => setTeams([]))
  }, [realm, teams])

  // The steps this launch shows: a model already connected is not asked for,
  // nor the welcome, for a business added from the menu.
  const shown = STEPS.filter((s) => (s !== "model" || ready === false) && (s !== "hello" || !fresh))
  const at = shown.indexOf(step)
  const go = (s: Step) => { setError(null); setStep(s) }
  const next = () => go(shown[Math.min(at + 1, shown.length - 1)])
  const back = () => go(shown[Math.max(at - 1, 0)])
  async function work(fn: () => Promise<void>) {
    setBusy(true); setError(null)
    try { await fn() } catch (e) { setError((e as Error).message) } finally { setBusy(false) }
  }
  function finish() {
    if (!fresh) try { localStorage.setItem("airth-welcomed", "1") } catch { /* private window */ }
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
          <Labelled group label="Where it is registered" note="Its legal calendar follows: filings, accounts, tax.">
            <div role="radiogroup" className="grid grid-cols-3 gap-2">
              {COUNTRIES.map(([code, label]) => (
                <button key={label} type="button" role="radio" aria-checked={f.country === code} onClick={() => { setF((x) => ({ ...x, country: code })); setRec(null) }}
                  className={cn("h-11 rounded-xl border text-sm transition-colors", f.country === code ? "border-foreground bg-foreground text-background" : "hover:bg-accent/60")}>{label}</button>
              ))}
            </div>
          </Labelled>
          {REGISTER[f.country] && !planned && (
            <div className="space-y-2">
              <span className="block text-sm font-medium">{REGISTER[f.country].number}</span>
              <div className="flex gap-2">
                <Input value={number} onChange={(e) => { setNumber(e.target.value); setRec(null) }} placeholder={REGISTER[f.country].example} spellCheck={false}
                  aria-label={REGISTER[f.country].number} className="h-11 font-mono" onKeyDown={(e) => e.key === "Enter" && number.trim() && lookUp()} />
                <Button variant="outline" className="h-11" disabled={looking || !number.trim() || (f.country === "GB" && !gbKey && !key.trim())} onClick={lookUp}>
                  {looking ? <Loader className="animate-spin" /> : <Search />} Look it up
                </Button>
              </div>
              {f.country === "GB" && gbKey === false && (
                <label className="flex h-11 items-center gap-3 rounded-xl border bg-surface px-3.5 focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30">
                  <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                  <input type="password" autoComplete="off" spellCheck={false} value={key} onChange={(e) => setKey(e.target.value)} placeholder="Companies House API key" aria-label="Companies House API key"
                    className="min-w-0 flex-1 bg-transparent font-mono text-sm outline-none placeholder:text-muted-foreground" />
                </label>
              )}
              <span className="block text-xs text-muted-foreground">
                {f.country === "GB" && gbKey === false
                  ? <>Companies House needs a free key, once: create one at <a className="text-sky-500 hover:underline" href="https://developer.company-information.service.gov.uk/manage-applications" target="_blank" rel="noreferrer">developer.company-information.service.gov.uk</a>. It stays with Airth. </>
                  : <>Airth reads its record from {REGISTER[f.country].name}, for you to check. </>}
                <button type="button" className="underline-offset-2 hover:text-foreground hover:underline" onClick={() => { setPlanned(true); setRec(null) }}>Not registered yet</button>
              </span>
            </div>
          )}
          {planned && (
            <p className="text-xs text-muted-foreground">Not registered yet: add its number on the Company page once it is. <button type="button" className="underline-offset-2 hover:text-foreground hover:underline" onClick={() => setPlanned(false)}>It is registered</button></p>
          )}
          {rec && (
            <div className="rounded-2xl border bg-surface p-4">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Check className="size-3.5 text-emerald-500" /> From {REGISTER[rec.country]?.name ?? "its register"}{rec.activity ? ` · ${rec.activity}` : ""}
                {!rec.active && <span className="ml-auto rounded-md bg-amber-500/10 px-1.5 py-0.5 text-amber-600 dark:text-amber-400">Not active</span>}
              </div>
              <dl className="mt-3 grid gap-2">
                {FIELDS.map(([k, label, hint]) => (
                  <div key={k} className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-center gap-2 max-sm:grid-cols-1 max-sm:gap-1">
                    <dt className="text-xs text-muted-foreground">{label}</dt>
                    <dd><Input value={(rec[k] as string | null) ?? ""} placeholder={hint} onChange={(e) => setRec({ ...rec, [k]: e.target.value })} className="h-8 text-sm" aria-label={label} /></dd>
                  </div>
                ))}
              </dl>
              <p className="mt-3 text-xs text-muted-foreground">Check them: they go to its profile, and its legal calendar reads them.</p>
            </div>
          )}
          <Labelled label="Its name" note={id && id !== f.name.trim() ? <>Its folder: <span className="font-mono">{id}</span></> : undefined}>
            <Input value={f.name} onChange={set("name")} placeholder="Northwind" className="h-11 text-base" />
          </Labelled>
          <Labelled label="What it does" note="For whom, and how it sounds. Every agent reads it.">
            <Textarea rows={3} value={f.what} onChange={set("what")} className="text-[15px] leading-relaxed"
              placeholder="A research media on X for crypto allocators. Calm, sourced, never hype." />
          </Labelled>
          {folder ? (
            <>
              <Labelled label="Its folder" note="Where its files and code already are. Empty: a private folder Airth makes.">
                <Input value={f.root} onChange={set("root")} placeholder="~/code/northwind" spellCheck={false} className="font-mono" />
              </Labelled>
              <Labelled group label="How tasks work in it">
                <Select value={f.isolation} onValueChange={(v) => setF((x) => ({ ...x, isolation: String(v ?? "auto") }))}>
                  <SelectTrigger className="w-full"><SelectValue>{(v: string) => ISOLATION.find(([k]) => k === v)?.[1] ?? v}</SelectValue></SelectTrigger>
                  <SelectContent>{ISOLATION.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
                </Select>
              </Labelled>
            </>
          ) : (
            <button type="button" onClick={() => setFolder(true)} className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"><ChevronDown className="size-3.5" /> It already has a folder</button>
          )}
        </div>
      )
      aside = !fresh && existing.length > 0 && <Button variant="ghost" onClick={() => { setRealm(existing[0].name); setF((x) => ({ ...x, name: x.name || existing[0].name })); next() }}>Use {existing[0].name}</Button>
      primary = <Button disabled={busy || !id} onClick={() => work(async () => {
        const r = await createBusiness({ name: id, what: f.what, root: f.root, isolation: f.isolation })
        // What the register said, as the founder left it; empty fields are not written.
        const record: { [k: string]: string } = {}
        if (rec) {
          for (const [k] of FIELDS) { const v = String(rec[k] ?? "").trim(); if (v) record[k] = v }
          record.currency = rec.currency
          record.status = "registered"
        } else if (planned) record.status = "planned"
        await api(`realms/${enc(r)}/business`, { profile: { name: f.name.trim(), ...(f.country ? { country: f.country } : {}), ...record } }, "PATCH")
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
  } else if (step === "connectors") {
    title = "Connect its tools"
    lead = <>What its agents work with. Each one signs in or takes a key on its side; the key stays with Airth and never reaches a model. {installed?.connectors?.length || installed?.setup.length ? "Those its team needs come first. " : ""}Add more later, in the Marketplace.</>
    const hint = [...(installed?.connectors ?? []), ...(installed?.setup ?? [])].join(" ").toLowerCase() + (f.country === "FR" ? " qonto" : "")
    body = realm && (
      <div className="space-y-4">
        <label className="flex h-10 items-center gap-2 rounded-xl border bg-surface px-3 text-sm text-muted-foreground focus-within:border-ring">
          <Search className="size-4" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search connectors" className="min-w-0 flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
        </label>
        <Connectors realm={realm} q={q} hint={hint} />
      </div>
    )
    aside = <Button variant="ghost" onClick={next}>Later</Button>
    primary = <Button onClick={next}>Done <ArrowRight /></Button>
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
    <div className={cn("grid min-h-svh bg-surface", step !== "connectors" && "lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]")}>
      <div className="flex min-h-svh flex-col px-6 pt-[max(env(safe-area-inset-top),24px)] pb-8 sm:px-12">
        <header className="flex items-center gap-3">
          <Ring className="size-6" />
          <span className="text-[15px] font-semibold tracking-tight">Airth</span>
          {step !== "hello" && step !== "ready" && (
            <ol className="ml-auto flex items-center gap-1.5" aria-label={`Step ${at + (fresh ? 1 : 0)} of ${shown.length - (fresh ? 1 : 2)}`}>
              {shown.slice(fresh ? 0 : 1, -1).map((s) => {
                const n = shown.indexOf(s)
                return <li key={s} className={cn("h-1 rounded-full transition-all duration-500", n === at ? "w-7 bg-foreground" : n < at ? "w-3 bg-foreground/50" : "w-3 bg-muted")} />
              })}
            </ol>
          )}
          {fresh && onCancel && step !== "ready" && (
            <Button variant="ghost" size="icon" aria-label="Close" className="-mr-2" onClick={onCancel}><X /></Button>
          )}
        </header>
        <main key={`${step}${realm ? "+" : ""}`} className={cn("mx-auto flex w-full flex-1 flex-col justify-center py-10 fade-up", step === "connectors" ? "max-w-5xl" : "max-w-md")}>
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
      {step !== "connectors" && <Stage step={step} name={name} what={f.what} country={f.country} crew={crew} goal={f.goal} by={f.by} number={rec?.registration_number} />}
    </div>
  )
}

/** The business as it is being built, beside the questions. */
function Stage({ step, name, what, country, crew, goal, by, number }: { step: Step; name: string; what: string; country: string; crew: TeamAgent[]; goal: string; by: string; number?: string }) {
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
              <div className="text-xs text-muted-foreground">{name && past("business") && place ? (country ? `${number ? `No ${number} · ` : ""}Registered in ${place}` : "A company") : " "}</div>
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
