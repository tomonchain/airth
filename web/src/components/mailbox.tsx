import { useEffect, useState } from "react"
import { ChevronDown, Inbox, Loader, Mail, TriangleAlert } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api, enc } from "@/lib/api"
import { PRESETS, guess, matching, preset, type Security, type Servers } from "@/lib/mail-presets"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"

/** A role's mailbox as Settings serves it: never its password, only
    whether one is saved. */
export type Box = {
  agent: string; title: string; lead?: boolean; address: string | null; server: string | null
  servers: Servers | null; password: boolean; last: string | null; read: string | null; read_error: string | null; error: string | null
}

type Tone = "ok" | "wait" | "bad"

/** What the card says of a mailbox: connected and read when, or plainly
    what stops it. */
function state(box: Box): { tone: Tone; text: string } {
  if (box.error) return { tone: "bad", text: box.error }
  if (!box.password) return { tone: "bad", text: "Password missing: mail is not read" }
  if (box.read_error) return { tone: "bad", text: `Last read failed${box.read ? ` ${ago(box.read)}` : ""}: ${box.read_error}` }
  if (box.read) return { tone: "ok", text: `Connected · read ${ago(box.read)}` }
  if (box.last) return { tone: "ok", text: `Connected · last mail ${ago(box.last)}` }
  return { tone: "wait", text: "Connected · first read within a few minutes" }
}

const DOT: Record<Tone, string> = { ok: "bg-emerald-500", wait: "bg-amber-500", bad: "bg-red-500" }

/** One mailbox: the button that connects it, or its card once connected.
    `compact` is a role's row, under the business's own mailbox. */
export function MailboxCard({ realm, box, onChanged, compact }: { realm: string; box: Box; onChanged: () => void; compact?: boolean }) {
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [leaving, setLeaving] = useState(false)
  const dialog = <ConnectMailbox realm={realm} box={box} open={open} onClose={() => setOpen(false)} onConnected={onChanged} />

  if (!box.address) {
    return compact ? (
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{box.title}</div>
          <div className="text-xs text-muted-foreground">No address of its own</div>
        </div>
        <Button size="sm" variant="outline" onClick={() => setOpen(true)}><Mail /> Connect a mailbox</Button>
        {dialog}
      </div>
    ) : (
      <div className="flex flex-col items-center px-6 py-10 text-center">
        <span className="grid size-11 place-items-center rounded-xl bg-background text-muted-foreground ring-1 ring-foreground/10"><Mail className="size-5" /></span>
        <p className="mt-3 text-sm font-medium">No mailbox yet</p>
        <p className="mt-1 max-w-sm text-sm text-muted-foreground">Connect the address customers and partners write to. Airth checks it with the provider before saving it.</p>
        <Button className="mt-4" size="lg" onClick={() => setOpen(true)}><Mail /> Connect a mailbox</Button>
        {dialog}
      </div>
    )
  }

  async function read() {
    setBusy(true)
    try {
      const r = await api<{ kept: number }>(`realms/${enc(realm)}/mailboxes/${enc(box.agent)}/read`, {})
      toast.success(r.kept ? `${r.kept} new mail read.` : "Read: no new mail.")
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false); onChanged() }
  }
  async function disconnect() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/mailboxes/${enc(box.agent)}`, { address: "", password: "" }, "PUT")
      toast.success(`Disconnected: ${box.address}`)
      setLeaving(false)
      onChanged()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }

  const s = state(box)
  const provider = box.servers ? matching(box.servers) : "other"
  const name = provider === "other" ? box.servers?.imap_host ?? box.server : preset(provider).name
  return (
    <div className={cn("flex gap-3 px-4 py-3.5 max-sm:flex-col", compact ? "sm:items-center" : "sm:items-center sm:py-4")}>
      <div className="flex min-w-0 flex-1 items-start gap-3">
        <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-lg bg-background text-muted-foreground ring-1 ring-foreground/10"><Mail className="size-4" /></span>
        <div className="min-w-0 flex-1">
          {compact && <div className="truncate text-xs text-muted-foreground">{box.title}</div>}
          <div className="truncate text-sm font-medium">{box.address}</div>
          <div className="mt-0.5 flex items-start gap-1.5 text-xs text-muted-foreground">
            <span className={cn("mt-1 size-1.5 shrink-0 rounded-full", DOT[s.tone])} />
            <span className={cn("min-w-0 break-words", s.tone === "bad" && "text-red-600 dark:text-red-400")}>
              {name && <>{name} · </>}{s.text}
            </span>
          </div>
        </div>
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-1.5 max-sm:pl-12">
        {leaving ? (
          <>
            <span className="text-xs text-muted-foreground">Stop reading and sending from it?</span>
            <Button size="sm" variant="destructive" disabled={busy} onClick={disconnect}>Disconnect</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setLeaving(false)}>Keep</Button>
          </>
        ) : (
          <>
            {box.password && !box.error && (
              <Button size="sm" variant="outline" disabled={busy} onClick={read}>{busy ? <Loader className="animate-spin" /> : <Inbox />} Read now</Button>
            )}
            <Button size="sm" variant={box.password && !box.error && !box.read_error ? "outline" : "default"} disabled={busy} onClick={() => setOpen(true)}>Change</Button>
            <Button size="sm" variant="ghost" className="text-red-600 hover:text-red-600 dark:text-red-400" disabled={busy} onClick={() => setLeaving(true)}>Disconnect…</Button>
          </>
        )}
      </div>
      {dialog}
    </div>
  )
}

type Form = Servers & { address: string; password: string }

function start(box: Box): { form: Form; provider: string } {
  if (box.address && box.servers) {
    return { form: { ...box.servers, address: box.address, password: "" }, provider: matching(box.servers) }
  }
  return { form: { ...preset("ovh").servers!, address: box.address ?? "", password: "" }, provider: "ovh" }
}

/** "Connect a mailbox": its address and password, a provider's servers or
    its own, tried by Airth before anything is saved. */
export function ConnectMailbox({ realm, box, open, onClose, onConnected }: { realm: string; box: Box; open: boolean; onClose: () => void; onConnected: () => void }) {
  const [form, setForm] = useState<Form>(() => start(box).form)
  const [provider, setProvider] = useState(() => start(box).provider)
  // Once the founder picks a provider, the address no longer picks one.
  const [picked, setPicked] = useState(!!box.address)
  const [advanced, setAdvanced] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Each opening starts from the mailbox as saved.
  useEffect(() => {
    if (!open) return
    const s = start(box)
    setForm(s.form); setProvider(s.provider); setPicked(!!box.address); setAdvanced(false); setError(null)
  }, [open]) // eslint-disable-line react-hooks/exhaustive-deps
  function pick(id: string) {
    setPicked(true)
    setProvider(id)
    const servers = preset(id).servers
    if (servers) setForm((f) => ({ ...f, ...servers }))
  }
  function address(value: string) {
    setForm((f) => ({ ...f, address: value }))
    const found = guess(value)
    if (!picked && found && found !== provider) {
      setProvider(found)
      setForm((f) => ({ ...f, ...preset(found).servers! }))
    }
  }
  function server<K extends keyof Servers>(key: K, value: Servers[K]) {
    const next = { ...form, [key]: value }
    setForm(next)
    setProvider(matching(next))
    setPicked(true)
  }

  // A saved password is kept for the same address only.
  const saved = box.password && !!box.address && form.address.trim().toLowerCase() === box.address.toLowerCase()
  const ready = /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(form.address.trim()) && (form.password !== "" || saved)
    && form.imap_host.trim() !== "" && form.imap_port > 0 && form.smtp_port > 0
  const hint = preset(provider).hint
  const showAdvanced = advanced || provider === "other"

  async function save() {
    setBusy(true)
    setError(null)
    try {
      await api(`realms/${enc(realm)}/mailboxes/${enc(box.agent)}/connect`, {
        address: form.address.trim(), password: form.password,
        imap_host: form.imap_host.trim(), imap_port: form.imap_port,
        smtp_host: form.smtp_host.trim(), smtp_port: form.smtp_port, smtp_security: form.smtp_security,
      })
      toast.success(`Connected: ${form.address.trim()}`)
      onConnected()
      onClose()
    } catch (e) {
      setError((e as Error).message)
    } finally { setBusy(false) }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o && !busy) onClose() }}>
      <DialogContent className="max-h-[calc(100dvh-2rem)] gap-5 overflow-y-auto sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Connect a mailbox</DialogTitle>
          <DialogDescription>
            {box.lead ? "The business's address" : `${box.title}'s own address`}. Airth logs in to check it before saving; the password stays in Airth, never in the business folder.
          </DialogDescription>
        </DialogHeader>

        <form className="grid gap-4" onSubmit={(e) => { e.preventDefault(); if (ready && !busy) save() }}>
          <div className="grid gap-1.5">
            <Label htmlFor="mb-address">Email</Label>
            <Input id="mb-address" type="email" value={form.address} onChange={(e) => address(e.target.value)} placeholder="contact@your-domain.com"
              autoComplete="off" spellCheck={false} disabled={busy} className="h-9" />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="mb-password">Password</Label>
            <Input id="mb-password" type="password" value={form.password} onChange={(e) => setForm((f) => ({ ...f, password: e.target.value }))}
              placeholder={saved ? "Saved" : ""} autoComplete="new-password" disabled={busy} className="h-9" />
          </div>

          <div className="grid gap-2">
            <span className="text-sm font-medium">Provider</span>
            <div role="radiogroup" aria-label="Provider" className="flex flex-wrap gap-1.5">
              {PRESETS.map((p) => (
                <button key={p.id} type="button" role="radio" aria-checked={provider === p.id} disabled={busy} onClick={() => pick(p.id)}
                  className={cn("h-7 rounded-md border px-2.5 text-xs transition-colors",
                    provider === p.id ? "border-foreground/30 bg-accent font-medium text-foreground" : "text-muted-foreground hover:text-foreground")}>
                  {p.name}
                </button>
              ))}
            </div>
            {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
          </div>

          <div className="grid gap-3">
            {provider !== "other" && (
              <button type="button" onClick={() => setAdvanced((a) => !a)} aria-expanded={showAdvanced}
                className="flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
                <ChevronDown className={cn("size-3.5 transition-transform", !showAdvanced && "-rotate-90")} /> Advanced
              </button>
            )}
            {showAdvanced && (
              <div className="grid gap-3 rounded-lg bg-muted/50 p-3">
                <HostPort label="IMAP server" id="mb-imap" host={form.imap_host} port={form.imap_port} disabled={busy}
                  onHost={(v) => server("imap_host", v)} onPort={(v) => server("imap_port", v)} />
                <HostPort label="SMTP server" id="mb-smtp" host={form.smtp_host} port={form.smtp_port} disabled={busy}
                  onHost={(v) => server("smtp_host", v)} onPort={(v) => server("smtp_port", v)} />
                <div className="flex items-center justify-between gap-3">
                  <span className="text-xs text-muted-foreground">SMTP security</span>
                  <div role="radiogroup" aria-label="SMTP security" className="flex rounded-lg border bg-background p-0.5">
                    {([["ssl", "SSL/TLS"], ["starttls", "STARTTLS"]] as [Security, string][]).map(([v, l]) => (
                      <button key={v} type="button" role="radio" aria-checked={form.smtp_security === v} disabled={busy}
                        onClick={() => server("smtp_security", v)}
                        className={cn("rounded-md px-2.5 py-1 text-xs transition-colors", form.smtp_security === v ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")}>{l}</button>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </div>

          {error && (
            <div role="alert" className="flex gap-2 rounded-lg bg-red-500/10 px-3 py-2.5 text-sm text-red-700 dark:text-red-300">
              <TriangleAlert className="mt-0.5 size-4 shrink-0" />
              <span className="min-w-0 break-words">{error}</span>
            </div>
          )}

          <DialogFooter className="mt-1">
            <Button type="button" variant="ghost" disabled={busy} onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!ready || busy}>
              {busy ? <><Loader className="animate-spin" /> Checking the mailbox…</> : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function HostPort({ label, id, host, port, onHost, onPort, disabled }: {
  label: string; id: string; host: string; port: number; onHost: (v: string) => void; onPort: (v: number) => void; disabled?: boolean
}) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id} className="text-xs font-normal text-muted-foreground">{label}</Label>
      <div className="flex gap-2">
        <Input id={id} value={host} onChange={(e) => onHost(e.target.value)} autoComplete="off" spellCheck={false} disabled={disabled} className="h-9 min-w-0 flex-1 bg-background" />
        <Input aria-label={label.replace("server", "port")} inputMode="numeric" value={port || ""} disabled={disabled} className="h-9 w-20 bg-background tabular-nums"
          onChange={(e) => onPort(Number(e.target.value.replace(/\D/g, "").slice(0, 5)) || 0)} />
      </div>
    </div>
  )
}
