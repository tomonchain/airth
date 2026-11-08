import { useMemo, useState, type DragEvent } from "react"
import {
  Background, Controls, Handle, MiniMap, Position, ReactFlow, ReactFlowProvider, useReactFlow,
  type Connection, type Edge, type EdgeChange, type Node, type NodeChange, type NodeProps,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import { ArrowLeft, Plus, BookOpen, FlaskConical, GitBranch, Hand, Loader, Play, Save, Sparkles, Trash2, Undo2, Zap, type LucideIcon } from "lucide-react"
import { toast } from "sonner"
import { BlockFields, TriggerFields, type Declared, type Draft } from "@/components/routine-editor"
import { TRIGGER, autoLayout, compose, linksOf, problems, type Links, type Report } from "@/lib/routine-graph"
import { Button } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useTheme } from "@/hooks/use-theme"
import { api, called, enc, type Agent, type GraphBlock, type Routine } from "@/lib/api"
import { cn } from "@/lib/utils"

// The five blocks the founder drags in (CLAUDE.md); the trigger is always there.
const KINDS: { type: string; icon: LucideIcon; label: string; hint: string }[] = [
  { type: "read", icon: BookOpen, label: "Read", hint: "Fetch items from a source" },
  { type: "ai", icon: Sparkles, label: "AI", hint: "Sort, judge, draft: no tool" },
  { type: "condition", icon: GitBranch, label: "If", hint: "Then this way, else that way" },
  { type: "approval", icon: Hand, label: "You approve", hint: "Waits in Needs you" },
  { type: "act", icon: Zap, label: "Act", hint: "One declared action" },
]
const KIND = Object.fromEntries(KINDS.map((k) => [k.type, k]))

const DAYS: Record<string, string> = { mon: "Mondays", tue: "Tuesdays", wed: "Wednesdays", thu: "Thursdays", fri: "Fridays", sat: "Saturdays", sun: "Sundays" }

/** A schedule as the founder reads it: "weekdays 09:00" → "Weekdays at 09:00". */
export function schedule(every: string): string {
  const t = every.trim().toLowerCase()
  const m = t.match(/^every (\d+)\s*(m|min|mins|h|hr|hrs|d|day|days)$/)
  if (m) {
    const n = Number(m[1])
    const unit = m[2][0] === "m" ? "min" : m[2][0] === "h" ? (n === 1 ? "hour" : "hours") : (n === 1 ? "day" : "days")
    return n === 1 && unit !== "min" ? `Every ${unit}` : `Every ${n} ${unit}`
  }
  const at = t.match(/^(\S+) (\d{1,2}:\d{2})$/)
  if (!at) return every
  const days = ({ daily: "Every day", weekdays: "Weekdays", weekends: "Weekends" } as Record<string, string>)[at[1]]
    ?? at[1].split(",").map((d) => DAYS[d] ?? d).join(", ")
  return `${days} at ${at[2]}`
}

export function triggerText(t: Record<string, unknown>): string {
  if (typeof t.schedule === "string") return schedule(t.schedule)
  if (t.telegram) return t.telegram === "x_post" ? "When you send a post link on Telegram" : `On Telegram: ${String(t.telegram)}`
  if (t.webhook) return `On a webhook: ${String(t.webhook)}`
  return "By hand"
}

/** A block's title and one line on what it does. */
export function describe(b: GraphBlock): [string, string] {
  const s = (k: string) => (typeof b[k] === "string" ? (b[k] as string) : "")
  const n = (k: string) => (typeof b[k] === "number" ? (b[k] as number) : null)
  const list = (k: string) => (Array.isArray(b[k]) ? (b[k] as string[]) : [])
  const join = (xs: unknown[]) => xs.filter(Boolean).join(" · ")
  switch (b.type) {
    case "read":
      if (s("source") === "business") return ["Business records", s("records") === "week" ? "How the last 7 days went" : s("records")]
      if (s("source") === "rss") {
        const feeds = list("feeds").length
        return [`${feeds} news feed${feeds === 1 ? "" : "s"}`, join([n("max_age_days") && `last ${n("max_age_days")} days`, s("dedupe") && "no repeats", n("max") && `at most ${n("max")}`])]
      }
      if (s("source") === "x_post") return ["The post you sent", ""]
      return [s("source"), s("action") || s("url")]
    case "ai":
      return [b.id, join([s("prompt").replace(/^prompts\//, ""), b.over === "list" && "all at once", b.model === "fast" && "fast model", n("limit") && `best ${n("limit")}`])]
    case "condition": {
      const c = ((b.if as object | undefined) ?? {}) as { field?: string; op?: string; value?: unknown }
      return [b.id, `if ${c.field ?? ""} ${c.op ?? ""}${c.value === undefined ? "" : ` ${JSON.stringify(c.value)}`}`]
    }
    case "approval":
      return ["You, in Needs you", join([b.editable ? "you may edit" : "approve or reject", n("expires_after") && `expires after ${n("expires_after")} h`])]
    case "act":
      return [s("action") || "No action yet", b.id]
    default:
      return [b.id, ""]
  }
}

type NodeData = { kind: string; title: string; sub: string; wrong: boolean; ran?: { ok: boolean | null; text: string } }

function BlockNode({ data, selected }: NodeProps<Node<NodeData>>) {
  const trigger = data.kind === TRIGGER
  const k = KIND[data.kind]
  const Icon = trigger ? Play : k?.icon ?? Zap
  return (
    <div className={cn("w-52 rounded-xl border bg-card px-3 py-2.5 text-left shadow-sm",
      data.kind === "approval" && "border-amber-400/50 bg-amber-500/5",
      selected && "ring-2 ring-sky-500", data.wrong && "border-red-500 ring-2 ring-red-500/40")}>
      {!trigger && <Handle type="target" position={Position.Left} className="!size-3 !bg-muted-foreground" />}
      <div className="flex items-center gap-1.5 text-[10.5px] font-semibold tracking-wide text-muted-foreground uppercase">
        <Icon className="size-3.5" /> {trigger ? "Trigger" : k?.label ?? data.kind}
      </div>
      <div className="mt-0.5 truncate text-[13px] font-medium">{data.title}</div>
      {data.sub && <div className="truncate text-[11.5px] text-muted-foreground">{data.sub}</div>}
      {data.ran && <div className={cn("mt-1 font-mono text-[11px]", data.ran.ok === null ? "text-muted-foreground" : data.ran.ok ? "text-emerald-500" : "text-amber-500")}>{data.ran.text}</div>}
      {data.kind === "condition" ? (
        <>
          <Handle id="then" type="source" position={Position.Right} style={{ top: "35%" }} className="!size-3 !bg-emerald-500" />
          <span className="absolute top-[35%] -right-9 -translate-y-1/2 text-[10px] text-emerald-500">then</span>
          <Handle id="else" type="source" position={Position.Right} style={{ top: "75%" }} className="!size-3 !bg-rose-500" />
          <span className="absolute top-[75%] -right-8 -translate-y-1/2 text-[10px] text-rose-500">else</span>
        </>
      ) : <Handle type="source" position={Position.Right} className="!size-3 !bg-sky-500" />}
    </div>
  )
}
const NODE_TYPES = { block: BlockNode }

const NEW: Record<string, (d: { prompts: string[]; actions: Declared[] }) => Omit<GraphBlock, "id">> = {
  read: () => ({ type: "read", source: "business", records: "week" }),
  ai: ({ prompts }) => ({ type: "ai", prompt: prompts[0] ?? "prompts/" }),
  condition: () => ({ type: "condition", if: { field: "", op: "exists" } }),
  approval: () => ({ type: "approval", show: "answer" }),
  act: ({ actions }) => ({ type: "act", action: actions.find((a) => !a.id.startsWith("airth."))?.id ?? actions[0]?.id ?? "", fields: {} }),
}

/** A routine on a canvas (decision 91): the founder drags blocks in, links
    their outputs, and sets each one's parameters; saving writes the same
    file, checked as loading checks it. */
export function RoutineCanvas(props: CanvasProps) {
  // A file that is not a routine's shape cannot be drawn: drawing over it
  // would lose it.
  if (props.routine && !parse(props.routine.source)) {
    return (
      <div className="space-y-3 rounded-xl border bg-card p-4 text-sm">
        <Button size="sm" variant="ghost" onClick={props.onClose}><ArrowLeft /> Routines</Button>
        <p>routines/{props.routine.name}.json is not a routine's JSON (an agent, a trigger and a list of blocks), so it cannot be drawn. Fix it in Documents first.</p>
        {props.routine.error && <p className="font-mono text-xs break-words text-red-500">{props.routine.error}</p>}
      </div>
    )
  }
  return <ReactFlowProvider><Canvas {...props} /></ReactFlowProvider>
}

type CanvasProps = {
  realm: string; routine: Routine | null; agents: Agent[]; actions: Declared[]; prompts: string[]
  onClose: () => void; onSaved: (name: string) => Promise<void>
}

function parse(source: string): Draft | null {
  try {
    const d = JSON.parse(source) as Draft
    const ok = d && typeof d === "object" && typeof d.trigger === "object" && d.trigger !== null && Array.isArray(d.blocks)
      && d.blocks.every((b) => b && typeof b === "object" && typeof b.id === "string" && typeof b.type === "string")
    return ok ? d : null
  } catch { return null }
}

function Canvas({ realm, routine, agents, actions, prompts, onClose, onSaved }: CanvasProps) {
  const { theme } = useTheme()
  const flow = useReactFlow()
  // The file the draft was read from, and its version: a save is checked
  // against it, so a change made elsewhere meanwhile is refused, not lost.
  const [base, setBase] = useState(() => ({ draft: routine ? parse(routine.source) : null, version: routine?.version ?? "" }))
  const saved = base.draft
  const [name, setName] = useState(routine?.name ?? "")
  const [draft, setDraft] = useState<Draft>(() => saved ?? { description: "", agent: agents[0]?.name ?? "", trigger: { manual: true }, blocks: [] })
  const [links, setLinks] = useState<Links>(() => linksOf(draft))
  const [entry, setEntry] = useState<string | null>(() => draft.blocks[0]?.id ?? null)
  const [selected, setSelected] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [refused, setRefused] = useState<{ text: string; block?: string } | null>(null)
  const [dry, setDry] = useState<{ running: boolean; report?: Report; error?: string }>({ running: false })
  const [ran, setRan] = useState<{ running: boolean; report?: string; error?: string }>({ running: false })

  const composed = useMemo(() => compose(draft, links, entry), [draft, links, entry])
  const dirty = !routine || !saved || JSON.stringify(composed) !== JSON.stringify(saved)
  // A newer file replaces the canvas's copy, unless the founder changed it.
  if (routine && routine.version !== base.version && !dirty) {
    const fresh = parse(routine.source)
    if (fresh) {
      setBase({ draft: fresh, version: routine.version })
      setDraft(fresh); setLinks(linksOf(fresh)); setEntry(fresh.blocks[0]?.id ?? null)
    }
  }
  const issues = problems(draft, links, entry)
  const report = dirty ? undefined : dry.report
  const at = useMemo(() => ({ ...autoLayout(draft, links, entry), ...((draft.layout as Record<string, [number, number]> | undefined) ?? {}) }), [draft, links, entry])

  const nodes: Node<NodeData>[] = useMemo(() => [
    { id: TRIGGER, type: "block", position: { x: at[TRIGGER][0], y: at[TRIGGER][1] }, deletable: false, selected: selected === TRIGGER,
      data: { kind: TRIGGER, title: triggerText(draft.trigger), sub: ((x) => (x ? called(x) : draft.agent))(agents.find((a) => a.name === draft.agent)), wrong: false } },
    ...draft.blocks.map((b) => {
      const [title, sub] = describe(b)
      const r = report?.blocks.find((x) => x.id === b.id)
      return {
        id: b.id, type: "block", position: { x: at[b.id]?.[0] ?? 0, y: at[b.id]?.[1] ?? 0 }, selected: selected === b.id,
        data: { kind: b.type, title, sub, wrong: refused?.block === b.id,
          ran: report && (r ? { ok: !r.notes.length, text: `${r.items_in} → ${r.items_out} · ${r.secs.toFixed(1)} s` } : { ok: null, text: "not reached" }) },
      }
    }),
  ], [draft, at, selected, report, refused, agents])

  const edges: Edge[] = useMemo(() => {
    const out: Edge[] = []
    if (entry) out.push({ id: `${TRIGGER}->${entry}`, source: TRIGGER, target: entry })
    for (const [id, l] of Object.entries(links)) {
      if (l.out) out.push({ id: `${id}->${l.out}`, source: id, target: l.out })
      if (l.then) out.push({ id: `${id}:then->${l.then}`, source: id, sourceHandle: "then", target: l.then, label: "then", style: { stroke: "#10b981" } })
      if (l.else) out.push({ id: `${id}:else->${l.else}`, source: id, sourceHandle: "else", target: l.else, label: "else", style: { stroke: "#f43f5e" } })
    }
    return out.map((e) => ({ ...e, animated: false }))
  }, [links, entry])

  function edit(d: Draft) { setDraft(d); setRefused(null) }
  function place(id: string, x: number, y: number) {
    // Once one block is placed by hand, every one is written: a mix with
    // the automatic layout would overlap.
    edit({ ...draft, layout: { ...at, [id]: [Math.round(x), Math.round(y)] } })
  }

  function onNodesChange(changes: NodeChange<Node<NodeData>>[]) {
    for (const c of changes) {
      if (c.type === "position" && c.position && !c.dragging) place(c.id, c.position.x, c.position.y)
      if (c.type === "position" && c.position && c.dragging) {
        // Follow the pointer while dragging, without writing yet.
        setDraft((d) => ({ ...d, layout: { ...at, ...((d.layout as object) ?? {}), [c.id]: [c.position!.x, c.position!.y] } }))
      }
      if (c.type === "select" && c.selected) setSelected(c.id)
      if (c.type === "remove" && c.id !== TRIGGER) remove(c.id)
    }
  }

  function onEdgesChange(changes: EdgeChange[]) {
    for (const c of changes) {
      if (c.type !== "remove") continue
      const e = edges.find((x) => x.id === c.id)
      if (!e) continue
      if (e.source === TRIGGER) setEntry(null)
      else setLinks((ls) => ({ ...ls, [e.source]: { ...ls[e.source], [e.sourceHandle ?? "out"]: null } }))
      setRefused(null)
    }
  }

  function onConnect(c: Connection) {
    if (!c.source || !c.target || c.source === c.target || c.target === TRIGGER) return
    if (c.source === TRIGGER) setEntry(c.target)
    else setLinks((ls) => ({ ...ls, [c.source]: { ...ls[c.source], [c.sourceHandle ?? "out"]: c.target } }))
    setRefused(null)
  }

  function remove(id: string) {
    setDraft((d) => {
      const layout = { ...((d.layout as Record<string, [number, number]>) ?? {}) }
      delete layout[id]
      return { ...d, blocks: d.blocks.filter((b) => b.id !== id), layout: Object.keys(layout).length ? layout : undefined }
    })
    setLinks((ls) => {
      const next: Links = {}
      for (const [k, l] of Object.entries(ls)) {
        if (k === id) continue
        next[k] = Object.fromEntries(Object.entries(l).map(([h, t]) => [h, t === id ? null : t]))
      }
      return next
    })
    setEntry((e) => (e === id ? null : e))
    setSelected(null)
  }

  function rename(from: string, to: string) {
    if (!to || to === from || to === TRIGGER || draft.blocks.some((b) => b.id === to)) return
    const layout = Object.fromEntries(Object.entries((draft.layout as Record<string, [number, number]>) ?? {}).map(([k, v]) => [k === from ? to : k, v]))
    edit({ ...draft, blocks: draft.blocks.map((b) => (b.id === from ? { ...b, id: to } : b)), layout: draft.layout ? layout : undefined })
    setLinks((ls) => Object.fromEntries(Object.entries(ls).map(([k, l]) => [k === from ? to : k, Object.fromEntries(Object.entries(l).map(([h, t]) => [h, t === from ? to : t]))])))
    setEntry((e) => (e === from ? to : e))
    setSelected(to)
  }

  function add(type: string, x?: number, y?: number) {
    let id = type
    for (let n = 2; draft.blocks.some((b) => b.id === id); n++) id = `${type}-${n}`
    const block = { id, ...NEW[type]({ prompts, actions }) } as GraphBlock
    const last = draft.blocks[draft.blocks.length - 1]?.id ?? TRIGGER
    const [px, py] = x === undefined ? [(at[last]?.[0] ?? 0) + 260, at[last]?.[1] ?? 0] : [x, y!]
    edit({ ...draft, blocks: [...draft.blocks, block], layout: { ...at, [id]: [Math.round(px), Math.round(py)] } })
    setLinks((ls) => ({ ...ls, [id]: type === "condition" ? { then: null, else: null } : { out: null } }))
    if (!entry && !draft.blocks.length) setEntry(id)
    setSelected(id)
  }

  function onDrop(e: DragEvent) {
    e.preventDefault()
    const type = e.dataTransfer.getData("application/airth-block")
    if (!KIND[type]) return
    const p = flow.screenToFlowPosition({ x: e.clientX, y: e.clientY })
    add(type, p.x - 104, p.y - 30)
  }

  function discard() {
    if (!saved) return
    setDraft(saved); setLinks(linksOf(saved)); setEntry(saved.blocks[0]?.id ?? null); setRefused(null)
  }

  async function save() {
    if (issues.length) { setRefused({ text: issues[0] }); return }
    setSaving(true)
    try {
      // Indented as the files are, so git shows only what changed.
      const text = JSON.stringify(composed, null, 2) + (!routine || routine.source.endsWith("\n") ? "\n" : "")
      const done = routine
        ? await api<{ version: string }>(`realms/${enc(realm)}/graphs/${enc(routine.name)}`, { text, version: base.version }, "PUT")
        : await api<{ version: string }>(`realms/${enc(realm)}/graphs`, { name, text })
      setBase({ draft: composed, version: done.version })
      toast.success(`${name} saved. Commit it in the business folder to keep it in git.`)
      setRefused(null)
      await onSaved(name)
    } catch (e) {
      const text = (e as Error).message
      setRefused({ text, block: text.match(/(?:block|places) '([^']+)'/)?.[1] })
    } finally { setSaving(false) }
  }

  async function dryRun() {
    setDry({ running: true })
    try { setDry({ running: false, report: await api<Report>(`realms/${enc(realm)}/graphs/${enc(name)}/dry-run`, {}, undefined, 600000) }) }
    catch (e) { setDry({ running: false, error: (e as Error).message }) }
  }
  async function runNow() {
    setRan({ running: true })
    try {
      const out = await api<{ report: string }>(`realms/${enc(realm)}/graphs/${enc(name)}/run`, {}, undefined, 600000)
      setRan({ running: false, report: out.report }); toast.success(`${name} ran.`)
    } catch (e) { setRan({ running: false, error: (e as Error).message }) }
  }

  const block = draft.blocks.find((b) => b.id === selected)
  const busy = saving || dry.running || ran.running
  const validName = /^[a-z0-9][a-z0-9-]*$/.test(name)

  return (
    <div className="flex h-[calc(100svh-7.5rem)] min-h-[560px] flex-col overflow-hidden rounded-xl border bg-card">
      <header className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
        <Button size="icon-sm" variant="ghost" onClick={onClose} aria-label="Back to the routines"><ArrowLeft /></Button>
        {routine ? <span className="font-mono text-sm font-medium">{routine.name}</span>
          : <Input value={name} autoFocus placeholder="routine-name" spellCheck={false} className="h-8 w-48 font-mono text-[13px]" aria-label="Routine name"
              onChange={(e) => setName(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, "-"))} />}
        {/* On a narrow screen the palette is hidden: blocks are added from here. */}
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button size="sm" variant="outline" className="md:hidden" />}><Plus /> Block</DropdownMenuTrigger>
          <DropdownMenuContent>
            {KINDS.map((k) => <DropdownMenuItem key={k.type} onClick={() => add(k.type)}><k.icon /> {k.label}</DropdownMenuItem>)}
          </DropdownMenuContent>
        </DropdownMenu>
        {dirty && <span className="rounded-md bg-amber-500/15 px-1.5 py-0.5 text-[11px] text-amber-600 dark:text-amber-300">{routine ? "Not saved" : "New"}</span>}
        <span className="flex-1" />
        {routine && dirty && <Button size="sm" variant="ghost" disabled={saving} onClick={discard}><Undo2 /> Discard</Button>}
        <Button size="sm" disabled={!dirty || saving || !validName} onClick={save}>{saving ? <><Loader className="animate-spin" /> Saving…</> : <><Save /> Save</>}</Button>
        <Button size="sm" variant="outline" disabled={!routine || dirty || busy} onClick={dryRun} title={dirty ? "Save first: a dry run runs what is saved" : "Everything but the acts: nothing is sent"}>
          {dry.running ? <><Loader className="animate-spin" /> Dry…</> : <><FlaskConical /> Dry run</>}
        </Button>
        <Button size="sm" variant="outline" disabled={!routine || dirty || busy} onClick={runNow} title={dirty ? "Save first" : "Run it now, for real"}>
          {ran.running ? <><Loader className="animate-spin" /> Running…</> : <><Play /> Run now</>}
        </Button>
      </header>

      <div className="flex min-h-0 flex-1 flex-col md:flex-row">
        <aside className="hidden w-44 shrink-0 space-y-1.5 overflow-y-auto border-r p-2.5 md:block" aria-label="Blocks">
          <p className="px-1 pb-1 text-[11px] text-muted-foreground">Drag a block in, or click to add it.</p>
          {KINDS.map((k) => (
            <button key={k.type} type="button" draggable onClick={() => add(k.type)}
              onDragStart={(e) => { e.dataTransfer.setData("application/airth-block", k.type); e.dataTransfer.effectAllowed = "move" }}
              className={cn("flex w-full cursor-grab items-start gap-2 rounded-lg border bg-background px-2.5 py-2 text-left hover:bg-accent active:cursor-grabbing", k.type === "approval" && "border-amber-400/40")}>
              <k.icon className="mt-0.5 size-4 shrink-0" />
              <span><span className="block text-[13px] font-medium">{k.label}</span><span className="block text-[11px] leading-tight text-muted-foreground">{k.hint}</span></span>
            </button>
          ))}
        </aside>

        <div className="min-h-[300px] min-w-0 flex-1" onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "move" }} onDrop={onDrop}>
          <ReactFlow nodes={nodes} edges={edges} nodeTypes={NODE_TYPES} onNodesChange={onNodesChange} onEdgesChange={onEdgesChange} onConnect={onConnect}
            onPaneClick={() => setSelected(null)} colorMode={theme === "dark" ? "dark" : "light"} fitView fitViewOptions={{ padding: 0.25, maxZoom: 1, minZoom: 0.2 }} minZoom={0.2}
            deleteKeyCode={["Backspace", "Delete"]} proOptions={{ hideAttribution: true }} defaultEdgeOptions={{ type: "smoothstep" }}>
            <Background gap={20} size={1} />
            <Controls showInteractive={false} />
            <MiniMap pannable zoomable className="!hidden md:!block" />
          </ReactFlow>
        </div>

        <aside className="max-h-[45%] w-full shrink-0 space-y-4 overflow-y-auto border-t p-3.5 text-sm md:max-h-none md:w-80 md:border-t-0 md:border-l" aria-label="Details">
          {refused && <p className="rounded-lg bg-red-500/10 px-3 py-2 font-mono text-xs break-words text-red-600 dark:text-red-300">{refused.text}
            {refused.block && selected !== refused.block && <button className="mt-1 block underline" onClick={() => setSelected(refused.block!)}>Show {refused.block}</button>}</p>}
          {block ? (
            <>
              <div className="flex items-center gap-2">
                <div className="flex-1 space-y-1.5">
                  <Label className="text-xs">Block id</Label>
                  <Input key={block.id} defaultValue={block.id} spellCheck={false} className="font-mono text-[13px]"
                    onBlur={(e) => rename(block.id, e.target.value.trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-"))} />
                </div>
                <Button size="icon-sm" variant="ghost" className="mt-5" aria-label={`Remove ${block.id}`} onClick={() => remove(block.id)}><Trash2 /></Button>
              </div>
              <BlockFields block={block} actions={actions} prompts={prompts}
                onChange={(nb) => edit({ ...draft, blocks: draft.blocks.map((x) => (x.id === nb.id ? nb : x)) })} />
              {report && <BlockRun report={report} id={block.id} />}
            </>
          ) : selected === TRIGGER ? (
            <>
              <TriggerKind draft={draft} onChange={edit} />
              <TriggerFields draft={draft} onChange={edit} when={schedule} />
            </>
          ) : (
            <>
              <div className="space-y-1.5">
                <Label className="text-xs">Agent</Label>
                <Select value={draft.agent} onValueChange={(v) => edit({ ...draft, agent: String(v) })}>
                  <SelectTrigger className="w-full"><SelectValue>{(v: string) => ((x) => (x ? called(x) : v))(agents.find((a) => a.name === v))}</SelectValue></SelectTrigger>
                  <SelectContent>{agents.map((a) => <SelectItem key={a.name} value={a.name}>{called(a)}</SelectItem>)}</SelectContent>
                </Select>
              </div>
              <TriggerFields draft={draft} onChange={edit} when={schedule} />
              {issues.length > 0 && <ul className="space-y-1 text-xs text-amber-600 dark:text-amber-300">{issues.map((i) => <li key={i}>{i}</li>)}</ul>}
              <p className="text-xs text-muted-foreground">Drag from a block's right dot to another block to link them; select a link or a block and press Delete to remove it. A condition has two outputs, then and else.</p>
              {ran.error && <p className="rounded-lg bg-red-500/10 px-3 py-2 font-mono text-xs break-words text-red-600 dark:text-red-300">{ran.error}</p>}
              {ran.report && <p className="rounded-lg bg-muted/60 px-3 py-2 text-xs whitespace-pre-wrap text-muted-foreground">{ran.report}</p>}
              {dry.error && <p className="rounded-lg bg-red-500/10 px-3 py-2 font-mono text-xs break-words text-red-600 dark:text-red-300">{dry.error}</p>}
              {report && <p className="text-xs text-muted-foreground">Dry run: {report.cost.replace(/^cost: /, "")}. Choose a block to see what it did.</p>}
              {routine && (
                <div>
                  <div className="mb-1.5 text-xs font-medium">Last runs</div>
                  {routine.runs.length ? routine.runs.map((x) => (
                    <div key={x.at} className="flex items-center justify-between gap-2 text-xs" title={x.error ?? undefined}>
                      <span className="text-muted-foreground">{new Date(x.at).toLocaleString("en-GB", { weekday: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</span>
                      <span className={x.ok ? "text-emerald-500" : "text-red-500"}>{x.ok ? "Done" : "Failed"}</span>
                    </div>
                  )) : <p className="text-xs text-muted-foreground">No run yet.</p>}
                </div>
              )}
            </>
          )}
        </aside>
      </div>
    </div>
  )
}

/** The trigger's kind: a schedule, by hand, or a post sent on Telegram. */
function TriggerKind({ draft, onChange }: { draft: Draft; onChange: (d: Draft) => void }) {
  const t = draft.trigger
  const kind = typeof t.schedule === "string" ? "schedule" : t.telegram ? "telegram" : t.webhook ? "webhook" : "manual"
  const to: Record<string, Record<string, unknown>> = { schedule: { schedule: "mon 08:00" }, manual: { manual: true }, telegram: { telegram: "x_post" }, webhook: { webhook: typeof t.webhook === "string" ? t.webhook : "" } }
  const labels: Record<string, string> = { schedule: "On a schedule", manual: "By hand", telegram: "When you send a post on Telegram", webhook: "On a webhook" }
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">Starts</Label>
      <Select value={kind} onValueChange={(v) => onChange({ ...draft, trigger: to[String(v)] })}>
        <SelectTrigger className="w-full"><SelectValue>{(v: string) => labels[v]}</SelectValue></SelectTrigger>
        <SelectContent>{Object.entries(labels).map(([k, l]) => <SelectItem key={k} value={k}>{l}</SelectItem>)}</SelectContent>
      </Select>
      {kind === "webhook" && <Input value={String(t.webhook ?? "")} placeholder="webhook name" className="font-mono text-[13px]" onChange={(e) => onChange({ ...draft, trigger: { webhook: e.target.value } })} />}
    </div>
  )
}

function BlockRun({ report, id }: { report: Report; id: string }) {
  const ran = report.blocks.find((b) => b.id === id)
  if (!ran) return <p className="text-xs text-muted-foreground">{id} was not reached in this dry run.</p>
  return (
    <section className="space-y-1 rounded-lg bg-surface p-3" aria-label={`What ${id} did`}>
      <h3 className="mb-1.5 text-xs font-medium">What {id} did</h3>
      {ran.lines.map((l, i) => <pre key={`l${i}`} className="font-mono text-[11.5px] whitespace-pre-wrap">{l}</pre>)}
      {ran.notes.map((l, i) => <pre key={`n${i}`} className="font-mono text-[11.5px] whitespace-pre-wrap text-amber-600 dark:text-amber-400">{l}</pre>)}
      {!ran.lines.length && !ran.notes.length && <p className="text-xs text-muted-foreground">Nothing to say: its items went on.</p>}
    </section>
  )
}
