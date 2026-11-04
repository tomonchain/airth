import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"

/** What an agent of this business may be given (decision 64): its
    integrations, its MCP servers (connectors) and the catalog skills
    installed. An agent's `integrations` names the first; `skills` the
    other two. */
export type Tools = { integrations: string[]; servers: { name: string; description: string }[]; skills: string[] }

export function useTools(realm: string) {
  return usePoll<Tools>(async () => {
    const [detail, mcp, catalog] = await Promise.all([
      api<{ integrations: { name: string }[] }>(`realms/${enc(realm)}`),
      api<{ name: string; description: string }[]>(`realms/${enc(realm)}/mcp`),
      api<{ installed: string[] }>(`realms/${enc(realm)}/catalog`),
    ])
    return { integrations: detail.integrations.map((i) => i.name), servers: mcp.map((m) => ({ name: m.name, description: m.description })), skills: catalog.installed }
  }, 600000, [realm])
}
