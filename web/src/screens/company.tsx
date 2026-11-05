import { useState } from "react"
import { Compass, FolderGit2, Pencil, Target } from "lucide-react"
import { toast } from "sonner"
import { Empty, Loading, Page, Panel, Section } from "@/components/page"
import { Rich } from "@/components/rich"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { usePoll } from "@/hooks/use-poll"
import { api, enc, type Company as Data } from "@/lib/api"
import { Fact } from "@/screens/finance"

/** The company: its legal identity, its goal, its lines and repositories,
    as written in the business folder (decision 44). */
export function Company({ realm, description }: { realm: string; description: string | null }) {
  const { data } = usePoll<Data>(() => api(`realms/${enc(realm)}/company`), 60000, [realm])
  const p = data?.profile
  const g = data?.goal
  return (
    <Page eyebrow={p?.status ?? undefined} title={p?.legal_name ?? realm} lede={description}>
      {!data ? <Loading /> : (
        <>
          <Section title="Legal identity">
            <Panel className="grid grid-cols-2 gap-x-6 gap-y-3 px-4 py-3.5 text-sm sm:grid-cols-3">
              <Fact label="Legal form" value={p?.legal_form?.toUpperCase()} />
              <Fact label="Country" value={p?.country} />
              <Fact label="Registration number" value={p?.registration_number} />
              <Fact label="Incorporated" value={p?.incorporated_on} />
              <Fact label="Registered office" value={p?.registered_office} />
              <Fact label="Domain" value={p?.domain} />
            </Panel>
          </Section>

          <Section title="Goal">
            {g?.goal ? (
              <Panel className="px-4 py-3.5 text-sm">
                <p className="text-base font-medium">{g.goal}</p>
                <p className="mt-1 text-muted-foreground">{[g.measure && `Measured by ${g.measure}`, g.target && `target ${g.target}`, g.by && `by ${g.by}`].filter(Boolean).join(", ")}</p>
              </Panel>
            ) : <Empty icon={Target} title="No goal yet">Write it in business/goal.md: the goal, how it is measured, the target and the date.</Empty>}
          </Section>

          <Strategy realm={realm} />

          <Section title="Repositories">
            {data.repos.length ? (
              <Panel className="divide-y">
                {data.repos.map((r) => (
                  <div key={r.name} className="flex items-center gap-3 px-4 py-2.5 text-sm">
                    <FolderGit2 className="size-4 text-muted-foreground" /> <span className="flex-1 font-mono text-[13px]">{r.name}</span>
                    {r.url && <a href={r.url} target="_blank" rel="noreferrer" className="text-xs text-muted-foreground hover:text-foreground">{r.url}</a>}
                  </div>
                ))}
              </Panel>
            ) : <Empty icon={FolderGit2} title="No repository" />}
          </Section>

          {data.missing.length > 0 && (
            <Section title="To fill in" note={`${data.missing.length} fields`}>
              <Panel className="flex flex-wrap gap-1.5 px-4 py-3.5">
                {data.missing.map((m) => <span key={m} className="rounded-md bg-muted px-2 py-0.5 font-mono text-[11.5px] text-muted-foreground">{m}</span>)}
              </Panel>
            </Section>
          )}
        </>
      )}
    </Page>
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
        <Panel className="px-4 py-3.5 text-sm leading-relaxed"><Rich text={text} /></Panel>
      ) : (
        <Empty icon={Compass} title="No strategy yet">
          The goals, the line every agent follows, what the team may do alone. <button className="text-sky-500 hover:underline" onClick={() => setDraft("")}>Write it</button>
        </Empty>
      )}
    </Section>
  )
}
