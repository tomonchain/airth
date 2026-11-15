import { useEffect } from "react"

/** Where a horizontal swipe does nothing: where the finger types, scrolls
    sideways, or sits in a dialog or menu over the page. */
function held(target: EventTarget | null, root: HTMLElement): boolean {
  for (let el = target as HTMLElement | null; el && el !== root; el = el.parentElement) {
    if (el.matches("input, textarea, select, [contenteditable], [role=dialog], [role=menu], [data-no-swipe]")) return true
    const x = getComputedStyle(el).overflowX
    if ((x === "auto" || x === "scroll") && el.scrollWidth > el.clientWidth) return true
  }
  return !!document.querySelector("[role=dialog], [role=menu]")
}

/** A sideways swipe on `root`: a quick, mostly horizontal stroke of at least
    70 px, started away from the screen's edges, which the system keeps.
    The element, not a ref, so it is listened to once it is there. */
export function useSwipe(root: HTMLElement | null, enabled: boolean, onSwipe: (to: "left" | "right") => void) {
  useEffect(() => {
    if (!enabled || !root) return
    let start: { x: number; y: number; t: number } | null = null
    const down = (e: TouchEvent) => {
      const t = e.touches[0]
      start = e.touches.length === 1 && t.clientX > 24 && t.clientX < innerWidth - 24 && !held(e.target, root) ? { x: t.clientX, y: t.clientY, t: Date.now() } : null
    }
    const up = (e: TouchEvent) => {
      if (!start) return
      const t = e.changedTouches[0]
      const dx = t.clientX - start.x, dy = t.clientY - start.y
      const quick = Date.now() - start.t < 600
      start = null
      if (quick && Math.abs(dx) > 70 && Math.abs(dx) > 1.8 * Math.abs(dy) && !getSelection()?.toString()) onSwipe(dx < 0 ? "left" : "right")
    }
    root.addEventListener("touchstart", down, { passive: true })
    root.addEventListener("touchend", up, { passive: true })
    root.addEventListener("touchcancel", () => (start = null), { passive: true })
    return () => {
      root.removeEventListener("touchstart", down)
      root.removeEventListener("touchend", up)
    }
  }, [root, enabled, onSwipe])
}
