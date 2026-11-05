import { useEffect, useState, type ReactNode } from "react"
import { Bot, Brain, Building2, Check, Cpu, RotateCw, FolderGit2, Info, KeyRound, Plug, Shield, Smartphone, User, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { usePrefs } from "@/hooks/use-prefs"
import { useTheme } from "@/hooks/use-theme"
import { api, enc } from "@/lib/api"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"
import { McpServers } from "@/screens/settings-mcp"
import { Business, Memory, Permissions, Phone, PictureEditor, Repositories } from "@/screens/settings-business"
import { Keys } from "@/screens/settings-keys"

export type Section = "account" | "business" | "models" | "keys" | "connections" | "permissions" | "repositories" | "memory" | "phone" | "about"
const SECTIONS: { id: Section; label: string; icon: LucideIcon }[] = [
  { id: "account", label: "Account", icon: User },
  { id: "business", label: "Business", icon: Building2 },
  { id: "models", label: "Models", icon: Cpu },
  { id: "keys", label: "Keys", icon: KeyRound },
  { id: "connections", label: "Connections", icon: Plug },
  { id: "permissions", label: "Permissions", icon: Shield },
  { id: "repositories", label: "Repositories", icon: FolderGit2 },
  { id: "memory", label: "Memory", icon: Brain },
  { id: "phone", label: "Phone", icon: Smartphone },
  { id: "about", label: "About", icon: Info },
]

type Tier = { provider: string | null; model: string | null; effort: string | null; source: string | null }
type Detail = {
  model: string | null
  model_error: string | null
  models: Record<"main" | "fast" | "fallback", Tier>
  policy: Record<string, string>
  integrations: { name: string; description: string; base_url: string; missing_secrets: string[] }[]
  repos: { name: string; github: string | null; branch: string | null; main: boolean; delivery: string }[]
  isolation: string
  delivery: string
}
type Providers = { default: string; providers: { id: string; name: string; configured: boolean; models: string[]; key: string | null; error: string | null }[] }
type Connector = { connected: boolean; connector: { id: string; name: string; description: string; category: string } }
type Owner = { name: string; about: string; avatar: string | null; preferences: string[]; updated_at?: string }

/** Settings: the founder's account and the business's models, connections,
    keys, permissions, repositories and phone. */
export function Settings({ realm, start }: { realm: string; start?: { section: Section; at: number } | null }) {
  const [section, setSection] = useState<Section>(start?.section ?? "account")
  useEffect(() => { if (start) setSection(start.section) }, [start])
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
            : section === "business" ? <Business realm={realm} onChanged={() => {}} />
            : section === "memory" ? <Memory realm={realm} />
            : section === "models" ? <Models realm={realm} onKeys={() => setSection("keys")} />
            : section === "keys" ? <Keys realm={realm} />
            : section === "connections" ? <Connections realm={realm} />
            : section === "permissions" ? <Permissions realm={realm} />
            : section === "repositories" ? <Repositories realm={realm} onKeys={() => setSection("keys")} />
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


const SEG = (on: boolean) => cn("rounded-md px-2.5 py-1 text-xs transition-colors", on ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")

function Seg<T extends string>({ label, value, options, onPick }: { label: string; value: string; options: [T, string][]; onPick: (v: T) => void }) {
  return (
    <div role="radiogroup" aria-label={label} className="flex rounded-lg border p-0.5">
      {options.map(([v, l]) => <button key={v} role="radio" aria-checked={value === v} onClick={() => onPick(v)} className={SEG(value === v)}>{l}</button>)}
    </div>
  )
}

/** The founder: picture, name, who they are and how they like things done
    (given to the businesses that share the profile), and how Airth reads. */
function Account() {
  const { choice, setTheme } = useTheme()
  const { prefs, setPref } = usePrefs()
  const { data, refresh } = usePoll<Owner>(() => api("owner"), 600000, [])
  const [name, setName] = useState("")
  const [about, setAbout] = useState("")
  const [likes, setLikes] = useState("")
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (data) { setName(data.name); setAbout(data.about); setLikes(data.preferences.join("\n")) } }, [data])
  const list = (s: string) => s.split("\n").map((x) => x.trim()).filter(Boolean)
  const changed = !!data && (name !== data.name || about !== data.about || list(likes).join("\n") !== data.preferences.join("\n"))
  async function save() {
    if (!data) return
    setBusy(true)
    try { await api("owner", { ...data, name: name.trim(), about: about.trim(), preferences: list(likes) }, "PUT"); toast.success("Saved."); refresh() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  const mod = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl "
  return (
    <>
      <Head title="Account">What Airth knows about you. Businesses that share your profile give it to every agent; goals and limits belong to each business's strategy.</Head>
      {!data ? <Loading /> : (
        <Panel className="space-y-4 p-4">
          <div className="space-y-1.5"><Label>Picture</Label><PictureEditor path="owner/avatar" version={data.avatar} text={data.name || "?"} round onDone={refresh} /></div>
          <div className="space-y-1.5"><Label htmlFor="owner-name">Name</Label><Input id="owner-name" value={name} onChange={(e) => setName(e.target.value)} /></div>
          <div className="space-y-1.5">
            <Label htmlFor="owner-about">About you</Label>
            <Textarea id="owner-about" rows={3} value={about} onChange={(e) => setAbout(e.target.value)} placeholder="Who you are, what you do." />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="owner-likes">Preferences</Label>
            <Textarea id="owner-likes" rows={3} value={likes} onChange={(e) => setLikes(e.target.value)} placeholder={"Answer in French\nShort, direct answers"} />
            <p className="text-xs text-muted-foreground">How you like things done, one per line.</p>
          </div>
          <div className="flex items-center justify-end gap-3">
            {data.updated_at && !changed && <span className="text-xs text-muted-foreground">Updated {ago(data.updated_at)}</span>}
            <Button size="sm" disabled={busy || !changed} onClick={save}>Save</Button>
          </div>
        </Panel>
      )}
      <h3 className="mt-8 mb-2.5 text-sm font-semibold">Appearance</h3>
      <p className="-mt-1.5 mb-2.5 text-xs text-muted-foreground">Kept in this browser or app window.</p>
      <Panel className="divide-y">
        <Row label="Theme" hint="Light, dark, or the same as your Mac."><Seg label="Theme" value={choice} options={[["system", "System"], ["light", "Light"], ["dark", "Dark"]]} onPick={setTheme} /></Row>
        <Row label="Text size" hint="Conversations."><Seg label="Text size" value={prefs.text} options={[["s", "Small"], ["m", "Default"], ["l", "Large"], ["xl", "Larger"]]} onPick={(v) => setPref("text", v)} /></Row>
        <Row label="Reading width" hint="How wide conversations can get."><Seg label="Reading width" value={prefs.width} options={[["narrow", "Narrow"], ["normal", "Normal"], ["wide", "Wide"]]} onPick={(v) => setPref("width", v)} /></Row>
        <Row label="Send with" hint="In the message box; the other one adds a new line."><Seg label="Send with" value={prefs.send} options={[["enter", "↵"], ["mod", `${mod}↵`]]} onPick={(v) => setPref("send", v)} /></Row>
      </Panel>
    </>
  )
}

const TIERS = [
  { id: "main", label: "Main", hint: "Missions and the AI blocks that write." },
  { id: "fast", label: "Fast", hint: "Sorting, scoring and checks: cheap and quick." },
  { id: "fallback", label: "Fallback", hint: "Used when the main one fails." },
] as const

const AUTO = "auto"
const EFFORTS = ["", "low", "medium", "high", "xhigh", "max"]
const SOURCES: Record<string, string> = { realm: "This business", all: "All businesses", env: "Environment" }

/** Which model each kind of work uses, and how hard it thinks: for this
    business, or for all of them at once. */
function Models({ realm, onKeys }: { realm: string; onKeys: () => void }) {
  const detail = usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  const options = usePoll<Providers>(() => api(`realms/${enc(realm)}/models`), 600000, [realm])
  const [all, setAll] = useState(false)
  const [fresh, setFresh] = useState(false)
  async function set(tier: string, choice: string, effort: string) {
    const [provider, ...model] = choice === AUTO ? ["", ""] : choice.split("/")
    try {
      await api(`realms/${enc(realm)}/models`, { tier, provider, model: model.join("/"), effort, all_realms: all })
      toast.success(all ? `${tier} model saved for all businesses.` : `${tier} model saved.`)
      setAll(false)
      detail.refresh()
    } catch (e) { toast.error((e as Error).message) }
  }
  async function refresh() {
    setFresh(true)
    try { await api(`realms/${enc(realm)}/models?refresh=1`); options.refresh(); toast.success("The model lists are up to date.") }
    catch (e) { toast.error((e as Error).message) }
    finally { setFresh(false) }
  }
  const ready = options.data?.providers.filter((p) => p.configured && p.models.length) ?? []
  const d = detail.data
  return (
    <>
      <Head title="Models">Which model each kind of work uses, for this business. With a fast model, simple requests go to it and cost much less.</Head>
      {!d || !options.data ? <Loading /> : (
        <>
          <Panel className="divide-y">
            {TIERS.map((t) => {
              const cur = d.models[t.id]
              const choice = cur.model ? `${cur.provider ?? d.models.main.provider ?? "anthropic"}/${cur.model}` : t.id === "main" && cur.provider ? `${cur.provider}/` : AUTO
              const label = (v: string) => v === AUTO ? (t.id === "main" ? "Default" : "None") : ready.flatMap((p) => p.models.map((m) => [`${p.id}/${m}`, `${p.name} · ${m}`])).find(([k]) => k === v)?.[1] ?? v.replace("/", " · ")
              return (
                <div key={t.id} className="flex items-center gap-3 px-4 py-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 text-sm font-medium">{t.label}{cur.source && <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-normal text-muted-foreground" title="Where this setting comes from">{SOURCES[cur.source] ?? cur.source}</span>}</div>
                    <div className="text-xs text-muted-foreground">{t.hint}</div>
                  </div>
                  <Select value={choice} onValueChange={(v) => set(t.id, String(v), cur.effort ?? "")}>
                    <SelectTrigger className="w-64"><SelectValue>{(v: string) => label(v)}</SelectValue></SelectTrigger>
                    <SelectContent>
                      <SelectItem value={AUTO}>{t.id === "main" ? "Default" : "None"}</SelectItem>
                      {ready.map((p) => p.models.map((m) => <SelectItem key={`${p.id}/${m}`} value={`${p.id}/${m}`}>{p.name} · {m}</SelectItem>))}
                    </SelectContent>
                  </Select>
                  <Select value={cur.effort ?? ""} disabled={choice === AUTO && t.id !== "main"} onValueChange={(v) => set(t.id, choice, String(v ?? ""))}>
                    <SelectTrigger className="w-32" title="Reasoning effort: higher thinks more and costs more"><SelectValue>{(v: string) => `Effort: ${v || "default"}`}</SelectValue></SelectTrigger>
                    <SelectContent>{EFFORTS.map((e) => <SelectItem key={e} value={e}>{e || "Default"}</SelectItem>)}</SelectContent>
                  </Select>
                </div>
              )
            })}
          </Panel>
          <label className="mt-3 flex items-start gap-2.5 text-sm">
            <Switch checked={all} onCheckedChange={setAll} className="mt-0.5" />
            <span>Apply my next change to all businesses<span className="block text-xs text-muted-foreground">It also replaces this business's own setting. Otherwise a change applies here only, and a business's own setting wins over the shared one.</span></span>
          </label>
          <p className="mt-3 text-xs text-muted-foreground">In use: {d.model ? <span className="text-foreground">{d.model}</span> : <span className="text-red-500">{d.model_error ?? "not configured"}</span>}</p>
        </>
      )}
      {options.data && (
        <>
          <div className="mt-8 mb-2.5 flex items-center">
            <h3 className="flex-1 text-sm font-semibold">Providers</h3>
            <Button size="sm" variant="ghost" disabled={fresh} onClick={refresh}><RotateCw className={cn(fresh && "animate-spin")} /> Refresh the lists</Button>
          </div>
          <Panel className="divide-y">
            {options.data.providers.map((p) => (
              <Row key={p.id} label={p.name} hint={p.error ?? (p.configured ? `${p.models.length} models` : p.key ? `Needs ${p.key}` : "Not set up")}>
                {p.configured ? <span className="inline-flex items-center gap-1 text-xs text-emerald-500"><Check className="size-3.5" /> Ready</span>
                  : <span className="text-xs text-muted-foreground">Off</span>}
              </Row>
            ))}
          </Panel>
          <p className="mt-2"><Button size="sm" variant="outline" onClick={onKeys}><KeyRound /> Add a model key</Button></p>
        </>
      )}
    </>
  )
}

function Connections({ realm }: { realm: string }) {
  const conn = usePoll<{ connectors: Connector[] }>(() => api(`realms/${enc(realm)}/connectors`), 600000, [realm])
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
      <McpServers realm={realm} />
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
          <p className="mt-2 text-xs text-muted-foreground">Connect more from the Marketplace.</p>
        </>
      )}
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
