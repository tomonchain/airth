import { Fragment, type ReactNode } from "react"
import { cn } from "@/lib/utils"

// An agent's words, with what reads well in a chat: links, bullet lists,
// bold and italic, tables (Markdown, or a ```csv block), charts (a ```chart
// block) and code. Headings, quotes and images are dropped, never shown raw.

const INLINE = /(\[([^\]]+)\]\((https?:\/\/[^\s)]+)\))|(https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"])|(\*\*([^*]+)\*\*|__([^_]+)__)|(\*([^*\s][^*]*?)\*|_([^_\s][^_]*?)_)|(`([^`]+)`)/g

function inline(text: string): ReactNode[] {
  const out: ReactNode[] = []
  let last = 0
  for (const m of text.matchAll(INLINE)) {
    if (m.index! > last) out.push(text.slice(last, m.index))
    const k = out.length
    if (m[1]) out.push(<a key={k} href={m[3]} target="_blank" rel="noreferrer" className="underline decoration-current/30 underline-offset-2 hover:decoration-current">{m[2]}</a>)
    else if (m[4]) out.push(<a key={k} href={m[4]} target="_blank" rel="noreferrer" className="underline decoration-current/30 underline-offset-2 break-all hover:decoration-current">{m[4].replace(/^https?:\/\//, "")}</a>)
    else if (m[5]) out.push(<strong key={k} className="font-semibold">{inline(m[6] ?? m[7])}</strong>)
    else if (m[8]) out.push(<em key={k}>{inline(m[9] ?? m[10])}</em>)
    else if (m[11]) out.push(m[12])
    last = m.index! + m[0].length
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

const BULLET = /^\s*(?:[-*•]|\d+[.)])\s+/

export function Rich({ text }: { text: string }) {
  const lines = text.replace(/\r/g, "").split("\n")
  const blocks: ReactNode[] = []
  let list: { ordered: boolean; items: string[] } | null = null
  let para: string[] = []
  const flushPara = () => {
    if (para.length) blocks.push(<p key={blocks.length}>{para.map((l, i) => <Fragment key={i}>{i > 0 && <br />}{inline(l)}</Fragment>)}</p>)
    para = []
  }
  const flushList = () => {
    if (list) {
      const items = list.items.map((it, i) => <li key={i}>{inline(it)}</li>)
      blocks.push(list.ordered
        ? <ol key={blocks.length} className="list-decimal space-y-0.5 pl-5">{items}</ol>
        : <ul key={blocks.length} className="list-disc space-y-0.5 pl-5 marker:text-muted-foreground">{items}</ul>)
    }
    list = null
  }
  const flush = () => { flushPara(); flushList() }
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i]
    // A fenced block: a table from CSV, a chart, or code.
    const fence = raw.match(/^\s*```\s*([\w-]*)/)
    if (fence) {
      flush()
      const body: string[] = []
      while (++i < lines.length && !/^\s*```/.test(lines[i])) body.push(lines[i])
      const lang = fence[1].toLowerCase()
      const code = body.join("\n")
      if (lang === "csv" || lang === "tsv") blocks.push(<Table key={blocks.length} rows={csv(code)} />)
      else if (lang === "chart") blocks.push(<ChartBlock key={blocks.length} spec={code} />)
      else if (code.trim()) blocks.push(<pre key={blocks.length} className="overflow-x-auto rounded-lg bg-muted/60 px-3 py-2 font-mono text-[12px] leading-relaxed">{code}</pre>)
      continue
    }
    // A Markdown table: a row of cells, then a line of dashes.
    if (/^\s*\|/.test(raw) && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] ?? "")) {
      flush()
      const rows = [cells(raw)]
      i++
      while (i + 1 < lines.length && /^\s*\|/.test(lines[i + 1])) rows.push(cells(lines[++i]))
      blocks.push(<Table key={blocks.length} rows={rows} />)
      continue
    }
    if (/^\s*(---+\s*$|\|)/.test(raw)) continue
    const line = raw.replace(/^\s*#{1,6}\s+/, "").replace(/^\s*>\s?/, "")
    if (!line.trim()) { flush(); continue }
    if (BULLET.test(line)) {
      flushPara()
      const ordered = /^\s*\d/.test(line)
      if (!list || list.ordered !== ordered) { flushList(); list = { ordered, items: [] } }
      list.items.push(line.replace(BULLET, ""))
    } else {
      flushList()
      para.push(line)
    }
  }
  flush()
  return <div className="space-y-2">{blocks}</div>
}

const cells = (row: string) => row.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim())

/** CSV (or semicolon- or tab-separated) rows; quoted fields may hold the
    separator, quotes doubled, and line breaks. */
export function csv(text: string): string[][] {
  const first = text.split("\n")[0] ?? ""
  const sep = ["\t", ";", ","].map((c) => [c, first.split(c).length] as const).sort((a, b) => b[1] - a[1])[0][0]
  const rows: string[][] = []
  let row: string[] = [], field = "", quoted = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { field += '"'; i++ }
      else if (c === '"') quoted = false
      else field += c
    } else if (c === '"' && !field) quoted = true
    else if (c === sep) { row.push(field.trim()); field = "" }
    else if (c === "\n") { row.push(field.trim()); rows.push(row); row = []; field = "" }
    else field += c
  }
  if (field || row.length) { row.push(field.trim()); rows.push(row) }
  return rows.filter((r) => r.some((c) => c))
}

/** The first row is the header. Long tables scroll inside the bubble. */
function Table({ rows }: { rows: string[][] }) {
  if (!rows.length) return null
  const [head, ...body] = rows
  const number = (c: string) => /^[-+]?[$€£]?\s?[\d,.\s]+%?$/.test(c)
  // A column of figures is aligned right, its header too.
  const figures = head.map((_, k) => body.length > 0 && body.every((r) => !r[k] || number(r[k])))
  return (
    <div className="max-h-96 overflow-auto rounded-lg border bg-background/60">
      <table className="w-full border-collapse text-[12.5px]">
        <thead className="sticky top-0 bg-muted/80 backdrop-blur">
          <tr>{head.map((c, i) => <th key={i} className={cn("px-2.5 py-1.5 font-medium whitespace-nowrap", figures[i] ? "text-right" : "text-left")}>{inline(c)}</th>)}</tr>
        </thead>
        <tbody>
          {body.map((r, i) => (
            <tr key={i} className="border-t">
              {head.map((_, k) => <td key={k} className={cn("px-2.5 py-1.5 align-top", figures[k] && "text-right tabular-nums whitespace-nowrap")}>{inline(r[k] ?? "")}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

type Spec = { type?: "bar" | "line"; title?: string; unit?: string; labels: string[]; series: { name?: string; values: number[] }[] }
const COLORS = ["#0ea5e9", "#f59e0b", "#10b981", "#8b5cf6", "#f43f5e"]

/** A chart an agent wrote as JSON: bars (side by side for several series)
    or lines over the labels. JSON that is not a chart shows as code. */
function ChartBlock({ spec }: { spec: string }) {
  let c: Spec
  try {
    c = JSON.parse(spec)
    if (!Array.isArray(c.labels) || !Array.isArray(c.series) || !c.series.every((s) => Array.isArray(s.values))) throw new Error()
  } catch {
    return <pre className="overflow-x-auto rounded-lg bg-muted/60 px-3 py-2 font-mono text-[12px]">{spec}</pre>
  }
  const all = c.series.flatMap((s) => s.values.filter((v) => typeof v === "number"))
  const max = Math.max(Number.MIN_VALUE, ...all)
  // Bars start at zero; a line spans its own values, so a trend shows.
  const low = c.type === "line" ? Math.min(...all) - (max - Math.min(...all)) * 0.1 : 0
  const y = (v: number) => 38 - (36 * ((v ?? 0) - low)) / Math.max(max - low, Number.MIN_VALUE)
  const fmt = (v: number) => `${v.toLocaleString("en-GB", { maximumFractionDigits: 2 })}${c.unit ? ` ${c.unit}` : ""}`
  const every = Math.ceil(c.labels.length / 8)
  return (
    <figure className="rounded-lg border bg-background/60 px-3 pt-2.5 pb-1.5">
      {(c.title || c.series.length > 1) && (
        <figcaption className="mb-2 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs">
          {c.title && <span className="font-medium">{c.title}</span>}
          {c.series.length > 1 && c.series.map((s, i) => <span key={i} className="inline-flex items-center gap-1 text-muted-foreground"><i className="size-2 rounded-sm" style={{ background: COLORS[i % COLORS.length] }} />{s.name ?? `Series ${i + 1}`}</span>)}
        </figcaption>
      )}
      {c.type === "line" ? (
        <svg viewBox="0 0 100 40" preserveAspectRatio="none" className="h-36 w-full overflow-visible">
          {c.series.map((s, i) => (
            <polyline key={i} fill="none" stroke={COLORS[i % COLORS.length]} strokeWidth="1.5" vectorEffect="non-scaling-stroke"
              points={s.values.map((v, k) => `${(k / Math.max(c.labels.length - 1, 1)) * 100},${y(v)}`).join(" ")} />
          ))}
        </svg>
      ) : (
        <div className="flex h-36 items-end gap-[3px]">
          {c.labels.map((l, k) => (
            <div key={k} className="flex h-full min-w-0 flex-1 items-end gap-px" title={`${l}\n${c.series.map((s) => `${s.name ? `${s.name}: ` : ""}${fmt(s.values[k] ?? 0)}`).join("\n")}`}>
              {c.series.map((s, i) => <span key={i} className="min-w-0 flex-1 rounded-t-sm hover:opacity-80" style={{ height: `${(100 * (s.values[k] ?? 0)) / max}%`, background: COLORS[i % COLORS.length] }} />)}
            </div>
          ))}
        </div>
      )}
      <div className="mt-1 flex text-[10px] text-muted-foreground">
        {c.labels.map((l, k) => <span key={k} className="min-w-0 flex-1 truncate text-center">{k % every === 0 ? l : ""}</span>)}
      </div>
      <div className="text-right text-[10px] text-muted-foreground">max {fmt(max)}</div>
    </figure>
  )
}
