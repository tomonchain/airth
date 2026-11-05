import { useEffect, useState } from "react"
import { Check, ChevronRight, CircleAlert, Code, ExternalLink, GitMerge, GitPullRequest, Loader, RotateCw, Trash2, Wrench, X } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { api, enc } from "@/lib/api"
import { cn } from "@/lib/utils"

type CheckResult = { command: string; ok: boolean; output: string; duration_ms: number }
export type Change = {
  branch: string; base_branch: string | null; base: string; head: string; status: "open" | "adopted" | "discarded"; files: string[]
  checks: { commit: string; started_at: string; finished: boolean; results: CheckResult[] } | null
  conflicts?: string[]; merging?: string | null; pull_request?: { number: number; url: string; pushed: string } | null
}
export type Review = { live: boolean; checks_required: boolean; delivery: "adopt" | "pull_request"; config: { files: string[]; agents: string[] }; change: Change }
type Pr = { state: "open" | "merged" | "closed"; title: string; url: string; head: string; merge_state: string; checks: "none" | "pending" | "passing" | "failing"; failed: string[] }
type Other = { id: string; title?: string | null; objective?: string; change: string | null; touches?: string[] }

const MERGE: Record<string, string> = {
  clean: "Ready to merge", unstable: "Ready to merge", has_hooks: "Ready to merge", dirty: "Conflicts",
  blocked: "Waiting for its rules", draft: "Draft", unknown: "GitHub is checking it",
}
const STATUS: Record<string, string> = { A: "Added", M: "Changed", D: "Deleted", R: "Renamed" }

/** A task's change to a repository (decisions 11 and 44): its files, its
    checks, where it stands against its base, and the one action that moves
    it on — adopt it into the checkout, or open and merge its pull request. */
export function ChangePanel({ realm, id, title, r, onChange, onAsk }: {
  realm: string; id: string; title: string; r: Review; onChange: () => void; onAsk: (text: string) => Promise<void>
}) {
  const c = r.change
  const path = `realms/${enc(realm)}/runs/${enc(id)}`
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [sure, setSure] = useState(false)
  const [diff, setDiff] = useState<string | null>(null)
  const [others, setOthers] = useState<Other[]>([])
  const [pr, setPr] = useState<{ status: Pr | null; error?: string } | null>(null)

  const changed = c.base !== c.head
  const checks = c.checks
  const current = !!checks && checks.commit === c.head
  const passed = current && checks.finished && checks.results.every((x) => x.ok)
  const failed = checks?.results.find((x) => !x.ok)
  const failedNow = current && checks.finished && !passed
  const open = c.status === "open"
  const conflicts = c.conflicts ?? []
  const resolving = !!c.merging
  const idle = open && changed && !r.live && !resolving
  const base = c.base_branch ?? "main"
  const viaPr = r.delivery === "pull_request"
  const files = new Set(c.files.map((f) => f.split("\t").at(-1)!))

  // Other open tasks that touch the same files: one should go first.
  useEffect(() => {
    if (!open || r.live) return
    let live = true
    api<Other[]>(`realms/${enc(realm)}/runs`).then((all) => live && setOthers(all.filter((o) => o.id !== id && o.change === "open" && (o.touches ?? []).some((f) => files.has(f)))), () => {})
    return () => { live = false }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [realm, id, open, r.live, c.head])

  // The pull request on GitHub, looked at every 30 seconds while shown.
  const number = c.pull_request?.number
  useEffect(() => {
    if (!number || !open) return
    let live = true
    const look = () => api<{ status: Pr }>(`${path}/pull-request`).then((x) => live && setPr({ status: x.status }), (e: Error) => live && setPr({ status: null, error: e.message }))
    look()
    const t = setInterval(() => document.visibilityState === "visible" && look(), 30000)
    return () => { live = false; clearInterval(t) }
  }, [path, number, open])

  async function act(action: string) {
    setBusy(action)
    setError(null)
    try {
      const out = await api<{ checking?: boolean; adopted?: boolean; resolving?: boolean; merged?: boolean; conflicts?: string[]; pull_request?: { number: number } }>(`${path}/${action}`, {}, undefined, 120000)
      if (out.checking) toast.success(`Checking the change with the latest ${base}, then adopting it.`)
      if (out.adopted) toast.success("Adopted into your checkout.")
      if (out.resolving) toast.success("The agent is resolving the conflict.")
      if (out.merged) toast.success("Merged on GitHub.")
      if (out.pull_request) toast.success(`Pull request #${out.pull_request.number} opened.`)
      if (action === "discard") toast.success("Change discarded.")
      if (action === "merge") setPr(null)
      onChange()
    } catch (e) { setError((e as Error).message) }
    finally { setBusy(null); setSure(false) }
  }
  async function showDiff() {
    try { setDiff((await api<{ diff: string }>(`${path}/diff`)).diff || "(empty diff)") } catch (e) { toast.error((e as Error).message) }
  }
  async function fix(f: CheckResult) {
    const tail = f.output.split("\n").slice(-60).join("\n")
    setBusy("fix")
    try { await onAsk(`The check \`${f.command}\` failed:\n\n\`\`\`\n${tail}\n\`\`\`\n\nFix the problem, then summarize what you changed.`) }
    catch (e) { setError((e as Error).message) }
    finally { setBusy(null) }
  }

  const g = pr?.status
  const pushed = !!c.pull_request && c.pull_request.pushed === c.head
  const ready = !!g && g.state === "open" && g.checks === "passing" && pushed && g.head === c.head && !["dirty", "behind"].includes(g.merge_state)

  return (
    <div className="mb-5 ml-10 max-w-[85%] rounded-2xl border bg-bubble/60 fade-up">
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-semibold">Change</span>
            <span className={cn("rounded-md px-1.5 py-0.5 text-[11px] capitalize",
              c.status === "adopted" ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400" : c.status === "discarded" ? "bg-muted text-muted-foreground" : "bg-sky-500/10 text-sky-600 dark:text-sky-400")}>{c.status}</span>
          </div>
          <div className="truncate font-mono text-[11.5px] text-muted-foreground" title={`${c.branch}, from ${c.base}`}>{base} ← {c.branch.replace(/^airth\//, "")}</div>
        </div>
        {changed && <Button size="icon-sm" variant="ghost" title="View the diff" aria-label="View the diff" onClick={showDiff}><Code /></Button>}
        {idle && <Button size="icon-sm" variant="ghost" title={checks ? "Run the checks again" : "Run the checks"} aria-label="Run the checks" disabled={!!busy} onClick={() => act("checks")}><RotateCw /></Button>}
        {idle && failed && <Button size="icon-sm" variant="ghost" title="Ask the agent to fix the failed check" aria-label="Ask to fix" disabled={!!busy} onClick={() => fix(failed)}><Wrench /></Button>}
        {idle && (sure
          ? <Button size="sm" variant="destructive" disabled={!!busy} onClick={() => act("discard")}>{c.pull_request ? "Close & discard?" : "Discard?"}</Button>
          : <Button size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-red-500" aria-label="Discard"
              title={c.pull_request ? "Discard: close the pull request and throw the change away" : "Discard this change"} onClick={() => { setSure(true); setTimeout(() => setSure(false), 3000) }}><Trash2 /></Button>)}
      </div>

      <div className="space-y-3 px-4 py-3">
        {open && resolving && (
          <Note icon={GitMerge} title={r.live ? `Resolving the conflict with ${base}…` : `The conflict with ${base} is not resolved yet`}>{conflicts.join(", ")}</Note>
        )}
        {open && !resolving && conflicts.length > 0 && (
          <Note icon={GitMerge} title={`Conflicts with ${base}`} action={!r.live && <Button size="sm" disabled={!!busy} onClick={() => act("resolve")}>Resolve with the agent</Button>}>
            {base} changed in the same places: {conflicts.join(", ")}
          </Note>
        )}
        {changed && r.config.files.length > 0 && (
          <Note icon={CircleAlert} title="This change edits the business's config">
            {r.config.agents.length > 0 && <div>Agents: {r.config.agents.join(", ")}</div>}
            <div>{r.config.files.join(", ")}</div>
          </Note>
        )}
        {others.length > 0 && (
          <p className="flex gap-1.5 text-xs text-muted-foreground">
            <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
            <span>Also changed by {others.map((o) => o.title ?? o.objective ?? o.id).join(", ")}: {[...new Set(others.flatMap((o) => (o.touches ?? []).filter((f) => files.has(f))))].join(", ")}.{viaPr ? "" : " Adopt one first; the other is then updated with it."}</span>
          </p>
        )}
        {error && <p className="rounded-lg bg-red-500/10 px-3 py-2 text-xs break-words text-red-600 dark:text-red-300">{error}</p>}

        {changed ? (
          <div className="space-y-0.5">
            {c.files.map((f) => {
              const [st, ...rest] = f.split("\t")
              const k = st[0]
              return (
                <div key={f} className="flex items-center gap-2 font-mono text-[12px]" title={`${STATUS[k] ?? k}: ${rest.join(" → ")}`}>
                  <span className={cn("w-3 text-center font-semibold", k === "A" ? "text-emerald-500" : k === "D" ? "text-red-500" : "text-amber-500")}>{k}</span>
                  <span className="truncate">{rest.join(" → ")}</span>
                </div>
              )
            })}
          </div>
        ) : <p className="text-xs text-muted-foreground">{r.live ? "Changes appear here when the agent finishes." : "No files changed."}</p>}

        {checks ? (
          <div className="space-y-1">
            <div className="text-xs font-medium">Checks{!current && <span className="font-normal text-muted-foreground"> · on an older commit</span>}</div>
            {checks.results.map((x, i) => <CheckRow key={`${checks.started_at}${i}`} x={x} />)}
            {!checks.finished && (r.live
              ? <p className="flex items-center gap-1.5 text-xs text-muted-foreground"><Loader className="size-3.5 animate-spin" /> Running the checks…</p>
              : <p className="text-xs text-muted-foreground">The checks did not finish.</p>)}
          </div>
        ) : open && changed && r.checks_required && !r.live ? <p className="text-xs text-muted-foreground">Checks have not run on this change.</p> : null}

        {open && !resolving && !conflicts.length && c.pull_request && (
          <div className="rounded-xl border bg-surface p-3">
            <div className="flex items-center gap-2">
              <GitPullRequest className="size-4 shrink-0 text-muted-foreground" />
              <a href={c.pull_request.url} target="_blank" rel="noreferrer" className="min-w-0 flex-1 truncate text-sm hover:underline">
                <span className="text-muted-foreground">#{c.pull_request.number}</span> {g?.title ?? "Pull request"}
              </a>
              <a href={c.pull_request.url} target="_blank" rel="noreferrer" aria-label="Open on GitHub" title="Open on GitHub" className="text-muted-foreground hover:text-foreground"><ExternalLink className="size-3.5" /></a>
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-3 text-xs">
              {!pushed ? <span className="text-muted-foreground">New commits are pushed once the checks pass</span> : g ? (
                <>
                  {g.checks === "passing" ? <span className="inline-flex items-center gap-1 text-emerald-500"><Check className="size-3.5" /> CI passed</span>
                    : g.checks === "failing" ? <span className="inline-flex items-center gap-1 text-red-500" title={g.failed.join(", ")}><X className="size-3.5" /> CI failed</span>
                    : g.checks === "pending" ? <span className="inline-flex items-center gap-1 text-muted-foreground"><Loader className="size-3.5 animate-spin" /> CI running</span>
                    : <span className="text-muted-foreground">No CI yet</span>}
                  {g.state === "open"
                    ? <span className="text-muted-foreground">{g.merge_state === "behind" ? `Behind ${base}: the agent updates it` : MERGE[g.merge_state] ?? g.merge_state}</span>
                    : <span className="text-muted-foreground capitalize">{g.state}</span>}
                </>
              ) : pr?.error ? <span className="text-red-500" title={pr.error}>GitHub unreachable</span> : <span className="text-muted-foreground">Looking at GitHub…</span>}
            </div>
            {idle && (
              <Button className="mt-3 w-full" size="sm" disabled={!ready || !!busy} onClick={() => act("merge")}
                title={ready ? "Squash-merge it on GitHub, titled like the pull request" : "Once GitHub's checks pass on the latest commit"}>
                {busy === "merge" ? <Loader className="animate-spin" /> : <GitMerge />} Squash and merge
              </Button>
            )}
          </div>
        )}

        {idle && !conflicts.length && viaPr && !c.pull_request && (
          <Button className="w-full" size="sm" disabled={!!busy} onClick={() => act("pull-request")}
            title={passed || !r.checks_required ? "Push the branch and open its pull request" : "The checks did not pass: open it anyway, as work in progress"}>
            {busy === "pull-request" ? <Loader className="animate-spin" /> : <GitPullRequest />} Open pull request
          </Button>
        )}
        {idle && !conflicts.length && !viaPr && (
          <Button className="w-full" size="sm" disabled={failedNow || !!busy} onClick={() => act("adopt")}
            title={failedNow ? "The checks must pass first" : `Brings in what changed on ${base}, checks again if needed, then merges into your checkout`}>
            {busy === "adopt" ? <Loader className="animate-spin" /> : <Check />} Adopt into checkout
          </Button>
        )}
      </div>

      <Sheet open={diff !== null} onOpenChange={(o) => !o && setDiff(null)}>
        <SheetContent className="w-[min(64rem,92vw)] gap-0 p-0 data-[side=right]:sm:max-w-none">
          <SheetHeader className="border-b">
            <SheetTitle>{title}</SheetTitle>
            <SheetDescription>{((n) => `${n} file${n === 1 ? "" : "s"} changed`)((diff?.match(/^diff --git /gm) ?? []).length)}</SheetDescription>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-auto">{diff !== null && <Diff text={diff} />}</div>
        </SheetContent>
      </Sheet>
    </div>
  )
}

function Note({ icon: Icon, title, action, children }: { icon: React.ComponentType<{ className?: string }>; title: string; action?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex items-start gap-2 rounded-xl bg-amber-500/10 px-3 py-2.5 text-xs text-amber-700 dark:text-amber-300">
      <Icon className="mt-0.5 size-3.5 shrink-0" />
      <div className="min-w-0 flex-1"><div className="font-medium">{title}</div>{children && <div className="mt-0.5 break-words">{children}</div>}</div>
      {action}
    </div>
  )
}

function CheckRow({ x }: { x: CheckResult }) {
  const [open, setOpen] = useState(false)
  return (
    <div>
      <button type="button" onClick={() => setOpen(!open)} aria-expanded={open} className="flex w-full items-center gap-2 rounded-md py-0.5 text-left text-xs hover:bg-muted/50">
        <ChevronRight className={cn("size-3 text-muted-foreground transition-transform", open && "rotate-90")} />
        {x.ok ? <Check className="size-3.5 text-emerald-500" /> : <X className="size-3.5 text-red-500" />}
        <code className="min-w-0 flex-1 truncate" title={x.command}>{x.command}</code>
        <span className="text-muted-foreground tabular-nums">{(x.duration_ms / 1000).toFixed(1)}s</span>
      </button>
      {open && <pre className="mt-1 max-h-72 overflow-auto rounded-lg bg-muted/60 p-2.5 font-mono text-[11px] whitespace-pre-wrap">{x.output || "(no output)"}</pre>}
    </div>
  )
}

/** A unified diff, each line coloured by what it does. */
function Diff({ text }: { text: string }) {
  return (
    <pre className="p-4 font-mono text-[12px] leading-[1.55]">
      {text.split("\n").map((l, i) => (
        <div key={i} className={cn("px-2 whitespace-pre-wrap",
          l.startsWith("diff --git") ? "mt-4 border-t pt-3 font-semibold first:mt-0 first:border-0 first:pt-0"
            : l.startsWith("@@") ? "text-sky-600 dark:text-sky-400"
            : l.startsWith("+") && !l.startsWith("+++") ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
            : l.startsWith("-") && !l.startsWith("---") ? "bg-red-500/10 text-red-700 dark:text-red-300"
            : /^(index |\+\+\+|---|new file|deleted file|similarity|rename )/.test(l) ? "text-muted-foreground" : "")}>{l || " "}</div>
      ))}
    </pre>
  )
}
