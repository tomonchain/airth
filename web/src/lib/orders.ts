import { api, enc } from "@/lib/api"

/** Send an order: to `agent`, or, with "route", to the founder's
    interlocutor (decision 120) or the agent whose job it is. The task's id,
    and the agent who has it. */
export async function sendOrder(realm: string, text: string, agent: string): Promise<{ id: string; agent: string }> {
  let to = agent
  if (agent === "route") {
    const r = await api<{ agent: string | null; error?: string }>(`realms/${enc(realm)}/orders/route`, { text })
    if (!r.agent) throw new Error(r.error ?? "No agent fits: choose one.")
    to = r.agent
  }
  const out = await api<{ id: string }>(`realms/${enc(realm)}/orders`, { text, agent: to, watched: true })
  return { id: out.id, agent: to }
}
