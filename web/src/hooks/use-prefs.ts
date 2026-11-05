import { useEffect, useState } from "react"

// Reading preferences, kept in this browser under the classic interface's
// keys (/theme.js), so both read the same choice.
const DEFAULTS = { text: "m", width: "normal", send: "enter" }
export type Prefs = typeof DEFAULTS
const EVENT = "airth-prefs"

function read(): Prefs {
  const get = (k: keyof Prefs) => { try { return localStorage.getItem(`airth-${k}`) ?? DEFAULTS[k] } catch { return DEFAULTS[k] } }
  return { text: get("text"), width: get("width"), send: get("send") }
}

export function usePrefs() {
  const [prefs, set] = useState(read)
  useEffect(() => {
    const on = () => set(read())
    addEventListener(EVENT, on)
    return () => removeEventListener(EVENT, on)
  }, [])
  return {
    prefs,
    setPref: (k: keyof Prefs, v: string) => {
      try { localStorage.setItem(`airth-${k}`, v) } catch { /* private window */ }
      dispatchEvent(new Event(EVENT))
    },
  }
}

// How each choice reads in a conversation.
export const ZOOM: Record<string, number> = { s: 0.92, m: 1, l: 1.1, xl: 1.22 }
export const WIDTH: Record<string, string> = { narrow: "max-w-2xl", normal: "max-w-3xl", wide: "max-w-5xl" }
