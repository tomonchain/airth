import { useEffect } from "react"
import { api, enc } from "@/lib/api"

/** The event a business's pages reload on: something they show may have
    changed. */
export const CHANGED = "airth:changed"

/** While the tab is visible, waits on the server for `realm` to change
    (its Needs you, its tasks, a run saved) and tells the page at once:
    pages that show it reload then, not at their next look. */
export function useChanges(realm: string | null) {
  useEffect(() => {
    if (!realm) return
    let on = true
    // Left (another business, the page gone): the server's wait ends too.
    const stop = new AbortController()
    let mark: string | null = null
    let timer: ReturnType<typeof setTimeout> | null = null
    const later = (ms: number) => { timer = setTimeout(() => { timer = null; wait() }, ms) }
    const wait = async () => {
      if (!on) return
      if (document.visibilityState !== "visible") { later(1500); return }
      try {
        const q = mark === null ? "" : `?since=${enc(mark)}`
        const r = await api<{ mark: string }>(`realms/${enc(realm)}/changes${q}`, undefined, undefined, 35000, stop.signal)
        if (!on) return
        if (mark !== null && r.mark !== mark) window.dispatchEvent(new CustomEvent(CHANGED, { detail: realm }))
        mark = r.mark
        wait()
      } catch {
        // Not answering: look again in a while, the pages' own looks go on.
        if (on) later(5000)
      }
    }
    wait()
    return () => { on = false; stop.abort(); if (timer) clearTimeout(timer) }
  }, [realm])
}
