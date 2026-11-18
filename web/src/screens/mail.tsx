import { useEffect, useRef, useState } from "react"
import { ArrowRight, ArrowUpRight, ChevronDown, Inbox, Mail as MailIcon, Paperclip, Settings } from "lucide-react"
import { Character } from "@/components/character"
import { Empty } from "@/components/page"
import { Button } from "@/components/ui/button"
import { usePoll } from "@/hooks/use-poll"
import { usePrefs, WIDTH, ZOOM } from "@/hooks/use-prefs"
import { api, called, enc, type Agent } from "@/lib/api"
import { reply, went, type Mailbox, type MailLine, type MailText } from "@/lib/mail"
import { stamp } from "@/lib/time"
import { cn } from "@/lib/utils"

/** Where the thread sends the founder: a task, a line in Needs you (or
    Needs you itself), Settings to give the mailbox its address, or the
    interlocutor's conversation to write to it. */
export type MailLinks = { onTask: (id: string) => void; onItem: (id: number | null) => void; onSettings: () => void }

/** The business's mail as a thread of Chat (decision 152): the
    interlocutor tells the founder, mail by mail, what it did with each,
    newest at the bottom. Read only: nothing is written here, and a mail
    opens as plain text, never as HTML. */
export function MailThread({ realm, lead, agents, box, links, onConversation, aside }: {
  realm: string; lead: Agent; agents: Agent[]; links: MailLinks; onConversation: () => void
  /** The business's mailbox, as Chat polls it for its line to this thread. */
  box: { data: Mailbox | null; error: string | null }
  /** Above the line at the bottom: Chat's lines to its other threads. */
  aside?: React.ReactNode
}) {
  const { prefs } = usePrefs()
  const name = (n: string) => { const a = agents.find((x) => x.name === n); return a ? called(a) : n }
  // Oldest first, as a chat: the newest is at the bottom, where it opens.
  const mails = [...(box.data?.mails ?? [])].reverse()
  const scroller = useRef<HTMLDivElement>(null)
  const newest = mails.at(-1)?.id
  useEffect(() => { scroller.current?.scrollTo({ top: scroller.current.scrollHeight }) }, [newest, box.data?.address])
  // The line at the bottom floats over the thread, which ends with room
  // for it, as the conversation's box does.
  const dock = useRef<HTMLDivElement>(null)
  const [room, setRoom] = useState(96)
  useEffect(() => {
    if (!dock.current) return
    const o = new ResizeObserver(([e]) => setRoom(Math.ceil(e.target.getBoundingClientRect().height)))
    o.observe(dock.current)
    return () => o.disconnect()
  }, [])

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
      <div ref={scroller} className="no-dock min-h-0 flex-1 overflow-y-auto">
        <div className={cn("mx-auto px-3 pt-4 [overflow-wrap:anywhere] max-md:pt-[68px] md:px-6 md:pt-6", WIDTH[prefs.width] ?? WIDTH.normal)} style={{ zoom: ZOOM[prefs.text] ?? 1, paddingBottom: room + 16 }}>
          <div className="mb-6 text-center">
            <p className="text-[15px] font-semibold">Mail</p>
            <p className="text-xs text-muted-foreground">{box.data?.address ? `What ${called(lead)} did with each mail to ${box.data.address}` : `${called(lead)}'s mailbox`}</p>
          </div>
          {!box.data ? <p className="py-20 text-center text-sm text-muted-foreground">{box.error ?? "Loading…"}</p>
            : box.data.error ? (
              <Empty icon={MailIcon} title="The mailbox's settings are invalid">
                {box.data.error}
                <Button variant="outline" size="sm" className="mx-auto mt-4 flex" onClick={links.onSettings}><Settings /> Open Settings</Button>
              </Empty>
            ) : !box.data.address ? (
              <Empty icon={MailIcon} title="No mailbox yet">
                Give {called(lead)} the business's address, such as contact@yourdomain.com, in Settings, Mailboxes. Each mail it receives is then told here, with what was done with it.
                <Button variant="outline" size="sm" className="mx-auto mt-4 flex" onClick={links.onSettings}><Settings /> Open Settings</Button>
              </Empty>
            ) : !mails.length ? (
              <Empty icon={Inbox} title="No mail yet">Each mail sent to {box.data.address} is told here, with what was done with it.</Empty>
            ) : mails.map((m, i) => (
              <div key={m.id} className={cn("flex items-end gap-2.5", mails[i + 1] ? "mb-3" : "mb-5")}>
                <span className="mb-6 hidden w-8 shrink-0 md:block">{!mails[i + 1] && <Character agent={lead} size={32} still />}</span>
                <div className="min-w-0 flex-1"><MailMessage realm={realm} line={m} agent={agents.find((a) => a.name === m.agent)} name={name} links={links} /></div>
              </div>
            ))}
        </div>
      </div>
      <div ref={dock} className={cn("dock-lift pointer-events-none absolute inset-x-0 bottom-0 mx-auto w-full space-y-2 px-3 pb-3 md:px-6 md:pb-5 [&>*]:pointer-events-auto", WIDTH[prefs.width] ?? WIDTH.normal)}>
        {aside}
        {box.data?.address && !box.data.password && (
          <p className="rounded-2xl bg-amber-500/10 px-4 py-2.5 text-sm text-amber-700 backdrop-blur dark:text-amber-300">
            The mailbox's password is not set, so no new mail is read. <button className="underline underline-offset-2" onClick={links.onSettings}>Set it in Settings</button>.
          </p>
        )}
        {/* Nothing is written here: an order goes to the conversation. */}
        <button onClick={onConversation} className="glass flex w-full items-center gap-2 rounded-[26px] py-1.5 pr-1.5 pl-5 text-left text-sm text-muted-foreground hover:text-foreground">
          <span className="min-w-0 flex-1 truncate py-2">Write to {lead.first_name ?? lead.title} in the conversation</span>
          <span className="grid size-9 shrink-0 place-items-center rounded-full bg-primary text-primary-foreground"><ArrowRight className="size-4" /></span>
        </button>
      </div>
    </div>
  )
}

const TONE = { agent: "", you: "", bank: "text-amber-700 dark:text-amber-300", quiet: "text-muted-foreground" }

/** One mail, as the interlocutor tells it: who wrote and about what, what
    it did with it, and what to open. The mail itself opens under it. */
function MailMessage({ realm, line, agent, name, links }: {
  realm: string; line: MailLine; agent?: Agent; name: (n: string) => string; links: MailLinks
}) {
  const [open, setOpen] = useState(false)
  const where = went(line, name)
  const r = reply(line)
  return (
    <div>
      <div className="rounded-2xl rounded-bl-md bg-bubble px-4 py-3 md:max-w-[85%]">
        <p className="text-[13.5px] leading-relaxed"><span className="font-semibold">Mail from {line.sender || "an unknown sender"}</span> · {line.subject || "(no subject)"}</p>
        <p className={cn("mt-1 text-[13.5px] leading-relaxed", TONE[where.tone])}>
          {agent && where.tone === "agent" && <Character agent={agent} size={16} still className="mr-1.5 -mb-0.5" />}
          {where.text}{r && ` ${r.text}`}
        </p>
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          <Button size="sm" variant="outline" className="h-8" onClick={() => setOpen(!open)} aria-expanded={open}>
            <ChevronDown className={cn("transition-transform", open && "rotate-180")} />{open ? "Hide the mail" : "Open the mail"}
          </Button>
          {r?.item != null && <Button size="sm" className="h-8" onClick={() => links.onItem(r.item!)}>Review the reply</Button>}
          {where.needs && <Button size="sm" variant="outline" className="h-8" onClick={() => links.onItem(where.item ?? null)}><Inbox /> Open in Needs you</Button>}
          {where.task && <Button size="sm" variant="outline" className="h-8" onClick={() => links.onTask(where.task!)}><ArrowUpRight /> Open the task</Button>}
        </div>
        {open && <MailBody realm={realm} id={line.id} />}
      </div>
      <div className="mt-1 pl-1 text-[11px] text-muted-foreground">{stamp(line.date)}</div>
    </div>
  )
}

/** A mail's words, as plain text: React escapes them, and nothing here
    renders HTML or Markdown. */
function MailBody({ realm, id }: { realm: string; id: number }) {
  const mail = usePoll<MailText>(() => api(`realms/${enc(realm)}/mail/${id}`), 300000, [realm, id])
  const m = mail.data
  return (
    <div className="mt-3 border-t pt-3">
      {!m ? <p className="text-xs text-muted-foreground">{mail.error ?? "Loading…"}</p> : (
        <>
          <p className="text-xs text-muted-foreground">
            From {m.from || "an unknown sender"}{m.to.length ? ` to ${m.to.join(", ")}` : ""} · {new Date(m.date).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" })}
          </p>
          {m.attachments.length > 0 && <p className="mt-1 flex items-start gap-1.5 text-xs text-muted-foreground"><Paperclip className="mt-0.5 size-3.5 shrink-0" />{m.attachments.join(", ")} (not opened)</p>}
          <div className="mt-2.5 text-[13.5px] leading-relaxed break-words whitespace-pre-wrap">{m.text.trim() ? m.text : "(no text)"}</div>
        </>
      )}
    </div>
  )
}
