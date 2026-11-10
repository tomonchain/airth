import { useState } from "react"
import { Smartphone } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { usePoll } from "@/hooks/use-poll"
import { api, enc } from "@/lib/api"

type Asking = { asking: { id: string; name: string; code: string }[] }

/** A device asking for access (decision 118), on whatever page is open: the
    founder who just pressed Ask on a new phone answers here at once. */
export function DeviceAsks() {
  const { data, refresh } = usePoll<Asking>(() => api("devices"), 10000, [])
  const [busy, setBusy] = useState(false)
  const a = data?.asking[0]
  if (!a) return null
  async function answer(verdict: "allow" | "refuse") {
    setBusy(true)
    try {
      await api(`devices/asking/${enc(a!.id)}/${verdict}`, {})
      toast.success(verdict === "allow" ? `${a!.name} is allowed.` : `${a!.name} is refused.`)
    } catch (e) { toast.error((e as Error).message) } finally { setBusy(false); refresh() }
  }
  return (
    <div role="alertdialog" aria-label="A device asks for access"
      className="fixed inset-x-3 top-[calc(env(safe-area-inset-top)+0.75rem)] z-50 mx-auto max-w-md rounded-xl border bg-popover p-4 text-sm shadow-lg duration-200 animate-in fade-in slide-in-from-top-2">
      <div className="flex items-start gap-3">
        <Smartphone className="mt-0.5 size-5 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <p className="font-medium">{a.name} asks for access</p>
          <p className="mt-0.5 text-muted-foreground">Allow it only if it shows <span className="font-mono font-semibold tracking-widest text-foreground">{a.code}</span> and you hold it.</p>
          <div className="mt-3 flex gap-2">
            <Button size="sm" disabled={busy} onClick={() => answer("allow")}>Allow</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => answer("refuse")}>Refuse</Button>
          </div>
        </div>
      </div>
    </div>
  )
}
