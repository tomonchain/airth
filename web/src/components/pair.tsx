import { useState } from "react"
import { Smartphone } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { pair } from "@/lib/api"

/** A phone without Airth's key: the home-screen app on its first opening,
    whose storage is apart from the browser's. */
export function Pair() {
  const [link, setLink] = useState("")
  const [wrong, setWrong] = useState(false)
  function go() {
    if (pair(link.trim())) location.replace("/")
    else setWrong(true)
  }
  return (
    <div className="grid h-svh place-items-center bg-background px-6 pt-[env(safe-area-inset-top)]">
      <div className="w-full max-w-sm text-center">
        <Smartphone className="mx-auto size-9 text-muted-foreground" strokeWidth={1.6} />
        <h1 className="mt-4 text-xl font-semibold">Pair this phone</h1>
        <p className="mt-2 text-sm text-muted-foreground">
          Paste the pairing link. In the browser where Airth is already open on this phone, it is under More, Settings, Phone, Copy the pairing link. Or scan the code on your Mac again.
        </p>
        <div className="mt-5 flex gap-2">
          <Input value={link} onChange={(e) => { setLink(e.target.value); setWrong(false) }} onKeyDown={(e) => e.key === "Enter" && go()}
            placeholder="https://…ts.net/#token=…" spellCheck={false} autoCapitalize="off" autoCorrect="off" aria-label="Pairing link" />
          <Button disabled={!link.trim()} onClick={go}>Pair</Button>
        </div>
        {wrong && <p className="mt-2 text-sm text-red-500">This is not a pairing link: it ends with #token= and a long key.</p>}
      </div>
    </div>
  )
}
