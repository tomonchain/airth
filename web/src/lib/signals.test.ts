import { describe, expect, it, vi } from "vitest"
import { either } from "./signals.ts"

describe("either", () => {
  it("aborts when either signal does, then lets go of both", () => {
    const a = new AbortController(), b = new AbortController()
    const removed = vi.spyOn(a.signal, "removeEventListener")
    const { signal } = either(a.signal, b.signal)
    b.abort("left")
    expect(signal.aborted).toBe(true)
    expect(signal.reason).toBe("left")
    expect(removed).toHaveBeenCalledTimes(1)
  })

  it("keeps no listener on a reused signal once a request is done", () => {
    const stop = new AbortController()
    const added = vi.spyOn(stop.signal, "addEventListener")
    const removed = vi.spyOn(stop.signal, "removeEventListener")
    for (let i = 0; i < 3; i++) either(AbortSignal.timeout(1000), stop.signal).done()
    expect(added).toHaveBeenCalledTimes(3)
    expect(removed).toHaveBeenCalledTimes(3)
  })

  it("is aborted at once by a signal already aborted, and is the one signal alone", () => {
    const a = new AbortController()
    a.abort("gone")
    expect(either(new AbortController().signal, a.signal).signal.aborted).toBe(true)
    expect(either(a.signal).signal).toBe(a.signal)
  })
})
