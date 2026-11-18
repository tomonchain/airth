/** Airth's version as the server says it, "0.11.0 (8f4a4b4)", in its two
    parts: the release, and the build (the commit it was built from, "+"
    when built with changes not committed). No build from a plain release. */
export function parseVersion(v: string | null | undefined): { release: string; build: string | null } | null {
  if (!v) return null
  const m = v.trim().match(/^(\S+)(?:\s+\(([0-9a-f]+\+?)\))?$/)
  return m ? { release: m[1], build: m[2] ?? null } : { release: v.trim(), build: null }
}
