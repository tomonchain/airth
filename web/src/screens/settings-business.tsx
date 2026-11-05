import { useEffect, useRef, useState, type ReactNode } from "react"
import { Code2, GitPullRequest, Loader, Plus, Smartphone, X } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, raw, type Department } from "@/lib/api"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"
import { Remove } from "@/screens/settings-keys"

type Repo = { name: string; path: string; url: string | null; github: string | null; branch: string | null; main: boolean; delivery: string; checks: string[] }
type Detail = {
  name: string; description: string; root: string; own_root: boolean; image: string | null; default: boolean; archived: boolean; share_owner_profile: boolean
  policy: Record<string, string>; repos: Repo[]; github_token: boolean; departments: Department[]
  memory: { id: string; text: string; source: string; created_at: string }[]
}

export function Head({ title, children }: { title: string; children?: ReactNode }) {
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

const tilde = (p: string) => p.replace(/^\/Users\/[^/]+/, "~")
const useDetail = (realm: string) => usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])

/** Changes the business's settings; the server answers with all of them. */
async function patch(realm: string, body: object, done: string) {
  try { await api(`realms/${enc(realm)}`, body, "PATCH"); toast.success(done); return true }
  catch (e) { toast.error((e as Error).message); return false }
}

/** A picked image as a 256×256 PNG, centred (the classic interface's). */
async function square(file: File): Promise<Blob> {
  const bitmap = await createImageBitmap(file)
  const side = Math.min(bitmap.width, bitmap.height)
  const canvas = document.createElement("canvas")
  canvas.width = canvas.height = 256
  canvas.getContext("2d")!.drawImage(bitmap, (bitmap.width - side) / 2, (bitmap.height - side) / 2, side, side, 0, 0, 256, 256)
  return new Promise((ok, no) => canvas.toBlob((b) => (b ? ok(b) : no(new Error("cannot read this image"))), "image/png"))
}

/** A picture kept by the server, fetched with the key; its initial without one. */
export function Picture({ path, version, text, round, className }: { path: string; version: string | null; text: string; round?: boolean; className?: string }) {
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    if (!version) { setUrl(null); return }
    let live = true, made: string | null = null
    raw(path).then((r) => r.blob()).then((b) => { if (live) setUrl((made = URL.createObjectURL(b))) }, () => {})
    return () => { live = false; if (made) URL.revokeObjectURL(made) }
  }, [path, version])
  return (
    <span className={cn("grid size-14 shrink-0 place-items-center overflow-hidden bg-muted text-xl font-semibold text-muted-foreground", round ? "rounded-full" : "rounded-2xl", className)}>
      {url ? <img src={url} alt="" className="size-full object-cover" /> : text.slice(0, 1).toUpperCase()}
    </span>
  )
}

export function PictureEditor({ path, version, text, round, onDone }: { path: string; version: string | null; text: string; round?: boolean; onDone: () => void }) {
  const input = useRef<HTMLInputElement>(null)
  async function send(method: "PUT" | "DELETE", body?: Blob) {
    try { await raw(path, { method, body }); toast.success(method === "PUT" ? "Picture saved." : "Picture removed."); onDone() }
    catch (e) { toast.error((e as Error).message) }
  }
  return (
    <div className="flex items-center gap-3">
      <Picture path={path} version={version} text={text} round={round} />
      <Button size="sm" variant="outline" onClick={() => input.current?.click()}>{version ? "Change picture" : "Add picture"}</Button>
      {version && <Button size="sm" variant="ghost" onClick={() => send("DELETE")}>Remove</Button>}
      <input ref={input} type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden
        onChange={async (e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) send("PUT", await square(f).catch((err: Error) => { toast.error(err.message); throw err })) }} />
    </div>
  )
}

/** The business itself: picture, description, profile sharing, default, archive. */
export function Business({ realm, onChanged }: { realm: string; onChanged: () => void }) {
  const { data: d, refresh } = useDetail(realm)
  const [desc, setDesc] = useState("")
  useEffect(() => { if (d) setDesc(d.description) }, [d])
  const change = async (body: object, done: string) => { if (await patch(realm, body, done)) { refresh(); onChanged() } }
  return (
    <>
      <Head title="Business">{realm}, as the app and its agents see it.</Head>
      {!d ? <Loading /> : (
        <>
          <Panel className="space-y-4 p-4">
            <div className="space-y-1.5"><Label>Picture</Label><PictureEditor path={`realms/${enc(realm)}/image`} version={d.image} text={realm} onDone={() => { refresh(); onChanged() }} /><p className="text-xs text-muted-foreground">Shown in the app only.</p></div>
            <div className="space-y-1.5">
              <Label htmlFor="b-desc">Description</Label>
              <Textarea id="b-desc" rows={2} value={desc} onChange={(e) => setDesc(e.target.value)} placeholder="What this business is about: the agents read it in every task." />
              <div className="flex justify-end"><Button size="sm" disabled={desc.trim() === d.description} onClick={() => change({ description: desc.trim() }, "Description saved.")}>Save</Button></div>
            </div>
            <div className="space-y-0.5 text-sm">
              <div className="font-medium">Workspace</div>
              <div className="font-mono text-xs text-muted-foreground" title={d.root}>{tilde(d.root)}{d.own_root ? "" : " · a private folder in Airth's home"}</div>
            </div>
          </Panel>
          <Panel className="mt-2.5 divide-y">
            <Row label="Share your profile with this business's tasks" hint="Your name, what you do and your preferences, from Account.">
              <Switch checked={d.share_owner_profile} onCheckedChange={(on) => change({ share_owner_profile: on }, on ? "Your profile is shared." : "Your profile is no longer shared.")} />
            </Row>
            <Row label="Default business" hint={d.default ? "Airth opens this business when nothing else is chosen." : "The business Airth opens when nothing else is chosen."}>
              {d.default ? <span className="text-xs text-muted-foreground">Default</span> : <Button size="sm" variant="outline" onClick={() => change({ default: true }, "Default business set.")}>Make default</Button>}
            </Row>
            <Row label={d.archived ? "Archived" : "Archive"} hint="Hides the business from the switcher and stops its routines. Its tasks, memory and files are kept; you can restore it.">
              {d.archived ? <Button size="sm" variant="outline" onClick={() => change({ archived: false }, "Restored.")}>Restore</Button>
                : <Remove label={realm} onRemove={() => change({ archived: true }, "Archived.")} />}
            </Row>
          </Panel>
        </>
      )}
    </>
  )
}

/** The business's departments: the nine every business has, and the ones
    the founder adds (decision 79). Only here: an agent cannot add one. */
export function Departments({ realm }: { realm: string }) {
  const { data: d, refresh } = useDetail(realm)
  const [name, setName] = useState("")
  const own = (d?.departments ?? []).filter((x) => x.own).map((x) => x.key)
  const save = async (list: string[], done: string) => { if (await patch(realm, { departments: list }, done)) { setName(""); refresh() } }
  return (
    <>
      <Head title="Departments">How the team is grouped. Agents choose among these; only you add one.</Head>
      {!d ? <Loading /> : (
        <>
          <Panel className="divide-y">
            {d.departments.map((x) => (
              <div key={x.key} className="flex items-center gap-3 px-4 py-2.5">
                <span className="size-2.5 shrink-0 rounded-full" style={{ background: x.own ? "var(--dept-none)" : `var(--dept-${x.key})` }} />
                <span className="flex-1 text-sm">{x.name}</span>
                <span className="font-mono text-xs text-muted-foreground">{x.key}</span>
                {x.own ? <Remove label={x.name} onRemove={() => save(own.filter((k) => k !== x.key), `${x.name} removed.`)} />
                  : <span className="text-xs whitespace-nowrap text-muted-foreground">Built in</span>}
              </div>
            ))}
          </Panel>
          <div className="mt-2.5 flex gap-2">
            <Input value={name} maxLength={40} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && name.trim() && save([...own, name.trim()], "Department added.")} placeholder="A new department, like Events" />
            <Button variant="outline" disabled={!name.trim()} onClick={() => save([...own, name.trim()], "Department added.")}><Plus /> Add</Button>
          </div>
          <p className="mt-2 text-xs text-muted-foreground">A department with agents in it cannot be removed: move them first.</p>
        </>
      )}
    </>
  )
}

/** What the business remembers; other businesses never see it. */
export function Memory({ realm }: { realm: string }) {
  const { data: d, refresh } = useDetail(realm)
  const [text, setText] = useState("")
  async function add() {
    if (!text.trim()) return
    try { await api(`realms/${enc(realm)}/memory`, { text: text.trim() }); setText(""); toast.success("Remembered."); refresh() }
    catch (e) { toast.error((e as Error).message) }
  }
  return (
    <>
      <Head title="Memory">What the agents remember in this business. Tasks in other businesses never see it.</Head>
      <div className="mb-2.5 flex gap-2">
        <Input value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} placeholder="Something the agents should remember in this business…" />
        <Button variant="outline" disabled={!text.trim()} onClick={add}><Plus /> Add</Button>
      </div>
      {!d ? <Loading /> : (
        <Panel className="divide-y">
          {d.memory.length ? d.memory.map((m) => (
            <div key={m.id} className="flex items-start gap-3 px-4 py-2.5">
              <div className="min-w-0 flex-1">
                <p className="text-sm">{m.text}</p>
                <p className="text-xs text-muted-foreground">{m.source === "owner" ? "Added by you" : "Learned in a task"} · {ago(m.created_at)}</p>
              </div>
              <Button size="icon-sm" variant="ghost" className="text-muted-foreground" aria-label="Forget" title="Forget"
                onClick={async () => { try { await api(`realms/${enc(realm)}/memory/${enc(m.id)}`, undefined, "DELETE"); refresh() } catch (e) { toast.error((e as Error).message) } }}><X /></Button>
            </div>
          )) : <p className="px-4 py-3 text-sm text-muted-foreground">Nothing yet. Tasks add what they learn; you can add facts here.</p>}
        </Panel>
      )}
    </>
  )
}

const CAPS: [string, string, string][] = [
  ["read", "Read files", "Read files and list folders"],
  ["write", "Change files", "Create and change files"],
  ["execute", "Run commands", "Run shell commands"],
  ["memory", "Remember things", "Save facts to this business's memory"],
  ["network", "Use the web and integrations", "Call integrations and websites"],
  ["skill", "Use skills", "Use the skills you installed"],
]
const RULES: [string, string, string][] = [
  ["allow", "Allow", "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"],
  ["ask", "Ask", "bg-amber-500/15 text-amber-600 dark:text-amber-400"],
  ["deny", "Deny", "bg-red-500/15 text-red-600 dark:text-red-400"],
]

/** What an agent's mission may do alone, ask first, or never. Only the
    founder changes it; the server checks every call against it. */
export function Permissions({ realm }: { realm: string }) {
  const { data: d, refresh } = useDetail(realm)
  return (
    <>
      <Head title="Permissions">What an agent's mission may do in this business. Allow does it without asking, Ask waits for your approval, Deny never. Routines follow their own autonomy levels.</Head>
      {!d ? <Loading /> : (
        <Panel className="divide-y">
          {CAPS.map(([cap, name, help]) => (
            <Row key={cap} label={name} hint={help}>
              <div role="radiogroup" aria-label={name} className="flex rounded-lg border p-0.5">
                {RULES.map(([rule, label, on]) => (
                  <button key={rule} role="radio" aria-checked={d.policy[cap] === rule}
                    onClick={async () => { if (d.policy[cap] !== rule && await patch(realm, { policy: { [cap]: rule } }, `${name}: ${label.toLowerCase()}.`)) refresh() }}
                    className={cn("rounded-md px-2.5 py-1 text-xs transition-colors", d.policy[cap] === rule ? on : "text-muted-foreground hover:text-foreground")}>{label}</button>
                ))}
              </div>
            </Row>
          ))}
        </Panel>
      )}
    </>
  )
}

/** The git repositories the business's tasks work in: connect, choose how
    changes come back, set the checks, forget. */
export function Repositories({ realm, onKeys }: { realm: string; onKeys: () => void }) {
  const { data: d, refresh } = useDetail(realm)
  const [adding, setAdding] = useState(false)
  const [how, setHow] = useState<"github" | "address">("github")
  const [f, setF] = useState({ source: "", name: "", path: "" })
  const [busy, setBusy] = useState(false)
  const path = `realms/${enc(realm)}/repos`
  async function change(name: string, body: object, done: string) {
    try { await api(`${path}/${enc(name)}`, body, "PATCH"); toast.success(done); refresh() } catch (e) { toast.error((e as Error).message) }
  }
  async function connectFrom(body: typeof f) {
    if (!body.source.trim()) return
    setBusy(true)
    setF(body)
    try { await api(path, body, undefined, 300000); toast.success(`${body.name || "Repository"} connected.`); setAdding(false); setF({ source: "", name: "", path: "" }); refresh() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  const connect = () => connectFrom(f)
  const cloning = /:\/\/|^git@/.test(f.source)
  return (
    <>
      <div className="mb-6 flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-semibold tracking-tight">Repositories</h2>
          <p className="mt-1 text-sm text-muted-foreground">The git repositories this business's tasks work in. A task picks one, works on its own copy, and you review what it changed.</p>
        </div>
        {!adding && <Button size="sm" variant="outline" onClick={() => setAdding(true)}><Plus /> Connect</Button>}
      </div>
      {adding && (
        <Panel className="mb-3 p-4">
          <div className="mb-3 flex items-center gap-1 text-xs">
            {([["github", "From GitHub"], ["address", "By address or folder"]] as const).map(([v, l]) => (
              <button key={v} onClick={() => setHow(v)} className={cn("rounded-md px-2.5 py-1", how === v ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")}>{l}</button>
            ))}
          </div>
          {how === "github" ? (
            <GitHubPicker realm={realm} connected={d?.repos ?? []} busy={busy ? f.source : null} onKeys={onKeys}
              onPick={(r) => connectFrom({ source: r.url, name: r.name, path: "" })} onCancel={() => setAdding(false)} />
          ) : (
            <div className="space-y-3">
              <Input autoFocus value={f.source} onChange={(e) => setF({ ...f, source: e.target.value })} onKeyDown={(e) => e.key === "Enter" && connect()}
                placeholder="https://github.com/owner/name or ~/Code/project" spellCheck={false} className="font-mono" />
              <div className="grid grid-cols-2 gap-2">
                <Input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} placeholder="Name (optional)" spellCheck={false} />
                <Input value={f.path} onChange={(e) => setF({ ...f, path: e.target.value })} placeholder="Where to clone it (optional)" spellCheck={false} className="font-mono" />
              </div>
              <p className="text-xs text-muted-foreground">A URL is cloned next to the business's folder, or where you say; a folder is used where it is. Airth detects its checks.</p>
              <div className="flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setAdding(false)}>Cancel</Button>
                <Button size="sm" disabled={busy || !f.source.trim()} onClick={connect}>{busy ? <><Loader className="animate-spin" /> {cloning ? "Cloning…" : "Connecting…"}</> : "Connect"}</Button>
              </div>
            </div>
          )}
        </Panel>
      )}
      {!d ? <Loading /> : (
        <>
          {d.repos.some((r) => r.delivery === "pull_request") && !d.github_token && (
            <p className="mb-3 rounded-lg bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
              Pull requests need a GitHub token: add GITHUB_TOKEN in <button className="underline" onClick={onKeys}>Keys</button> (a fine-grained token for these repositories, with Contents and Pull requests).
            </p>
          )}
          <div className="space-y-2.5">
            {d.repos.length ? d.repos.map((r) => <RepoCard key={r.name} r={r} onChange={change} onForget={async () => {
              try { await api(`${path}/${enc(r.name)}`, undefined, "DELETE"); toast.success("Repository forgotten; its files stay."); refresh() } catch (e) { toast.error((e as Error).message) }
            }} />) : !adding && (
              <Panel className="flex flex-col items-center px-6 py-10 text-center">
                <Code2 className="size-7 text-muted-foreground/60" />
                <p className="mt-3 text-sm font-medium">No repositories</p>
                <p className="mt-1 max-w-sm text-sm text-muted-foreground">Connect one to have agents work on code: each task gets its own copy, checked before you review it.</p>
              </Panel>
            )}
          </div>
        </>
      )}
    </>
  )
}

function RepoCard({ r, onChange, onForget }: { r: Repo; onChange: (name: string, body: object, done: string) => void; onForget: () => Promise<void> }) {
  const [checks, setChecks] = useState(r.checks.join("\n"))
  useEffect(() => setChecks(r.checks.join("\n")), [r.checks])
  const lines = (s: string) => s.split("\n").map((x) => x.trim()).filter(Boolean)
  const same = lines(checks).join("\n") === r.checks.join("\n")
  return (
    <Panel className="p-4">
      <div className="flex items-start gap-3">
        {r.github ? <GitPullRequest className="mt-0.5 size-4 text-muted-foreground" /> : <Code2 className="mt-0.5 size-4 text-muted-foreground" />}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">{r.name}</span>
            {r.main && <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">Business folder</span>}
            {r.branch && <span className="text-xs text-muted-foreground">on {r.branch}</span>}
          </div>
          <div className="truncate font-mono text-xs text-muted-foreground" title={r.path}>
            {tilde(r.path)}{r.github && <> · <a className="hover:underline" href={`https://github.com/${r.github}`} target="_blank" rel="noreferrer">{r.github}</a></>}
          </div>
        </div>
        {!r.main && <Remove label={r.name} onRemove={onForget} />}
      </div>
      <div className="mt-4 flex items-center gap-4">
        <div className="w-20 text-xs text-muted-foreground">Changes</div>
        <div className="flex rounded-lg border p-0.5">
          {([["adopt", "Adopt here"], ["pull_request", "Pull requests"]] as const).map(([v, label]) => (
            <button key={v} disabled={v === "pull_request" && !r.github} title={v === "pull_request" && !r.github ? "Only for a repository on GitHub" : undefined}
              onClick={() => r.delivery !== v && onChange(r.name, { delivery: v }, v === "adopt" ? "You adopt changes here." : "Changes become pull requests.")}
              className={cn("rounded-md px-2.5 py-1 text-xs transition-colors disabled:opacity-40", r.delivery === v ? "bg-accent text-foreground" : "text-muted-foreground enabled:hover:text-foreground")}>{label}</button>
          ))}
        </div>
      </div>
      <div className="mt-3 flex gap-4">
        <div className="w-20 pt-2 text-xs text-muted-foreground">Checks</div>
        <div className="min-w-0 flex-1 space-y-1.5">
          <Textarea rows={Math.max(2, lines(checks).length)} value={checks} onChange={(e) => setChecks(e.target.value)} spellCheck={false}
            placeholder="One command per line, e.g. cargo test" className="font-mono text-xs" />
          <div className="flex items-center gap-2">
            <p className="flex-1 text-xs text-muted-foreground">Run on every change, in the task's sandbox, before you review it.</p>
            {!same && <Button size="xs" onClick={() => onChange(r.name, { checks: lines(checks) }, "Checks saved.")}>Save</Button>}
          </div>
        </div>
      </div>
    </Panel>
  )
}

type Pairing = { host: string; link: string; qr: { width: number; cells: string } }

/** The phone (decision 37): Tailscale brings Airth to the founder's own
    devices; this pairs one with a code to scan. */
export function Phone() {
  const { data, refresh } = usePoll<{ hosts: Pairing[] }>(() => api("phone"), 600000, [])
  const [host, setHost] = useState("")
  async function allow() {
    try { await api("phone/hosts", { host: host.trim() }); toast.success("Name allowed."); setHost(""); refresh() }
    catch (e) { toast.error((e as Error).message) }
  }
  return (
    <>
      <Head title="Phone">Use Airth from your phone, at home or away. It stays on this Mac: Tailscale brings it to your own devices only, encrypted, and nothing is opened to the internet. Telegram stays the channel for Needs you.</Head>
      <Panel className="space-y-3 p-4">
        <ol className="list-decimal space-y-1.5 pl-5 text-sm">
          <li>Install Tailscale on this Mac and on your phone, signed in to the same account on both.</li>
          <li>On this Mac, in a terminal: <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">tailscale serve --bg 7457</code>. It prints the Mac's address, like https://mac.tail1234.ts.net.</li>
          <li>Give that name below, then scan the code with your phone's camera.</li>
        </ol>
        <div className="flex gap-2">
          <Input value={host} onChange={(e) => setHost(e.target.value)} onKeyDown={(e) => e.key === "Enter" && allow()} placeholder="mac.tail1234.ts.net" spellCheck={false} className="font-mono" />
          <Button disabled={!host.trim()} onClick={allow}>Allow</Button>
        </div>
      </Panel>
      {!data ? <Loading /> : data.hosts.map((x) => (
        <Panel key={x.host} className="mt-2.5 flex gap-5 p-4">
          <Qr qr={x.qr} label={`Pairing code for ${x.host}`} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 text-sm font-medium"><Smartphone className="size-4 text-muted-foreground" /> {x.host}</div>
            <p className="mt-1 text-xs text-muted-foreground">Scan it with the phone's camera, with Tailscale on. It carries Airth's key: whoever scans it controls Airth, so keep it to yourself.</p>
            <Button className="mt-3" size="sm" variant="ghost" onClick={async () => {
              try { await api(`phone/hosts/${enc(x.host)}`, undefined, "DELETE"); toast.success(`${x.host} forgotten.`); refresh() } catch (e) { toast.error((e as Error).message) }
            }}>Forget this name</Button>
          </div>
        </Panel>
      ))}
    </>
  )
}

/** A QR code from the server's modules ("1" dark), drawn as a grid. */
function Qr({ qr, label }: { qr: Pairing["qr"]; label: string }) {
  return (
    <div role="img" aria-label={label} className="shrink-0 rounded-lg bg-white p-2.5">
      <div className="grid size-36" style={{ gridTemplateColumns: `repeat(${qr.width}, 1fr)` }}>
        {[...qr.cells].map((c, i) => <span key={i} className={c === "1" ? "bg-black" : ""} />)}
      </div>
    </div>
  )
}

type GhRepo = { full_name: string; name: string; private: boolean; archived: boolean; description: string | null; url: string; default_branch: string; pushed_at: string | null }

/** The repositories the business's GITHUB_TOKEN reaches, to pick one:
    cloned with that token, never typed in. */
function GitHubPicker({ realm, connected, busy, onKeys, onPick, onCancel }: {
  realm: string; connected: Repo[]; busy: string | null; onKeys: () => void; onPick: (r: GhRepo) => void; onCancel: () => void
}) {
  const { data } = usePoll<{ token: boolean; repos: GhRepo[]; error?: string }>(() => api(`realms/${enc(realm)}/github/repos`), 600000, [realm])
  const [q, setQ] = useState("")
  // Already here: the same GitHub repository, whatever the address's form.
  const slug = (u: string | null | undefined) => u?.match(/github\.com[/:]([^/]+\/[^/.]+?)(\.git)?\/?$/i)?.[1]?.toLowerCase()
  const have = new Set(connected.map((r) => slug(r.github ? `github.com/${r.github}` : r.url)).filter(Boolean))
  if (!data) return <p className="py-2 text-sm text-muted-foreground"><Loader className="mr-1.5 inline size-3.5 animate-spin" />Asking GitHub…</p>
  if (!data.token) {
    return (
      <div className="space-y-2 text-sm">
        <p>To pick from your GitHub repositories, this business needs a <span className="font-mono">GITHUB_TOKEN</span>: a fine-grained token for the repositories its agents may work on, with Contents and Pull requests (read and write).</p>
        <div className="flex gap-2">
          <Button size="sm" onClick={onKeys}>Add GITHUB_TOKEN in Keys</Button>
          <a className="inline-flex items-center gap-1 text-xs text-sky-500 hover:underline" href="https://github.com/settings/personal-access-tokens/new" target="_blank" rel="noreferrer">Create one on GitHub</a>
          <span className="flex-1" />
          <Button size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
        </div>
      </div>
    )
  }
  if (data.error) return <p className="rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300">{data.error}</p>
  const shown = data.repos.filter((r) => !q || `${r.full_name} ${r.description ?? ""}`.toLowerCase().includes(q.toLowerCase()))
  return (
    <div className="space-y-2.5">
      <Input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Search ${data.repos.length} repositories`} aria-label="Search repositories" />
      <div className="max-h-80 divide-y overflow-y-auto rounded-lg border">
        {shown.length ? shown.map((r) => {
          const here = have.has(r.full_name.toLowerCase())
          return (
            <div key={r.full_name} className={cn("flex items-center gap-3 px-3 py-2", r.archived && "opacity-60")}>
              <GitPullRequest className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-sm">
                  <span className="truncate font-medium">{r.full_name}</span>
                  {r.private && <span className="rounded bg-muted px-1 text-[10px] text-muted-foreground">Private</span>}
                  {r.archived && <span className="rounded bg-muted px-1 text-[10px] text-muted-foreground">Archived</span>}
                </div>
                <div className="truncate text-xs text-muted-foreground">{[r.description, r.pushed_at && `pushed ${ago(r.pushed_at)}`].filter(Boolean).join(" · ")}</div>
              </div>
              {here ? <span className="text-xs text-emerald-500">Connected</span>
                : <Button size="xs" variant="outline" disabled={!!busy} onClick={() => onPick(r)}>{busy === r.url ? <><Loader className="animate-spin" /> Cloning…</> : "Connect"}</Button>}
            </div>
          )
        }) : <p className="px-3 py-3 text-sm text-muted-foreground">No repository matches.</p>}
      </div>
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">Cloned next to the business's folder with its token. Not listed? The token does not reach it.</p>
        <Button size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  )
}
