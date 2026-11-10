import { useEffect, useState } from "react"
import { Loader, Smartphone } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { accessStatus, askAccess, deviceName, pair } from "@/lib/api"

type Asked = { secret: string; code: string }

/** A device without a key asks the founder for one (decision 118): it shows
    a code, the founder allows it on Telegram or on a device already in, and
    it gets a key of its own. A pairing link still works, kept below. */
export function Pair() {
  const [asked, setAsked] = useState<Asked | null>(null)
  const [busy, setBusy] = useState(false)
  const [said, setSaid] = useState("")
  const [linking, setLinking] = useState(false)

  async function ask() {
    setBusy(true); setSaid("")
    try { setAsked(await askAccess()) } catch (e) { setSaid((e as Error).message) } finally { setBusy(false) }
  }

  // Waiting: ask again every two seconds until the founder answers.
  useEffect(() => {
    if (!asked) return
    let stop = false
    const tick = async () => {
      if (stop) return
      try {
        const s = await accessStatus(asked.secret)
        if (s === "allowed") { location.replace("/"); return }
        if (s === "gone") { setAsked(null); setSaid("Not allowed: the request was refused, or waited too long. You can ask again."); return }
      } catch { /* Airth is restarting: keep asking */ }
      setTimeout(tick, 2000)
    }
    const first = setTimeout(tick, 2000)
    return () => { stop = true; clearTimeout(first) }
  }, [asked])

  return (
    <div className="grid h-svh place-items-center bg-background px-6 pt-[env(safe-area-inset-top)]">
      <div className="w-full max-w-sm text-center">
        <Smartphone className="mx-auto size-9 text-muted-foreground" strokeWidth={1.6} />
        {!asked ? (
          <>
            <h1 className="mt-4 text-xl font-semibold">Open Airth on this device</h1>
            <p className="mt-2 text-sm text-muted-foreground">
              Ask for access. You allow it on Telegram, or in Airth on a device already in.
            </p>
            <Button className="mt-5 w-full" disabled={busy} onClick={ask}>Ask for access</Button>
            {said && <p className="mt-3 text-sm text-muted-foreground">{said}</p>}
          </>
        ) : (
          <>
            <h1 className="mt-4 text-xl font-semibold">Allow this device</h1>
            <p className="mt-2 text-sm text-muted-foreground">On Telegram, or in Airth on a device already in, allow the request showing this code:</p>
            <p className="mt-5 font-mono text-4xl font-semibold tracking-[0.3em]" aria-label="Code">{asked.code}</p>
            <p className="mt-2 text-xs text-muted-foreground">{deviceName()}</p>
            <p className="mt-5 flex items-center justify-center gap-2 text-sm text-muted-foreground"><Loader className="size-4 animate-spin" /> Waiting for your answer</p>
          </>
        )}
        <div className="mt-10 border-t pt-4">
          {linking ? <Link /> : (
            <button className="text-xs text-muted-foreground underline-offset-4 hover:underline" onClick={() => setLinking(true)}>I have a pairing link</button>
          )}
        </div>
      </div>
    </div>
  )
}

/** The main key's link, from `airth phone pair` on the server. */
function Link() {
  const [link, setLink] = useState("")
  const [wrong, setWrong] = useState(false)
  function go() {
    if (pair(link.trim())) location.replace("/")
    else setWrong(true)
  }
  return (
    <>
      <div className="flex gap-2">
        <Input value={link} onChange={(e) => { setLink(e.target.value); setWrong(false) }} onKeyDown={(e) => e.key === "Enter" && go()}
          placeholder="https://…ts.net/#token=…" spellCheck={false} autoCapitalize="off" autoCorrect="off" aria-label="Pairing link" />
        <Button variant="outline" disabled={!link.trim()} onClick={go}>Pair</Button>
      </div>
      {wrong && <p className="mt-2 text-sm text-red-500">This is not a pairing link: it ends with #token= and a long key.</p>}
    </>
  )
}
