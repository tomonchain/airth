import { Fragment, type ReactNode } from "react"

// An agent's words, with only what reads well in a chat: links, bullet
// lists, bold and italic. Every other Markdown mark is dropped, never shown
// raw and never rendered (no headings, tables, code blocks or images).

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
  for (const raw of lines) {
    if (/^\s*(```|---+\s*$|\|)/.test(raw)) continue
    const line = raw.replace(/^\s*#{1,6}\s+/, "").replace(/^\s*>\s?/, "")
    if (!line.trim()) { flushPara(); flushList(); continue }
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
  flushPara(); flushList()
  return <div className="space-y-2">{blocks}</div>
}
