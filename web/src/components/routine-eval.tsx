import { useEffect, useState } from "react"
import { ClipboardCheck, Loader, X } from "lucide-react"
import { Before, Results } from "@/components/evaluation"
import { Button } from "@/components/ui/button"
import { api, enc } from "@/lib/api"
import type { Estimate, Evaluation } from "@/lib/evaluation"
import { usd } from "@/lib/time"

/** A routine's AI blocks checked against the founder's past answers
    (decision 151), in its panel: what it would ask first, then, on the
    founder's word, the replay and how each block does. */
export function RoutineEval({ realm, name, onClose }: { realm: string; name: string; onClose: () => void }) {
  const [estimate, setEstimate] = useState<Estimate | null>(null)
  const [state, setState] = useState<{ running: boolean; result?: Evaluation; error?: string }>({ running: false })
  const path = `realms/${enc(realm)}/graphs/${enc(name)}/eval`

  useEffect(() => {
    const stop = new AbortController()
    api<Estimate>(path, undefined, undefined, undefined, stop.signal)
      .then(setEstimate)
      .catch((e) => { if (!stop.signal.aborted) setState({ running: false, error: (e as Error).message }) })
    return () => stop.abort()
  }, [path])

  async function run() {
    setState({ running: true })
    // Blocking, as a dry run: one model call per answer and block.
    try { setState({ running: false, result: await api<Evaluation>(path, {}, undefined, 600000) }) }
    catch (e) { setState({ running: false, error: (e as Error).message }) }
  }

  const result = state.result
  return (
    <section className="@container space-y-3 border-t pt-3" aria-label="Evaluation">
      <div className="flex items-center gap-2">
        <ClipboardCheck className="size-4 text-muted-foreground" />
        <h3 className="flex-1 text-sm font-semibold">Evaluation</h3>
        {result && result.cases > 0 && <span className="text-xs text-muted-foreground tabular-nums" title={result.cost.replace(/^cost: /, "")}>{usd(result.cost_usd) || "$0"} in all</span>}
        <Button size="icon-sm" variant="ghost" aria-label="Close the evaluation" disabled={state.running} onClick={onClose}><X /></Button>
      </div>
      {state.error && <p className="rounded-lg bg-red-500/10 px-3 py-2 text-xs break-words text-red-600 dark:text-red-300">{state.error}</p>}
      {state.running ? (
        <p className="flex items-center gap-2 rounded-lg bg-bubble px-3 py-3 text-sm text-muted-foreground" role="status">
          <Loader className="size-4 animate-spin" /> Replaying {estimate ? `${estimate.cases} answer${estimate.cases === 1 ? "" : "s"}` : "your answers"}…
        </p>
      ) : result ? (
        <Results e={result} onAgain={run} />
      ) : estimate ? (
        <Before estimate={estimate} onRun={run} />
      ) : !state.error && (
        <p className="flex items-center gap-2 text-xs text-muted-foreground"><Loader className="size-3.5 animate-spin" /> Reading your past answers…</p>
      )}
    </section>
  )
}
