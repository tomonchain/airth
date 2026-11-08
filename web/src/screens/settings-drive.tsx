import { useEffect, useState } from "react"
import { ChevronRight, ExternalLink, Folder, FolderOpen, Loader } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { cn } from "@/lib/utils"

type Status = { connected: boolean; signed_in: boolean; client: boolean; folder: string | null; name: string | null; error: string | null }
type Item = { id: string; name: string }

/** Google Drive, read only, one folder (decisions 85 and 104): the
    founder's Google client once for every business, then one click to sign
    in, then the folder picked from a list. Agents reach it when their file
    names google-drive. */
export function DriveConnection({ realm, heading = true }: { realm: string; heading?: boolean }) {
  // Faster while Google's page is open in the browser.
  const [waiting, setWaiting] = useState(0)
  const s = usePoll<Status>(() => api(`realms/${enc(realm)}/drive`), waiting > Date.now() ? 2000 : 30000, [realm, waiting])
  const [client, setClient] = useState(false)
  const [form, setForm] = useState({ client_id: "", client_secret: "" })
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (s.data?.signed_in) setWaiting(0) }, [s.data?.signed_in])

  async function signIn(withClient: boolean) {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/drive`, withClient ? form : {})
      setClient(false); setForm({ client_id: "", client_secret: "" })
      setWaiting(Date.now() + 180000)
      toast.success("Google's page opened in your browser: choose the account, allow, then come back.")
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  async function disconnect() {
    try { await api(`realms/${enc(realm)}/drive`, undefined, "DELETE"); s.refresh(); toast.success("Google Drive disconnected.") }
    catch (e) { toast.error((e as Error).message) }
  }

  const d = s.data
  return (
    <>
      {heading && <h3 className="mt-8 mb-2.5 text-sm font-semibold">Google Drive</h3>}
      {!d ? <Loading /> : d.connected ? (
        <Panel className="flex items-center gap-3 px-4 py-3">
          <FolderOpen className="size-5 shrink-0 text-sky-500" />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium">{d.name ?? "The shared folder"}</div>
            <div className="text-xs text-muted-foreground">{d.error ?? "Read only. Give an agent google-drive in its Edit form to let it read this folder."}</div>
          </div>
          <Button size="sm" variant="outline" onClick={() => s.refresh()}>Refresh</Button>
          <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={disconnect}>Disconnect</Button>
        </Panel>
      ) : d.signed_in ? (
        <Picker realm={realm} onChosen={() => { s.refresh(); toast.success("Folder chosen: agents given google-drive read it now.") }} onCancel={disconnect} />
      ) : d.client && !client ? (
        <Panel className="flex items-center gap-4 p-4">
          <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-sky-500/10"><FolderOpen className="size-5 text-sky-500" /></span>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium">Google Drive</div>
            <div className="text-xs text-muted-foreground">Agents read one folder you choose, never anything else, and never write to it.</div>
          </div>
          <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setClient(true)}>Change client</Button>
          <Button disabled={busy} onClick={() => signIn(false)}>{busy || waiting > Date.now() ? <><Loader className="animate-spin" /> Waiting for Google</> : "Connect"}</Button>
        </Panel>
      ) : (
        <Panel className="space-y-4 p-4">
          <p className="text-sm text-foreground/80">Google asks for your own sign-in client, <b>once for all your businesses</b>. After this, each business connects in one click.</p>
          <ol className="list-decimal space-y-1.5 pl-5 text-sm text-foreground/80">
            <li>In <a className="underline" href="https://console.cloud.google.com/projectcreate" target="_blank" rel="noreferrer">Google Cloud <ExternalLink className="inline size-3" /></a>, create a project (any name, e.g. “Airth”).</li>
            <li>Enable the <a className="underline" href="https://console.cloud.google.com/apis/library/drive.googleapis.com" target="_blank" rel="noreferrer">Google Drive API <ExternalLink className="inline size-3" /></a>.</li>
            <li>In <a className="underline" href="https://console.cloud.google.com/auth/overview" target="_blank" rel="noreferrer">Google Auth Platform <ExternalLink className="inline size-3" /></a>, set it up as External, then under Audience click <b>Publish app</b>: left in Testing, Google ends the connection every 7 days. When you connect, Google says it has not verified the app: it is yours, choose Advanced, then continue.</li>
            <li>Under <a className="underline" href="https://console.cloud.google.com/auth/clients" target="_blank" rel="noreferrer">Clients <ExternalLink className="inline size-3" /></a>, create a client of type <b>Desktop app</b>, and copy its ID and secret below.</li>
          </ol>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5"><Label htmlFor="d-id">Client ID</Label><Input id="d-id" value={form.client_id} spellCheck={false} placeholder="…apps.googleusercontent.com" onChange={(e) => setForm({ ...form, client_id: e.target.value })} /></div>
            <div className="space-y-1.5"><Label htmlFor="d-secret">Client secret</Label><Input id="d-secret" type="password" value={form.client_secret} onChange={(e) => setForm({ ...form, client_secret: e.target.value })} /></div>
          </div>
          <div className="flex items-center justify-end gap-2">
            {d.client && <Button variant="ghost" onClick={() => setClient(false)}>Cancel</Button>}
            <Button disabled={busy || !form.client_id.trim() || !form.client_secret.trim()} onClick={() => signIn(true)}>Save and connect</Button>
          </div>
        </Panel>
      )}
    </>
  )
}

/** The business's folder, picked from the account's Drive once signed in. */
function Picker({ realm, onChosen, onCancel }: { realm: string; onChosen: () => void; onCancel: () => void }) {
  const [path, setPath] = useState<Item[]>([{ id: "root", name: "My Drive" }])
  const here = path[path.length - 1]
  const list = usePoll<{ folders: Item[] }>(() => api(`realms/${enc(realm)}/drive/folders?in=${enc(here.id)}`), 600000, [realm, here.id])
  const [busy, setBusy] = useState(false)
  const top = path.length === 1
  async function choose(f: Item) {
    setBusy(true)
    try { await api(`realms/${enc(realm)}/drive/folder`, { id: f.id }); onChosen() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  return (
    <Panel className="overflow-hidden">
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium">Choose the business's folder</div>
          <div className="text-xs text-muted-foreground">Its agents will read this folder and what is under it, nothing else.</div>
        </div>
        <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={onCancel}>Cancel</Button>
      </div>
      <div className="flex items-center gap-1 overflow-x-auto px-3 pt-2.5 text-xs">
        {top ? [["root", "My Drive"], ["shared", "Shared with me"]].map(([id, name]) => (
          <button key={id} onClick={() => setPath([{ id, name }])} className={cn("rounded-md px-2 py-1", here.id === id ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")}>{name}</button>
        )) : path.map((p, n) => (
          <span key={p.id} className="flex items-center gap-1">
            {n > 0 && <ChevronRight className="size-3 text-muted-foreground" />}
            <button onClick={() => setPath(path.slice(0, n + 1))} className={cn("rounded px-1 py-0.5 hover:bg-accent", n === path.length - 1 ? "font-medium" : "text-muted-foreground")}>{p.name}</button>
          </span>
        ))}
      </div>
      <ul className="max-h-72 overflow-y-auto p-2">
        {!list.data ? <li className="px-2 py-3 text-sm text-muted-foreground"><Loader className="mr-1.5 inline size-4 animate-spin" />Asking Google Drive…</li>
          : list.error ? <li className="px-2 py-3 text-sm text-red-500">{list.error}</li>
          : !list.data.folders.length ? <li className="px-2 py-3 text-sm text-muted-foreground">No folder here.</li>
          : list.data.folders.map((f) => (
            <li key={f.id} className="group flex items-center gap-2 rounded-lg px-2 py-1.5 hover:bg-accent/60">
              <button className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm" onClick={() => setPath([...path, f])}>
                <Folder className="size-4 shrink-0 text-sky-500" /><span className="truncate">{f.name}</span>
              </button>
              <Button size="sm" variant="outline" disabled={busy} className="opacity-0 group-hover:opacity-100 max-md:opacity-100" onClick={() => choose(f)}>Choose</Button>
            </li>
          ))}
      </ul>
      {!top && (
        <div className="flex justify-end border-t px-4 py-3">
          <Button disabled={busy} onClick={() => choose(here)}>Choose “{here.name}”</Button>
        </div>
      )}
    </Panel>
  )
}
