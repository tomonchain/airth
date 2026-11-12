import { api, enc } from "@/lib/api"

/** How tasks work in a business's folder. */
export const ISOLATION: [string, string][] = [
  ["auto", "Automatic: a worktree per task for git repositories"],
  ["none", "None: work directly in the folder"],
  ["worktree", "A worktree per task"],
]

/** Creates a business: its name, what it does (its profile, which every
    agent reads), its folder and how tasks work in it. Returns its name. */
export async function createBusiness(f: { name: string; what: string; root: string; isolation?: string }): Promise<string> {
  const what = f.what.trim()
  const r = await api<{ name: string }>("realms", {
    name: f.name.trim(), description: what.split("\n")[0].slice(0, 200), root: f.root.trim(),
    isolation: f.isolation && f.isolation !== "auto" ? f.isolation : null,
  })
  if (what) await api(`realms/${enc(r.name)}/profile`, { text: what }, "PUT")
  return r.name
}

export const slug = (v: string) => v.toLowerCase().replace(/[^a-z0-9_-]/g, "-")
