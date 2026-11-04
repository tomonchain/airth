import { useCallback, useEffect, useRef, useState } from "react"

/** Loads `load()` now and again every `ms`, while the tab is visible:
    the floor is live without a push channel from the server. */
export function usePoll<T>(load: (() => Promise<T>) | null, ms: number, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [at, setAt] = useState<number>(0)
  const ref = useRef(load)
  ref.current = load
  // Which load is current: an answer for an agent or business the founder
  // has left must not land on the one they opened since.
  const generation = useRef(0)
  const busy = useRef(false)
  const last = useRef<string | null>(null)

  const refresh = useCallback(async () => {
    if (!ref.current) return
    const mine = generation.current
    busy.current = true
    try {
      const next = await ref.current()
      if (mine !== generation.current) return
      // The same answer as before changes nothing on screen: keep the old
      // object, so nothing below it renders again.
      const text = JSON.stringify(next)
      if (text !== last.current) { last.current = text; setData(next) }
      setError(null)
      setAt(Date.now())
    } catch (e) {
      if (mine === generation.current) setError((e as Error).message)
    } finally {
      if (mine === generation.current) busy.current = false
    }
  }, [])

  useEffect(() => {
    generation.current++
    busy.current = false
    last.current = null
    setData(null)
    refresh()
    // A slow server is not asked again before it has answered.
    const tick = () => document.visibilityState === "visible" && !busy.current && refresh()
    const t = setInterval(tick, ms)
    // Back on the tab: what changed meanwhile, at once.
    const back = () => document.visibilityState === "visible" && !busy.current && refresh()
    document.addEventListener("visibilitychange", back)
    return () => { clearInterval(t); document.removeEventListener("visibilitychange", back) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  return { data, error, at, refresh }
}
