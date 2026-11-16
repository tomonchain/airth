import { useEffect, useRef } from "react"
import { dot, lightAt, spots } from "@/lib/dither"
import type { Moment } from "@/lib/greeting"

// Each drawn dot is this many screen pixels: coarse enough to read as a
// dither, cheap enough to redraw a dozen times a second.
const CELL = 3
const FPS = 12

/** The Home's background: the moment's light, dithered into grey dots
    (light on a dark page, dark on a light one). It drifts slowly, stays
    still when the system asks for less motion, and stops while the tab is
    hidden. */
export function Dither({ moment }: { moment: Moment }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const ctx = canvas?.getContext("2d")
    if (!canvas || !ctx) return
    const motion = matchMedia("(prefers-reduced-motion: reduce)")
    const start = performance.now() - 120_000
    let img: ImageData | null = null
    let ink = [0, 0, 0, 0]
    // The ink follows the theme: read again whenever the page's class changes.
    function readInk() {
      const style = getComputedStyle(canvas!)
      const rgb = style.getPropertyValue("--dither-ink").trim().split(/\s+/).map(Number)
      ink = [...rgb, Math.round(255 * Number(style.getPropertyValue("--dither-alpha") || 0.14))]
    }
    function draw(now: number) {
      const w = Math.ceil(canvas!.clientWidth / CELL)
      const h = Math.ceil(canvas!.clientHeight / CELL)
      if (!w || !h) return
      if (!img || img.width !== w || img.height !== h) {
        canvas!.width = w
        canvas!.height = h
        img = ctx!.createImageData(w, h)
      }
      const data = img.data
      data.fill(0)
      // The spots move once per frame; every dot then reads them.
      const s = spots(moment, (now - start) / 1000)
      // The field is a square: on a wide page it stays round.
      const side = Math.max(w, h)
      const ox = (side - w) / 2
      const oy = (side - h) / 2
      for (let y = 0; y < h; y++) {
        const v = (y + oy) / side
        for (let x = 0; x < w; x++) {
          if (!dot(x, y, lightAt(s, (x + ox) / side, v))) continue
          const i = (y * w + x) * 4
          data[i] = ink[0]; data[i + 1] = ink[1]; data[i + 2] = ink[2]; data[i + 3] = ink[3]
        }
      }
      ctx!.putImageData(img, 0, 0)
    }
    let frame = 0
    let last = 0
    function loop(now: number) {
      if (now - last >= 1000 / FPS) { last = now; draw(now) }
      frame = requestAnimationFrame(loop)
    }
    // Moving only when the system allows motion and the tab is shown.
    function run() {
      cancelAnimationFrame(frame)
      frame = 0
      if (!motion.matches && document.visibilityState === "visible") frame = requestAnimationFrame(loop)
      else draw(performance.now())
    }
    readInk()
    run()
    const resize = new ResizeObserver(() => draw(performance.now()))
    resize.observe(canvas)
    const theme = new MutationObserver(() => { readInk(); draw(performance.now()) })
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })
    motion.addEventListener("change", run)
    document.addEventListener("visibilitychange", run)
    return () => {
      cancelAnimationFrame(frame)
      resize.disconnect()
      theme.disconnect()
      motion.removeEventListener("change", run)
      document.removeEventListener("visibilitychange", run)
    }
  }, [moment])
  return <canvas ref={ref} aria-hidden className="dither pointer-events-none absolute inset-0 size-full" />
}
