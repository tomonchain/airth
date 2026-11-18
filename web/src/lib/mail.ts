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
export type Mailbox = { agent: string | null; address: string | null; password: boolean; mails: MailLine[] }
export type MailText = { id: number; from: string; to: string[]; subject: string; date: string; text: string; attachments: string[] }

/** Where a mail went, in a few words, and what that opens: its task, its
    line in Needs you, or Needs you itself. */
export type Went = { text: string; tone: "agent" | "you" | "bank" | "quiet"; task?: string; item?: number; needs?: boolean }

export function went(m: MailLine, name: (agent: string) => string): Went {
  const said = m.summary ? `: ${m.summary}` : ""
  const waiting = m.item_status === "pending" && m.item != null ? m.item : undefined
  switch (m.outcome) {
    case "agent": return { text: `To ${name(m.agent ?? "")}${said}`, tone: "agent", task: m.run ?? undefined }
    case "bank": return { text: "Bank details: sent to you", tone: "bank", item: waiting, needs: true }
    case "founder": return { text: `Sent to you${said}`, tone: "you", needs: true }
    case "failed": return { text: "Sent to you: Airth could not pass it on", tone: "you", needs: true }
    case "old": return { text: "Not passed on: older than 3 days", tone: "quiet" }
    case "before": return { text: "Not passed on: it came before mail went to the team", tone: "quiet" }
    case "sorting": return { text: "Being passed on", tone: "quiet" }
    default: return { text: "Not passed on yet", tone: "quiet" }
  }
}

/** Where the reply a mail's task drafted stands, if it drafted one; the
    line in Needs you while it waits for the founder. */
export function reply(m: MailLine): { text: string; item?: number } | null {
  if (m.replied) return { text: "Reply sent" }
  if (m.outcome !== "agent" || m.item == null) return null
  switch (m.item_status) {
    case "pending": return { text: "Its reply waits for you", item: m.item }
    case "approved": return { text: "Reply approved" }
    case "done": return { text: "Reply sent" }
    case "rejected": return { text: "You turned its reply down" }
    case "expired": return { text: "Its reply expired" }
    case "failed": return { text: "Its reply could not be sent" }
    default: return null
  }
}
