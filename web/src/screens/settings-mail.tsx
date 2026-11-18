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

export type Box = { agent: string; title: string; lead?: boolean; address: string | null; server: string | null; password: boolean; last: string | null; error: string | null }

/** The business's mailbox is the interlocutor's (decision 152): each mail
    goes to the agent it is for. A role's own address (decision 70) is
    folded below, open only when one is already set. */
export function Mailboxes({ realm }: { realm: string }) {
  const { data, refresh } = usePoll<{ mailboxes: Box[] }>(() => api(`realms/${enc(realm)}/mailboxes`), 60000, [realm])
  const lead = data?.mailboxes.find((b) => b.lead)
  const others = data?.mailboxes.filter((b) => !b.lead) ?? []
  const own = others.some((b) => b.address)
  return (
    <>
      <Head title="Mailboxes">
        The business has one address, its interlocutor's. Each mail it receives is read as data, never as orders, and passed to the agent it is for; every reply waits for your approval, signed as prepared by an AI agent, with a copy to archive@. A mail about bank details always comes to you.
      </Head>
      {!data ? <Loading /> : (
        <>
          {lead ? (
            <Panel>
              <MailboxRow key={`${lead.agent}:${lead.address ?? ""}`} realm={realm} box={lead} onSaved={refresh} />
            </Panel>
          ) : (
            <Panel className="px-4 py-3.5 text-sm text-muted-foreground">
              The business's mailbox belongs to the agent who answers you in Chat, and this business has none yet.
            </Panel>
          )}
          {others.length > 0 && (
            <details open={own} className="group mt-6">
              <summary className="cursor-pointer text-sm text-muted-foreground select-none">A role's own address, when the volume calls for it</summary>
              <Panel className="mt-3 divide-y">
                {others.map((b) => <MailboxRow key={`${b.agent}:${b.address ?? ""}`} realm={realm} box={b} onSaved={refresh} />)}
              </Panel>
            </details>
          )}
        </>
      )}
    </>
  )
}

export function MailboxRow({ realm, box, onSaved }: { realm: string; box: Box; onSaved: () => void }) {
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
