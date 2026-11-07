import { useEffect, useState } from "react"

// A phone: one column at a time, the pages in a bar at the bottom. The
// same width as Tailwind's `md`, so a class and this hook never disagree.
const QUERY = "(max-width: 767px)"

/** Whether the screen is a phone's now, for a first value. */
export const isNarrow = () => matchMedia(QUERY).matches

export function useNarrow() {
  const [narrow, set] = useState(isNarrow)
  useEffect(() => {
    const m = matchMedia(QUERY)
    const on = () => set(m.matches)
    m.addEventListener("change", on)
    return () => m.removeEventListener("change", on)
  }, [])
  return narrow
}
