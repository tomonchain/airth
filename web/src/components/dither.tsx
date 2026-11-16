import { useEffect, useRef } from "react"
import { dot, field } from "@/lib/dither"
import type { Moment } from "@/lib/greeting"

// Each drawn dot is this many screen pixels: coarse enough to read as a
// dither, cheap enough to redraw a dozen times a second.
const CELL = 3
const FPS = 12

/** The Home's background: the moment's light, dithered into grey dots
    (light on a dark page, dark on a light one). It drifts slowly, stays
    still when the system asks for less motion, and rests when the tab is
    hidden. */
export function Dither({ moment }: { moment: Moment }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const ctx = canvas?.getContext("2d")
    if (!canvas || !ctx) return
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches
    let frame = 0
    let last = 0
    const start = performance.now() - 120_000
    function draw(now: number) {
      const w = Math.ceil(canvas!.clientWidth / CELL)
      const h = Math.ceil(canvas!.clientHeight / CELL)
      if (!w || !h) return
      if (canvas!.width !== w || canvas!.height !== h) { canvas!.width = w; canvas!.height = h }
      // The ink follows the theme: read each frame, so a switch shows at once.
      const ink = getComputedStyle(canvas!).getPropertyValue("--dither-ink").trim().split(/\s+/).map(Number)
      const alpha = Math.round(255 * Number(getComputedStyle(canvas!).getPropertyValue("--dither-alpha") || 0.14))
      const img = ctx!.createImageData(w, h)
      const t = (now - start) / 1000
      // The field is a square: on a wide page it stays round.
      const side = Math.max(w, h)
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const u = (x + (side - w) / 2) / side
          const v = (y + (side - h) / 2) / side
          if (!dot(x, y, field(moment, u, v, t))) continue
          const i = (y * w + x) * 4
          img.data[i] = ink[0]; img.data[i + 1] = ink[1]; img.data[i + 2] = ink[2]; img.data[i + 3] = alpha
        }
      }
      ctx!.putImageData(img, 0, 0)
    }
    function loop(now: number) {
      if (now - last >= 1000 / FPS) { last = now; draw(now) }
      frame = requestAnimationFrame(loop)
    }
    if (still) draw(performance.now())
    else frame = requestAnimationFrame(loop)
    const resize = new ResizeObserver(() => draw(performance.now()))
    resize.observe(canvas)
    return () => { cancelAnimationFrame(frame); resize.disconnect() }
  }, [moment])
  return <canvas ref={ref} aria-hidden className="dither pointer-events-none absolute inset-0 size-full" />
}
