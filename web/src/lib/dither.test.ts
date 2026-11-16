import { describe, expect, it } from "vitest"
import { BAYER, dot, field } from "./dither.ts"

describe("the Bayer matrix", () => {
  it("ranks the 64 cells once each", () => {
    expect(BAYER).toHaveLength(64)
    expect(new Set(BAYER).size).toBe(64)
    expect(Math.min(...BAYER)).toBe(0)
    expect(Math.max(...BAYER)).toBe(63)
  })
})

describe("dot", () => {
  it("draws about as many dots as the brightness asks", () => {
    for (const v of [0, 0.25, 0.5, 0.9]) {
      let n = 0
      for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) if (dot(x, y, v)) n++
      expect(n).toBe(Math.round(v * 64))
    }
  })
})

describe("field", () => {
  it("stays in 0..1, bright at the moment's spot and dark at the edges", () => {
    for (const m of ["morning", "afternoon", "evening", "night"] as const) {
      for (let i = 0; i <= 10; i++) for (let j = 0; j <= 10; j++) {
        const f = field(m, i / 10, j / 10, 123)
        expect(f).toBeGreaterThanOrEqual(0)
        expect(f).toBeLessThanOrEqual(1)
      }
      expect(field(m, 0, 1, 0)).toBeLessThan(0.1)
    }
    expect(field("morning", 0.28, 0.3, 0)).toBeGreaterThan(field("morning", 0.75, 0.25, 0))
    expect(field("evening", 0.7, 0.62, 0)).toBeGreaterThan(field("evening", 0.25, 0.7, 0))
  })
})
