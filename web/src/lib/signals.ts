/** Aborted when `a` or `b` is: by hand, as `AbortSignal.any` is missing
    from the iPhones of before iOS 17.4. `done` lets go of both: a signal
    reused across many requests (a page's long wait) must not keep a
    listener per request. */
export function either(a: AbortSignal, b?: AbortSignal): { signal: AbortSignal; done: () => void } {
  if (!b) return { signal: a, done: () => {} }
  const both = new AbortController()
  const done = () => { a.removeEventListener("abort", onA); b.removeEventListener("abort", onB) }
  const end = (s: AbortSignal) => () => { done(); both.abort(s.reason) }
  const onA = end(a), onB = end(b)
  if (a.aborted || b.aborted) both.abort((a.aborted ? a : b).reason)
  else { a.addEventListener("abort", onA); b.addEventListener("abort", onB) }
  return { signal: both.signal, done }
}
