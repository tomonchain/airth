// An agent's face: a geometric shape in its department's colour (decision
// 65), with eyes that say its state. The shape comes from the agent's seed,
// so it stays the same from one visit to the next.
import type { AgentState } from "@/lib/api"

const SHAPES = [
  '<circle cx="50" cy="50" r="46"/>',
  '<rect x="6" y="6" width="88" height="88" rx="26"/>',
  '<path d="M50 4 L94 50 L50 96 L6 50 Z" stroke-linejoin="round" stroke-width="10"/>',
  '<path d="M27 8 H73 L96 50 L73 92 H27 L4 50 Z" stroke-linejoin="round" stroke-width="8"/>',
  '<rect x="16" y="4" width="68" height="92" rx="34"/>',
  '<path d="M50 6 L94 86 H6 Z" stroke-linejoin="round" stroke-width="12"/>',
]
const EYE_Y = [46, 46, 48, 48, 42, 62]
const DEPTS = ["leadership", "marketing", "sales", "support", "tech", "operations", "finance", "legal", "data"]
const LABEL: Record<AgentState, string> = { idle: "idle", working: "working", needs: "needs you", incident: "incident", asleep: "asleep" }
const INK = "#111113"

function eyes(state: string, y: number) {
  const l = 36, r = 64
  if (state === "asleep") return `<g fill="none" stroke="${INK}" stroke-width="4" stroke-linecap="round"><path d="M${l - 7} ${y} q7 6 14 0"/><path d="M${r - 7} ${y} q7 6 14 0"/></g>`
  if (state === "incident") return `<g fill="none" stroke="${INK}" stroke-width="3.6" stroke-linecap="round"><path d="M${l - 5} ${y - 5} l10 10 m0 -10 l-10 10"/><path d="M${r - 5} ${y - 5} l10 10 m0 -10 l-10 10"/></g>`
  const up = state === "needs" ? -2.5 : 0
  return `<g class="eyes"><ellipse cx="${l}" cy="${y}" rx="10" ry="11.5" fill="#fff"/><ellipse cx="${r}" cy="${y}" rx="10" ry="11.5" fill="#fff"/>`
    + `<g class="pupils${state === "working" ? " look" : ""}"><circle cx="${l + 1.5}" cy="${y + 1.5 + up}" r="5.2" fill="${INK}"/><circle cx="${r + 1.5}" cy="${y + 1.5 + up}" r="5.2" fill="${INK}"/></g></g>`
}

export type Look = { look?: string | null; department: string | null; accessory?: string | null; state: AgentState; title?: string }

export function characterSvg(a: Look, still = false) {
  const seed = /^[0-9a-f]{8}$/.test(a.look ?? "") ? a.look! : "00000000"
  const i = parseInt(seed.slice(0, 2), 16) % SHAPES.length
  const state = LABEL[a.state] ? a.state : "idle"
  const key = (a.department ?? "").trim().toLowerCase()
  const dept = DEPTS.includes(key) ? key : "none"
  // The business's own department: a colour of its own, from its name. Only
  // the number reaches the SVG, never the name.
  const hue = key && dept === "none" ? [...key].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7) : null
  const [top, bottom] = hue === null ? [`var(--dept-${dept})`, `var(--dept-${dept}-dark)`] : [`oklch(0.72 0.13 ${hue})`, `oklch(0.55 0.13 ${hue})`]
  const id = `g${seed}${hue ?? dept}`
  return `<svg class="ch st-${state}${still ? " still" : ""}" viewBox="0 0 100 100" width="100%" height="100%">`
    + `<defs><linearGradient id="${id}" x1="0" y1="0" x2="0.4" y2="1"><stop offset="0" stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/></linearGradient></defs>`
    + `<g class="body" fill="url(#${id})" stroke="url(#${id})">${SHAPES[i]}</g>`
    + eyes(state, EYE_Y[i]) + "</svg>"
}

/** The agent's face; the SVG is built here from known values only. */
export function Character({ agent, size = 48, still, className }: { agent: Look; size?: number; still?: boolean; className?: string }) {
  return (
    <span
      role="img"
      aria-label={`${agent.title ?? "Agent"}, ${LABEL[agent.state] ?? "idle"}`}
      className={className}
      style={{ width: size, height: size, display: "inline-block", flexShrink: 0 }}
      dangerouslySetInnerHTML={{ __html: characterSvg(agent, still) }}
    />
  )
}
