import { useCallback, useEffect, useRef, useState } from "react"

/** Loads `load()` now and again every `ms`, while the tab is visible:
    the floor is live without a push channel from the server. */
export function usePoll<T>(load: (() => Promise<T>) | null, ms: number, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [at, setAt] = useState<number>(0)
  const ref = useRef(load)
  ref.current = load

  const refresh = useCallback(async () => {
    if (!ref.current) return
    try {
      setData(await ref.current())
      setError(null)
      setAt(Date.now())
    } catch (e) {
      setError((e as Error).message)
    }
  }, [])

  useEffect(() => {
    setData(null)
    refresh()
    const t = setInterval(() => document.visibilityState === "visible" && refresh(), ms)
    return () => clearInterval(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  return { data, error, at, refresh }
}
