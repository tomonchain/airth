import { ChevronRight, RotateCcw, Sparkles } from "lucide-react"
import { Button } from "@/components/ui/button"
import { DAYS, DID, MAX, agreement, estimateText, skips, split, type BlockEval, type Case, type Estimate, type Evaluation } from "@/lib/evaluation"
import { ago, usd } from "@/lib/time"
import { cn } from "@/lib/utils"

// What an evaluation shows (decision 151), apart from how it is fetched:
// what it would ask before the run, then how each block does.

const WHAT = `Replays its AI blocks on what you decided in Needs you, the last ${DAYS} days, ${MAX} answers at most, with its prompts and models as they are now. Nothing is sent and nothing reaches Needs you.`

function NoAnswers() {
  return (
    <div className="rounded-xl bg-bubble px-4 py-6 text-center">
      <p className="text-sm font-medium">No past answers yet</p>
      <p className="mt-1 text-xs text-muted-foreground">An evaluation replays what you approved, edited or rejected from this routine in Needs you. Nothing of it reached you in the last {DAYS} days.</p>
    </div>
  )
}

/** Before the run: what it does, and what it may cost. */
export function Before({ estimate, onRun }: { estimate: Estimate; onRun: () => void }) {
  if (!estimate.cases) return <NoAnswers />
  if (!estimate.blocks) {
    return <p className="text-xs text-muted-foreground">Nothing to measure: none of its AI blocks keeps items or drafts them one at a time before your approval.</p>
  }
  return (
    <div className="space-y-2.5">
      <p className="text-xs leading-relaxed text-muted-foreground">{WHAT}</p>
      <p className="text-xs">{estimateText(estimate)}</p>
      <Button size="sm" onClick={onRun}><Sparkles /> Replay {estimate.cases} answer{estimate.cases === 1 ? "" : "s"}</Button>
    </div>
  )
}

/** The replay: each block measured, then what was not. */
export function Results({ e, onAgain }: { e: Evaluation; onAgain?: () => void }) {
  if (!e.cases) return <NoAnswers />
  const skipped = skips(e)
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <p className="flex-1 text-xs text-muted-foreground">{e.cases} past answer{e.cases === 1 ? "" : "s"} replayed, nothing sent.</p>
        {onAgain && <Button size="xs" variant="ghost" className="text-muted-foreground" onClick={onAgain}><RotateCcw /> Again</Button>}
      </div>
      {e.blocks.map((b) => <Block key={b.id} b={b} />)}
      {!e.blocks.length && <p className="text-xs text-muted-foreground">No block could be measured.</p>}
      {skipped.length > 0 && (
        <Fold label={`Not measured: ${skipped.length}`}>
          <ul className="space-y-1.5">
            {skipped.map((s, i) => (
              <li key={i} className="text-xs leading-relaxed">
                {s.block && <span className="font-mono text-[11.5px]">{s.block}</span>}
                {s.title && <span className="text-muted-foreground"> · {s.title}</span>}
                <span className="block text-muted-foreground">{s.why}</span>
              </li>
            ))}
          </ul>
        </Fold>
      )}
    </div>
  )
}

/** A section closed until asked for. */
function Fold({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <details className="group">
      <summary className="flex cursor-pointer list-none items-center gap-1 py-1 text-xs text-muted-foreground hover:text-foreground [&::-webkit-details-marker]:hidden">
        <ChevronRight className="size-3.5 transition-transform group-open:rotate-90" /> {label}
      </summary>
      <div className="mt-1.5">{children}</div>
    </details>
  )
}

function Block({ b }: { b: BlockEval }) {
  const prompt = b.prompt.replace(/^prompts\//, "")
  return (
    <div className="space-y-2.5 rounded-xl border p-3">
      <div className="flex items-baseline gap-2">
        <span className="inline-flex min-w-0 items-center gap-1 font-mono text-[13px]"><Sparkles className="size-3 shrink-0 self-center text-muted-foreground" /><span className="truncate">{b.id}</span></span>
        <span className="min-w-0 flex-1 truncate text-[11.5px] text-muted-foreground">{prompt}</span>
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{usd(b.cost_usd) || "$0"}</span>
      </div>
      {b.kind === "keeps" ? <Keeps b={b} /> : <Drafts b={b} />}
    </div>
  )
}

/** A block that keeps or ranks: how often it does as the founder did, and
    where it does not, first. */
function Keeps({ b }: { b: BlockEval }) {
  const a = agreement(b.score)
  const s = b.score
  if (!a || !s) return <p className="text-xs text-muted-foreground">No case could be scored.</p>
  const { differ, match } = split(b.cases)
  return (
    <>
      <div>
        <p className="text-sm font-medium tabular-nums">{a.now}</p>
        <p className="text-xs text-muted-foreground tabular-nums">
          Keeps {s.wanted_kept} of the {s.wanted} you wanted, drops {s.rejected_dropped} of the {s.rejected} you rejected. Before: {a.before}, as it let everything through.
        </p>
      </div>
      {differ.length ? (
        <div className="space-y-1.5">
          <p className="text-xs font-medium">Where it differs from you</p>
          <ul className="space-y-1.5">{differ.map((c) => <KeptRow key={c.item} c={c} differs />)}</ul>
        </div>
      ) : <p className="text-xs text-muted-foreground">It matches every decision you made.</p>}
      {differ.length > 0 && match.length > 0 && (
        <Fold label={`Where it matches you: ${match.length}`}>
          <ul className="space-y-1.5">{match.map((c) => <KeptRow key={c.item} c={c} />)}</ul>
        </Fold>
      )}
    </>
  )
}

function KeptRow({ c, differs }: { c: Case; differs?: boolean }) {
  return (
    <li className={cn("rounded-lg px-3 py-2 text-xs leading-relaxed", differs ? "bg-amber-500/10" : "bg-bubble")}>
      <p className="font-medium text-foreground">{c.title}</p>
      <p className="mt-0.5 text-muted-foreground">
        {DID[c.founder]}{c.founder === "reject" && c.said.trim() ? `: “${c.said.trim()}”` : ""} · {ago(c.at)}
      </p>
      <p className={cn("mt-0.5", differs ? "text-amber-700 dark:text-amber-300" : "text-muted-foreground")}>
        {c.kept ? "Now it keeps it" : "Now it drops it"}{c.kept && c.score !== undefined ? `, scored ${c.score}` : ""}.
      </p>
    </li>
  )
}

/** A block that drafts: each new draft beside what the founder approved,
    for the founder to read; a draft is not scored. */
function Drafts({ b }: { b: BlockEval }) {
  if (!b.cases.length) return <p className="text-xs text-muted-foreground">No new draft to read.</p>
  return (
    <>
      <p className="text-xs text-muted-foreground">{b.cases.length} new draft{b.cases.length === 1 ? "" : "s"} to read beside yours. Drafts are not scored: you judge them.</p>
      <ul className="space-y-3">
        {b.cases.map((c) => (
          <li key={c.item} className="space-y-1.5">
            <p className="text-xs"><span className="font-medium">{c.title}</span> <span className="text-muted-foreground">· {DID[c.founder]} · {ago(c.at)}</span></p>
            {/* Side by side where the panel is wide enough, stacked on a phone. */}
            <div className="grid gap-2 @lg:grid-cols-2">
              <Yours c={c} />
              <div className="rounded-lg border border-sky-500/30 px-3 py-2">
                <p className="text-[11px] font-medium text-sky-600 dark:text-sky-400">New draft</p>
                <Text>{c.new || "Empty: it wrote nothing."}</Text>
              </div>
            </div>
          </li>
        ))}
      </ul>
    </>
  )
}

/** What the founder made of the draft they read: their edit, the draft as
    approved, or what they said of it. */
function Yours({ c }: { c: Case }) {
  const label = c.founder === "edit" ? "Your edit" : c.founder === "approve" ? "What you approved" : c.founder === "reject" ? "What you rejected" : "What you asked to redo"
  return (
    <div className="rounded-lg bg-bubble px-3 py-2">
      <p className="text-[11px] font-medium text-muted-foreground">{label}</p>
      {c.founder === "edit" ? (
        <>
          <Text>{c.edited}</Text>
          <p className="mt-1.5 text-[11px] text-muted-foreground">Its draft, before your edit</p>
          <Text className="text-muted-foreground line-through decoration-muted-foreground/40">{c.draft}</Text>
        </>
      ) : (
        <>
          {(c.founder === "reject" || c.founder === "redo") && c.said.trim() && <p className="mt-1 text-[13px]">“{c.said.trim()}”</p>}
          <Text className={c.founder === "approve" ? undefined : "text-muted-foreground"}>{c.draft}</Text>
        </>
      )}
    </div>
  )
}

function Text({ children, className }: { children: React.ReactNode; className?: string }) {
  return <p className={cn("mt-1 max-h-64 overflow-y-auto text-[13px] leading-relaxed break-words whitespace-pre-wrap", className)}>{children}</p>
}
