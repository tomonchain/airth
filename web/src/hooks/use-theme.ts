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

function chosen(): "light" | "dark" | "system" {
  try {
    const t = localStorage.getItem(KEY)
    if (t === "light" || t === "dark") return t
  } catch { /* private window */ }
  return "system"
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
  // Following the system: a change of the Mac's appearance shows at once.
  useEffect(() => {
    const m = matchMedia("(prefers-color-scheme: light)")
    const on = () => { if (chosen() === "system") set(initial()) }
    m.addEventListener("change", on)
    return () => m.removeEventListener("change", on)
  }, [])
  return {
    theme,
    choice: chosen(),
    setTheme: (t: "light" | "dark" | "system") => {
      try { if (t === "system") localStorage.removeItem(KEY); else localStorage.setItem(KEY, t) } catch { /* private window */ }
      set(initial())
      dispatchEvent(new Event(EVENT))
    },
  }
}
