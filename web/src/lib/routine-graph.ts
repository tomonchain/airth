// A routine's links as the canvas edits them, and the file they write
// (decision 91). Pure: no React, so the round trip can be checked alone.

import type { GraphBlock } from "@/lib/api"

export type Draft = { description?: string; agent: string; trigger: Record<string, unknown>; blocks: GraphBlock[]; [key: string]: unknown }

/** A copy of `o` with `key` set, or removed when `value` is undefined; a
    key already there keeps its place, so the file's diff stays small. */
export function put<T extends Record<string, unknown>>(o: T, key: string, value: unknown): T {
  const out: Record<string, unknown> = { ...o }
  if (value === undefined) delete out[key]
  else out[key] = value
  return out as T
}

/** The trigger's node: outside the block ids, which the check keeps from
    being "trigger", the trigger's key in the file's layout. */
export const TRIGGER = "trigger"

/** Where each block's outputs lead, explicitly: what the file leaves
    implicit (the next block in the list) is written out here. */
export type Link = { out?: string | null; then?: string | null; else?: string | null }
export type Links = Record<string, Link>

export type Report = { blocks: { id: string; items_in: number; items_out: number; secs: number; lines: string[]; notes: string[] }[]; cost: string; secs: number; failed?: string[] }

export function linksOf(d: Draft): Links {
  const links: Links = {}
  d.blocks.forEach((b, i) => {
    const following = d.blocks[i + 1]?.id ?? null
    if (b.type === "condition") links[b.id] = { then: typeof b.then === "string" ? b.then : following, else: typeof b.else === "string" && b.else ? b.else : null }
    else links[b.id] = { out: b.next === null ? null : typeof b.next === "string" ? b.next : following }
  })
  return links
}

/** The file the canvas says (decision 91): the first block is the
    trigger's, a link to the block that follows is left implicit, any other
    is written, an output linked nowhere stops the run. */
export function compose(d: Draft, links: Links, entry: string | null): Draft {
  const first = d.blocks.find((b) => b.id === entry)
  const order = first ? [first, ...d.blocks.filter((b) => b.id !== entry)] : d.blocks
  const blocks = order.map((b, i) => {
    const following = order[i + 1]?.id ?? null
    const l = links[b.id] ?? {}
    if (b.type === "condition") {
      let o = put(b, "next", undefined)
      o = put(o, "then", l.then && l.then !== following ? l.then : undefined)
      return put(o, "else", l.else ?? undefined)
    }
    const out = l.out ?? null
    return put(b, "next", out === following ? undefined : out === null ? (following ? null : undefined) : out)
  })
  return { ...d, blocks }
}

/** What the canvas cannot write yet, said before saving. */
export function problems(d: Draft, links: Links, entry: string | null): string[] {
  const out: string[] = []
  if (!d.blocks.length) out.push("Drag a first block in from the left.")
  else if (!entry) out.push("Link the trigger to the first block.")
  const first = d.blocks.find((b) => b.id === entry)
  const order = first ? [first, ...d.blocks.filter((b) => b.id !== entry)] : d.blocks
  order.forEach((b, i) => {
    if (b.type !== "condition") return
    const l = links[b.id] ?? {}
    if (!l.else) out.push(`Link ${b.id}'s else output.`)
    if (!l.then && order[i + 1]) out.push(`Link ${b.id}'s then output.`)
  })
  return out
}

/** Left to right by depth from the trigger, when the file places nothing. */
export function autoLayout(d: Draft, links: Links, entry: string | null): Record<string, [number, number]> {
  // A Map: a block may be called "constructor".
  const depth = new Map<string, number>([[TRIGGER, 0]])
  const queue: string[] = []
  if (entry) { depth.set(entry, 1); queue.push(entry) }
  while (queue.length) {
    const id = queue.shift()!
    const l = Object.hasOwn(links, id) ? links[id] : {}
    for (const t of [l.out, l.then, l.else]) {
      if (t && !depth.has(t) && d.blocks.some((b) => b.id === t)) { depth.set(t, depth.get(id)! + 1); queue.push(t) }
    }
  }
  const rows = new Map<number, number>()
  const at: Record<string, [number, number]> = Object.create(null)
  for (const id of [TRIGGER, ...d.blocks.map((b) => b.id)]) {
    const x = depth.get(id) ?? Math.max(1, ...depth.values()) + 1
    const y = rows.get(x) ?? 0
    rows.set(x, y + 1)
    at[id] = [x * 260, y * 140]
  }
  return at
}

