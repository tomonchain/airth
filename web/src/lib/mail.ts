/** The business's mail as the founder sees it (decision 152): each mail of
    the interlocutor's mailbox, and where it went. */
export type MailLine = {
  id: number; sender: string; subject: string; date: string; snippet: string
  /** It speaks of bank details: it is the founder's alone. */
  bank: boolean
  /** None until Airth takes it. */
  outcome: "sorting" | "agent" | "founder" | "bank" | "failed" | "old" | "before" | null
  agent: string | null; summary: string | null
  /** The task it started. */
  run: string | null
  /** Its line in Needs you: its task's reply, or the bank flag. */
  item: number | null; item_status: string | null
  replied: boolean
}
/** The business's mailbox: its address, whether its password is set, how
    many mails came this week (every one, not only those listed), its last
    mails, and why it cannot be read when its settings are wrong. */
export type Mailbox = { agent: string | null; address: string | null; password: boolean; recent: number; mails: MailLine[]; error?: string }
export type MailText = { id: number; from: string; to: string[]; subject: string; date: string; text: string; attachments: string[] }

/** What the interlocutor says it did with a mail, and what that opens: its
    task, its line in Needs you, or Needs you itself. */
export type Went = { text: string; tone: "agent" | "you" | "bank" | "quiet"; task?: string; item?: number; needs?: boolean }

const sentence = (s: string) => (/[.!?]$/.test(s) ? s : `${s}.`)

export function went(m: MailLine, name: (agent: string) => string): Went {
  const said = m.summary ? `: ${m.summary}` : ""
  const waiting = m.item_status === "pending" && m.item != null ? m.item : undefined
  switch (m.outcome) {
    case "agent": return { text: sentence(`Passed to ${name(m.agent ?? "")}${said}`), tone: "agent", task: m.run ?? undefined }
    case "bank": return { text: "It speaks of bank details, so it is yours alone: it is in Needs you. Change nothing on a mail's word; check by phone.", tone: "bank", item: waiting, needs: true }
    case "founder": return { text: `${sentence(`For you${said}`)} It is in Needs you.`, tone: "you", needs: true }
    case "failed": return { text: "I could not pass it on, so it is in Needs you.", tone: "you", needs: true }
    case "old": return { text: "Not passed on: it is older than 3 days.", tone: "quiet" }
    case "before": return { text: "Not passed on: it came before mail went to the team.", tone: "quiet" }
    case "sorting": return { text: "Passing it on now.", tone: "quiet" }
    default: return { text: "Not passed on yet.", tone: "quiet" }
  }
}

/** Where the reply a mail's task drafted stands, if it drafted one; the
    line in Needs you while it waits for the founder. */
export function reply(m: MailLine): { text: string; item?: number } | null {
  if (m.replied) return { text: "Its reply was sent." }
  if (m.outcome !== "agent" || m.item == null) return null
  switch (m.item_status) {
    case "pending": return { text: "Its reply waits for you.", item: m.item }
    case "approved": return { text: "You approved its reply." }
    case "done": return { text: "Its reply was sent." }
    case "rejected": return { text: "You turned its reply down." }
    case "expired": return { text: "Its reply expired." }
    case "failed": return { text: "Its reply could not be sent." }
    default: return null
  }
}
