import { Check, CircleDashed, Clock, X } from "lucide-react"
import type { Run } from "@/lib/api"
import { took, usd } from "@/lib/time"
import { cn } from "@/lib/utils"

const MARK = {
  ok: { icon: Check, cls: "bg-emerald-500/15 text-emerald-500" },
  waiting: { icon: Clock, cls: "bg-amber-500/15 text-amber-500" },
  failed: { icon: X, cls: "bg-red-500/15 text-red-500" },
  other: { icon: CircleDashed, cls: "bg-muted text-muted-foreground" },
}

/** A routine's run, block by block: what came in, what went out, how long,
    what it cost, and where it stopped. */
export function RunTimeline({ run }: { run: Run }) {
  const cost = run.blocks.reduce((s, b) => s + (b.cost_usd ?? 0), 0)
  return (
    <div>
      <div className="mb-3 flex items-baseline justify-between text-xs text-muted-foreground">
        <span className="font-mono">{run.routine}</span>
        <span>{usd(cost)}</span>
      </div>
      <ol className="relative">
        {run.blocks.map((b, i) => {
          const m = MARK[b.outcome as keyof typeof MARK] ?? MARK.other
          const last = i === run.blocks.length - 1
          return (
            <li key={b.block + i} className="relative flex gap-3 pb-4 last:pb-0">
              {!last && <span className="absolute top-7 bottom-0 left-[13px] w-px bg-border" />}
              <span className={cn("z-10 grid size-7 shrink-0 place-items-center rounded-full", m.cls)}>
                <m.icon className="size-3.5" strokeWidth={2.5} />
              </span>
              <div className="min-w-0 flex-1 pt-0.5">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="text-sm font-medium capitalize">{b.block}</span>
                  <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
                    {[took(b.started_at, b.finished_at), usd(b.cost_usd)].filter(Boolean).join(" · ")}
                  </span>
                </div>
                {b.items_in != null && (
                  <p className="text-xs text-muted-foreground">
                    {b.items_in} in → {b.items_out ?? 0} out{b.outcome === "waiting" ? ", waiting for you" : ""}
                  </p>
                )}
                {b.error && (
                  <p className="mt-1.5 line-clamp-3 rounded-md bg-red-500/10 px-2 py-1.5 font-mono text-[11px] leading-relaxed text-red-600 dark:text-red-300">
                    {b.error.split("\n").find((l) => l.includes("detail")) ?? b.error.split("\n")[0]}
                  </p>
                )}
              </div>
            </li>
          )
        })}
      </ol>
    </div>
  )
}
