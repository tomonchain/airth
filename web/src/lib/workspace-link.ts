// A link an agent writes to a file of the business's workspace (decision
// 146): a relative path from the workspace's root, "docs/finance/q3.md".
// It opens in the Documents page, never as an address. What reaches the
// server is still checked there (`documents::resolve` keeps every read
// inside the workspace); this only decides what the page offers as a link.

/** The workspace path `href` names, normalised ("docs/./a.md" → "docs/a.md"),
    or null when it is anything else: an address with a scheme (http:,
    file:, javascript:, a drive "C:"), an absolute path, a backslash, a
    query or fragment, or a step out of the workspace ("..", also encoded). */
export function workspacePath(href: string | null | undefined): string | null {
  if (!href) return null
  let raw = href.trim()
  if (!raw) return null
  // Markdown encodes some characters (a space as %20): read them once.
  try { raw = decodeURIComponent(raw) } catch { return null }
  // Anything still encoded after that is a trick, not a file name.
  if (/%[0-9a-f]{2}/i.test(raw)) return null
  if (/^[a-z][a-z0-9+.-]*:/i.test(raw)) return null
  if (/^[/~]/.test(raw) || /[\\?#\u0000-\u001f]/.test(raw)) return null
  const parts: string[] = []
  for (const part of raw.split("/")) {
    if (part === "" || part === ".") continue
    if (part === "..") return null
    parts.push(part)
  }
  if (!parts.length || raw.endsWith("/")) return null
  return parts.join("/")
}
