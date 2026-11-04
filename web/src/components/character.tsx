// Agent characters (decision 65, docs/design/characters), ported from
// src/ui/next/js/11-characters.js: drawn by code as SVG from the agent's
// seed, department, accessory and state.
import type { AgentState } from "@/lib/api"

type Shape = { body: string; eye: number; top: number; belly: number[]; feet: number; armY: number; armDx: number }
const SHAPES: Record<string, Shape> = {
  round: { body: '<ellipse cx="60" cy="70" rx="38" ry="35"/>', eye: 62, top: 35, belly: [60, 84, 22, 14], feet: 104, armY: 76, armDx: 40 },
  bean: { body: '<rect x="28" y="24" width="64" height="80" rx="32"/>', eye: 54, top: 24, belly: [60, 82, 20, 15], feet: 104, armY: 72, armDx: 34 },
  block: { body: '<rect x="25" y="34" width="70" height="70" rx="24"/>', eye: 60, top: 34, belly: [60, 84, 22, 13], feet: 104, armY: 76, armDx: 37 },
}
const INK = "#0F1011"
const DEPTS = ["leadership", "marketing", "sales", "support", "product", "operations", "finance"]
const LABEL: Record<AgentState, string> = { idle: "idle", working: "working", needs: "needs you", incident: "incident", asleep: "asleep" }

function accessory(kind: string | null, s: Shape, state: string, dark: string) {
  if (kind === "pen") return `<g transform="translate(${60 + s.armDx - 2} ${s.armY - 18}) rotate(28)"><g class="acc${state === "working" ? " scribble" : ""}"><rect x="-3.5" y="0" width="7" height="30" rx="2" fill="#ECEDEE"/><rect x="-3.5" y="0" width="7" height="6" rx="2" fill="#8FA6FF"/><path d="M-3.5 30 L0 38 L3.5 30 Z" fill="#ECEDEE"/><path d="M-1.2 34.5 L0 38 L1.2 34.5 Z" fill="${INK}"/></g></g>`
  const t = s.top
  if (kind === "lens") return `<g class="acc${state === "working" ? " scan" : ""}"><path d="M60 ${t + 2} C60 ${t - 10} 66 ${t - 14} 70 ${t - 18}" fill="none" stroke="${dark}" stroke-width="4" stroke-linecap="round"/><circle cx="72" cy="${t - 21}" r="7.5" fill="#ECEDEE"/><circle cx="72" cy="${t - 21}" r="4.5" fill="#8FA6FF"/><circle cx="70.5" cy="${t - 22.5}" r="1.6" fill="#fff"/></g>`
  if (kind === "book") return `<g transform="translate(60 ${s.armY + 12})"><g class="acc${state === "working" ? " flip" : ""}"><rect x="-17" y="-11" width="34" height="22" rx="3" fill="#ECEDEE"/><rect x="-1.2" y="-11" width="2.4" height="22" fill="#C9BDFB"/><path d="M-12 -4 H-5 M-12 1 H-5 M5 -4 H12 M5 1 H12" stroke="#8B929B" stroke-width="1.6" stroke-linecap="round"/></g></g>`
  return ""
}

function eyes(s: Shape, state: string, gap: number) {
  const y = s.eye, l = 48 - gap, r = 72 + gap
  if (state === "asleep") return `<g fill="none" stroke="${INK}" stroke-width="3" stroke-linecap="round"><path d="M${l - 6} ${y} q6 5 12 0"/><path d="M${r - 6} ${y} q6 5 12 0"/></g>`
  if (state === "incident") return `<g fill="none" stroke="${INK}" stroke-width="2.6" stroke-linecap="round"><path d="M${l - 4} ${y - 5} l8 8 m0 -8 l-8 8"/><path d="M${r - 4} ${y - 5} l8 8 m0 -8 l-8 8"/></g>`
  const up = state === "needs" ? -2 : 0
  return `<g class="eyes"><ellipse cx="${l}" cy="${y}" rx="9" ry="10" fill="#fff"/><ellipse cx="${r}" cy="${y}" rx="9" ry="10" fill="#fff"/>`
    + `<g class="pupils${state === "working" ? " look" : ""}"><circle cx="${l + 1}" cy="${y + 1 + up}" r="4.6" fill="${INK}"/><circle cx="${r + 1}" cy="${y + 1 + up}" r="4.6" fill="${INK}"/>`
    + `<circle cx="${l + 2.5}" cy="${y - 1 + up}" r="1.4" fill="#fff"/><circle cx="${r + 2.5}" cy="${y - 1 + up}" r="1.4" fill="#fff"/></g></g>`
}

function mouth(s: Shape, state: string) {
  const y = s.eye + 14
  if (state === "asleep") return `<ellipse cx="60" cy="${y + 1}" rx="3" ry="2.2" fill="${INK}" fill-opacity=".6"/>`
  if (state === "incident") return `<path d="M53 ${y + 3} q3.5 -4 7 0 q3.5 4 7 0" fill="none" stroke="${INK}" stroke-width="2.4" stroke-linecap="round"/>`
  if (state === "needs") return `<ellipse cx="60" cy="${y + 1}" rx="4" ry="3.6" fill="${INK}"/>`
  return `<path d="M54 ${y} q6 5 12 0" fill="none" stroke="${INK}" stroke-width="2.6" stroke-linecap="round"/>`
}

function arms(s: Shape, dark: string, state: string) {
  const y = s.armY, dx = s.armDx
  const left = `<ellipse cx="${60 - dx}" cy="${y}" rx="6" ry="10" fill="${dark}" transform="rotate(18 ${60 - dx} ${y})"/>`
  const right = state === "needs"
    ? `<g class="wave" style="transform-origin:${60 + dx - 6}px ${y - 6}px;transform-box:view-box"><ellipse cx="${60 + dx - 3}" cy="${y - 17}" rx="6" ry="12" fill="${dark}" transform="rotate(22 ${60 + dx - 3} ${y - 17})"/></g>`
    : `<ellipse cx="${60 + dx}" cy="${y}" rx="6" ry="10" fill="${dark}" transform="rotate(-18 ${60 + dx} ${y})"/>`
  return left + right
}

export type Look = { look?: string | null; department: string | null; accessory: string | null; state: AgentState; title?: string }

export function characterSvg(a: Look, still = false) {
  const seed = /^[0-9a-f]{8}$/.test(a.look ?? "") ? a.look! : "00000000"
  const s = SHAPES[["round", "bean", "block"][parseInt(seed.slice(0, 2), 16) % 3]]
  const gap = (parseInt(seed[2], 16) % 3) - 1
  const state = LABEL[a.state] ? a.state : "idle"
  const dept = a.department && DEPTS.includes(a.department) ? a.department : "none"
  const color = `var(--dept-${dept})`, dark = `var(--dept-${dept}-dark)`
  const [bx, by, brx, bry] = s.belly
  const behind = a.accessory === "lens"
  const body = '<g class="body">'
    + (behind ? accessory(a.accessory, s, state, dark) : "")
    + `<g fill="${dark}"><ellipse cx="47" cy="${s.feet}" rx="9" ry="5"/><ellipse cx="73" cy="${s.feet}" rx="9" ry="5"/></g>`
    + arms(s, dark, state)
    + `<g fill="${color}">${s.body}</g>`
    + `<ellipse cx="${bx}" cy="${by}" rx="${brx}" ry="${bry}" fill="#fff" fill-opacity=".18"/>`
    + eyes(s, state, gap) + mouth(s, state)
    + (behind ? "" : accessory(a.accessory, s, state, dark))
    + "</g>"
  return `<svg class="ch st-${state}${still ? " still" : ""}" viewBox="0 6 120 110" width="100%" height="100%" style="overflow:visible">${body}</svg>`
}

/** The agent as a character; the SVG is built here from known values only. */
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
