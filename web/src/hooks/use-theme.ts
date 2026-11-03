import { useEffect, useState } from "react"

const KEY = "airth-theme"

// Light or dark, shared with the other interfaces through the same key
// (/theme.js); the system's choice until the founder picks one.
function initial(): "light" | "dark" {
  try {
    const t = localStorage.getItem(KEY)
    if (t === "light" || t === "dark") return t
  } catch {}
  return matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark"
}

export function useTheme() {
  const [theme, set] = useState(initial)
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark")
    document.documentElement.dataset.theme = theme
  }, [theme])
  return {
    theme,
    setTheme: (t: "light" | "dark") => {
      try { localStorage.setItem(KEY, t) } catch {}
      set(t)
    },
  }
}
