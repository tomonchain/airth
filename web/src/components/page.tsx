import type { ReactNode } from "react"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"

/** Every page's frame: one column, the same header, the same sections. */
export function Page({ eyebrow, title, lede, actions, children, wide }: {
  eyebrow?: string; title: string; lede?: ReactNode; actions?: ReactNode; children: ReactNode; wide?: boolean
}) {
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className={cn("mx-auto px-8 pt-9 pb-16", wide ? "max-w-5xl" : "max-w-3xl")}>
        <header className="flex items-end gap-4">
          <div className="min-w-0 flex-1">
            {eyebrow && <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{eyebrow}</p>}
            <h1 className="mt-1 text-2xl font-semibold tracking-tight">{title}</h1>
            {lede && <p className="mt-1.5 text-base text-foreground/80">{lede}</p>}
          </div>
          {actions}
        </header>
        <div className="mt-7 space-y-9">{children}</div>
      </div>
    </div>
  )
}

export function Section({ title, note, children }: { title: string; note?: ReactNode; children: ReactNode }) {
  return (
    <section>
      <div className="mb-2.5 flex items-baseline justify-between gap-3">
        <h2 className="text-sm font-semibold">{title}</h2>
        {note && <span className="text-xs text-muted-foreground">{note}</span>}
      </div>
      {children}
    </section>
  )
}

export function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("rounded-xl border bg-bubble/40", className)}>{children}</div>
}

export function Figure({ label, value, note, tone }: { label: string; value: ReactNode; note?: ReactNode; tone?: "bad" | "warn" | "ok" }) {
  return (
    <div className="rounded-xl border bg-bubble/40 px-3.5 py-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cn("mt-1 text-xl font-semibold tracking-tight tabular-nums",
        tone === "bad" && "text-red-500", tone === "warn" && "text-amber-500", tone === "ok" && "text-emerald-500")}>{value}</div>
      {note && <div className="mt-0.5 text-xs text-muted-foreground">{note}</div>}
    </div>
  )
}

export function Empty({ icon: Icon, title, children }: { icon: React.ComponentType<{ className?: string }>; title: string; children?: ReactNode }) {
  return (
    <Panel className="flex flex-col items-center px-6 py-10 text-center">
      <Icon className="size-7 text-muted-foreground/60" />
      <p className="mt-3 text-sm font-medium">{title}</p>
      {children && <p className="mt-1 max-w-md text-sm text-muted-foreground">{children}</p>}
    </Panel>
  )
}

export function Loading() {
  return <div className="space-y-2">{[0, 1, 2].map((i) => <Skeleton key={i} className="h-14 rounded-xl" />)}</div>
}
