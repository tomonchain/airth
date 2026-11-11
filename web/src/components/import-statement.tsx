import { useRef, useState } from "react"
import { FileUp } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { enc, raw } from "@/lib/api"

type Imported = { added: number; known: number; not_completed: number; from: string; to: string }

/** A Revolut statement, as exported in CSV, read into the business's
    figures. */
export function ImportStatement({ realm, onDone, small }: { realm: string; onDone: () => void; small?: boolean }) {
  const input = useRef<HTMLInputElement>(null)
  const [busy, setBusy] = useState(false)
  async function send(file: File) {
    setBusy(true)
    try {
      const r: Imported = await (await raw(`realms/${enc(realm)}/finance/statement`, { method: "POST", body: file })).json()
      const added = r.added === 1 ? "1 new transaction" : `${r.added} new transactions`
      toast.success(`Statement read: ${added}`, {
        description: `${r.from} to ${r.to}${r.known ? ` · ${r.known} already in` : ""}${r.not_completed ? ` · ${r.not_completed} not completed, left out` : ""}`,
      })
      onDone()
    } catch (e) {
      toast.error("Statement not read", { description: (e as Error).message })
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <input ref={input} type="file" accept=".csv,text/csv" hidden
        onChange={(e) => { const f = e.target.files?.[0]; if (f) send(f); e.target.value = "" }} />
      {small ? (
        <button className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground disabled:opacity-50" disabled={busy} onClick={() => input.current?.click()}>
          <FileUp className="size-3.5" /> Import statement
        </button>
      ) : (
        <Button variant="outline" size="sm" disabled={busy} onClick={() => input.current?.click()}>
          <FileUp /> {busy ? "Reading…" : "Import a Revolut statement (CSV)"}
        </Button>
      )}
    </>
  )
}
