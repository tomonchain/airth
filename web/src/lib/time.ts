export function ago(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return ""
  const s = Math.max(0, (now - Date.parse(iso)) / 1000)
  if (s < 45) return "just now"
  if (s < 3600) return `${Math.round(s / 60)}m ago`
  if (s < 86400) return `${Math.round(s / 3600)}h ago`
  if (s < 7 * 86400) return `${Math.round(s / 86400)}d ago`
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" })
}

export function took(from: string, to: string | null): string {
  if (!to) return ""
  const s = (Date.parse(to) - Date.parse(from)) / 1000
  if (s < 1) return "<1s"
  if (s < 60) return `${Math.round(s)}s`
  return `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`
}

export const usd = (n: number | null | undefined) => (n ? `$${n < 0.01 ? n.toFixed(3) : n.toFixed(2)}` : "")

/** A list's time: the clock today, the day before that. */
export function stamp(iso: string | null | undefined, now = new Date()): string {
  if (!iso) return ""
  const d = new Date(iso)
  if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
  const y = new Date(now); y.setDate(now.getDate() - 1)
  if (d.toDateString() === y.toDateString()) return "Yesterday"
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short" })
}
