import { useEffect, useMemo, useState } from "react"
import { Bot, Check, ExternalLink, Loader, Plug, Search, ShieldAlert, Sparkles, Store, Users, Workflow, RotateCw, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Character } from "@/components/character"
import { ConnectDialog, type Method } from "@/components/connect-dialog"
import { Empty, Loading } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { LOGOS } from "@/lib/logos"
import { cn } from "@/lib/utils"

type Tab = "teams" | "skills" | "connectors" | "agents"
const TABS: { id: Tab; label: string; icon: LucideIcon }[] = [
  { id: "teams", label: "Teams", icon: Users },
  { id: "skills", label: "Skills", icon: Sparkles },
  { id: "connectors", label: "Connectors", icon: Plug },
  { id: "agents", label: "Agents", icon: Bot },
]

// The official remote MCP servers checked end to end with Airth's sign-in
// (decision 20), as in the classic interface.
const FEATURED = [
  ["notion", "Notion", "https://mcp.notion.com/mcp", "Search, read and write your pages and databases.", "Productivity"],
  ["linear", "Linear", "https://mcp.linear.app/mcp", "Find, create and update issues and projects.", "Product"],
  ["stripe", "Stripe", "https://mcp.stripe.com", "Payments, customers, invoices and balance.", "Finance"],
  ["vercel", "Vercel", "https://mcp.vercel.com", "Projects, deployments and their logs.", "Development"],
  ["sentry", "Sentry", "https://mcp.sentry.dev/mcp", "Errors and issues across your projects.", "Development"],
  ["paypal", "PayPal", "https://mcp.paypal.com/mcp", "Transactions, invoices and disputes.", "Finance"],
  ["canva", "Canva", "https://mcp.canva.com/mcp", "Find, create and export designs.", "Design"],
  ["intercom", "Intercom", "https://mcp.intercom.com/mcp", "Conversations and contacts.", "Support"],
  ["figma", "Figma", "https://mcp.figma.com/mcp", "Read files, frames and design context.", "Design"],
  ["webflow", "Webflow", "https://mcp.webflow.com/mcp", "Sites, pages and CMS collections.", "Marketing"],
  ["cloudflare", "Cloudflare", "https://mcp.cloudflare.com/mcp", "Workers, DNS and your account.", "Development"],
  ["square", "Square", "https://mcp.squareup.com/mcp", "Orders, payments and catalog.", "Sales"],
  ["supabase", "Supabase", "https://mcp.supabase.com/mcp", "Databases, tables and SQL.", "Development"],
  // Official too, not yet tried end to end with Airth's sign-in.
  ["todoist", "Todoist", "https://ai.todoist.net/mcp", "Tasks, projects and due dates.", "Productivity", "new"],
  ["posthog", "PostHog", "https://mcp.posthog.com/mcp", "Product analytics: events, insights, feature flags.", "Analytics", "new"],
].map(([id, name, url, description, category, tried]) => ({ id, name, url, description, category, untried: tried === "new" }))

type Skill = { name: string; title: string; description: string; files: number; skipped: number }
type Catalog = { source: string; fetched_at: string | null; error: string | null; refreshing: boolean; skills: Skill[] }
type Installed = { installed: string[]; installing: string[]; errors: Record<string, string> }
type Server = { name: string; url?: string; signed_in: boolean; error: string | null; enabled: boolean; connecting: boolean }
type Listed = { name: string; title: string; description: string | null; url: string; website: string | null }

const hue = (s: string) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7)

/** A company's mark when Airth has it (Simple Icons, embedded), else its
    initial on a colour of its own. */
function Logo({ name, id, size = 40 }: { name: string; id?: string; size?: number }) {
  const mark = id ? LOGOS[id] : undefined
  if (mark) {
    return (
      <span className="grid shrink-0 place-items-center rounded-xl border bg-white shadow-xs dark:bg-white" style={{ width: size, height: size }}>
        <svg viewBox="0 0 24 24" style={{ width: size * 0.55, height: size * 0.55, color: mark[0] ?? "#111" }} aria-hidden="true">
          <path fill="currentColor" d={mark[1]} />
        </svg>
      </span>
    )
  }
  const h = hue(name)
  return (
    <span className="grid shrink-0 place-items-center rounded-xl font-semibold text-white" style={{ width: size, height: size, fontSize: size * 0.42, background: `linear-gradient(145deg, oklch(0.68 0.14 ${h}), oklch(0.5 0.14 ${h}))` }}>
      {name.replace(/^[^a-z0-9]+/i, "")[0]?.toUpperCase() ?? "?"}
    </span>
  )
}

/** The marketplace: what can be added to this business's agents, from
    outside. Skills from Anthropic's catalog (decision 18), connectors from
    the official servers and the MCP Registry (decision 20). */
export function Marketplace({ realm }: { realm: string }) {
  const [tab, setTab] = useState<Tab>("teams")
  const [q, setQ] = useState("")
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="border-b px-8 pt-7 pb-0">
        <div className="mx-auto flex max-w-5xl items-end gap-4">
          <div className="flex-1">
            <h1 className="flex items-center gap-2 text-2xl font-semibold tracking-tight"><Store className="size-6 text-muted-foreground" /> Marketplace</h1>
            <p className="mt-1 text-sm text-muted-foreground">Teams, skills, connectors and agents to add to {realm}.</p>
          </div>
          <label className="mb-1 flex h-9 w-72 items-center gap-2 rounded-lg bg-muted px-3 text-sm text-muted-foreground focus-within:ring-2 focus-within:ring-ring/40">
            <Search className="size-4" />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Search ${tab}`} className="flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
          </label>
        </div>
        <nav className="mx-auto mt-5 flex max-w-5xl gap-1">
          {TABS.map((t) => (
            <button key={t.id} onClick={() => setTab(t.id)}
              className={cn("relative flex items-center gap-1.5 px-3 pb-2.5 text-sm text-muted-foreground transition-colors hover:text-foreground", tab === t.id && "text-foreground")}>
              <t.icon className="size-4" /> {t.label}
              {tab === t.id && <span className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-foreground" />}
            </button>
          ))}
        </nav>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div key={tab} className="mx-auto max-w-5xl px-8 py-6 duration-200 animate-in fade-in slide-in-from-bottom-1">
          {tab === "teams" ? <Teams realm={realm} q={q} onConnectors={() => setTab("connectors")} /> : tab === "skills" ? <Skills realm={realm} q={q} /> : tab === "connectors" ? <Connectors realm={realm} q={q} /> : <Agents />}
        </div>
      </div>
    </div>
  )
}

function Grid({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">{children}</div>
}

function Card({ name, logo, title, description, meta, action, onOpen }: { name: string; logo?: string; title: string; description: string; meta?: React.ReactNode; action: React.ReactNode; onOpen?: () => void }) {
  return (
    <div className="group flex flex-col rounded-xl border bg-bubble/40 p-4 transition-colors hover:border-foreground/20">
      <button onClick={onOpen} className="flex items-start gap-3 text-left" disabled={!onOpen}>
        <Logo name={name} id={logo} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-semibold">{title}</span>
          {meta && <span className="block truncate text-xs text-muted-foreground">{meta}</span>}
        </span>
      </button>
      <p className="mt-3 line-clamp-3 flex-1 text-[13px] leading-relaxed text-muted-foreground">{description}</p>
      <div className="mt-4 flex items-center justify-end gap-2">{action}</div>
    </div>
  )
}

const Done = ({ children }: { children: React.ReactNode }) => (
  <span className="inline-flex items-center gap-1 text-xs font-medium text-emerald-500"><Check className="size-3.5" /> {children}</span>
)

function Skills({ realm, q }: { realm: string; q: string }) {
  const cat = usePoll<Catalog>(() => api("catalog"), 600000, [])
  const [fresh, setFresh] = useState(false)
  // The catalog is kept a while; this asks Anthropic's list again now.
  async function refresh() {
    setFresh(true)
    try { await api("catalog?refresh=1"); cat.refresh() } catch (e) { toast.error((e as Error).message) } finally { setFresh(false) }
  }
  const mine = usePoll<Installed>(() => api(`realms/${enc(realm)}/catalog`), 3000, [realm])
  const [open, setOpen] = useState<Skill | null>(null)
  const shown = (cat.data?.skills ?? []).filter((s) => !q || `${s.title} ${s.description}`.toLowerCase().includes(q.toLowerCase()))
  async function act(s: Skill, remove: boolean) {
    try {
      await api(`realms/${enc(realm)}/catalog/${enc(s.name)}${remove ? "" : "/install"}`, remove ? undefined : {}, remove ? "DELETE" : undefined)
      toast.success(remove ? `${s.title} removed.` : `Installing ${s.title}…`)
      mine.refresh()
    } catch (e) { toast.error((e as Error).message) }
  }
  const state = (s: Skill) => mine.data?.installing.includes(s.name) ? "installing" : mine.data?.installed.includes(s.name) ? "installed" : mine.data?.errors[s.name] ? "error" : "none"
  const button = (s: Skill) => {
    const st = state(s)
    return st === "installing" ? <span className="inline-flex items-center gap-1 text-xs text-muted-foreground"><Loader className="size-3.5 animate-spin" /> Installing</span>
      : st === "installed" ? <><Done>Installed</Done><Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => act(s, true)}>Remove</Button></>
      : <Button size="sm" variant="outline" onClick={() => setOpen(s)}>Add</Button>
  }
  return (
    <>
      <p className="mb-4 flex items-center gap-1.5 text-xs text-muted-foreground">
        From <a href={cat.data?.source ?? "https://github.com/anthropics/skills"} target="_blank" rel="noreferrer" className="inline-flex items-center gap-0.5 hover:text-foreground">Anthropic's skills catalog <ExternalLink className="size-3" /></a>
        · Text only: an agent reads a skill as data, under this business's rules.
        <button onClick={refresh} disabled={fresh} className="ml-auto inline-flex items-center gap-1 hover:text-foreground disabled:opacity-50"><RotateCw className={cn("size-3", fresh && "animate-spin")} /> Refresh the list</button>
      </p>
      {!cat.data ? <Loading /> : cat.data.error && !cat.data.skills.length ? <Empty icon={Sparkles} title="The catalog cannot be reached">{cat.data.error}</Empty> : (
        <Grid>
          {shown.map((s) => (
            <Card key={s.name} name={s.name} title={s.title} meta={`${s.files} file${s.files === 1 ? "" : "s"}`} description={s.description} onOpen={() => setOpen(s)} action={button(s)} />
          ))}
        </Grid>
      )}
      <Dialog open={!!open} onOpenChange={(o) => !o && setOpen(null)}>
        {open && (
          <DialogContent className="sm:max-w-lg">
            <DialogHeader>
              <div className="flex items-center gap-3"><Logo name={open.name} /><div><DialogTitle>{open.title}</DialogTitle><DialogDescription>Skill · {open.files} files{open.skipped ? `, ${open.skipped} left out` : ""}</DialogDescription></div></div>
            </DialogHeader>
            <p className="max-h-60 overflow-y-auto text-sm leading-relaxed">{open.description}</p>
            <p className="rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">Its text files are copied into this business's skills. Agents can read it with <span className="font-mono">use_skill</span>; it never runs on its own.</p>
            <DialogFooter>
              <a href={`https://github.com/anthropics/skills/tree/main/skills/${open.name}`} target="_blank" rel="noreferrer" className="mr-auto inline-flex items-center gap-1 self-center text-xs text-muted-foreground hover:text-foreground">Read it first <ExternalLink className="size-3" /></a>
              <Button variant="ghost" onClick={() => setOpen(null)}>Cancel</Button>
              <Button disabled={state(open) !== "none" && state(open) !== "error"} onClick={() => { act(open, false); setOpen(null) }}>Add to {realm}</Button>
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </>
  )
}

function Connectors({ realm, q }: { realm: string; q: string }) {
  const servers = usePoll<Server[]>(() => api(`realms/${enc(realm)}/mcp`), 15000, [realm])
  const [registry, setRegistry] = useState<Listed[] | null>(null)
  const [searching, setSearching] = useState(false)
  const [method, setMethod] = useState<Method | null>(null)
  useEffect(() => {
    let live = true
    setSearching(true)
    const t = setTimeout(() => api<Listed[]>(`registry?search=${encodeURIComponent(q)}`)
      .then((r) => live && setRegistry(r)).catch(() => live && setRegistry([])).finally(() => live && setSearching(false)), 400)
    return () => { live = false; clearTimeout(t) }
  }, [q])
  const host = (u: string) => { try { return new URL(u).host } catch { return u } }
  const byHost = useMemo(() => new Map((servers.data ?? []).filter((s) => s.url).map((s) => [host(s.url!), s])), [servers.data])
  const featured = FEATURED.filter((c) => !q || `${c.name} ${c.description} ${c.category}`.toLowerCase().includes(q.toLowerCase()))
  const featuredHosts = new Set(FEATURED.map((c) => host(c.url)))
  const listed = (registry ?? []).filter((r) => !featuredHosts.has(host(r.url)))

  const status = (url: string, m: Method) => {
    const s = byHost.get(host(url))
    if (!s) return null
    if (s.signed_in && !s.error) return <Done>Connected</Done>
    return <Button size="sm" variant="ghost" className={s.error ? "text-red-500" : "text-amber-500"} onClick={() => setMethod(m)}>{s.error ? "Error: retry" : "Finish signing in"}</Button>
  }
  const slug = (t: string) => t.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 32) || "connector"

  return (
    <>
      <h2 className="mb-1 text-sm font-semibold">Official</h2>
      <p className="mb-4 text-xs text-muted-foreground">Published by each company, checked with Airth's sign-in. Connecting opens their sign-in page; the key stays with Airth and never reaches a model.</p>
      <Grid>
        {featured.map((c) => (
          <Card key={c.id} name={c.name} logo={c.id} title={c.name} meta={c.untried ? `${c.category} · not tried with Airth yet` : c.category} description={c.description}
            action={(() => {
              const m: Method = { kind: "sign-in", name: c.id, title: c.name, url: c.url, description: c.description, logo: c.id, checked: true, tried: !c.untried }
              return status(c.url, m) ?? <Button size="sm" variant="outline" onClick={() => setMethod(m)}>Connect</Button>
            })()} />
        ))}
      </Grid>
      <WithKey realm={realm} q={q} skip={new Set(FEATURED.map((c) => c.id))} />
      <h2 className="mt-9 mb-1 flex items-center gap-2 text-sm font-semibold">From the MCP Registry {searching && <Loader className="size-3.5 animate-spin text-muted-foreground" />}</h2>
      <p className="mb-4 flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-400"><ShieldAlert className="size-3.5" /> Not checked by Airth. Connect only what the company itself publishes: look at the address.</p>
      {!registry ? <Loading /> : !listed.length ? <Empty icon={Plug} title="Nothing found">Try another word.</Empty> : (
        <Grid>
          {listed.slice(0, 30).map((r) => (
            <Card key={r.name} name={r.title} title={r.title} meta={host(r.url)} description={r.description ?? ""}
              action={(() => {
                const m: Method = { kind: "sign-in", name: slug(r.title), title: r.title, url: r.url, description: r.description ?? "", checked: false, tried: false }
                return status(r.url, m) ?? <Button size="sm" variant="outline" onClick={() => setMethod(m)}>Connect</Button>
              })()} />
          ))}
        </Grid>
      )}
      <ConnectDialog realm={realm} method={method} onClose={() => setMethod(null)} onConnected={servers.refresh} />
    </>
  )
}

function Agents() {
  return (
    <Empty icon={Bot} title="No agent library yet">
      There is no shared library of business roles to trust yet. The proposal: an Airth library of agents written for businesses, each one reviewed before it is listed. It needs your answer first.
    </Empty>
  )
}

type TeamAgent = { name: string; title: string; department: string | null; accessory: string | null; mission: string | null }
type Team = {
  id: string; title: string; description: string; installed: boolean
  agents: TeamAgent[]; routines: { name: string; agent: string; description: string; schedule: string | null }[]
  skills: string[]; connectors: string[]; setup: string[]
}

const connectorName = (id: string) => FEATURED.find((c) => c.id === id)?.name ?? id

/** Teams from Airth's library (decision 74): agents, their routines and
    prompts, the skills and connectors they need, installed together. */
function Teams({ realm, q, onConnectors }: { realm: string; q: string; onConnectors: () => void }) {
  const { data, refresh } = usePoll<{ teams: Team[] }>(() => api(`realms/${enc(realm)}/library`), 600000, [realm])
  const [open, setOpen] = useState<Team | null>(null)
  const [busy, setBusy] = useState(false)
  const [done, setDone] = useState<Team | null>(null)
  const shown = (data?.teams ?? []).filter((t) => !q || `${t.title} ${t.description}`.toLowerCase().includes(q.toLowerCase()))

  async function install(t: Team) {
    setBusy(true)
    try {
      const out = await api<{ written: string[]; team: Team }>(`realms/${enc(realm)}/library/${enc(t.id)}/install`, {})
      toast.success(`${t.title} team installed: ${out.written.length} files.`)
      setDone(out.team)
      refresh()
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  const face = (a: TeamAgent) => ({ department: a.department, accessory: a.accessory, state: "idle" as const, title: a.title, look: null })

  return (
    <>
      <p className="mb-4 text-xs text-muted-foreground">From Airth's library: each team is reviewed before it ships with Airth. Installing one adds its agents, routines and prompts to the business folder, never over a file that is there.</p>
      {!data ? <Loading /> : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {shown.map((t) => (
            <button key={t.id} onClick={() => { setDone(null); setOpen(t) }} className="group flex flex-col rounded-2xl border bg-bubble/40 p-5 text-left transition-colors hover:border-foreground/20">
              <div className="flex items-center gap-1.5">
                {t.agents.map((a, i) => (
                  <span key={a.name} className="rounded-full ring-2 ring-surface" style={{ marginLeft: i ? -10 : 0, zIndex: t.agents.length - i }}>
                    <Character agent={face(a)} size={38} still />
                  </span>
                ))}
                {t.installed && <span className="ml-auto"><Done>Installed</Done></span>}
              </div>
              <h3 className="mt-4 text-base font-semibold">{t.title}</h3>
              <p className="mt-1 text-[13px] leading-relaxed text-muted-foreground">{t.description}</p>
              <div className="mt-4 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
                <span className="rounded-md bg-muted px-1.5 py-0.5">{t.agents.length} agent{t.agents.length === 1 ? "" : "s"}</span>
                <span className="rounded-md bg-muted px-1.5 py-0.5">{t.routines.length} routine{t.routines.length === 1 ? "" : "s"}</span>
                {t.skills.length > 0 && <span className="rounded-md bg-muted px-1.5 py-0.5">{t.skills.length} skills</span>}
                {t.connectors.map((c) => <span key={c} className="rounded-md bg-muted px-1.5 py-0.5">{connectorName(c)}</span>)}
              </div>
            </button>
          ))}
        </div>
      )}
      <Dialog open={!!open} onOpenChange={(o) => !o && setOpen(null)}>
        {open && (
          <DialogContent className="flex max-h-[88svh] flex-col gap-0 p-0 sm:max-w-2xl">
            <DialogHeader className="border-b p-5">
              <DialogTitle className="text-lg">{done ? `${open.title} team installed` : `${open.title} team`}</DialogTitle>
              <DialogDescription>{done ? "What to do next, so each agent can start." : open.description}</DialogDescription>
            </DialogHeader>
            <div className="min-h-0 flex-1 space-y-6 overflow-y-auto p-5">
              {done ? (
                <>
                  <ol className="space-y-2">
                    {done.setup.map((step, i) => (
                      <li key={i} className="flex gap-3 rounded-xl border bg-bubble/40 px-3.5 py-3 text-sm">
                        <span className="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-xs font-semibold tabular-nums">{i + 1}</span>
                        <span className="leading-relaxed">{step}</span>
                      </li>
                    ))}
                  </ol>
                  {done.connectors.length > 0 && (
                    <Button variant="outline" onClick={() => { setOpen(null); onConnectors() }}><Plug /> Connect {done.connectors.map(connectorName).join(" and ")}</Button>
                  )}
                </>
              ) : (
                <>
                  <section>
                    <h4 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Agents</h4>
                    <div className="space-y-2">
                      {open.agents.map((a) => (
                        <div key={a.name} className="flex gap-3 rounded-xl border bg-bubble/40 p-3">
                          <Character agent={face(a)} size={34} still />
                          <div className="min-w-0">
                            <div className="text-sm font-medium">{a.title} <span className="text-xs font-normal text-muted-foreground capitalize">· {a.department}</span></div>
                            <p className="mt-0.5 text-[13px] leading-relaxed text-muted-foreground">{a.mission}</p>
                          </div>
                        </div>
                      ))}
                    </div>
                  </section>
                  <section>
                    <h4 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Routines</h4>
                    <div className="space-y-1">
                      {open.routines.map((r) => (
                        <div key={r.name} className="flex items-center gap-2.5 text-sm">
                          <Workflow className="size-3.5 text-muted-foreground" />
                          <span className="font-mono text-[12.5px]">{r.name}</span>
                          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{r.description}</span>
                          <span className="text-xs text-muted-foreground">{r.schedule ?? "after setup"}</span>
                        </div>
                      ))}
                    </div>
                  </section>
                  {(open.skills.length > 0 || open.connectors.length > 0) && (
                    <section className="grid grid-cols-2 gap-4">
                      <div>
                        <h4 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Skills</h4>
                        <p className="text-sm">{open.skills.length ? open.skills.join(", ") : "None"}</p>
                      </div>
                      <div>
                        <h4 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Connectors, yours to connect</h4>
                        <div className="flex flex-wrap gap-2">
                          {open.connectors.length ? open.connectors.map((c) => (
                            <span key={c} className="inline-flex items-center gap-1.5 text-sm"><Logo name={connectorName(c)} id={c} size={22} /> {connectorName(c)}</span>
                          )) : <span className="text-sm">None</span>}
                        </div>
                      </div>
                    </section>
                  )}
                  <section>
                    <h4 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">After installing</h4>
                    <ul className="list-disc space-y-1 pl-5 text-[13px] leading-relaxed text-muted-foreground">{open.setup.map((s, i) => <li key={i}>{s}</li>)}</ul>
                  </section>
                </>
              )}
            </div>
            <DialogFooter className="m-0 rounded-b-xl border-t bg-muted/30 p-4">
              {done ? <Button onClick={() => setOpen(null)}>Done</Button> : (
                <>
                  <Button variant="ghost" onClick={() => setOpen(null)}>Cancel</Button>
                  <Button disabled={busy || open.installed} onClick={() => install(open)}>{open.installed ? "Installed" : busy ? "Installing…" : `Install the ${open.title} team`}</Button>
                </>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </>
  )
}

type Preset = { connected: boolean; connector: { id: string; name: string; description: string; category: string; docs?: string; token_url?: string; token_help?: string } }

/** Services connected with an API key the founder creates on their side
    (decision 19): the key goes to the business's secrets, never to a model. */
function WithKey({ realm, q, skip }: { realm: string; q: string; skip: Set<string> }) {
  const { data, refresh } = usePoll<{ connectors: Preset[] }>(() => api(`realms/${enc(realm)}/connectors`), 600000, [realm])
  const [method, setMethod] = useState<Method | null>(null)
  const list = (data?.connectors ?? []).filter((c) => !skip.has(c.connector.id) || c.connected)
    .filter((c) => !q || `${c.connector.name} ${c.connector.description} ${c.connector.category}`.toLowerCase().includes(q.toLowerCase()))
  async function disconnect(c: Preset) {
    try { await api(`realms/${enc(realm)}/connectors/${enc(c.connector.id)}`, undefined, "DELETE"); toast.success(`${c.connector.name} disconnected.`); refresh() }
    catch (e) { toast.error((e as Error).message) }
  }
  if (!data || !list.length) return null
  return (
    <>
      <h2 className="mt-9 mb-1 text-sm font-semibold">With an API key</h2>
      <p className="mb-4 text-xs text-muted-foreground">For services without a sign-in page: create a key on their side and paste it here. It is kept in this business's secrets and never reaches a model.</p>
      <Grid>
        {list.map((c) => (
          <Card key={c.connector.id} name={c.connector.name} logo={c.connector.id} title={c.connector.name} meta={c.connector.category} description={c.connector.description}
            action={c.connected
              ? <><Done>Connected</Done><Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => disconnect(c)}>Disconnect</Button></>
              : <Button size="sm" variant="outline" onClick={() => setMethod({ kind: "key", id: c.connector.id, title: c.connector.name, description: c.connector.description, help: c.connector.token_help, keyUrl: c.connector.token_url })}>Add a key</Button>} />
        ))}
      </Grid>
      <ConnectDialog realm={realm} method={method} onClose={() => setMethod(null)} onConnected={refresh} />
    </>
  )
}
