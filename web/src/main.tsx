import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import App from "./App"
import { MetalGradient } from "@/components/rail"
import { startWorker } from "@/lib/push"
import { applyTheme } from "@/hooks/use-theme"
import { useNarrow } from "@/hooks/use-narrow"
import "./index.css"

// No inline script runs under the app's CSP: this is the earliest the theme can be set.
applyTheme()
startWorker()

/** On a phone, toasts come down from the top, under the notch, clear of
    the thumb and the tab bar; on a computer, they stay at the bottom. */
function Toasts() {
  const narrow = useNarrow()
  // Sonner reads `mobileOffset` under 600 px only; a phone is under 768 here.
  const top = { top: "calc(env(safe-area-inset-top) + 8px)" }
  return <Toaster position={narrow ? "top-center" : "bottom-center"} offset={narrow ? top : undefined} mobileOffset={top} />
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <MetalGradient />
      <App />
      <Toasts />
    </TooltipProvider>
  </StrictMode>,
)
