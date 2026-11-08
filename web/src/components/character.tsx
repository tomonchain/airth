// An agent's face: a simple shape with well-rounded corners, filled with its
// department's colour (decision 65), and two long white eyes that say its
// state. The shape comes from the agent's seed, so it stays the same from one
// visit to the next.
import type { AgentState } from "@/lib/api"

// Sharp shapes are rounded by a stroke of their own colour with round joins;
// each is sized so that, stroke included, all of them fill the same box.
// `ey` is where the eyes sit, `gap` half the space between them.
const SHAPES: { d: string; sw: number; ey: number; gap: number }[] = [
  { d: '<circle cx="50" cy="50" r="40"/>', sw: 0, ey: 48, gap: 10 }, // sphere
  { d: '<rect x="12" y="12" width="76" height="76" rx="22"/>', sw: 0, ey: 48, gap: 10 }, // square
  { d: '<path d="M50 18 L82 50 L50 82 L18 50 Z"/>', sw: 16, ey: 50, gap: 10 }, // diamond
  { d: '<path d="M50 22 L82 78 H18 Z"/>', sw: 18, ey: 60, gap: 8.5 }, // triangle
  { d: '<path d="M34 20 H66 L84 50 L66 80 H34 L16 50 Z"/>', sw: 14, ey: 50, gap: 10 }, // hexagon
  { d: '<rect x="26" y="10" width="48" height="80" rx="24"/>', sw: 0, ey: 46, gap: 8.5 }, // capsule
  { d: '<path d="M50 18 L82 42 L70 80 H30 L18 42 Z"/>', sw: 14, ey: 52, gap: 10 }, // pentagon
]
const DEPTS = ["leadership", "marketing", "sales", "support", "tech", "operations", "finance", "legal", "data"]
const LABEL: Record<AgentState, string> = { idle: "idle", working: "working", needs: "needs you", incident: "incident", asleep: "asleep" }

// Long white eyes, no pupils. Asleep: two short lines. Incident: two lines
// slanted like a frown. Needs you: taller and raised. Working: they look
// from side to side (index.css).
function eyes(state: string, y: number, gap: number) {
  const l = 50 - gap, r = 50 + gap
  if (state === "asleep") return `<g stroke="#fff" stroke-width="4.5" stroke-linecap="round"><path d="M${l - 4} ${y + 6} h8"/><path d="M${r - 4} ${y + 6} h8"/></g>`
  if (state === "incident") return `<g stroke="#fff" stroke-width="5" stroke-linecap="round"><path d="M${l - 4} ${y - 4} l8 8"/><path d="M${r + 4} ${y - 4} l-8 8"/></g>`
  const h = state === "needs" ? 32 : 26
  const top = y - h / 2 - (state === "needs" ? 4 : 0)
  return `<g class="look"><g class="eyes"><rect x="${l - 4.5}" y="${top}" width="9" height="${h}" rx="4.5" fill="#fff"/><rect x="${r - 4.5}" y="${top}" width="9" height="${h}" rx="4.5" fill="#fff"/></g></g>`
}

export type Look = { look?: string | null; department: string | null; accessory?: string | null; state: AgentState; title?: string }

export function characterSvg(a: Look, still = false) {
  const seed = /^[0-9a-f]{8}$/.test(a.look ?? "") ? a.look! : "00000000"
  const shape = SHAPES[parseInt(seed.slice(0, 2), 16) % SHAPES.length]
  const state = LABEL[a.state] ? a.state : "idle"
  const key = (a.department ?? "").trim().toLowerCase()
  const dept = DEPTS.includes(key) ? key : "none"
  // The business's own department: a colour of its own, from its name. Only
  // the number reaches the SVG, never the name.
  const hue = key && dept === "none" ? [...key].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7) : null
  const fill = hue === null ? `var(--dept-${dept})` : `oklch(0.68 0.13 ${hue})`
  return `<svg class="ch st-${state}${still ? " still" : ""}" viewBox="0 0 100 100" width="100%" height="100%">`
    + `<g class="body"><g fill="${fill}" stroke="${fill}" stroke-width="${shape.sw}" stroke-linejoin="round">${shape.d}</g>`
    + eyes(state, shape.ey, shape.gap) + "</g></svg>"
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
