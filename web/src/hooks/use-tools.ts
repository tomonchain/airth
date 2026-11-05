import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"

/** What an agent of this business may be given (decision 64): its
    integrations, its MCP servers (connectors) and the catalog skills
    installed. An agent's `integrations` names the first; `skills` the
    other two. */
export type Tools = { integrations: string[]; servers: { name: string; description: string }[]; skills: string[] }

/** Loaded only while `on` (the dialog open): three requests for a list the
    founder sees now and then. */
export function useTools(realm: string, on = true) {
  return usePoll<Tools>(on ? async () => {
    const [detail, mcp, catalog] = await Promise.all([
      api<{ integrations: { name: string }[] }>(`realms/${enc(realm)}`),
      api<{ name: string; description: string }[]>(`realms/${enc(realm)}/mcp`),
      api<{ installed: string[] }>(`realms/${enc(realm)}/catalog`),
    ])
    return { integrations: detail.integrations.map((i) => i.name), servers: mcp.map((m) => ({ name: m.name, description: m.description })), skills: catalog.installed }
  } : null, 600000, [realm, on])
}
