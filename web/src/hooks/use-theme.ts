import { useEffect, useState } from "react"

const KEY = "airth-theme"
const EVENT = "airth-theme"

// Light or dark, shared with the other interfaces through the same key
// (/theme.js); the system's choice until the founder picks one.
function initial(): "light" | "dark" {
  try {
    const t = localStorage.getItem(KEY)
    if (t === "light" || t === "dark") return t
  } catch { /* private window */ }
  return matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark"
}

export function useTheme() {
  const [theme, set] = useState(initial)
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark")
    document.documentElement.dataset.theme = theme
  }, [theme])
  // Every user of the theme follows a change made by any of them.
  useEffect(() => {
    const on = () => set(initial())
    addEventListener(EVENT, on)
    return () => removeEventListener(EVENT, on)
  }, [])
  return {
    theme,
    setTheme: (t: "light" | "dark") => {
      try { localStorage.setItem(KEY, t) } catch { /* private window */ }
      set(t)
      dispatchEvent(new Event(EVENT))
    },
  }
}
