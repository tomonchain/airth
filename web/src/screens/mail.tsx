import { useState } from "react"
import { ArrowUpRight, ChevronLeft, Clock, Inbox, Landmark, Mail as MailIcon, Paperclip, Settings, UserRound } from "lucide-react"
import { Character } from "@/components/character"
import { Empty, Loading, Panel } from "@/components/page"
import { Button } from "@/components/ui/button"
import { usePoll } from "@/hooks/use-poll"
import { api, called, enc, type Agent } from "@/lib/api"
import { reply, went, type Mailbox, type MailLine, type MailText, type Went } from "@/lib/mail"
import { stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

/** Where the view sends the founder: a task, a line in Needs you (or Needs
    you itself), or Settings to give the mailbox its address. */
export type MailLinks = { onTask: (id: string) => void; onItem: (id: number | null) => void; onSettings: () => void }

/** The business's mail (decision 152), from the interlocutor's page: each
    mail its mailbox received, newest first, and where it went. A mail
    opens as plain text: its HTML is never shown. */
export function MailView({ realm, lead, agents, onBack, links }: {
  realm: string; lead: Agent; agents: Agent[]; onBack: () => void; links: MailLinks
}) {
  const box = usePoll<Mailbox>(() => api(`realms/${enc(realm)}/mail`), 30000, [realm], realm)
  const [open, setOpen] = useState<number | null>(null)
  const line = box.data?.mails.find((m) => m.id === open) ?? null
  const agent = (n: string | null) => agents.find((a) => a.name === n)
  const name = (n: string) => { const a = agent(n); return a ? called(a) : n }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex h-14 shrink-0 items-center gap-2 border-b px-3 max-md:h-[60px] max-md:pl-[68px] md:px-5">
        <Button variant="ghost" size="sm" className="-ml-1.5 shrink-0 px-2" onClick={line ? () => setOpen(null) : onBack} aria-label={line ? "Back to the mail" : "Back to the conversation"}>
          <ChevronLeft /><span className="max-md:hidden">{line ? "Mail" : "Conversation"}</span>
        </Button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] leading-tight font-semibold">Mail</div>
          <div className="truncate text-xs text-muted-foreground">{box.data?.address ?? `${called(lead)}'s mailbox`}</div>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-3xl px-4 pt-5 pb-32 md:px-8 md:pb-10">
          {!box.data ? (box.error ? <p className="text-sm text-destructive">{box.error}</p> : <Loading />)
            : line ? <Opened realm={realm} line={line} where={went(line, name)} agent={agent(line.agent)} links={links} />
            : !box.data.address ? (
              <Empty icon={MailIcon} title="No mailbox yet">
                Give {called(lead)} the business's address, such as contact@yourdomain.com, in Settings, Mailboxes. The mail it receives shows here, with the agent each one went to.
                <Button variant="outline" size="sm" className="mx-auto mt-4 flex" onClick={links.onSettings}><Settings /> Open Settings</Button>
              </Empty>
            ) : (
              <>
                {!box.data.password && (
                  <p className="mb-4 rounded-xl bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
                    The mailbox's password is not set, so no new mail is read. <button className="underline underline-offset-2" onClick={links.onSettings}>Set it in Settings</button>.
                  </p>
                )}
                {!box.data.mails.length ? (
                  <Empty icon={Inbox} title="No mail yet">Mail sent to {box.data.address} shows here, with the agent each one went to.</Empty>
                ) : (
                  <Panel className="divide-y divide-border/60 overflow-hidden">
                    {box.data.mails.map((m) => <Row key={m.id} line={m} where={went(m, name)} agent={agent(m.agent)} onOpen={() => setOpen(m.id)} />)}
                  </Panel>
                )}
              </>
            )}
        </div>
      </div>
    </div>
  )
}

const TONE: Record<Went["tone"], string> = {
  agent: "text-foreground/80", you: "text-foreground/80", bank: "text-amber-600 dark:text-amber-400", quiet: "text-muted-foreground",
}

/** Where it went, with its sign: the agent's face, the founder, the bank flag. */
function Where({ where, agent, whole, className }: { where: Went; agent?: Agent; whole?: boolean; className?: string }) {
  const Icon = where.tone === "bank" ? Landmark : where.tone === "you" ? UserRound : Clock
  return (
    <span className={cn("flex min-w-0 gap-1.5", whole ? "items-start" : "items-center", TONE[where.tone], className)}>
      {agent && where.tone === "agent" ? <Character agent={agent} size={16} still className={cn("shrink-0", whole && "mt-0.5")} /> : <Icon className={cn("size-3.5 shrink-0", whole && "mt-1")} />}
      <span className={whole ? "break-words" : "truncate"}>{where.text}</span>
    </span>
  )
}

function Row({ line, where, agent, onOpen }: { line: MailLine; where: Went; agent?: Agent; onOpen: () => void }) {
  const r = reply(line)
  return (
    <button onClick={onOpen} className="block w-full px-4 py-2.5 text-left transition-colors hover:bg-accent/50">
      <span className="flex items-baseline gap-2">
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{line.sender || "Unknown sender"}</span>
        {line.bank && line.outcome !== "bank" && <span className="shrink-0 text-xs text-amber-600 dark:text-amber-400">Bank details</span>}
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{stamp(line.date)}</span>
      </span>
      <span className="block truncate text-[13px]">{line.subject || "(no subject)"}</span>
      <span className="mt-1 flex items-center gap-2 text-xs">
        <Where where={where} agent={agent} />
        {r && <span className={cn("shrink-0 text-muted-foreground max-sm:hidden", r.item && "text-amber-600 dark:text-amber-400")}>· {r.text}</span>}
      </span>
    </button>
  )
}

/** One mail: where it went and what to open, then its words as text. */
function Opened({ realm, line, where, agent, links }: { realm: string; line: MailLine; where: Went; agent?: Agent; links: MailLinks }) {
  const text = usePoll<MailText>(() => api(`realms/${enc(realm)}/mail/${line.id}`), 300000, [realm, line.id])
  const r = reply(line)
  const m = text.data
  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg leading-snug font-semibold break-words">{line.subject || "(no subject)"}</h2>
        <p className="mt-1 text-xs break-words text-muted-foreground">
          From {line.sender || "an unknown sender"}{m?.to.length ? ` to ${m.to.join(", ")}` : ""} · {new Date(line.date).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" })}
        </p>
      </div>
      <Panel className="space-y-2.5 px-4 py-3.5">
        <Where where={where} agent={agent} whole className="text-sm" />
        {r && <p className="text-xs text-muted-foreground">{r.text}</p>}
        {line.bank && <p className="text-xs text-muted-foreground">It speaks of bank details: change nothing on a mail's word. Check by phone, with a number you already had.</p>}
        <div className="flex flex-wrap gap-2">
          {where.task && <Button size="sm" variant="outline" onClick={() => links.onTask(where.task!)}><ArrowUpRight /> Open the task</Button>}
          {r?.item != null && <Button size="sm" onClick={() => links.onItem(r.item!)}>Review the reply</Button>}
          {where.needs && <Button size="sm" variant="outline" onClick={() => links.onItem(where.item ?? null)}><Inbox /> Open Needs you</Button>}
        </div>
      </Panel>
      {!!m?.attachments.length && (
        <p className="flex items-start gap-1.5 text-xs text-muted-foreground"><Paperclip className="mt-0.5 size-3.5 shrink-0" />{m.attachments.join(", ")} (not opened)</p>
      )}
      {/* The mail's words as plain text: React escapes them, and nothing here renders HTML or Markdown. */}
      {!m ? (text.error ? <p className="text-sm text-destructive">{text.error}</p> : <Loading />) : (
        <Panel className="px-4 py-3.5">
          <div className="text-sm leading-relaxed break-words whitespace-pre-wrap">{m.text.trim() || "(no text)"}</div>
        </Panel>
      )}
    </div>
  )
}
