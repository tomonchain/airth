import { useState } from "react"
import { ChevronRight } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { isOpen, remember, remembered, type Outcome } from "@/lib/fold"
import { cn } from "@/lib/utils"

// The browser's storage, when the page may use it.
export function storage(): Storage | null {
  try { return localStorage } catch { return null }
}

/** Whether an exchange is open: the founder's choice when they made one
    (kept in the browser), else `byDefault`; and the click that turns it. */
export function useFold(key: string, byDefault: boolean): [boolean, () => void] {
  const [mine, setMine] = useState<boolean | null>(() => remembered(storage(), key))
  const open = isOpen(byDefault, mine)
  return [open, () => { remember(storage(), key, !open); setMine(!open) }]
}

/** A finished exchange, folded: what was asked, what came of it, when. A
    click opens it. */
export function FoldedLine({ request, result, outcome, when, onOpen, inset = true }: {
  request: string; result: string; outcome: Outcome; when: string; onOpen: () => void
  /** Under the agent's bubbles, past its picture's column. */
  inset?: boolean
}) {
  return (
    <button onClick={onOpen} aria-expanded={false}
      className={cn("mb-2 flex w-full min-w-0 items-center gap-2 rounded-xl px-2 py-1.5 text-left text-[13px] transition-colors hover:bg-accent", inset && "md:pl-10")}>
      <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="max-w-[45%] shrink-0 truncate font-medium text-foreground/90">{request}</span>
      <Result result={result} outcome={outcome} />
      <span className="ml-auto shrink-0 pl-2 text-[11px] text-muted-foreground tabular-nums">{when}</span>
    </button>
  )
}

/** Above an open exchange that is over: the line that folds it again. */
export function OpenLine({ result, outcome, when, onFold, inset = true }: { result: string; outcome: Outcome; when: string; onFold: () => void; inset?: boolean }) {
  return (
    <button onClick={onFold} aria-expanded
      className={cn("mb-2 flex w-full min-w-0 items-center gap-2 rounded-xl px-2 py-1 text-left text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground", inset && "md:pl-10")}>
      <ChevronRight className="size-3.5 shrink-0 rotate-90" />
      <Result result={result} outcome={outcome} />
      <span className="ml-auto shrink-0 pl-2 text-[11px] tabular-nums">{when}</span>
    </button>
  )
}

function Result({ result, outcome }: { result: string; outcome: Outcome }) {
  if (outcome === "failed") return <Badge variant="destructive">{result}</Badge>
  if (outcome === "stopped") return <Badge variant="secondary">{result}</Badge>
  return <span className="min-w-0 truncate text-muted-foreground">{result}</span>
}
