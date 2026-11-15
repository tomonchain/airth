import { useState } from "react"
import { Inbox } from "lucide-react"
import { toast } from "sonner"
import { Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { ago } from "@/lib/time"
import { Head } from "@/screens/settings-business"

type Box = { agent: string; title: string; address: string | null; server: string | null; password: boolean; last: string | null; error: string | null }

/** Each role's mailbox (decision 70): its address on the business's domain
    and its password, kept by Airth and seen by no agent. */
export function Mailboxes({ realm }: { realm: string }) {
  const { data, refresh } = usePoll<{ mailboxes: Box[] }>(() => api(`realms/${enc(realm)}/mailboxes`), 60000, [realm])
  return (
    <>
      <Head title="Mailboxes">
        A role can have its own address on the business's domain (OVH by default). What it receives is read as data, never as orders; every mail it sends waits for your approval, signed as prepared by an AI agent, with a copy to archive@. A mail about bank details always comes to you.
      </Head>
      {!data ? <Loading /> : (
        <Panel className="divide-y">
          {data.mailboxes.map((b) => <MailboxRow key={`${b.agent}:${b.address ?? ""}`} realm={realm} box={b} onSaved={refresh} />)}
        </Panel>
      )}
    </>
  )
}

function MailboxRow({ realm, box, onSaved }: { realm: string; box: Box; onSaved: () => void }) {
  const [address, setAddress] = useState(box.address ?? "")
  const [password, setPassword] = useState("")
  const [busy, setBusy] = useState(false)
  const changed = address.trim() !== (box.address ?? "") || password !== ""
  async function save() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/mailboxes/${enc(box.agent)}`, { address: address.trim(), password }, "PUT")
      setPassword("")
      toast.success(address.trim() ? `${box.title}'s mailbox is ${address.trim()}.` : `${box.title} has no mailbox now.`)
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  async function read() {
    setBusy(true)
    try {
      const r = await api<{ kept: number }>(`realms/${enc(realm)}/mailboxes/${enc(box.agent)}/read`, {})
      toast.success(r.kept ? `${r.kept} new mail read.` : "Connected: no new mail.")
      onSaved()
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false) }
  }
  return (
    <div className="space-y-2.5 px-4 py-3.5">
      <div className="flex items-baseline gap-2">
        <span className="text-sm font-medium">{box.title}</span>
        <span className="text-xs text-muted-foreground">
          {box.error ? <span className="text-destructive">{box.error}</span> : !box.address ? "no mailbox" : !box.password ? "password not set" : box.last ? `read ${ago(box.last)}` : "nothing read yet"}
        </span>
      </div>
      <div className="flex flex-wrap gap-2">
        <Input value={address} onChange={(e) => setAddress(e.target.value)} placeholder={`${box.agent}@your-domain.com`} aria-label={`${box.title}'s address`}
          className="h-9 min-w-48 flex-1" autoComplete="off" />
        <Input value={password} onChange={(e) => setPassword(e.target.value)} type="password" aria-label={`${box.title}'s password`}
          placeholder={box.password ? "Password saved" : "Password"} className="h-9 w-40 max-md:flex-1" autoComplete="new-password" />
        <Button size="sm" className="h-9" disabled={busy || !changed} onClick={save}>Save</Button>
        {box.address && box.password && !box.error && (
          <Button size="sm" variant="outline" className="h-9" disabled={busy || changed} title={changed ? "Save first" : undefined} onClick={read}><Inbox /> Read now</Button>
        )}
      </div>
    </div>
  )
}
