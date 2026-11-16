import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import App from "./App"
import { MetalGradient } from "@/components/rail"
import { startWorker } from "@/lib/push"
import { applyTheme } from "@/hooks/use-theme"
import "./index.css"

// No inline script runs under the app's CSP: this is the earliest the theme can be set.
applyTheme()
startWorker()

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <MetalGradient />
      <App />
      <Toaster position="bottom-center" />
    </TooltipProvider>
  </StrictMode>,
)
