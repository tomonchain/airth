import { Loading, Panel } from "@/components/page"
import { MailboxCard, type Box } from "@/components/mailbox"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"
import { Head } from "@/screens/settings-business"

/** The business's mailbox is the interlocutor's (decision 152): each mail
    goes to the agent it is for. A role's own address (decision 70) is
    folded below, open only when one is already set. Each is connected
    from the same dialog, tried before it is saved (decision 154). */
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
              <MailboxCard key={lead.agent} realm={realm} box={lead} onChanged={refresh} />
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
                {others.map((b) => <MailboxCard key={b.agent} realm={realm} box={b} onChanged={refresh} compact />)}
              </Panel>
            </details>
          )}
        </>
      )}
    </>
  )
}
