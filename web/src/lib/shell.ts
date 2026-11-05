// The Mac app (src/desktop) sets `window.airthShell` and listens on
// `window.ipc`: the page is its title bar.
type Ipc = { postMessage: (m: string) => void }
const w = window as unknown as { airthShell?: string; ipc?: Ipc }

export const mac = w.airthShell === "mac" && !!w.ipc

const INTERACTIVE = "button, a, input, textarea, select, label, [role=button], [role=menuitem]"

/** An empty spot of the top bar moves the window; a double click zooms it,
    as in other Mac apps. */
export function titleBar(e: React.MouseEvent) {
  if (!mac || e.button !== 0 || (e.target as HTMLElement).closest(INTERACTIVE)) return
  e.preventDefault()
  w.ipc!.postMessage(e.detail === 2 ? "zoom" : "drag")
}
