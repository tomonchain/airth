import { useEffect, useMemo, useState } from "react"
import { Bot, Check, ExternalLink, Loader, Plug, Search, ShieldAlert, Sparkles, Store, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Empty, Loading } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { cn } from "@/lib/utils"

type Tab = "skills" | "connectors" | "agents"
const TABS: { id: Tab; label: string; icon: LucideIcon }[] = [
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
].map(([id, name, url, description, category]) => ({ id, name, url, description, category }))

type Skill = { name: string; title: string; description: string; files: number; skipped: number }
type Catalog = { source: string; fetched_at: string | null; error: string | null; refreshing: boolean; skills: Skill[] }
type Installed = { installed: string[]; installing: string[]; errors: Record<string, string> }
type Server = { name: string; url?: string; signed_in: boolean; error: string | null; enabled: boolean; connecting: boolean }
type Listed = { name: string; title: string; description: string | null; url: string; website: string | null }

const hue = (s: string) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7)
function Logo({ name, size = 40 }: { name: string; size?: number }) {
  const h = hue(name)
  return (
    <span className="grid shrink-0 place-items-center rounded-xl text-white font-semibold" style={{ width: size, height: size, fontSize: size * 0.42, background: `linear-gradient(145deg, oklch(0.68 0.14 ${h}), oklch(0.5 0.14 ${h}))` }}>
      {name.replace(/^[^a-z0-9]+/i, "")[0]?.toUpperCase() ?? "?"}
    </span>
  )
}

/** The marketplace: what can be added to this business's agents, from
    outside. Skills from Anthropic's catalog (decision 18), connectors from
    the official servers and the MCP Registry (decision 20). */
export function Marketplace({ realm }: { realm: string }) {
  const [tab, setTab] = useState<Tab>("skills")
  const [q, setQ] = useState("")
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="border-b px-8 pt-7 pb-0">
        <div className="mx-auto flex max-w-5xl items-end gap-4">
          <div className="flex-1">
            <h1 className="flex items-center gap-2 text-2xl font-semibold tracking-tight"><Store className="size-6 text-muted-foreground" /> Marketplace</h1>
            <p className="mt-1 text-sm text-muted-foreground">Skills, connectors and agents to add to {realm}'s team.</p>
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
          {tab === "skills" ? <Skills realm={realm} q={q} /> : tab === "connectors" ? <Connectors realm={realm} q={q} /> : <Agents />}
        </div>
      </div>
    </div>
  )
}

function Grid({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">{children}</div>
}

function Card({ name, title, description, meta, action, onOpen }: { name: string; title: string; description: string; meta?: React.ReactNode; action: React.ReactNode; onOpen?: () => void }) {
  return (
    <div className="group flex flex-col rounded-xl border bg-bubble/40 p-4 transition-colors hover:border-foreground/20">
      <button onClick={onOpen} className="flex items-start gap-3 text-left" disabled={!onOpen}>
        <Logo name={name} />
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
  const servers = usePoll<Server[]>(() => api(`realms/${enc(realm)}/mcp`), 3000, [realm])
  const [registry, setRegistry] = useState<Listed[] | null>(null)
  const [searching, setSearching] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)
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

  async function connect(name: string, url: string, description: string) {
    setBusy(name)
    try {
      const out = await api<{ sign_in?: string }>(`realms/${enc(realm)}/connect`, { name, url, description })
      toast.success(out.sign_in ? "Sign in in the page that just opened." : "Connected.")
      servers.refresh()
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(null) }
  }
  const status = (url: string) => {
    const s = byHost.get(host(url))
    if (!s) return null
    if (s.signed_in && !s.error) return <Done>Connected</Done>
    return <span className="text-xs text-amber-500">{s.error ? "Error" : "Waiting for sign-in"}</span>
  }
  const slug = (t: string) => t.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 32) || "connector"

  return (
    <>
      <h2 className="mb-1 text-sm font-semibold">Official</h2>
      <p className="mb-4 text-xs text-muted-foreground">Published by each company, checked with Airth's sign-in. Connecting opens their sign-in page; the key stays with Airth and never reaches a model.</p>
      <Grid>
        {featured.map((c) => (
          <Card key={c.id} name={c.name} title={c.name} meta={c.category} description={c.description}
            action={status(c.url) ?? <Button size="sm" variant="outline" disabled={busy === c.id} onClick={() => connect(c.id, c.url, c.description)}>{busy === c.id ? "Opening…" : "Connect"}</Button>} />
        ))}
      </Grid>
      <h2 className="mt-9 mb-1 flex items-center gap-2 text-sm font-semibold">From the MCP Registry {searching && <Loader className="size-3.5 animate-spin text-muted-foreground" />}</h2>
      <p className="mb-4 flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-400"><ShieldAlert className="size-3.5" /> Not checked by Airth. Connect only what the company itself publishes: look at the address.</p>
      {!registry ? <Loading /> : !listed.length ? <Empty icon={Plug} title="Nothing found">Try another word.</Empty> : (
        <Grid>
          {listed.slice(0, 30).map((r) => (
            <Card key={r.name} name={r.title} title={r.title} meta={host(r.url)} description={r.description ?? ""}
              action={status(r.url) ?? <Button size="sm" variant="outline" disabled={busy === slug(r.title)} onClick={() => connect(slug(r.title), r.url, r.description ?? "")}>Connect</Button>} />
          ))}
        </Grid>
      )}
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
