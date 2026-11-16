import { describe, expect, it } from "vitest"
import { MAX_ZOOM, centred, clamp, moved, scale, source, zoomed } from "./crop.ts"

const V = 280
const close = (a: number, b: number) => expect(Math.abs(a - b)).toBeLessThan(1e-6)

describe("centred", () => {
  it("covers the view with the shorter side, and centres the longer", () => {
    const c = centred(1200, 800, V)
    expect(c.zoom).toBe(1)
    close(scale(1200, 800, V, 1), V / 800)
    close(c.y, 0)
    close(c.x, (V - 1200 * (V / 800)) / 2)
    const s = source(c, 1200, 800, V)
    close(s.side, 800)
    close(s.sx, 200)
    close(s.sy, 0)
  })
})

describe("clamp", () => {
  it("never leaves a gap in the view, and keeps the zoom in 1..MAX_ZOOM", () => {
    expect(clamp({ x: 50, y: 50, zoom: 1 }, 1000, 1000, V)).toEqual({ x: 0, y: 0, zoom: 1 })
    const far = clamp({ x: -99999, y: -99999, zoom: 1 }, 1200, 800, V)
    close(far.x, V - 1200 * scale(1200, 800, V, 1))
    close(far.y, 0)
    expect(clamp({ x: 0, y: 0, zoom: 0.2 }, 500, 500, V).zoom).toBe(1)
    expect(clamp({ x: 0, y: 0, zoom: 99 }, 500, 500, V).zoom).toBe(MAX_ZOOM)
  })
})

describe("moved", () => {
  it("drags the image, and stops at its edges", () => {
    const c = centred(1200, 800, V)
    const right = moved(c, 10, 0, 1200, 800, V)
    close(right.x, c.x + 10)
    // Down is already at the edge for a landscape image at zoom 1.
    close(moved(c, 0, 30, 1200, 800, V).y, 0)
  })
})

describe("zoomed", () => {
  it("keeps the view's centre on the same image point", () => {
    const c = centred(1000, 1000, V)
    const z = zoomed(c, 2, 1000, 1000, V)
    const before = source(c, 1000, 1000, V)
    const after = source(z, 1000, 1000, V)
    close(before.sx + before.side / 2, after.sx + after.side / 2)
    close(before.sy + before.side / 2, after.sy + after.side / 2)
    close(after.side, before.side / 2)
  })

  it("zooming back out stays inside the image", () => {
    const deep = moved(zoomed(centred(1000, 1000, V), 4, 1000, 1000, V), -9999, -9999, 1000, 1000, V)
    const out = zoomed(deep, 1, 1000, 1000, V)
    const s = source(out, 1000, 1000, V)
    expect(s.sx).toBeGreaterThanOrEqual(-1e-9)
    expect(s.sy).toBeGreaterThanOrEqual(-1e-9)
    expect(s.sx + s.side).toBeLessThanOrEqual(1000 + 1e-9)
    expect(s.sy + s.side).toBeLessThanOrEqual(1000 + 1e-9)
  })
})

describe("source", () => {
  it("is always a square inside the image", () => {
    for (const [w, h] of [[640, 480], [480, 640], [300, 300]]) {
      let c = centred(w, h, V)
      for (const [dx, dy, z] of [[30, -20, 1.5], [-500, 400, 3], [9999, 9999, 4]]) {
        c = moved(zoomed(c, z, w, h, V), dx, dy, w, h, V)
        const s = source(c, w, h, V)
        expect(s.sx).toBeGreaterThanOrEqual(-1e-9)
        expect(s.sy).toBeGreaterThanOrEqual(-1e-9)
        expect(s.sx + s.side).toBeLessThanOrEqual(w + 1e-9)
        expect(s.sy + s.side).toBeLessThanOrEqual(h + 1e-9)
      }
    }
  })
})
