import { useMemo } from "react"
import { cn } from "@/lib/utils"

/** The most rows shown: a bigger file is read in its editor or its app. */
const MAX_ROWS = 2000

/** A CSV's rows, as RFC 4180 has them: fields in quotes may hold the
    separator, line breaks and doubled quotes. The separator is a tab for a
    .tsv; otherwise whichever of `,` and `;` the first line has more of (a
    French export uses `;`). */
export function parseCsv(text: string, tsv = false): string[][] {
  const head = text.slice(0, text.indexOf("\n") + 1 || text.length)
  const count = (c: string) => head.split(c).length - 1
  const sep = tsv ? "\t" : count(";") > count(",") ? ";" : ","
  const rows: string[][] = []
  let row: string[] = [], field = "", quoted = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { field += '"'; i++ }
      else if (c === '"') quoted = false
      else field += c
    } else if (c === '"' && field === "") quoted = true
    else if (c === sep) { row.push(field); field = "" }
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++
      row.push(field); rows.push(row); row = []; field = ""
    } else field += c
  }
  if (field !== "" || row.length) { row.push(field); rows.push(row) }
  return rows.filter((r) => r.some((f) => f.trim() !== ""))
}

const NUMBER = /^[-+]?[\d\s.,]+%?$/

/** A CSV as a table: its first line as the header, kept in view; numbers
    to the right. Wide tables scroll inside their own box. */
export function CsvTable({ text, tsv }: { text: string; tsv?: boolean }) {
  const rows = useMemo(() => parseCsv(text, tsv), [text, tsv])
  if (!rows.length) return <p className="p-6 text-sm text-muted-foreground">This file is empty.</p>
  const [header, ...body] = rows
  const width = Math.max(...rows.map((r) => r.length))
  const shown = body.slice(0, MAX_ROWS)
  const numeric = Array.from({ length: width }, (_, c) => {
    const values = shown.map((r) => r[c]?.trim()).filter(Boolean)
    return values.length > 0 && values.every((v) => NUMBER.test(v!))
  })
  return (
    <div className="p-4">
      <div className="overflow-auto rounded-xl bg-bubble">
        <table className="w-max min-w-full border-collapse text-[13px]">
          <thead className="sticky top-0 bg-bubble">
            <tr>
              <th className="px-3 py-2 text-right text-[11px] font-normal text-muted-foreground tabular-nums">#</th>
              {Array.from({ length: width }, (_, c) => (
                <th key={c} className={cn("px-3 py-2 text-left font-medium whitespace-nowrap", numeric[c] && "text-right")}>{header[c] ?? ""}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {shown.map((r, i) => (
              <tr key={i} className="even:bg-foreground/[0.03] hover:bg-accent/60">
                <td className="px-3 py-1.5 text-right text-[11px] text-muted-foreground tabular-nums">{i + 1}</td>
                {Array.from({ length: width }, (_, c) => (
                  <td key={c} className={cn("max-w-96 truncate px-3 py-1.5", numeric[c] && "text-right tabular-nums")} title={r[c]}>{r[c] ?? ""}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-2 px-1 text-xs text-muted-foreground">
        {body.length} row{body.length === 1 ? "" : "s"} · {width} column{width === 1 ? "" : "s"}
        {body.length > MAX_ROWS && ` · the first ${MAX_ROWS} shown: Edit or the Mac's app shows the rest`}
      </p>
    </div>
  )
}
