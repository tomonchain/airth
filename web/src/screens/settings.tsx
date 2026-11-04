import { useEffect, useState, type ReactNode } from "react"
import { Bot, Check, Cpu, ExternalLink, FolderGit2, Info, Monitor, Moon, Plug, Shield, Smartphone, Sun, User, X, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { useTheme } from "@/hooks/use-theme"
import { api, enc } from "@/lib/api"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"

type Section = "account" | "models" | "connections" | "permissions" | "repositories" | "phone" | "about"
const SECTIONS: { id: Section; label: string; icon: LucideIcon }[] = [
  { id: "account", label: "Account", icon: User },
  { id: "models", label: "Models", icon: Cpu },
  { id: "connections", label: "Connections", icon: Plug },
  { id: "permissions", label: "Permissions", icon: Shield },
  { id: "repositories", label: "Repositories", icon: FolderGit2 },
  { id: "phone", label: "Phone", icon: Smartphone },
  { id: "about", label: "About", icon: Info },
]

type Tier = { provider: string | null; model: string | null; effort: string | null; source: string | null }
type Detail = {
  models: Record<"main" | "fast" | "fallback", Tier>
  policy: Record<string, string>
  integrations: { name: string; description: string; base_url: string; missing_secrets: string[] }[]
  repos: { name: string; github: string | null; branch: string | null; main: boolean; delivery: string }[]
  isolation: string
  delivery: string
}
type Providers = { default: string; providers: { id: string; name: string; configured: boolean; models: string[]; key: string | null; error: string | null }[] }
type Connector = { connected: boolean; connector: { id: string; name: string; description: string; category: string } }
type Mcp = { name: string; description: string; enabled: boolean; signed_in: boolean; error: string | null; connected_at: string | null; tools: unknown[] }
type Owner = { name: string; about: string; avatar: string | null; preferences: string[]; updated_at?: string }

/** Settings: the founder's account and the business's models, connections,
    permissions, repositories and phone. What holds a secret is set in the
    classic interface until it moves. */
export function Settings({ realm }: { realm: string }) {
  const [section, setSection] = useState<Section>("account")
  return (
    <>
      <nav className="w-56 shrink-0 border-r p-3">
        <h1 className="px-2 pt-1 pb-3 text-base font-semibold">Settings</h1>
        {SECTIONS.map((s) => (
          <button key={s.id} onClick={() => setSection(s.id)}
            className={cn("flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground", section === s.id && "bg-accent text-foreground")}>
            <s.icon className="size-4" /> {s.label}
          </button>
        ))}
      </nav>
      <div className="min-w-0 flex-1 overflow-y-auto">
        <div key={section} className="mx-auto max-w-2xl px-8 py-8 duration-200 animate-in fade-in slide-in-from-bottom-1">
          {section === "account" ? <Account />
            : section === "models" ? <Models realm={realm} />
            : section === "connections" ? <Connections realm={realm} />
            : section === "permissions" ? <Permissions realm={realm} />
            : section === "repositories" ? <Repositories realm={realm} />
            : section === "phone" ? <Phone />
            : <About />}
        </div>
      </div>
    </>
  )
}

function Head({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className="mb-6">
      <h2 className="text-xl font-semibold tracking-tight">{title}</h2>
      {children && <p className="mt-1 text-sm text-muted-foreground">{children}</p>}
    </header>
  )
}

function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center gap-4 px-4 py-3">
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium">{label}</div>
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  )
}

const Classic = ({ what }: { what: string }) => (
  <a href="/classic" className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"><ExternalLink className="size-3" /> {what} in the classic interface</a>
)

function Account() {
  const { theme, setTheme } = useTheme()
  const { data, refresh } = usePoll<Owner>(() => api("owner"), 600000, [])
  const [name, setName] = useState("")
  const [about, setAbout] = useState("")
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (data) { setName(data.name); setAbout(data.about) } }, [data])
  async function save() {
    if (!data) return
    setBusy(true)
    try { await api("owner", { ...data, name, about }, "PUT"); toast.success("Saved."); refresh() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  const themes = [{ id: "light", label: "Light", icon: Sun }, { id: "dark", label: "Dark", icon: Moon }] as const
  return (
    <>
      <Head title="Account">You, as the agents know you.</Head>
      {!data ? <Loading /> : (
        <Panel className="space-y-4 p-4">
          <div className="space-y-1.5"><Label htmlFor="owner-name">Name</Label><Input id="owner-name" value={name} onChange={(e) => setName(e.target.value)} /></div>
          <div className="space-y-1.5">
            <Label htmlFor="owner-about">About you</Label>
            <Textarea id="owner-about" rows={4} value={about} onChange={(e) => setAbout(e.target.value)} placeholder="What the agents should know about you: your role, how you like things done." />
          </div>
          <div className="flex justify-end"><Button size="sm" disabled={busy || (name === data.name && about === data.about)} onClick={save}>Save</Button></div>
        </Panel>
      )}
      <h3 className="mt-8 mb-2.5 text-sm font-semibold">Appearance</h3>
      <div className="grid grid-cols-3 gap-2.5">
        {themes.map((t) => (
          <button key={t.id} onClick={() => setTheme(t.id)}
            className={cn("flex flex-col items-center gap-2 rounded-xl border bg-bubble/40 py-4 text-sm transition-colors hover:border-foreground/20", theme === t.id && "border-sky-500 ring-3 ring-sky-500/20")}>
            <t.icon className="size-5" /> {t.label}
          </button>
        ))}
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed py-4 text-xs text-muted-foreground"><Monitor className="size-5" /> Follows the system until you pick</div>
      </div>
    </>
  )
}

const TIERS = [
  { id: "main", label: "Main", hint: "Missions and the AI blocks that write." },
  { id: "fast", label: "Fast", hint: "Sorting, scoring and checks: cheap and quick." },
  { id: "fallback", label: "Fallback", hint: "Used when the main one fails." },
] as const

function Models({ realm }: { realm: string }) {
  const detail = usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  const options = usePoll<Providers>(() => api(`realms/${enc(realm)}/models`), 600000, [realm])
  async function set(tier: string, provider: string, model: string) {
    try { await api(`realms/${enc(realm)}/models`, { tier, provider, model, effort: "" }); toast.success(`${tier} model: ${model}`); detail.refresh() }
    catch (e) { toast.error((e as Error).message) }
  }
  const ready = options.data?.providers.filter((p) => p.configured && p.models.length) ?? []
  return (
    <>
      <Head title="Models">Which model each kind of work uses, for this business.</Head>
      {!detail.data || !options.data ? <Loading /> : (
        <Panel className="divide-y">
          {TIERS.map((t) => {
            const cur = detail.data!.models[t.id]
            const value = cur.provider && cur.model ? `${cur.provider}/${cur.model}` : ""
            return (
              <Row key={t.id} label={t.label} hint={t.hint}>
                <Select value={value} onValueChange={(v) => { const [p, ...m] = String(v).split("/"); set(t.id, p, m.join("/")) }}>
                  <SelectTrigger className="w-60"><SelectValue placeholder={t.id === "fallback" ? "None" : "Choose"} /></SelectTrigger>
                  <SelectContent>
                    {ready.map((p) => p.models.map((m) => <SelectItem key={`${p.id}/${m}`} value={`${p.id}/${m}`}>{p.name} · {m}</SelectItem>))}
                  </SelectContent>
                </Select>
              </Row>
            )
          })}
        </Panel>
      )}
      {options.data && (
        <>
          <h3 className="mt-8 mb-2.5 text-sm font-semibold">Providers</h3>
          <Panel className="divide-y">
            {options.data.providers.map((p) => (
              <Row key={p.id} label={p.name} hint={p.error ?? (p.configured ? `${p.models.length} models` : p.key ? `Needs ${p.key}` : "Not set up")}>
                {p.configured ? <span className="inline-flex items-center gap-1 text-xs text-emerald-500"><Check className="size-3.5" /> Ready</span>
                  : <span className="text-xs text-muted-foreground">Off</span>}
              </Row>
            ))}
          </Panel>
          <p className="mt-2"><Classic what="Add a key" /></p>
        </>
      )}
    </>
  )
}

function Connections({ realm }: { realm: string }) {
  const detail = usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  const conn = usePoll<{ connectors: Connector[] }>(() => api(`realms/${enc(realm)}/connectors`), 600000, [realm])
  const mcp = usePoll<Mcp[]>(() => api(`realms/${enc(realm)}/mcp`), 600000, [realm])
  const [checking, setChecking] = useState<string | null>(null)
  async function check(id: string) {
    setChecking(id)
    try {
      const out = await api<{ check: [string, { ok: boolean; message?: string }] | { ok: boolean; message?: string } | null }>(`realms/${enc(realm)}/connectors/${enc(id)}/check`, {})
      const c = Array.isArray(out.check) ? out.check[1] : out.check
      c?.ok === false ? toast.error(c.message ?? "It does not answer.") : toast.success("It answers.")
    } catch (e) { toast.error((e as Error).message) }
    finally { setChecking(null) }
  }
  const connected = conn.data?.connectors.filter((c) => c.connected) ?? []
  const available = conn.data?.connectors.filter((c) => !c.connected) ?? []
  return (
    <>
      <Head title="Connections">What the agents can reach. Each key stays with its connection and never reaches a model.</Head>
      <h3 className="mb-2.5 text-sm font-semibold">Integrations</h3>
      {!detail.data ? <Loading /> : (
        <Panel className="divide-y">
          {detail.data.integrations.map((i) => (
            <Row key={i.name} label={i.description || i.name} hint={<span className="font-mono">{i.base_url}</span>}>
              {i.missing_secrets.length ? <span className="text-xs text-amber-500">Needs {i.missing_secrets.join(", ")}</span>
                : <span className="inline-flex items-center gap-1 text-xs text-emerald-500"><Check className="size-3.5" /> Ready</span>}
            </Row>
          ))}
        </Panel>
      )}
      <h3 className="mt-8 mb-2.5 text-sm font-semibold">MCP servers</h3>
      {!mcp.data ? <Loading /> : (
        <Panel className="divide-y">
          {mcp.data.map((m) => (
            <Row key={m.name} label={m.name} hint={m.error ?? `${m.description} · ${m.tools.length} tools${m.connected_at ? ` · connected ${ago(m.connected_at)}` : ""}`}>
              {m.error ? <span className="inline-flex items-center gap-1 text-xs text-red-500"><X className="size-3.5" /> Error</span>
                : m.signed_in ? <span className="inline-flex items-center gap-1 text-xs text-emerald-500"><Check className="size-3.5" /> Signed in</span>
                : <span className="text-xs text-muted-foreground">{m.enabled ? "Not signed in" : "Off"}</span>}
            </Row>
          ))}
        </Panel>
      )}
      <h3 className="mt-8 mb-2.5 text-sm font-semibold">Connectors</h3>
      {!conn.data ? <Loading /> : (
        <>
          {connected.length > 0 && (
            <Panel className="mb-2.5 divide-y">
              {connected.map((c) => (
                <Row key={c.connector.id} label={c.connector.name} hint={c.connector.description}>
                  <Button size="sm" variant="outline" disabled={checking === c.connector.id} onClick={() => check(c.connector.id)}>{checking === c.connector.id ? "Checking…" : "Check"}</Button>
                </Row>
              ))}
            </Panel>
          )}
          <div className="grid grid-cols-2 gap-2">
            {available.map((c) => (
              <div key={c.connector.id} className="rounded-xl border bg-bubble/40 px-3.5 py-2.5">
                <div className="flex items-center gap-2 text-sm font-medium"><Bot className="size-3.5 text-muted-foreground" /> {c.connector.name}<span className="ml-auto text-[11px] text-muted-foreground">{c.connector.category}</span></div>
                <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{c.connector.description}</p>
              </div>
            ))}
          </div>
          <p className="mt-2"><Classic what="Connect one" /></p>
        </>
      )}
    </>
  )
}

const POLICY: Record<string, string> = {
  read: "Read files", write: "Write files", execute: "Run commands", network: "Use the network", memory: "Remember things", skill: "Use a skill",
}

function Permissions({ realm }: { realm: string }) {
  const { data } = usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  const tone = (v: string) => (v === "allow" ? "text-emerald-500" : v === "deny" ? "text-red-500" : "text-amber-500")
  return (
    <>
      <Head title="Permissions">What an agent's mission may do alone, and what asks you first. Routines follow their own autonomy levels.</Head>
      {!data ? <Loading /> : (
        <>
          <Panel className="divide-y">
            {Object.entries(data.policy).map(([k, v]) => (
              <Row key={k} label={POLICY[k] ?? k}><span className={cn("text-xs font-medium capitalize", tone(v))}>{v === "ask" ? "Asks you" : v}</span></Row>
            ))}
          </Panel>
          <Panel className="mt-2.5 divide-y">
            <Row label="Isolation" hint="Where a mission works on code."><span className="text-xs capitalize text-muted-foreground">{data.isolation}</span></Row>
            <Row label="Delivery" hint="What happens to a mission's changes."><span className="text-xs capitalize text-muted-foreground">{data.delivery}</span></Row>
          </Panel>
          <p className="mt-2"><Classic what="Change them" /></p>
        </>
      )}
    </>
  )
}

function Repositories({ realm }: { realm: string }) {
  const { data } = usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  return (
    <>
      <Head title="Repositories">The code the business's developer agents work on.</Head>
      {!data ? <Loading /> : (
        <Panel className="divide-y">
          {data.repos.map((r) => (
            <Row key={r.name} label={r.name} hint={<span className="font-mono">{r.github ?? "local"}{r.branch ? ` · ${r.branch}` : ""}</span>}>
              <span className="text-xs text-muted-foreground">{r.main ? "Main · " : ""}{r.delivery}</span>
            </Row>
          ))}
        </Panel>
      )}
    </>
  )
}

function Phone() {
  const { data } = usePoll<{ hosts: { host: string }[] }>(() => api("phone"), 600000, [])
  return (
    <>
      <Head title="Phone">Open Airth on your phone through your private network. Telegram stays the channel for Needs you.</Head>
      {!data ? <Loading /> : (
        <Panel className="divide-y">
          {data.hosts.length ? data.hosts.map((h) => (
            <Row key={h.host} label={h.host} hint="Allowed"><Smartphone className="size-4 text-muted-foreground" /></Row>
          )) : <Row label="No phone address yet" hint="Add one with Tailscale."><span /></Row>}
        </Panel>
      )}
      <p className="mt-2"><Classic what="Pair a phone (QR code)" /></p>
    </>
  )
}

function About() {
  const { data } = usePoll<{ home: string; sandbox: boolean; max_iterations: number; tools: unknown[] }>(() => api("info"), 600000, [])
  return (
    <>
      <Head title="About">Airth on this machine.</Head>
      {!data ? <Loading /> : (
        <Panel className="divide-y">
          <Row label="Airth home" hint="Data, secrets and run records. Agents cannot reach it."><span className="font-mono text-xs text-muted-foreground">{data.home.split("/").slice(-2).join("/")}</span></Row>
          <Row label="Sandbox" hint="Commands of missions run isolated.">{data.sandbox ? <span className="text-xs text-emerald-500">On</span> : <span className="text-xs text-red-500">Off</span>}</Row>
          <Row label="Tools" hint="What a mission's agent can call."><span className="text-xs text-muted-foreground">{data.tools.length}</span></Row>
          <Row label="Steps per mission" hint="At most."><span className="text-xs text-muted-foreground">{data.max_iterations}</span></Row>
        </Panel>
      )}
    </>
  )
}
