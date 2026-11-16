import type { Moment } from "./greeting.ts"

// The Home's background: a soft field of light, ordered-dithered into grey
// dots (a Bayer matrix), as old screens drew greys with black and white.
// The drawing is in components/dither.tsx; what decides each dot is here.

/** The 8×8 Bayer matrix: each cell's threshold rank, 0 to 63. */
export const BAYER = (() => {
  const m = [[0]]
  let out = m
  for (let n = 1; n < 8; n *= 2) {
    const next: number[][] = Array.from({ length: n * 2 }, () => Array(n * 2).fill(0))
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
      const v = out[y][x] * 4
      next[y][x] = v
      next[y][x + n] = v + 2
      next[y + n][x] = v + 3
      next[y + n][x + n] = v + 1
    }
    out = next
  }
  return out.flat()
})()

/** Whether the dot at (x, y) is drawn for a brightness `v` in 0..1. */
export const dot = (x: number, y: number, v: number) => v > (BAYER[(y & 7) * 8 + (x & 7)] + 0.5) / 64

/** Where the light sits for each moment, in the page's 0..1 square: high
    on the left in the morning, wide at noon, low on the right at evening,
    faint and central at night. */
const SPOTS: Record<Moment, { x: number; y: number; r: number; k: number }[]> = {
  morning: [{ x: 0.28, y: 0.3, r: 0.32, k: 0.9 }, { x: 0.7, y: 0.65, r: 0.26, k: 0.45 }],
  afternoon: [{ x: 0.5, y: 0.35, r: 0.36, k: 0.85 }, { x: 0.22, y: 0.7, r: 0.24, k: 0.4 }],
  evening: [{ x: 0.7, y: 0.62, r: 0.32, k: 0.85 }, { x: 0.3, y: 0.32, r: 0.24, k: 0.45 }],
  night: [{ x: 0.5, y: 0.5, r: 0.3, k: 0.6 }, { x: 0.78, y: 0.25, r: 0.18, k: 0.3 }],
}

/** The light at (u, v) in 0..1 at time `t` (seconds): the moment's spots,
    each drifting slowly on its own small circle, fading to nothing at the
    page's edges. Always in 0..1. */
export function field(m: Moment, u: number, v: number, t: number): number {
  let s = 0
  SPOTS[m].forEach((p, i) => {
    const cx = p.x + 0.05 * Math.sin(t / (23 + i * 7) + i)
    const cy = p.y + 0.04 * Math.cos(t / (29 + i * 5) + i * 2)
    const d = ((u - cx) ** 2 + (v - cy) ** 2) / (p.r * p.r)
    s += p.k * Math.exp(-d)
  })
  return Math.min(1, Math.max(0, s))
}
