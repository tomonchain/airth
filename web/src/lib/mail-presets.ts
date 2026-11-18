// Known mail providers' servers (decision 154): a preset fills the dialog,
// Airth's server checks and saves what the founder sends, whatever it is.

export type Security = "ssl" | "starttls"

export type Servers = {
  imap_host: string
  imap_port: number
  smtp_host: string
  smtp_port: number
  smtp_security: Security
}

export type Preset = { id: string; name: string; servers: Servers | null; hint?: string }

const APP_PASSWORD = "Use an app password, made in the account's security settings: the usual password is refused."

export const PRESETS: Preset[] = [
  { id: "ovh", name: "OVH", servers: { imap_host: "ssl0.ovh.net", imap_port: 993, smtp_host: "ssl0.ovh.net", smtp_port: 465, smtp_security: "ssl" } },
  { id: "gmail", name: "Gmail", hint: APP_PASSWORD,
    servers: { imap_host: "imap.gmail.com", imap_port: 993, smtp_host: "smtp.gmail.com", smtp_port: 465, smtp_security: "ssl" } },
  { id: "microsoft", name: "Microsoft 365", hint: "The organisation's admin must allow IMAP and SMTP login with a password (basic authentication) for this mailbox.",
    servers: { imap_host: "outlook.office365.com", imap_port: 993, smtp_host: "smtp.office365.com", smtp_port: 587, smtp_security: "starttls" } },
  { id: "icloud", name: "iCloud", hint: APP_PASSWORD,
    servers: { imap_host: "imap.mail.me.com", imap_port: 993, smtp_host: "smtp.mail.me.com", smtp_port: 587, smtp_security: "starttls" } },
  { id: "fastmail", name: "Fastmail", hint: APP_PASSWORD,
    servers: { imap_host: "imap.fastmail.com", imap_port: 993, smtp_host: "smtp.fastmail.com", smtp_port: 465, smtp_security: "ssl" } },
  { id: "other", name: "Other", servers: null },
]

export const preset = (id: string): Preset => PRESETS.find((p) => p.id === id) ?? PRESETS[0]

const DOMAINS: Record<string, string> = {
  "gmail.com": "gmail", "googlemail.com": "gmail",
  "outlook.com": "microsoft", "hotmail.com": "microsoft", "live.com": "microsoft", "msn.com": "microsoft",
  "outlook.fr": "microsoft", "hotmail.fr": "microsoft", "live.fr": "microsoft", "hotmail.co.uk": "microsoft", "live.co.uk": "microsoft",
  "icloud.com": "icloud", "me.com": "icloud", "mac.com": "icloud",
  "fastmail.com": "fastmail", "fastmail.fm": "fastmail",
}

/** The provider an address obviously belongs to, from its domain; null
    when its domain says nothing (a business's own domain). */
export function guess(address: string): string | null {
  const domain = address.trim().toLowerCase().split("@")[1] ?? ""
  return DOMAINS[domain] ?? null
}

/** The preset whose servers are exactly these; "other" when none. */
export function matching(s: Servers): string {
  const same = (a: Servers) => a.imap_host === s.imap_host.trim().toLowerCase() && a.imap_port === s.imap_port
    && a.smtp_host === s.smtp_host.trim().toLowerCase() && a.smtp_port === s.smtp_port && a.smtp_security === s.smtp_security
  return PRESETS.find((p) => p.servers && same(p.servers))?.id ?? "other"
}
