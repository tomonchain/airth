import { usePoll } from "@/hooks/use-poll"
import { api } from "@/lib/api"
import { CHANGELOG, type Entry } from "@/lib/changelog"
import { cn } from "@/lib/utils"

const KINDS: { key: "added" | "improved" | "fixed"; label: string; dot: string }[] = [
  { key: "added", label: "New", dot: "bg-sky-500" },
  { key: "improved", label: "Improved", dot: "bg-violet-500" },
  { key: "fixed", label: "Fixed", dot: "bg-emerald-500" },
]

const day = (iso: string) => new Date(iso + "T12:00").toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" })

/** What changed in each version, newest first; the one running is marked. */
export function WhatsNew() {
  const { data } = usePoll<{ version: string }>(() => api("info"), 600000, [])
  // A build from a checkout says its commit too: "0.8.1 (abc1234)".
  const current = data?.version.split(" ")[0]
  return (
    <>
      <header className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">What's new</h2>
        <p className="mt-1 text-sm text-muted-foreground">What changed in each version of Airth.{current && <> This one is {current}.</>}</p>
      </header>
      <ol className="relative">
        {CHANGELOG.map((e, i) => <Version key={e.version} entry={e} current={e.version === current} last={i === CHANGELOG.length - 1} />)}
      </ol>
    </>
  )
}

function Version({ entry, current, last }: { entry: Entry; current: boolean; last: boolean }) {
  return (
    <li className="relative grid grid-cols-[1.25rem_minmax(0,1fr)] gap-x-3 pb-8 md:grid-cols-[6.5rem_1.25rem_minmax(0,1fr)]">
      {/* On a computer the version and its date sit left of the line. */}
      <div className="hidden pt-0.5 text-right md:block">
        <div className="text-sm font-semibold tabular-nums">{entry.version}</div>
        <div className="text-xs text-muted-foreground">{day(entry.date)}</div>
      </div>
      <div className="relative flex justify-center">
        {!last && <span className="absolute top-4 -bottom-8 w-px bg-border" />}
        <span className={cn("relative mt-1.5 size-2.5 rounded-full ring-4 ring-background", current ? "bg-sky-500" : "bg-muted-foreground/40")} />
      </div>
      <div className="min-w-0">
        <div className="mb-1 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 md:hidden">
          <span className="text-sm font-semibold tabular-nums">{entry.version}</span>
          <span className="text-xs text-muted-foreground">{day(entry.date)}</span>
        </div>
        <h3 className="flex flex-wrap items-center gap-2 text-[15px] font-semibold tracking-tight">
          {entry.title}
          {current && <span className="rounded-full bg-sky-500/15 px-2 py-0.5 text-[11px] font-medium text-sky-600 dark:text-sky-400">This version</span>}
        </h3>
        <div className="mt-2.5 space-y-3">
          {KINDS.filter((k) => entry[k.key]?.length).map((k) => (
            <section key={k.key}>
              <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                <span className={cn("size-1.5 rounded-full", k.dot)} /> {k.label}
              </div>
              <ul className="space-y-1 text-sm leading-relaxed">
                {entry[k.key]!.map((line) => <li key={line} className="relative pl-3.5 before:absolute before:top-[0.6em] before:left-0 before:size-1 before:rounded-full before:bg-foreground/30">{line}</li>)}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </li>
  )
}
