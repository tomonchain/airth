import { useState } from "react"
import { ExternalLink, FolderOpen } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"

type Status = { connected: boolean; folder: string | null; name: string | null; error: string | null }

/** Google Drive, read only, one folder (decision 85): connected with the
    founder's own Google OAuth client. Agents reach it when their file names
    google-drive. */
export function DriveConnection({ realm, heading = true }: { realm: string; heading?: boolean }) {
  const s = usePoll<Status>(() => api(`realms/${enc(realm)}/drive`), 30000, [realm])
  const [form, setForm] = useState({ client_id: "", client_secret: "", folder: "" })
  const [busy, setBusy] = useState(false)
  const port = location.port || "7457"
  async function connect() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/drive`, form)
      toast.success("Google's sign-in page opened: allow access, then come back here.")
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  async function disconnect() {
    try { await api(`realms/${enc(realm)}/drive`, undefined, "DELETE"); s.refresh(); toast.success("Google Drive disconnected.") }
    catch (e) { toast.error((e as Error).message) }
  }
  return (
    <>
      {heading && <h3 className="mt-8 mb-2.5 text-sm font-semibold">Google Drive</h3>}
      {!s.data ? <Loading /> : s.data.connected ? (
        <Panel className="flex items-center gap-3 px-4 py-3">
          <FolderOpen className="size-5 shrink-0 text-sky-500" />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium">{s.data.name ?? "The shared folder"}</div>
            <div className="text-xs text-muted-foreground">{s.data.error ?? "Read only. Give an agent google-drive in its Edit form to let it read this folder."}</div>
          </div>
          <Button size="sm" variant="outline" onClick={() => s.refresh()}>Refresh</Button>
          <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={disconnect}>Disconnect</Button>
        </Panel>
      ) : (
        <Panel className="space-y-4 p-4">
          <p className="text-sm text-foreground/80">Agents read one folder of your Drive, never anything else, and never write to it. Google asks for your own sign-in client, once:</p>
          <ol className="list-decimal space-y-1.5 pl-5 text-sm text-foreground/80">
            <li>In <a className="underline" href="https://console.cloud.google.com/projectcreate" target="_blank" rel="noreferrer">Google Cloud <ExternalLink className="inline size-3" /></a>, create a project (any name, e.g. “Airth”).</li>
            <li>Enable the <a className="underline" href="https://console.cloud.google.com/apis/library/drive.googleapis.com" target="_blank" rel="noreferrer">Google Drive API <ExternalLink className="inline size-3" /></a>.</li>
            <li>In <a className="underline" href="https://console.cloud.google.com/auth/overview" target="_blank" rel="noreferrer">Google Auth Platform <ExternalLink className="inline size-3" /></a>, set it up as External, then under Audience click <b>Publish app</b>: left in Testing, Google ends the connection every 7 days. When you connect, Google says it has not verified the app: it is yours, choose Advanced → continue.</li>
            <li>Under <a className="underline" href="https://console.cloud.google.com/auth/clients" target="_blank" rel="noreferrer">Clients <ExternalLink className="inline size-3" /></a>, create a client of type <b>Desktop app</b>, and copy its ID and secret below.</li>
            <li>In Drive, open the business's folder and copy its address.</li>
          </ol>
          <div className="grid gap-3">
            <div className="space-y-1.5"><Label htmlFor="d-id">Client ID</Label><Input id="d-id" value={form.client_id} spellCheck={false} placeholder="…apps.googleusercontent.com" onChange={(e) => setForm({ ...form, client_id: e.target.value })} /></div>
            <div className="space-y-1.5"><Label htmlFor="d-secret">Client secret</Label><Input id="d-secret" type="password" value={form.client_secret} onChange={(e) => setForm({ ...form, client_secret: e.target.value })} /></div>
            <div className="space-y-1.5"><Label htmlFor="d-folder">Folder</Label><Input id="d-folder" value={form.folder} spellCheck={false} placeholder="https://drive.google.com/drive/folders/…" onChange={(e) => setForm({ ...form, folder: e.target.value })} /></div>
          </div>
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs text-muted-foreground">Google brings you back to http://127.0.0.1:{port}. Nothing else is needed.</p>
            <Button disabled={busy || !form.client_id.trim() || !form.client_secret.trim() || !form.folder.trim()} onClick={connect}>Connect</Button>
          </div>
        </Panel>
      )}
    </>
  )
}
