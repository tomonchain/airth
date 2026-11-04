// From Beautiful UI (beautifului.dev, by Turbo), MIT licence: the pixel-grid
// loader of its "loading-state" item, its Drive wavefront only, without the
// remote video of its Surfer variant.
import { useEffect, useState } from "react"

const chevron = Array.from({ length: 9 }, (_, i) => {
  const r = Math.floor(i / 3), c = i % 3
  return (c + Math.abs(r - 1)) * 90
})

export function LoaderGrid({ className = "" }: { className?: string }) {
  return (
    <span aria-hidden className={`grid shrink-0 grid-cols-[repeat(3,4px)] gap-[1.5px] ${className}`}>
      {chevron.map((delay, i) => (
        <span key={i} className="size-[4px] rounded-[1px] bg-foreground" style={{ opacity: 0.15, animation: `pixel-on 650ms ease-in-out ${delay}ms infinite` }} />
      ))}
    </span>
  )
}

/** Seconds since it appeared, in tabular figures. */
export function useElapsed() {
  const [ds, setDs] = useState(0)
  useEffect(() => {
    const t = setInterval(() => setDs((d) => d + 1), 100)
    return () => clearInterval(t)
  }, [])
  const total = ds / 10
  return total < 60 ? `${total.toFixed(1)}s` : `${Math.floor(total / 60)}m ${(total % 60).toFixed(1)}s`
}
