// Which thread a task belongs in (decision 147), apart from React so it is
// tested: the Home's, a project's, or every one on an agent's page.

/** What these rules read of a task from the task list. */
export type Threaded = { project?: number | null; at?: string; updated_at?: string }

/** Which thread a conversation shows: every task (an agent's page), only
    those outside any project (the Home), or one project's. */
export type Scope = number | null | undefined

/** Whether a task belongs in a conversation of that scope. */
export function inThread(t: Threaded, scope: Scope): boolean {
  if (scope === undefined) return true
  return scope === null ? t.project == null : t.project === scope
}

export type ProjectLine = { id: number; name: string; count: number; at: string }

/** The Home's quiet line per project: each project whose tasks moved in the
    last `days`, with how many did, the most recent first. */
export function projectLines(tasks: Record<string, Threaded[]> | null, projects: { id: number; name: string }[], now: Date, days = 7): ProjectLine[] {
  const since = now.getTime() - days * 86400000
  const lines = new Map<number, ProjectLine>()
  for (const t of Object.values(tasks ?? {}).flat()) {
    const at = t.at ?? t.updated_at
    if (t.project == null || !at || !(Date.parse(at) >= since)) continue
    const p = projects.find((x) => x.id === t.project)
    if (!p) continue
    const line = lines.get(p.id) ?? { id: p.id, name: p.name, count: 0, at }
    line.count += 1
    if (Date.parse(at) > Date.parse(line.at)) line.at = at
    lines.set(p.id, line)
  }
  return [...lines.values()].sort((a, b) => Date.parse(b.at) - Date.parse(a.at))
}
