import { useState } from "react"
import { Building2, Calendar, Check, Compass, ExternalLink, FileText, Flag, FolderGit2, Globe, Layers, Pencil, Target } from "lucide-react"
import { toast } from "sonner"
import { Empty, Loading, Page, Panel, Section } from "@/components/page"
import { Rich } from "@/components/rich"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Company as Data } from "@/lib/api"
import { cn } from "@/lib/utils"

const COUNTRIES: Record<string, string> = { GB: "United Kingdom", FR: "France", US: "United States", IE: "Ireland", DE: "Germany", ES: "Spain", IT: "Italy", NL: "Netherlands", BE: "Belgium", CH: "Switzerland", LU: "Luxembourg", PT: "Portugal" }
const FORMS: Record<string, string> = { ltd: "Private limited company (Ltd)", plc: "Public limited company (PLC)", llp: "LLP", sas: "SAS", sasu: "SASU", sarl: "SARL", eurl: "EURL", sa: "SA", ei: "Sole trader (EI)", llc: "LLC" }
// What each config field is, in the founder's words, for "To fill in".
const FIELDS: Record<string, string> = {
  name: "Name", legal_name: "Legal name", legal_form: "Legal form", country: "Country", status: "Status", registration_number: "Registration number",
  registered_office: "Registered office", incorporated_on: "Incorporation date", fiscal_year_end: "Fiscal year end", vat_number: "VAT number",
  tax_regime: "Tax regime", currency: "Currency", bank: "Bank", domain: "Domain", links: "Links",
  goal: "The goal", measure: "How it is measured", target: "The target", by: "The deadline", milestones: "Milestones", baseline_minutes: "Your time before Airth",
}

const day = (iso: string) => new Date(`${iso}T12:00:00`)
const longDate = (iso: string) => day(iso).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" })
const daysTo = (iso: string) => Math.round((day(iso).getTime() - Date.now()) / 86400000)
function until(iso: string) {
  const d = daysTo(iso)
  if (d < 0) return `${-d} day${d === -1 ? "" : "s"} ago`
  if (d < 60) return `in ${d} day${d === 1 ? "" : "s"}`
  const m = Math.round(d / 30.4)
  return m < 24 ? `in ${m} months` : `in ${Math.round(m / 12)} years`
}
const list = (v: unknown): string[] => (Array.isArray(v) ? v : typeof v === "string" && v ? [v] : [])

/** The company (decision 44): who it is legally, where it is going, how it
    gets there, what it is made of, and what is still to write down, all as
    the business folder says. */
export function Company({ realm, description, onFile, onRepos }: {
  realm: string; description: string | null; onFile: (path: string) => void; onRepos: () => void
}) {
  const { data } = usePoll<Data>(() => api(`realms/${enc(realm)}/company`), 60000, [realm])
  const p = (data?.profile ?? {}) as Record<string, unknown>
  const g = (data?.goal ?? {}) as Record<string, unknown>
  const text = (o: Record<string, unknown>, k: string) => (typeof o[k] === "string" ? (o[k] as string) : null)
  const status = text(p, "status")
  return (
    <Page wide eyebrow="Business" title={text(p, "legal_name") ?? text(p, "name") ?? realm} lede={description}
      actions={status && (
        <span className={cn("rounded-full px-2.5 py-1 text-xs font-medium capitalize", status === "registered" ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400" : "bg-amber-500/10 text-amber-600 dark:text-amber-400")}>
          {status === "registered" ? "Registered" : status.replace(/_/g, " ")}
        </span>
      )}>
      {!data ? <Loading /> : (
        <>
          <Identity p={p} text={text} onFile={onFile} />
          <Goal g={g} text={text} onFile={onFile} />
          <Strategy realm={realm} />
          <div className="grid gap-9 md:grid-cols-2">
            <Section title="Lines" note="business/lines/">
              {data.lines.length ? (
                <Panel className="divide-y">
                  {data.lines.map((l) => {
                    const f = (l as { name: string; fields?: Record<string, unknown> }).fields ?? {}
                    const st = text(f, "status")
                    return (
                      <button key={l.name} onClick={() => onFile(`business/lines/${l.name}.md`)} className="flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm hover:bg-accent/50">
                        <Layers className="size-4 text-muted-foreground" />
                        <span className="min-w-0 flex-1"><span className="font-medium">{text(f, "title") ?? l.name}</span>{text(f, "kind") && <span className="text-muted-foreground"> · {text(f, "kind")}</span>}</span>
                        {st && <span className={cn("text-xs capitalize", st === "active" ? "text-emerald-500" : "text-muted-foreground")}>{st}</span>}
                      </button>
                    )
                  })}
                </Panel>
              ) : <Empty icon={Layers} title="No line yet">An editorial line, a range or a brand: one file each in business/lines/.</Empty>}
            </Section>
            <Section title="Repositories" note={<button onClick={onRepos} className="hover:text-foreground">Manage</button>}>
              {data.repos.length ? (
                <Panel className="divide-y">
                  {data.repos.map((r) => (
                    <div key={r.name} className="flex items-center gap-3 px-4 py-2.5 text-sm">
                      <FolderGit2 className="size-4 text-muted-foreground" />
                      <span className="flex-1 font-mono text-[13px]">{r.name}</span>
                      {r.url && <a href={r.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">{r.url.replace(/^https?:\/\/(www\.)?/, "")} <ExternalLink className="size-3" /></a>}
                    </div>
                  ))}
                </Panel>
              ) : <Empty icon={FolderGit2} title="No repository">The code the developer agents work on. Connect one in Settings.</Empty>}
            </Section>
          </div>
          <Missing missing={data.missing} onFile={onFile} />
        </>
      )}
    </Page>
  )
}

function Fact({ label, value, wide, mono }: { label: string; value: React.ReactNode; wide?: boolean; mono?: boolean }) {
  return (
    <div className={cn(wide && "sm:col-span-2")}>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className={cn("mt-0.5 text-sm", !value && "text-muted-foreground/60", mono && value && "font-mono text-[13px]")}>{value || "Not set"}</dd>
    </div>
  )
}

function Identity({ p, text, onFile }: { p: Record<string, unknown>; text: (o: Record<string, unknown>, k: string) => string | null; onFile: (path: string) => void }) {
  const country = text(p, "country"), form = text(p, "legal_form"), inc = text(p, "incorporated_on"), fy = text(p, "fiscal_year_end")
  const fyText = fy && /^\d{2}-\d{2}$/.test(fy) ? new Date(`2000-${fy}T12:00:00`).toLocaleDateString("en-GB", { day: "numeric", month: "long" }) : fy
  const links = list(p.links)
  return (
    <Section title="Legal identity" note={<button onClick={() => onFile("business/profile.md")} className="inline-flex items-center gap-1 hover:text-foreground"><Pencil className="size-3" /> business/profile.md</button>}>
      <Panel className="p-5">
        <dl className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-4">
          <Fact label="Legal form" value={form ? FORMS[form.toLowerCase()] ?? form.toUpperCase() : null} />
          <Fact label="Country" value={country ? COUNTRIES[country.toUpperCase()] ?? country : null} />
          <Fact label="Registration number" value={text(p, "registration_number")} mono />
          <Fact label="Incorporated" value={inc ? (/^\d{4}-\d{2}-\d{2}$/.test(inc) ? longDate(inc) : inc) : null} />
          <Fact label="Registered office" value={text(p, "registered_office")} wide />
          <Fact label="Fiscal year ends" value={fyText} />
          <Fact label="Currency" value={text(p, "currency")} />
          <Fact label="VAT number" value={text(p, "vat_number")} mono />
          <Fact label="Tax regime" value={text(p, "tax_regime")} />
          <Fact label="Bank" value={text(p, "bank")} />
          <Fact label="Domain" value={text(p, "domain") && <a className="hover:underline" href={`https://${text(p, "domain")}`} target="_blank" rel="noreferrer">{text(p, "domain")}</a>} />
        </dl>
        {links.length > 0 && (
          <div className="mt-4 flex flex-wrap gap-1.5 border-t pt-4">
            {links.map((l) => {
              const safe = /^(https?:|mailto:)/i.test(l)
              const label = l.replace(/^(https?:\/\/(www\.)?|mailto:)/i, "").replace(/\/$/, "")
              return safe
                ? <a key={l} href={l} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs hover:bg-accent"><Globe className="size-3 text-muted-foreground" />{label}</a>
                : <span key={l} className="rounded-full border px-2.5 py-1 text-xs">{l}</span>
            })}
          </div>
        )}
      </Panel>
    </Section>
  )
}

function Goal({ g, text, onFile }: { g: Record<string, unknown>; text: (o: Record<string, unknown>, k: string) => string | null; onFile: (path: string) => void }) {
  const goal = text(g, "goal"), by = text(g, "by")
  const dated = /^\d{4}-\d{2}-\d{2}$/.test(by ?? "")
  // A milestone's date is the last one written in it ("… by 2026-12-31").
  const steps = list(g.milestones).map((m) => ({ text: m.replace(/\s*by\s+\d{4}-\d{2}-\d{2}\s*$/i, ""), date: m.match(/(\d{4}-\d{2}-\d{2})(?!.*\d{4}-\d{2}-\d{2})/)?.[1] ?? null }))
  const next = steps.findIndex((s) => s.date && daysTo(s.date) >= 0)
  return (
    <Section title="Goal" note={<button onClick={() => onFile("business/goal.md")} className="inline-flex items-center gap-1 hover:text-foreground"><Pencil className="size-3" /> business/goal.md</button>}>
      {!goal ? <Empty icon={Target} title="No goal yet">Write it in business/goal.md: the goal, how it is measured, the target and the date.</Empty> : (
        <Panel className="overflow-hidden">
          <div className="p-5">
            <p className="text-lg leading-snug font-semibold tracking-tight">{goal}</p>
            <dl className="mt-4 grid gap-4 sm:grid-cols-3">
              <div className="sm:col-span-2"><dt className="text-xs text-muted-foreground">Measured by</dt><dd className="mt-0.5 text-sm">{text(g, "measure") ?? <span className="text-muted-foreground/60">Not set</span>}</dd></div>
              <div><dt className="text-xs text-muted-foreground">Target</dt><dd className="mt-0.5 text-sm font-medium">{text(g, "target") ?? <span className="font-normal text-muted-foreground/60">Not set</span>}</dd></div>
            </dl>
          </div>
          <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border-t bg-muted/30 px-5 py-3 text-sm">
            <span className="inline-flex items-center gap-1.5"><Flag className="size-4 text-muted-foreground" />{by ? <span>By <b className="font-medium">{dated ? longDate(by) : by}</b>{dated && <span className="text-muted-foreground">{`, ${until(by)}`}</span>}</span> : <span className="text-muted-foreground">No deadline</span>}</span>
            {text(g, "baseline_minutes") && <span className="inline-flex items-center gap-1.5 text-muted-foreground" title="Your time on the business before Airth: the yardstick for the time it saves you"><Calendar className="size-4" />Baseline: {text(g, "baseline_minutes")} min of your time</span>}
          </div>
          {steps.length > 0 && (
            <ol className="border-t px-5 py-4">
              {steps.map((s, i) => {
                const past = !!s.date && daysTo(s.date) < 0
                return (
                  <li key={i} className="relative flex gap-3 pb-4 last:pb-0">
                    {i < steps.length - 1 && <span className="absolute top-5 bottom-0 left-[9px] w-px bg-border" />}
                    <span className={cn("relative z-10 mt-0.5 grid size-[19px] shrink-0 place-items-center rounded-full border bg-surface",
                      past && "border-muted-foreground/40 text-muted-foreground", i === next && "border-sky-500 bg-sky-500 text-white")}>
                      {past ? <Check className="size-3" /> : <span className={cn("size-1.5 rounded-full", i === next ? "bg-white" : "bg-muted-foreground/50")} />}
                    </span>
                    <div className="min-w-0 flex-1">
                      <p className={cn("text-sm", past && "text-muted-foreground")}>{s.text}</p>
                      {s.date && <p className={cn("text-xs", i === next ? "text-sky-600 dark:text-sky-400" : "text-muted-foreground")}>{longDate(s.date)} · {until(s.date)}{i === next ? " · next" : ""}</p>}
                    </div>
                  </li>
                )
              })}
              <li className="mt-3 text-[11px] text-muted-foreground">A passed date is not a milestone reached: Airth does not know yet whether it was.</li>
            </ol>
          )}
        </Panel>
      )}
    </Section>
  )
}

/** What the business folder does not say yet, by file, each opening it. */
function Missing({ missing, onFile }: { missing: string[]; onFile: (path: string) => void }) {
  if (!missing.length) return null
  const byFile = new Map<string, string[]>()
  for (const m of missing) {
    const [file, field] = m.includes(": ") ? m.split(": ") : ["", m]
    byFile.set(file, [...(byFile.get(file) ?? []), field])
  }
  return (
    <Section title="To fill in" note={`${missing.length} field${missing.length === 1 ? "" : "s"}`}>
      <Panel className="divide-y">
        {[...byFile].map(([file, fields]) => (
          <div key={file} className="flex items-center gap-3 px-4 py-3">
            {file.includes("profile") ? <Building2 className="size-4 text-muted-foreground" /> : file.includes("goal") ? <Target className="size-4 text-muted-foreground" /> : <FileText className="size-4 text-muted-foreground" />}
            <div className="min-w-0 flex-1">
              <div className="text-sm">{fields.map((f) => FIELDS[f] ?? f.replace(/_/g, " ")).join(", ")}</div>
              <div className="font-mono text-xs text-muted-foreground">{file}</div>
            </div>
            {file && <Button size="sm" variant="outline" onClick={() => onFile(file)}><Pencil /> Fill in</Button>}
          </div>
        ))}
      </Panel>
    </Section>
  )
}

const EXAMPLE = `Your goals, the line every agent follows, and what the team may do alone.

For example:
- One article a week on the blog.
- Explain, never hype. Facts and numbers first.
- Ask me before publishing or spending money.`

/** The strategy every agent of the business reads before it works: read
    as text, opened for editing on demand. */
function Strategy({ realm }: { realm: string }) {
  const { data, refresh } = usePoll<{ strategy: string }>(() => api(`realms/${enc(realm)}/steering`), 120000, [realm])
  const [draft, setDraft] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const text = data?.strategy ?? ""
  // The file's own "# Strategy" title would repeat the section's.
  const shown = text.replace(/^\s*#\s*strategy\s*\n+/i, "")
  const editing = draft !== null
  async function save() {
    setBusy(true)
    try {
      await api(`realms/${enc(realm)}/strategy`, { text: draft }, "PUT")
      toast.success("Strategy saved: every agent reads it from its next run.")
      setDraft(null)
      refresh()
    } catch (e) { toast.error((e as Error).message) }
    finally { setBusy(false) }
  }
  return (
    <Section title="Strategy" note={editing ? "Every agent of this business reads it before it works" : text.trim() && <Button size="xs" variant="ghost" onClick={() => setDraft(text)}><Pencil /> Edit</Button>}>
      {!data ? <Loading /> : editing ? (
        <div className="space-y-2.5">
          <Textarea aria-label="Strategy" rows={12} value={draft} onChange={(e) => setDraft(e.target.value)} placeholder={EXAMPLE} autoFocus className="text-sm leading-relaxed" />
          <div className="flex gap-2">
            <Button size="sm" disabled={busy} onClick={save}>{busy ? "Saving…" : "Save the strategy"}</Button>
            <Button size="sm" variant="ghost" onClick={() => setDraft(null)}>Cancel</Button>
          </div>
        </div>
      ) : text.trim() ? (
        <Panel className="px-4 py-3.5 text-sm leading-relaxed"><Rich text={shown} /></Panel>
      ) : (
        <Empty icon={Compass} title="No strategy yet">
          The goals, the line every agent follows, what the team may do alone. <button className="text-sky-500 hover:underline" onClick={() => setDraft("")}>Write it</button>
        </Empty>
      )}
    </Section>
  )
}
