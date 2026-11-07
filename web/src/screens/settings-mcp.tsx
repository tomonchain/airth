import { useEffect, useState } from "react"
import { ChevronRight, Loader, Plug, Plus, RotateCw } from "lucide-react"
import { toast } from "sonner"
import { ConnectDialog, type Method } from "@/components/connect-dialog"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"
import { Remove } from "@/screens/settings-keys"

type Server = {
  name: string; command: string | null; url: string | null; description: string; enabled: boolean; signed_in: boolean; connecting: boolean
  server_info: string | null; connected_at: string | null; error: string | null; tools: { agent_name: string; description: string | null }[]
}

const slug = (v: string) => v.toLowerCase().replace(/[^a-z0-9_-]/g, "-")

/** The business's MCP servers (the classic interface's): each on or off for
    the tasks, reconnected or removed; a local one added by its command, a
    remote one by its address (it opens its sign-in page if it has one). */
export function McpServers({ realm }: { realm: string }) {
  const { data, refresh } = usePoll<Server[]>(() => api(`realms/${enc(realm)}/mcp`), 600000, [realm])
  const [adding, setAdding] = useState<"url" | "command" | null>(null)
  const [f, setF] = useState({ name: "", url: "", command: "", description: "", token: "" })
  const [method, setMethod] = useState<Method | null>(null)
  const [busy, setBusy] = useState(false)
  const path = `realms/${enc(realm)}/mcp`

  // While one connects, look again until it is done.
  const connecting = !!data?.some((s) => s.connecting)
  useEffect(() => {
    if (!connecting) return
    const t = setInterval(refresh, 1500)
    return () => clearInterval(t)
  }, [connecting, refresh])

  async function call(url: string, body?: object, verb?: "PATCH" | "DELETE") {
    try { await api(url, body, verb, 120000); refresh() } catch (e) { toast.error((e as Error).message); refresh() }
  }
  async function addCommand() {
    setBusy(true)
    try { await api(path, { name: f.name, command: f.command.trim(), description: f.description.trim() }, undefined, 120000); toast.success(`${f.name} added.`); setAdding(null); setF({ name: "", url: "", command: "", description: "", token: "" }); refresh() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  async function addUrl() {
    // A token the service gave (decision 89): no sign-in page.
    if (f.token.trim()) {
      setBusy(true)
      try {
        await api(`realms/${enc(realm)}/connect`, { name: f.name, url: f.url.trim(), description: f.description.trim(), token: f.token.trim() }, undefined, 120000)
        toast.success(`${f.name} added.`); setAdding(null); setF({ name: "", url: "", command: "", description: "", token: "" }); refresh()
      } catch (e) { toast.error((e as Error).message) }
      finally { setBusy(false) }
      return
    }
    // The connect dialog does the rest, as from the Marketplace: not checked
    // by Airth, so it says so.
    setMethod({ kind: "sign-in", name: f.name, title: f.name, url: f.url.trim(), description: f.description.trim(), checked: false, tried: false })
  }

  return (
    <>
      <div className="mb-2.5 flex items-center gap-2">
        <h3 className="flex-1 text-sm font-semibold">MCP servers</h3>
        <Button size="sm" variant="ghost" onClick={() => setAdding(adding === "url" ? null : "url")}><Plus /> By address</Button>
        <Button size="sm" variant="ghost" onClick={() => setAdding(adding === "command" ? null : "command")}><Plus /> Local, by command</Button>
      </div>
      {adding && (
        <Panel className="mb-2.5 space-y-2.5 p-4">
          <div className="grid grid-cols-[1fr_2fr] gap-2">
            <Input autoFocus value={f.name} onChange={(e) => setF({ ...f, name: slug(e.target.value) })} placeholder="Name, e.g. crm" spellCheck={false} aria-label="Name" />
            {adding === "url"
              ? <Input value={f.url} onChange={(e) => setF({ ...f, url: e.target.value })} placeholder="https://mcp.example.com/mcp" spellCheck={false} className="font-mono" aria-label="Server address" />
              : <Input value={f.command} onChange={(e) => setF({ ...f, command: e.target.value })} placeholder="npx -y @modelcontextprotocol/server-memory" spellCheck={false} className="font-mono" aria-label="Command" />}
          </div>
          <Input value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} placeholder="What it is for (optional)" aria-label="What it is for" />
          {adding === "url" && <Input type="password" value={f.token} onChange={(e) => setF({ ...f, token: e.target.value })} placeholder="Access token, if the service gave you one (optional)" autoComplete="off" spellCheck={false} aria-label="Access token" />}
          <p className="text-xs text-muted-foreground">{adding === "url"
            ? "A remote MCP server (Streamable HTTP). With a token, Airth sends it to this server only; without, its sign-in page opens if it has one. The access stays in this business, out of the agents' reach."
            : "Started on this Mac, in the sandbox, each time a task needs it. Only add a command you trust."}</p>
          <div className="flex justify-end gap-2">
            <Button size="sm" variant="ghost" onClick={() => setAdding(null)}>Cancel</Button>
            <Button size="sm" disabled={busy || !f.name || (adding === "url" ? !/^https:\/\//.test(f.url.trim()) : !f.command.trim())} onClick={adding === "url" ? addUrl : addCommand}>
              {busy ? <><Loader className="animate-spin" /> Connecting…</> : "Connect"}
            </Button>
          </div>
        </Panel>
      )}
      {!data ? <Loading /> : data.length ? (
        <Panel className="divide-y">{data.map((s) => <ServerRow key={s.name} s={s} path={`${path}/${enc(s.name)}`} call={call} />)}</Panel>
      ) : <Panel className="px-4 py-3 text-sm text-muted-foreground">None yet. Connect services from the Marketplace, or add a server here.</Panel>}
      <ConnectDialog realm={realm} method={method} onClose={() => { setMethod(null); refresh() }} onConnected={() => { setAdding(null); setF({ name: "", url: "", command: "", description: "", token: "" }); refresh() }} />
    </>
  )
}

function ServerRow({ s, path, call }: { s: Server; path: string; call: (url: string, body?: object, verb?: "PATCH" | "DELETE") => Promise<void> }) {
  const [open, setOpen] = useState(false)
  const status = s.connecting ? <span className="inline-flex items-center gap-1 text-xs text-muted-foreground"><Loader className="size-3.5 animate-spin" /> Connecting</span>
    : s.error ? <span className="text-xs text-red-500">Error</span>
    : s.tools.length ? <span className="text-xs text-emerald-500">{s.tools.length} tool{s.tools.length === 1 ? "" : "s"}</span>
    : <span className="text-xs text-muted-foreground">{s.url && !s.signed_in ? "Not signed in" : "Not connected"}</span>
  return (
    <div className={cn("px-4 py-3", !s.enabled && "opacity-60")}>
      <div className="flex items-center gap-3">
        <Plug className="size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2"><span className="font-mono text-[13px] font-medium">{s.name}</span>{status}</div>
          <div className="truncate text-xs text-muted-foreground">
            {[s.description, s.url ?? (s.command && `$ ${s.command}`), s.connected_at && `connected ${ago(s.connected_at)}`].filter(Boolean).join(" · ")}
          </div>
        </div>
        <Switch checked={s.enabled} title={s.enabled ? "Tasks can use it" : "Hidden from tasks"} aria-label={`${s.name} on for tasks`}
          onCheckedChange={(on) => call(path, { enabled: on }, "PATCH")} />
        <Button size="icon-sm" variant="ghost" disabled={s.connecting} title={s.tools.length ? "Reconnect" : "Connect"} aria-label={`Reconnect ${s.name}`} onClick={() => call(`${path}/connect`, {})}><RotateCw /></Button>
        <Remove label={s.name} onRemove={() => call(path, undefined, "DELETE")} />
      </div>
      {s.error && <pre className="mt-2 max-h-28 overflow-auto rounded-lg bg-red-500/10 p-2 font-mono text-[11px] whitespace-pre-wrap text-red-600 dark:text-red-300">{s.error}</pre>}
      {s.tools.length > 0 && (
        <div className="mt-1.5 pl-7">
          <button onClick={() => setOpen(!open)} className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
            <ChevronRight className={cn("size-3 transition-transform", open && "rotate-90")} /> Tools tasks get
          </button>
          {open && (
            <div className="mt-1 space-y-0.5">
              {s.tools.map((t) => <div key={t.agent_name} className="flex gap-2 text-xs"><code className="shrink-0 font-mono">{t.agent_name}</code><span className="truncate text-muted-foreground">{t.description}</span></div>)}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
