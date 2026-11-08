import type { ReactNode } from "react"
import { ChevronLeft, Search } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"

/** Every page's frame: one column, the same header, the same sections. */
export function Page({ eyebrow, title, lede, actions, children, wide }: {
  eyebrow?: string; title: string; lede?: ReactNode; actions?: ReactNode; children: ReactNode; wide?: boolean
}) {
  return (
    <div className="min-w-0 flex-1 overflow-y-auto">
      <div className={cn("mx-auto px-4 pt-6 pb-16 md:px-8 md:pt-9", wide ? "max-w-5xl" : "max-w-3xl")}>
        <header className="flex items-end gap-4 max-md:flex-col max-md:items-start max-md:gap-3">
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
  return <div className={cn("rounded-xl bg-bubble", className)}>{children}</div>
}

export function Figure({ label, value, note, tone }: { label: string; value: ReactNode; note?: ReactNode; tone?: "bad" | "warn" | "ok" }) {
  return (
    <div className="rounded-xl bg-bubble px-3.5 py-3">
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

/** The head of a side column: the same title, height and spacing on every
    page, with a count or buttons on the right. */
export function SideHead({ title, note, children }: { title: string; note?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex h-12 shrink-0 items-center gap-0.5 pr-2 pl-4">
      <h1 className="flex-1 truncate text-[15px] font-semibold">{title}</h1>
      {note !== undefined && <span className="pr-2 text-xs text-muted-foreground tabular-nums">{note}</span>}
      {children}
    </div>
  )
}

/** A side column's search box, under its head. */
export function SideSearch({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <div className="px-3 pb-2">
      <label className="flex h-9 items-center gap-2 rounded-lg bg-muted px-3 text-sm text-muted-foreground focus-within:ring-2 focus-within:ring-ring/40">
        <Search className="size-4" />
        <input value={value} onChange={(e) => onChange(e.target.value)} placeholder="Search" className="flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" />
      </label>
    </div>
  )
}

/** A choice of a few, side by side: a period, a unit. */
export function Seg<T extends string | number>({ label, value, options, onPick }: { label: string; value: T; options: [T, string][]; onPick: (v: T) => void }) {
  return (
    <div role="radiogroup" aria-label={label} className="flex rounded-lg border p-0.5">
      {options.map(([v, l]) => (
        <button key={String(v)} role="radio" aria-checked={value === v} onClick={() => onPick(v)}
          className={cn("rounded-md px-2.5 py-1 text-xs transition-colors", value === v ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")}>{l}</button>
      ))}
    </div>
  )
}

/** On a phone, a list and what is open in it take the screen in turn:
    this goes back to the list. Hidden where both fit side by side. */
export function Back({ onClick, label = "Back" }: { onClick: () => void; label?: string }) {
  return <Button variant="ghost" size="icon-sm" className="-ml-1 shrink-0 md:hidden" aria-label={label} onClick={onClick}><ChevronLeft /></Button>
}
