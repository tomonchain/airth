// A routine's AI blocks replayed on the founder's past answers (decision
// 151): the shapes `realms/<b>/graphs/<name>/eval` returns
// (src/routine/run/eval.rs), and what the screen makes of them.

/** What an evaluation would ask, before it runs. */
export type Estimate = { cases: number; blocks: number }

export type Evaluation = {
  routine: string
  /** The founder's answers replayed. */
  cases: number
  blocks: BlockEval[]
  cost: string
  cost_usd: number
  /** What was not measured, and why: a block, or one of its cases. */
  skipped: string[]
}

export type BlockEval = {
  id: string
  prompt: string
  kind: "keeps" | "drafts"
  score?: Score
  cases: Case[]
  cost_usd: number
}

export type Case = {
  item: number
  at: string
  title: string
  founder: "approve" | "edit" | "reject" | "redo"
  said: string
  draft: string
  edited: string
  kept?: boolean
  score?: number
  new?: string
}

export type Score = { wanted: number; wanted_kept: number; rejected: number; rejected_dropped: number; rate: number | null; before: number | null }

/** How far back, and how many answers at most, an evaluation replays. */
export const DAYS = 60
export const MAX = 20

/** What the founder did, in their words. */
export const DID: Record<Case["founder"], string> = {
  approve: "You approved",
  edit: "You edited",
  reject: "You rejected",
  redo: "You asked for a redo",
}

/** Whether a keeping block now does otherwise than the founder did: keeps
    what they rejected, or drops what they wanted. */
export const differs = (c: Case) => c.kept !== undefined && c.kept === (c.founder === "reject")

/** A keeping block's cases, split: where it differs from the founder first. */
export function split(cases: Case[]): { differ: Case[]; match: Case[] } {
  return { differ: cases.filter(differs), match: cases.filter((c) => !differs(c)) }
}

/** "x of y match your decisions", or null when no case could be scored. */
export function agreement(s: Score | undefined): { now: string; before: string } | null {
  const all = s ? s.wanted + s.rejected : 0
  if (!s || !all) return null
  return { now: `${s.wanted_kept + s.rejected_dropped} of ${all} match your decisions`, before: `${s.wanted} of ${all}` }
}

/** One thing not measured: the block, the past answer when it was one, and why. */
export type Skip = { block: string | null; title: string | null; why: string }

/** The engine says what it skipped in a line, "block: item 12: why" or
    "block: why"; the screen names the answer by its title, never its number. */
export function skips(e: Evaluation): Skip[] {
  const titles = new Map(e.blocks.flatMap((b) => b.cases.map((c) => [c.item, c.title] as const)))
  const upper = (t: string) => t.charAt(0).toUpperCase() + t.slice(1)
  return e.skipped.map((line) => {
    const one = /^([^:\s]+): item (\d+): ([\s\S]*)$/.exec(line)
    if (one) return { block: one[1], title: titles.get(Number(one[2])) ?? "A past answer", why: upper(one[3]) }
    const block = /^([^:\s]+): ([\s\S]*)$/.exec(line)
    if (block) return { block: block[1], title: null, why: upper(block[2]) }
    return { block: null, title: null, why: upper(line) }
  })
}

/** What a run may cost before it starts: one model call per answer and block. */
export function estimateText(e: Estimate): string {
  const answers = `${e.cases} past answer${e.cases === 1 ? "" : "s"}`
  const blocks = `${e.blocks} AI block${e.blocks === 1 ? "" : "s"}`
  const calls = e.cases * e.blocks
  return `${answers} × ${blocks}: ${calls} model call${calls === 1 ? "" : "s"}, one each.`
}
