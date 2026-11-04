import { useState } from "react"
import { Check, Globe, KeyRound, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"

type Integration = { name: string; description: string; base_url: string; headers: Record<string, string>; missing_secrets: string[] }
type Detail = { secrets: { key: string; provider: boolean }[]; integrations: Integration[] }

// The names a header asks for, as `[secret:NAME]`.
const asked = (i: Integration) => Object.values(i.headers).flatMap((v) => [...v.matchAll(/\[secret:([A-Za-z0-9_]+)\]/g)].map((m) => m[1]))

/** A button that asks once more before it removes. */
function Remove({ label, onRemove }: { label: string; onRemove: () => Promise<void> }) {
  const [sure, setSure] = useState(false)
  const [busy, setBusy] = useState(false)
  if (!sure) return <Button size="icon-sm" variant="ghost" className="text-muted-foreground" aria-label={`Remove ${label}`} title="Remove" onClick={() => setSure(true)}><Trash2 /></Button>
  return (
    <span className="flex items-center gap-1">
      <Button size="sm" variant="destructive" disabled={busy} onClick={async () => { setBusy(true); try { await onRemove() } finally { setBusy(false); setSure(false) } }}>Remove</Button>
      <Button size="sm" variant="ghost" onClick={() => setSure(false)}>Keep</Button>
    </span>
  )
}

/** The business's keys and the services its tasks may call with them
    (decisions 7 and 10): a key's value is written once and never shown
    again, and reaches a service only through an integration's header. */
export function Keys({ realm }: { realm: string }) {
  const detail = usePoll<Detail>(() => api(`realms/${enc(realm)}`), 600000, [realm])
  const [adding, setAdding] = useState<{ name: string } | null>(null)
  const [integrating, setIntegrating] = useState(false)
  const d = detail.data

  async function remove(path: string, what: string) {
    try { await api(`realms/${enc(realm)}/${path}`, undefined, "DELETE"); toast.success(`${what} removed.`); detail.refresh() }
    catch (e) { toast.error((e as Error).message) }
  }
  // AIRTH_* are the model choices the Models page writes, not keys.
  const keys = (d?.secrets ?? []).filter((s) => !s.key.startsWith("AIRTH_"))
  const usedBy = (key: string) => (d?.integrations ?? []).filter((i) => asked(i).includes(key)).map((i) => i.name)

  return (
    <>
      <header className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">Keys</h2>
        <p className="mt-1 text-sm text-muted-foreground">Kept in this business's secrets. A key is written once and never shown again, and never reaches a model.</p>
      </header>
      {!d ? <Loading /> : (
        <>
          <div className="mb-2.5 flex items-center">
            <h3 className="flex-1 text-sm font-semibold">Keys</h3>
            <Button size="sm" variant="outline" onClick={() => setAdding({ name: "" })}><Plus /> Add a key</Button>
          </div>
          <Panel className="divide-y">
            {keys.length ? keys.map((s) => {
              const users = usedBy(s.key)
              return (
                <div key={s.key} className="flex items-center gap-3 px-4 py-2.5">
                  <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <div className="font-mono text-[13px]">{s.key}</div>
                    <div className="text-xs text-muted-foreground">
                      {s.provider ? "For this business's models" : users.length ? `Sent by ${users.join(", ")}` : "Given to commands as an environment variable"}
                    </div>
                  </div>
                  <span className="font-mono text-xs tracking-widest text-muted-foreground">••••••••</span>
                  <Remove label={s.key} onRemove={() => remove(`secrets/${enc(s.key)}`, s.key)} />
                </div>
              )
            }) : <p className="px-4 py-3 text-sm text-muted-foreground">No key yet. Model keys (ANTHROPIC_API_KEY, OPENAI_API_KEY…) and services' keys go here.</p>}
          </Panel>

          <div className="mt-9 mb-1 flex items-center">
            <h3 className="flex-1 text-sm font-semibold">Integrations</h3>
            <Button size="sm" variant="outline" onClick={() => setIntegrating(true)}><Plus /> Add an integration</Button>
          </div>
          <p className="mb-2.5 text-xs text-muted-foreground">Services this business's tasks may call. A key goes out only in an integration's header, only to its address. Without one, tasks reach public websites only, with no key.</p>
          <Panel className="divide-y">
            {d.integrations.length ? d.integrations.map((i) => (
              <div key={i.name} className="flex items-center gap-3 px-4 py-2.5">
                <Globe className="size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium" title={i.description}>{i.name}{i.description && <span className="font-normal text-muted-foreground"> · {i.description}</span>}</div>
                  <div className="truncate font-mono text-xs text-muted-foreground">{i.base_url}{asked(i).length > 0 && <span className="font-sans"> · uses {asked(i).join(", ")}</span>}</div>
                </div>
                {i.missing_secrets.length
                  ? <Button size="sm" variant="ghost" className="text-amber-500" onClick={() => setAdding({ name: i.missing_secrets[0] })}>Add {i.missing_secrets[0]}</Button>
                  : <span className="inline-flex items-center gap-1 text-xs text-emerald-500"><Check className="size-3.5" /> Ready</span>}
                <Remove label={i.name} onRemove={() => remove(`integrations/${enc(i.name)}`, i.name)} />
              </div>
            )) : <p className="px-4 py-3 text-sm text-muted-foreground">No integration.</p>}
          </Panel>
        </>
      )}
      <AddKey realm={realm} start={adding} onClose={() => setAdding(null)} onSaved={detail.refresh} />
      <AddIntegration realm={realm} open={integrating} keys={keys.map((s) => s.key)} onClose={() => setIntegrating(false)} onSaved={detail.refresh} />
    </>
  )
}

function AddKey({ realm, start, onClose, onSaved }: { realm: string; start: { name: string } | null; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState("")
  const [value, setValue] = useState("")
  const [busy, setBusy] = useState(false)
  const [shown, setShown] = useState<typeof start>(null)
  if (start !== shown) { setShown(start); setName(start?.name ?? ""); setValue("") }
  const valid = /^[A-Z][A-Z0-9_]*$/.test(name) && value.length > 0
  async function save() {
    if (!valid) return
    setBusy(true)
    try { await api(`realms/${enc(realm)}/secrets`, { key: name, value }); toast.success(`${name} saved.`); onSaved(); onClose() }
    catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  return (
    <Dialog open={!!start} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add a key</DialogTitle>
          <DialogDescription>Kept in {realm}'s secrets. Once saved, it is never shown again; to change it, save it again under the same name.</DialogDescription>
        </DialogHeader>
        <div className="space-y-1.5">
          <Label htmlFor="key-name">Name</Label>
          <Input id="key-name" value={name} autoFocus={!start?.name} spellCheck={false} autoComplete="off" placeholder="GITHUB_TOKEN" className="font-mono"
            onChange={(e) => setName(e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, "_"))} />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="key-value">Value</Label>
          <Input id="key-value" type="password" value={value} autoFocus={!!start?.name} autoComplete="new-password" spellCheck={false} className="font-mono"
            onChange={(e) => setValue(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button disabled={!valid || busy} onClick={save}>{busy ? "Saving…" : "Save the key"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function AddIntegration({ realm, open, keys, onClose, onSaved }: { realm: string; open: boolean; keys: string[]; onClose: () => void; onSaved: () => void }) {
  const [f, setF] = useState({ name: "", url: "", description: "", header: "", value: "" })
  const [busy, setBusy] = useState(false)
  const set = (k: keyof typeof f) => (e: React.ChangeEvent<HTMLInputElement>) => setF({ ...f, [k]: e.target.value })
  const valid = /^[a-z0-9][a-z0-9_-]*$/.test(f.name) && /^https:\/\/[^\s/]+/.test(f.url) && (!f.header || !!f.value)
  async function save() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/integrations`, {
        name: f.name, base_url: f.url.trim(), description: f.description.trim(), headers: f.header.trim() ? { [f.header.trim()]: f.value.trim() } : {},
      })
      toast.success(`${f.name} saved.`)
      setF({ name: "", url: "", description: "", header: "", value: "" })
      onSaved(); onClose()
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  // A literal key in a header would be stored in the business's config, in
  // the clear: only placeholders are meant to go there.
  const literal = f.value.trim() && !/\[secret:[A-Za-z0-9_]+\]/.test(f.value)
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Add an integration</DialogTitle>
          <DialogDescription>A service this business's tasks may call. Requests stay under its address; redirects are never followed.</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-1.5"><Label htmlFor="i-name">Name</Label><Input id="i-name" value={f.name} onChange={(e) => setF({ ...f, name: e.target.value.toLowerCase() })} placeholder="github" spellCheck={false} autoFocus /></div>
          <div className="space-y-1.5"><Label htmlFor="i-desc">What it is for</Label><Input id="i-desc" value={f.description} onChange={set("description")} placeholder="Issues and pull requests" /></div>
        </div>
        <div className="space-y-1.5"><Label htmlFor="i-url">Address</Label><Input id="i-url" value={f.url} onChange={set("url")} placeholder="https://api.github.com" spellCheck={false} className="font-mono" /></div>
        <div className="grid grid-cols-[2fr_3fr] gap-3">
          <div className="space-y-1.5"><Label htmlFor="i-h">Header (optional)</Label><Input id="i-h" value={f.header} onChange={set("header")} placeholder="Authorization" spellCheck={false} className="font-mono" /></div>
          <div className="space-y-1.5"><Label htmlFor="i-v">Its value</Label><Input id="i-v" value={f.value} onChange={set("value")} placeholder={`Bearer [secret:${keys[0] ?? "GITHUB_TOKEN"}]`} spellCheck={false} className="font-mono" /></div>
        </div>
        <p className={literal ? "text-xs text-amber-600 dark:text-amber-400" : "text-xs text-muted-foreground"}>
          {literal ? "This looks like a key written in the clear. Save the key under Keys, then write [secret:ITS_NAME] here: Airth fills it in when the request leaves."
            : `Write a key as [secret:NAME]: Airth fills it in when the request leaves, only for this address.${keys.length ? ` Your keys: ${keys.join(", ")}.` : ""}`}
        </p>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button disabled={!valid || busy || !!literal} onClick={save}>{busy ? "Saving…" : "Save the integration"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
