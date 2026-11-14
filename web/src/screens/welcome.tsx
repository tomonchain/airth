import { useEffect, useMemo, useState } from "react"
import { ArrowLeft, ArrowRight, Check, ChevronDown, KeyRound, Loader, Search, X } from "lucide-react"
import { Character } from "@/components/character"
import { createBusiness, ISOLATION, slug } from "@/components/new-business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "@/components/ui/select"
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

// Where a business is registered: the founder's three first, then every
// country by its name. Airth reads the registers of the first two.
const FIRST = ["FR", "GB", "US"]
const OTHERS = ("AD AE AL AM AR AT AU AZ BA BD BE BG BH BR BY CA CH CL CN CO CR CY CZ DE DK DO DZ EC EE EG ES FI GE GH GR GT HK HR HU ID IE IL IN IQ IS IT JM JO JP KE KR KW KZ LB LI LK LT LU LV MA MC MD ME MK MT MU MX MY NG NL NO NZ OM PA PE PH PK PL PR PT PY QA RO RS RU SA SE SG SI SK SN TH TN TR TW UA UG UY UZ VE VN ZA").split(" ")
const regions = typeof Intl.DisplayNames === "function" ? new Intl.DisplayNames(["en"], { type: "region" }) : null
const countryName = (code: string) => (code && regions?.of(code)) || code
const OTHERS_BY_NAME = [...OTHERS].sort((a, b) => countryName(a).localeCompare(countryName(b)))
// A currency the profile can take for the countries Airth knows best.
const CURRENCY: { [country: string]: string } = { GB: "GBP", US: "USD", FR: "EUR", DE: "EUR", ES: "EUR", IT: "EUR", BE: "EUR", NL: "EUR", IE: "EUR", PT: "EUR", LU: "EUR", AT: "EUR", CH: "CHF", CA: "CAD" }

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
  // The business step in three: its country, its number, its details.
  const [phase, setPhase] = useState<"country" | "number" | "details">("country")
  const [found, setFound] = useState<Record | null>(null)
  const [registered, setRegistered] = useState(true)
  const [editing, setEditing] = useState(false)
  const blank: Record = { name: "", legal_name: "", legal_form: null, country: "", registration_number: "", registered_office: null, incorporated_on: null, fiscal_year_end: null, vat_number: null, currency: "", active: true, activity: null }
  const [hand, setHand] = useState<Record>(blank)
  const [gbKey, setGbKey] = useState<boolean | null>(null)
  const [key, setKey] = useState("")
  const [looking, setLooking] = useState(false)
  const [q, setQ] = useState("")
  const [f, setF] = useState({ key: "", name: "", what: "", country: "", root: "", isolation: "auto", goal: "", by: "" })
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
      setFound(await api<Record>(`registers/${f.country}/${encodeURIComponent(number.trim())}`))
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
  // In the business step, Back goes to its earlier question first.
  const inner = step === "business" && !realm && phase !== "country"
  const back = () => {
    if (inner) { setError(null); setPhase(phase === "details" && REGISTER[f.country] ? "number" : "country"); return }
    go(shown[Math.max(at - 1, 0)])
  }
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
        <label className="flex h-12 items-center gap-3 rounded-xl bg-bubble px-4 focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30">
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
      const register = REGISTER[f.country]
      if (phase === "country") {
        title = "Where is it registered?"
        lead = "Its legal calendar follows: filings, accounts, tax. Not registered yet? Choose where it will be."
        body = (
          <Select value={f.country || null} onValueChange={(v) => { setF((x) => ({ ...x, country: String(v ?? "") })); setRec(null) }}>
            <SelectTrigger className="h-12 w-full text-base" aria-label="Country"><SelectValue placeholder="Choose a country">{(v: string) => countryName(v)}</SelectValue></SelectTrigger>
            <SelectContent className="max-h-80">
              <SelectGroup>{FIRST.map((c) => <SelectItem key={c} value={c}>{countryName(c)}</SelectItem>)}</SelectGroup>
              <SelectSeparator />
              <SelectGroup>
                <SelectLabel>Other countries</SelectLabel>
                {OTHERS_BY_NAME.map((c) => <SelectItem key={c} value={c}>{countryName(c)}</SelectItem>)}
              </SelectGroup>
            </SelectContent>
          </Select>
        )
        aside = !fresh && existing.length > 0 && <Button variant="ghost" onClick={() => { setRealm(existing[0].name); setF((x) => ({ ...x, name: x.name || existing[0].name })); next() }}>Use {existing[0].name}</Button>
        primary = <Button disabled={!f.country} onClick={() => { setError(null); setRegistered(true); setPhase(register ? "number" : "details") }}>Continue <ArrowRight /></Button>
      } else if (phase === "number" && register) {
        title = "Its company number"
        lead = <>Airth reads its record from {register.name}, and shows it for you to check.</>
        body = (
          <div className="space-y-3">
            <Input value={number} onChange={(e) => setNumber(e.target.value)} placeholder={register.example} spellCheck={false} autoFocus
              aria-label={register.number} className="h-12 font-mono text-base" onKeyDown={(e) => e.key === "Enter" && number.trim() && (f.country !== "GB" || gbKey || key.trim()) && lookUp()} />
            <p className="text-xs text-muted-foreground">{register.number}, like {register.example}.</p>
            {f.country === "GB" && gbKey === false && (
              <>
                <label className="flex h-11 items-center gap-3 rounded-xl bg-bubble px-3.5 focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30">
                  <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                  <input type="password" autoComplete="off" spellCheck={false} value={key} onChange={(e) => setKey(e.target.value)} placeholder="Companies House API key" aria-label="Companies House API key"
                    className="min-w-0 flex-1 bg-transparent font-mono text-sm outline-none placeholder:text-muted-foreground" />
                </label>
                <p className="text-xs text-muted-foreground">Companies House needs a free key, once: create one at <a className="text-sky-500 hover:underline" href="https://developer.company-information.service.gov.uk/manage-applications" target="_blank" rel="noreferrer">developer.company-information.service.gov.uk</a>. It stays with Airth.</p>
              </>
            )}
          </div>
        )
        aside = <Button variant="ghost" onClick={() => { setError(null); setRec(null); setRegistered(false); setPhase("details") }}>No number: by hand</Button>
        primary = <Button disabled={looking || !number.trim() || (f.country === "GB" && !gbKey && !key.trim())} onClick={lookUp}>
          {looking ? <><Loader className="animate-spin" /> Looking</> : <>Look it up <ArrowRight /></>}
        </Button>
      } else {
        title = rec ? "Its details" : "Its details, by hand"
        lead = rec
          ? <>From {register?.name ?? "its register"}. Name it as people say it, and say what it does.</>
          : registered ? "What its registration says. Leave empty what you do not know yet: the Business page keeps it." : "Not registered yet: its registration goes on the Business page once it is."
        const fields = (r: Record, set: (r: Record) => void) => (
          <dl className="grid gap-2">
            {FIELDS.map(([k, label, hint]) => (
              <div key={k} className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-center gap-2 max-sm:grid-cols-1 max-sm:gap-1">
                <dt className="text-xs text-muted-foreground">{label}</dt>
                <dd><Input value={(r[k] as string | null) ?? ""} placeholder={hint} onChange={(e) => set({ ...r, [k]: e.target.value })} className="h-8 text-sm" aria-label={label} /></dd>
              </div>
            ))}
          </dl>
        )
        body = (
          <div className="space-y-5">
            {rec ? (
              <div className="rounded-2xl bg-bubble p-4">
                <div className="flex items-start gap-3">
                  <Check className="mt-0.5 size-4 shrink-0 text-emerald-500" />
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium">{rec.legal_name}</p>
                    <p className="mt-0.5 text-xs text-muted-foreground">{[rec.registration_number, rec.legal_form, rec.incorporated_on && `since ${rec.incorporated_on}`].filter(Boolean).join(" · ")}</p>
                  </div>
                  <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={() => setEditing(!editing)}>{editing ? "Done" : "Edit"}</button>
                </div>
                {editing && <div className="mt-3 border-t pt-3">{fields(rec, setRec)}</div>}
              </div>
            ) : (
              <>
                <div role="radiogroup" aria-label="Registered" className="grid grid-cols-2 gap-2">
                  {([[true, "Registered"], [false, "Not yet registered"]] as const).map(([v, label]) => (
                    <button key={label} type="button" role="radio" aria-checked={registered === v} onClick={() => setRegistered(v)}
                      className={cn("h-10 rounded-xl border text-sm transition-colors", registered === v ? "border-foreground bg-foreground text-background" : "hover:bg-accent/60")}>{label}</button>
                  ))}
                </div>
                {registered && (
                  <div className="rounded-2xl bg-bubble p-4">
                    {fields(rec ?? hand, setHand)}
                  </div>
                )}
              </>
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
        primary = <Button disabled={busy || !id} onClick={() => work(async () => {
          const r = await createBusiness({ name: id, what: f.what, root: f.root, isolation: f.isolation })
          // What the register said, or the founder wrote, as left: empty fields are not written.
          const source = rec ?? (registered ? hand : null)
          const record: { [k: string]: string } = { status: source ? "registered" : "planned" }
          for (const [k] of FIELDS) { const v = String(source?.[k] ?? "").trim(); if (v) record[k] = v }
          const currency = rec?.currency ?? CURRENCY[f.country]
          if (currency) record.currency = currency
          await api(`realms/${enc(r)}/business`, { profile: { name: f.name.trim(), country: f.country, ...record } }, "PATCH")
          setRealm(r); next()
        })}>{busy ? <><Loader className="animate-spin" /> Creating</> : <>Create <ArrowRight /></>}</Button>
      }
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
        <label className="flex h-10 items-center gap-2 rounded-xl bg-bubble px-3 text-sm text-muted-foreground focus-within:border-ring">
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
            {(at > 0 || inner) && step !== "ready" && <Button variant="ghost" size="icon" aria-label="Back" onClick={back}><ArrowLeft /></Button>}
            <span className="flex-1" />
            {aside}
            {primary}
          </div>
        </main>
      </div>
      {/* The record found, to accept or not before anything is written. */}
      <Dialog open={!!found} onOpenChange={(o) => { if (!o) setFound(null) }}>
        <DialogContent className="sm:max-w-md">
          {found && (
            <>
              <DialogHeader>
                <DialogTitle>{found.legal_name}</DialogTitle>
                <DialogDescription>
                  From {REGISTER[found.country]?.name ?? "its register"}{found.activity ? ` · ${found.activity}` : ""}
                  {!found.active && <span className="ml-2 rounded-md bg-amber-500/10 px-1.5 py-0.5 text-amber-600 dark:text-amber-400">Not active</span>}
                </DialogDescription>
              </DialogHeader>
              <dl className="grid gap-x-4 gap-y-2.5 text-sm sm:grid-cols-[8rem_minmax(0,1fr)]">
                {FIELDS.filter(([k]) => k !== "legal_name").map(([k, label]) => (
                  <div key={k} className="contents">
                    <dt className="text-muted-foreground max-sm:text-xs">{label}</dt>
                    <dd className={cn("[overflow-wrap:anywhere] max-sm:mb-1", !found[k] && "text-muted-foreground/60")}>{(found[k] as string | null) || "Not given"}</dd>
                  </div>
                ))}
              </dl>
              <DialogFooter>
                <Button variant="ghost" onClick={() => setFound(null)}>Not this one</Button>
                <Button onClick={() => {
                  setRec(found); setEditing(false)
                  setF((x) => ({ ...x, name: x.name.trim() ? x.name : found.name }))
                  setFound(null); setPhase("details")
                }}><Check /> Use these details</Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
      {step !== "connectors" && <Stage step={step} name={name} what={f.what} country={f.country} crew={crew} goal={f.goal} by={f.by} number={rec?.registration_number} planned={!rec && !registered && phase === "details"} />}
    </div>
  )
}

/** The business as it is being built, beside the questions. */
function Stage({ step, name, what, country, crew, goal, by, number, planned }: { step: Step; name: string; what: string; country: string; crew: TeamAgent[]; goal: string; by: string; number?: string; planned?: boolean }) {
  const place = country ? countryName(country) : null
  const past = (s: Step) => STEPS.indexOf(step) >= STEPS.indexOf(s)
  const when = useMemo(() => (by ? new Date(`${by}T12:00`).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" }) : null), [by])
  return (
    <aside aria-hidden className="relative hidden overflow-hidden border-l bg-background lg:block">
      <div className="absolute inset-0 bg-[radial-gradient(60%_50%_at_50%_40%,rgb(109_93_252/0.16),transparent_70%),radial-gradient(40%_40%_at_70%_60%,rgb(45_212_191/0.10),transparent_70%)]" />
      <div className="relative flex h-full items-center justify-center p-12">
        <div className="w-full max-w-sm rounded-3xl bg-bubble/90 p-7 shadow-[0_30px_80px_-30px_rgb(0_0_0/0.45)] backdrop-blur">
          <div className="flex items-center gap-3">
            <span className="size-9 shrink-0 rounded-full bg-[radial-gradient(circle_at_35%_30%,#d9f99d,#22c55e_55%,#14532d)] shadow-inner" />
            <div className="min-w-0">
              <div className={cn("truncate text-lg font-semibold tracking-tight transition-colors", !name && "text-muted-foreground/50")}>{name || "Your business"}</div>
              <div className="text-xs text-muted-foreground">{name && past("business") && place ? (country ? `${number ? `No ${number} · ` : ""}${planned ? "To be registered in" : "Registered in"} ${place}` : "A company") : " "}</div>
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

/** Airth's mark: the ring, in its greys. */
function Ring({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 64" className={className} aria-hidden>
      <defs><linearGradient id="airth-ring" x1="0" y1="0" x2="1" y2="1"><stop offset="0" style={{ stopColor: "var(--logo-from)" }} /><stop offset=".55" style={{ stopColor: "var(--logo-mid)" }} /><stop offset="1" style={{ stopColor: "var(--logo-to)" }} /></linearGradient></defs>
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
