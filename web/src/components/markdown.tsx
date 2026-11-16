import { createContext, useContext, type ReactNode } from "react"
import ReactMarkdown, { type Components } from "react-markdown"
import remarkGfm from "remark-gfm"
import { ImageOff } from "lucide-react"
import { cn } from "@/lib/utils"
import { workspacePath } from "@/lib/workspace-link"

// An agent's words, formatted: headings, bold, lists and task lists,
// strikethrough, tables, code, and Airth's own blocks (a ```csv table, a
// ```chart). What an agent writes is untrusted (decision 144): no HTML is
// rendered, no image is loaded, and a link goes only to http(s), mailto or
// a file of the workspace, opened in the Documents page (decision 146).

/** A URL a link may lead to: the web or an address, nothing that runs. */
const SAFE = /^(https?:|mailto:)/i

/** The host a URL points at, to show where a link or an image leads. */
export function domain(url: string): string {
  if (/^data:/i.test(url)) return "embedded data"
  try { return new URL(url).host || url } catch { return url }
}

/** A cell that holds a figure: 1,200 · -3.5% · €12 · 1 200 € · +4. */
const FIGURE = /^[-+−]?\s?[$€£¥]?\s?\d[\d,.\s ]*\s?(%|[$€£¥]|k|m|bn)?$/i
export const figure = (cell: string) => FIGURE.test(cell.trim())

type Hast = { type: string; tagName?: string; value?: string; properties?: Record<string, unknown>; children?: Hast[] }

const text = (n: Hast): string => n.type === "text" ? n.value ?? "" : (n.children ?? []).map(text).join("")
const rows = (n: Hast | undefined) => (n?.children ?? []).filter((c) => c.tagName === "tr")
const cellsOf = (tr: Hast) => (tr.children ?? []).filter((c) => c.tagName === "td" || c.tagName === "th")

/** Marks the cells of a column of figures (every filled body cell a
    number), and its header, so they align right. */
function figures() {
  const walk = (n: Hast) => {
    if (n.tagName === "table") {
      const head = rows((n.children ?? []).find((c) => c.tagName === "thead"))
      const body = rows((n.children ?? []).find((c) => c.tagName === "tbody"))
      const width = Math.max(0, ...[...head, ...body].map((r) => cellsOf(r).length))
      for (let k = 0; k < width; k++) {
        const filled = body.map((r) => cellsOf(r)[k]).filter((c) => c && text(c).trim())
        if (!filled.length || !filled.every((c) => figure(text(c!)))) continue
        for (const r of [...head, ...body]) {
          const c = cellsOf(r)[k]
          if (c) c.properties = { ...c.properties, dataFigure: "true" }
        }
      }
    }
    n.children?.forEach(walk)
  }
  return (tree: Hast) => walk(tree)
}

/** Links keep only safe addresses; an image's source is kept only to say
    where it is (it is never loaded). */
function url(value: string, key: string): string | null {
  if (key === "src") return value
  return SAFE.test(value.trim()) || workspacePath(value) ? value : null
}

/** What opens a workspace file in the Documents page (App provides it).
    Without it, a workspace link is plain text. */
export const FileLinks = createContext<((path: string) => void) | null>(null)

const LINK = "underline decoration-current/30 underline-offset-2 hover:decoration-current"

/** A link: to the web in a new tab, to a workspace file in the app, else
    its text alone. */
function Link({ href, children }: { href?: string; children?: ReactNode }) {
  const open = useContext(FileLinks)
  if (href && SAFE.test(href)) {
    return <a href={href} target="_blank" rel="noopener noreferrer" title={domain(href)} className={LINK}>{children}</a>
  }
  const path = workspacePath(href)
  if (path && open) {
    return <button type="button" onClick={() => open(path)} title={`Open ${path}`} data-file={path} className={cn(LINK, "cursor-pointer text-left")}>{children}</button>
  }
  return <span>{children}</span>
}

const TABLE = "max-w-full overflow-x-auto rounded-lg border"
// Cells break between words only: a narrow screen scrolls the table instead.
const TH = "border-b bg-muted/60 px-2.5 py-1.5 text-left font-semibold whitespace-nowrap [overflow-wrap:normal] data-[figure]:text-right"
const TD = "border-t px-2.5 py-1.5 align-top [overflow-wrap:normal] data-[figure]:text-right data-[figure]:whitespace-nowrap data-[figure]:tabular-nums [tr:first-child>&]:border-t-0"
const PRE = "max-w-full overflow-x-auto rounded-lg bg-muted/60 px-3 py-2 font-mono text-[12px] leading-relaxed"

const components: Components = {
  h1: ({ children }) => <h3 className="mt-3 mb-1 text-[17px] leading-snug font-semibold first:mt-0">{children}</h3>,
  h2: ({ children }) => <h3 className="mt-3 mb-1 text-[16px] leading-snug font-semibold first:mt-0">{children}</h3>,
  h3: ({ children }) => <h4 className="mt-3 mb-1 text-[15px] leading-snug font-semibold first:mt-0">{children}</h4>,
  h4: ({ children }) => <h5 className="mt-2 mb-0.5 font-semibold first:mt-0">{children}</h5>,
  h5: ({ children }) => <h6 className="mt-2 mb-0.5 font-semibold first:mt-0">{children}</h6>,
  h6: ({ children }) => <h6 className="mt-2 mb-0.5 font-semibold first:mt-0">{children}</h6>,
  p: ({ children }) => <p>{children}</p>,
  strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
  del: ({ children }) => <del className="text-muted-foreground">{children}</del>,
  ul: ({ children }) => <ul className="list-disc space-y-0.5 pl-5 marker:text-muted-foreground">{children}</ul>,
  ol: ({ children, start }) => <ol start={start} className="list-decimal space-y-0.5 pl-5 marker:text-muted-foreground">{children}</ol>,
  // A task sits in the bullet's place: its box is as wide as the indent.
  li: ({ className, children }) => <li className={className?.includes("task-list-item") ? "-ml-5 list-none" : undefined}>{children}</li>,
  input: ({ checked }) => <input type="checkbox" defaultChecked={!!checked} disabled className="mr-1.5 size-3.5 translate-y-[2px] accent-foreground" />,
  blockquote: ({ children }) => <blockquote className="space-y-2 border-l-2 pl-3 text-muted-foreground">{children}</blockquote>,
  hr: () => <hr className="my-3 border-border" />,
  a: ({ href, children }) => <Link href={href}>{children}</Link>,
  // Never loaded: a remote image would tell its server who reads, and when.
  img: ({ src, alt }) => (
    <span className="inline-flex max-w-full items-center gap-1 rounded-md border px-1.5 py-0.5 align-middle text-xs text-muted-foreground">
      <ImageOff className="size-3 shrink-0" aria-hidden="true" />
      <span className="truncate">{alt || "Image"}{typeof src === "string" && src ? ` · ${domain(src)}` : ""}</span>
    </span>
  ),
  table: ({ children }) => <div className={TABLE}><table className="w-full border-collapse text-[12.5px]">{children}</table></div>,
  th: ({ node, children }) => <th className={TH} data-figure={node?.properties?.dataFigure ? "true" : undefined}>{children}</th>,
  td: ({ node, children }) => <td className={TD} data-figure={node?.properties?.dataFigure ? "true" : undefined}>{children}</td>,
  code: ({ children }) => <code className="rounded bg-muted/70 px-1 py-px font-mono text-[0.9em]">{children}</code>,
  // A fenced block: Airth's table or chart, or code.
  pre: ({ node }) => {
    const code = (node?.children?.[0] ?? {}) as Hast
    const lang = (code.properties?.className as string[] | undefined)?.find((c) => c.startsWith("language-"))?.slice(9).toLowerCase()
    const body = text(code).replace(/\n$/, "")
    if (lang === "csv" || lang === "tsv") return <CsvTable rows={csv(body)} />
    if (lang === "chart") return <ChartBlock spec={body} />
    return body.trim() ? <pre className={PRE}><code>{body}</code></pre> : null
  },
}

export function Markdown({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn("min-w-0 space-y-2 [overflow-wrap:anywhere]", className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[figures]} components={components} skipHtml urlTransform={url}>
        {text}
      </ReactMarkdown>
    </div>
  )
}

/** CSV (or semicolon- or tab-separated) rows; quoted fields may hold the
    separator, quotes doubled, and line breaks. */
export function csv(text: string): string[][] {
  // The separator is the one the header uses most, counted outside quotes.
  const first = (text.split("\n")[0] ?? "").replace(/"(?:[^"]|"")*"/g, "")
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

/** The first row is the header; a column of figures aligns right. */
function CsvTable({ rows }: { rows: string[][] }) {
  if (!rows.length) return null
  const [head, ...body] = rows
  const figures = head.map((_, k) => body.some((r) => r[k]) && body.every((r) => !r[k] || figure(r[k])))
  const mark = (k: number) => (figures[k] ? "true" : undefined)
  return (
    <div className={cn(TABLE, "max-h-96")}>
      <table className="w-full border-collapse text-[12.5px]">
        <thead className="sticky top-0"><tr>{head.map((c, k) => <th key={k} className={TH} data-figure={mark(k)}>{c}</th>)}</tr></thead>
        <tbody>{body.map((r, i) => <tr key={i}>{head.map((_, k) => <td key={k} className={TD} data-figure={mark(k)}>{r[k] ?? ""}</td>)}</tr>)}</tbody>
      </table>
    </div>
  )
}

type Spec = { type?: "bar" | "line"; title?: string; unit?: string; labels: string[]; series: { name?: string; values: (number | null)[] }[] }
const COLORS = ["#0ea5e9", "#f59e0b", "#10b981", "#8b5cf6", "#f43f5e"]

const optional = (v: unknown) => v === undefined || typeof v === "string"

/** The chart an agent's JSON describes, or null when it is not one exactly:
    text where text goes, finite numbers (or nothing) for values. Anything
    else would reach the page as an object, so it shows as code instead. */
export function chart(spec: string): Spec | null {
  let c: Spec
  try { c = JSON.parse(spec) } catch { return null }
  if (!c || typeof c !== "object" || !optional(c.title) || !optional(c.unit) || (c.type !== undefined && c.type !== "bar" && c.type !== "line")) return null
  if (!Array.isArray(c.labels) || !c.labels.every((l) => typeof l === "string")) return null
  if (!Array.isArray(c.series) || !c.series.length) return null
  for (const s of c.series) {
    if (!s || typeof s !== "object" || !optional(s.name) || !Array.isArray(s.values)) return null
    if (!s.values.every((v) => v === null || (typeof v === "number" && Number.isFinite(v)))) return null
  }
  return c.series.some((s) => s.values.some((v) => typeof v === "number")) ? c : null
}

/** A chart an agent wrote as JSON: bars (side by side for several series)
    or lines over the labels. JSON that is not a chart shows as code. */
function ChartBlock({ spec }: { spec: string }): ReactNode {
  const c = chart(spec)
  if (!c) return <pre className={PRE}>{spec}</pre>
  const all = c.series.flatMap((s) => s.values.filter((v) => typeof v === "number"))
  // Bars grow from zero: one below it has no height.
  const max = Math.max(0, ...all)
  // Bars start at zero; a line spans its own values, so a trend shows.
  const low = c.type === "line" ? Math.min(...all) - (Math.max(...all) - Math.min(...all)) * 0.1 : 0
  const y = (v: number) => 38 - (36 * ((v ?? 0) - low)) / Math.max(Math.max(...all) - low, Number.MIN_VALUE)
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
              points={s.values.map((v, k) => (v === null ? "" : `${(k / Math.max(c.labels.length - 1, 1)) * 100},${y(v)}`)).filter(Boolean).join(" ")} />
          ))}
        </svg>
      ) : (
        <div className="flex h-36 items-end gap-[3px]">
          {c.labels.map((l, k) => (
            <div key={k} className="flex h-full min-w-0 flex-1 items-end gap-px" title={`${l}\n${c.series.map((s) => `${s.name ? `${s.name}: ` : ""}${fmt(s.values[k] ?? 0)}`).join("\n")}`}>
              {c.series.map((s, i) => <span key={i} className="min-w-0 flex-1 rounded-t-sm hover:opacity-80" style={{ height: `${max > 0 ? (100 * Math.max(0, s.values[k] ?? 0)) / max : 0}%`, background: COLORS[i % COLORS.length] }} />)}
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
