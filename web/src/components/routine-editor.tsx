import type { ReactNode } from "react"
import { Plus, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import type { GraphBlock } from "@/lib/api"
import { put, type Draft } from "@/lib/routine-graph"

export type Declared = { id: string; class: string; ceiling: number }

export { put, type Draft }

const OPS = ["==", "!=", "<", "<=", ">", ">=", "exists"]
const SOURCES: [string, string][] = [["business", "Business records"], ["rss", "News feeds"], ["web", "A web page"], ["connector", "A connector's read"], ["x_post", "The post you send on Telegram"]]

/** A read block switched to another source: only that source's fields. */
function source(block: GraphBlock, to: string): GraphBlock {
  let b = put(put(put(put(block, "records", undefined), "feeds", undefined), "url", undefined), "action", undefined)
  b = put(b, "source", to)
  if (to === "business") b = put(b, "records", "week")
  if (to === "rss") b = put(b, "feeds", [])
  if (to === "web") b = put(b, "url", "")
  if (to === "connector") b = put(b, "action", "")
  return b
}
const CLASS: Record<string, string> = { read: "Read", reversible: "Reversible", commits: "Commits" }

function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  )
}

/** A text field whose empty value removes its key from the file. */
function Text({ value, onChange, placeholder, mono = true }: { value: unknown; onChange: (v: string | undefined) => void; placeholder?: string; mono?: boolean }) {
  return <Input value={typeof value === "string" ? value : ""} placeholder={placeholder} spellCheck={false} className={mono ? "font-mono text-[13px]" : undefined}
    onChange={(e) => onChange(e.target.value === "" ? undefined : e.target.value)} />
}

/** A whole number, or nothing. */
function Count({ value, onChange, placeholder }: { value: unknown; onChange: (v: number | undefined) => void; placeholder?: string }) {
  return <Input type="number" min={1} value={typeof value === "number" ? value : ""} placeholder={placeholder}
    onChange={(e) => onChange(e.target.value === "" ? undefined : Math.max(0, Math.floor(Number(e.target.value))))} />
}

function Choice({ value, options, onChange }: { value: string; options: [string, string][]; onChange: (v: string) => void }) {
  return (
    <Select value={value} onValueChange={(v) => onChange(String(v))}>
      <SelectTrigger className="w-full"><SelectValue>{(v: string) => options.find(([k]) => k === v)?.[1] ?? v}</SelectValue></SelectTrigger>
      <SelectContent>{options.map(([k, l]) => <SelectItem key={k} value={k}>{l}</SelectItem>)}</SelectContent>
    </Select>
  )
}

/** The routine itself: what it is for, and its schedule when it has one. */
export function TriggerFields({ draft, onChange, when }: { draft: Draft; onChange: (d: Draft) => void; when: (s: string) => string }) {
  const t = draft.trigger
  return (
    <div className="space-y-3">
      <Field label="What it is for">
        <Textarea rows={2} value={draft.description ?? ""} onChange={(e) => onChange(put(draft, "description", e.target.value || undefined))} />
      </Field>
      {typeof t.schedule === "string" ? (
        <Field label="When" hint={t.schedule.trim() ? when(t.schedule) : "mon 08:00 · daily 07:00 · weekdays 09:00 · every 2h"}>
          <Input value={t.schedule} spellCheck={false} className="font-mono text-[13px]" onChange={(e) => onChange({ ...draft, trigger: put(t, "schedule", e.target.value) })} />
        </Field>
      ) : <p className="text-xs text-muted-foreground">Its trigger is not a schedule: it changes in the file.</p>}
    </div>
  )
}

/** A block's parameters as form fields of its type (decision 69). Its id,
    type and links stay in the file: they change the routine's shape. */
export function BlockFields({ block, onChange, actions, prompts }: {
  block: GraphBlock; onChange: (b: GraphBlock) => void; actions: Declared[]; prompts: string[]
}) {
  const set = (key: string) => (v: unknown) => onChange(put(block, key, v))
  switch (block.type) {
    case "read": {
      const feeds = Array.isArray(block.feeds) ? (block.feeds as string[]) : null
      return (
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label="Source">
            <Choice value={String(block.source ?? "")} options={SOURCES} onChange={(v) => onChange(source(block, v))} />
          </Field>
          {block.source === "business" && <Field label="Records"><Choice value={String(block.records ?? "week")} options={[["week", "How the last 7 days went"]]} onChange={set("records")} /></Field>}
          {block.source === "web" && <Field label="Address"><Text value={block.url} onChange={(v) => set("url")(v ?? "")} placeholder="https://" /></Field>}
          {block.source === "connector" && (
            <Field label="Read action" hint="An action classed as a read.">
              <Choice value={String(block.action ?? "")} options={actions.filter((a) => a.class === "read").map((a) => [a.id, a.id])} onChange={set("action")} />
            </Field>
          )}
          {feeds && (
            <div className="sm:col-span-2">
              <Field label="Feeds" hint="One address per line.">
                <Textarea rows={Math.min(8, Math.max(2, feeds.length))} value={feeds.join("\n")} spellCheck={false} className="font-mono text-[12.5px]"
                  onChange={(e) => set("feeds")(e.target.value.split("\n").map((l) => l.trim()).filter(Boolean))} />
              </Field>
            </div>
          )}
          <Field label="Only items from the last" hint="Days"><Count value={block.max_age_days} onChange={set("max_age_days")} placeholder="any age" /></Field>
          <Field label="At most"><Count value={block.max} onChange={set("max")} placeholder="no limit" /></Field>
          <Field label="No repeats of" hint="A field: an item already seen with it is dropped."><Text value={block.dedupe} onChange={set("dedupe")} placeholder="link" /></Field>
        </div>
      )
    }
    case "ai": {
      const prompt = String(block.prompt ?? "")
      const distinct = Array.isArray(block.distinct) ? (block.distinct as string[]) : typeof block.distinct === "string" ? [block.distinct] : []
      return (
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="sm:col-span-2">
            <Field label="Prompt">
              <Choice value={prompt} options={[...new Set([prompt, ...prompts])].filter(Boolean).map((p) => [p, p])} onChange={set("prompt")} />
            </Field>
          </div>
          <Field label="Model"><Choice value={block.model === "fast" ? "fast" : "main"} options={[["main", "The main model"], ["fast", "The fast model"]]} onChange={(v) => set("model")(v === "fast" ? "fast" : undefined)} /></Field>
          <Field label="Items"><Choice value={block.over === "list" ? "list" : "item"} options={[["item", "One call per item"], ["list", "All at once"]]} onChange={(v) => set("over")(v === "list" ? "list" : undefined)} /></Field>
          <Field label="Keep the best"><Count value={block.limit} onChange={set("limit")} placeholder="all" /></Field>
          <Field label="One per" hint="Fields, separated by commas.">
            <Input value={distinct.join(", ")} spellCheck={false} className="font-mono text-[13px]" onChange={(e) => {
              const v = e.target.value.split(",").map((x) => x.trim()).filter(Boolean)
              set("distinct")(v.length === 0 ? undefined : v.length === 1 ? v[0] : v)
            }} />
          </Field>
          <Field label="Keep if"><Text value={block.keep} onChange={set("keep")} placeholder="keep" /></Field>
          <Field label="Rank by"><Text value={block.rank} onChange={set("rank")} placeholder="score" /></Field>
          <Field label="Answer saved as" hint="Empty: answer. A check's answer has its own name."><Text value={block.as} onChange={set("as")} placeholder="answer" /></Field>
        </div>
      )
    }
    case "condition": {
      const test = ((block.if as Record<string, unknown> | undefined) ?? {})
      const value = test.value === undefined ? "" : JSON.stringify(test.value)
      return (
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label="Field"><Text value={test.field} onChange={(v) => set("if")(put(test, "field", v ?? ""))} /></Field>
          <Field label="Test"><Choice value={String(test.op ?? "==")} options={OPS.map((o) => [o, o])} onChange={(v) => set("if")(put(test, "op", v))} /></Field>
          <Field label="Value" hint="A number, true, or text in quotes.">
            <Input value={value} spellCheck={false} className="font-mono text-[13px]" disabled={test.op === "exists"} onChange={(e) => {
              const raw = e.target.value
              let v: unknown = raw
              try { v = raw === "" ? undefined : JSON.parse(raw) } catch { /* text as typed */ }
              set("if")(put(test, "value", v))
            }} />
          </Field>
        </div>
      )
    }
    case "approval":
      return (
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label="What you see"><Text value={block.show} onChange={(v) => set("show")(v ?? "")} placeholder="answer.draft" /></Field>
          <Field label="What you may edit" hint="Empty: approve or reject only."><Text value={block.editable} onChange={set("editable")} placeholder="answer.draft" /></Field>
          <Field label="Expires after" hint="Hours"><Count value={block.expires_after} onChange={set("expires_after")} placeholder="never" /></Field>
          <Field label="Quality flag" hint="A check's answer: the item shows its flag."><Text value={block.flag} onChange={set("flag")} placeholder="check" /></Field>
        </div>
      )
    case "act": {
      const action = String(block.action ?? "")
      const fields = ((block.fields as Record<string, string> | undefined) ?? {})
      const known = actions.find((a) => a.id === action)
      return (
        <div className="space-y-3">
          <Field label="Action" hint={known ? `${CLASS[known.class] ?? known.class} · at most level ${known.ceiling}` : "Not declared: the routine will be refused."}>
            <Choice value={action} options={[...new Set([action, ...actions.map((a) => a.id)])].filter(Boolean).map((a) => [a, a])} onChange={set("action")} />
          </Field>
          <Fields fields={fields} onChange={(f) => set("fields")(f)} />
        </div>
      )
    }
    default:
      return <p className="text-xs text-muted-foreground">This block changes in the file.</p>
  }
}

/** An act's fields: each a template the item fills ({{answer.draft}}). */
function Fields({ fields, onChange }: { fields: Record<string, string>; onChange: (f: Record<string, string>) => void }) {
  const keys = Object.keys(fields)
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">Fields</Label>
      {keys.map((k) => (
        <div key={k} className="flex items-center gap-2">
          <span className="w-32 shrink-0 truncate font-mono text-xs text-muted-foreground" title={k}>{k}</span>
          <Input value={fields[k]} spellCheck={false} className="font-mono text-[13px]" onChange={(e) => onChange({ ...fields, [k]: e.target.value })} />
          <Button size="icon-sm" variant="ghost" aria-label={`Remove ${k}`} onClick={() => onChange(put(fields, k, undefined))}><X /></Button>
        </div>
      ))}
      <form className="flex items-center gap-2" onSubmit={(e) => {
        e.preventDefault()
        const input = e.currentTarget.elements.namedItem("key") as HTMLInputElement
        const k = input.value.trim()
        if (k && !(k in fields)) onChange({ ...fields, [k]: "" })
        input.value = ""
      }}>
        <Input name="key" placeholder="another field" spellCheck={false} className="w-32 shrink-0 font-mono text-[13px]" />
        <Button size="sm" variant="ghost" type="submit"><Plus /> Add</Button>
      </form>
      <p className="text-xs text-muted-foreground">{"{{answer.draft}}"} puts the item's field in; anything else is sent as written.</p>
    </div>
  )
}
