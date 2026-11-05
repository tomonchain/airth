import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react"
import { AppWindow, ChevronRight, Eye, File, FileImage, FilePlus, FileText, Folder, FolderOpen, FolderPlus, Pencil, Search, Sparkles, Trash2, Upload, X } from "lucide-react"
import { toast } from "sonner"
import { selection } from "@/lib/editor-selection"
import { Rich } from "@/components/rich"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { api, enc, raw } from "@/lib/api"
import { ago } from "@/lib/time"
import { cn } from "@/lib/utils"

type Entry = { name: string; dir: boolean; size: number; modified: string | null }
type Listing = { root: string; own_root: boolean; path: string; entries: Entry[]; made: { path: string; run: string; run_title: string | null; at: string; edited: boolean }[] }
type Tab = { path: string; kind: "text" | "image" | "pdf" | "other"; text?: string; saved?: string; version?: string; url?: string; error?: string; preview?: boolean; changed?: boolean }

// CodeMirror and its languages, fetched the first time a text file opens.
const CodeEditor = lazy(() => import("@/components/code-editor"))

const IMAGE = /\.(png|jpe?g|gif|webp|svg|heic)$/i
const PDF = /\.pdf$/i
const MD = /\.(md|markdown)$/i
// What is not shown as text: opened with the Mac's app instead.
const BINARY = /\.(zip|gz|tgz|dmg|pkg|docx?|xlsx?|pptx?|key|numbers|pages|mp3|mp4|mov|wav|woff2?|ttf|otf|sqlite|db)$/i
const MAX_UPLOAD = 50 << 20
const LANG: [RegExp, string][] = [
  [/\.(m?js|cjs|jsx)$/i, "JavaScript"], [/\.(ts|mts|tsx)$/i, "TypeScript"], [/\.jsonl?$/i, "JSON"], [MD, "Markdown"], [/\.py$/i, "Python"],
  [/\.rs$/i, "Rust"], [/\.s?css$/i, "CSS"], [/\.html?$/i, "HTML"], [/\.ya?ml$/i, "YAML"], [/\.sql$/i, "SQL"], [/\.toml$/i, "TOML"],
  [/\.(sh|bash|zsh)$/i, "Shell"], [/\.csv$/i, "CSV"], [/\.swift$/i, "Swift"], [/\.go$/i, "Go"],
]

const parent = (p: string) => p.split("/").slice(0, -1).join("/")
const base = (p: string) => p.split("/").pop() ?? p
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name)
const kindOf = (p: string): Tab["kind"] => (PDF.test(p) ? "pdf" : IMAGE.test(p) ? "image" : BINARY.test(p) ? "other" : "text")
const lang = (p: string) => LANG.find(([re]) => re.test(p))?.[1] ?? "Plain text"
function size(n: number) {
  if (n < 1024) return `${n} B`
  if (n < 1 << 20) return `${Math.round(n / 1024)} KB`
  return `${(n / (1 << 20)).toFixed(n < 10 << 20 ? 1 : 0)} MB`
}
const Icon = ({ name, dir, open, className }: { name: string; dir?: boolean; open?: boolean; className?: string }) => {
  const I = dir ? (open ? FolderOpen : Folder) : IMAGE.test(name) ? FileImage : MD.test(name) || /\.txt$/i.test(name) ? FileText : File
  return <I className={cn("size-4 shrink-0", dir ? "text-sky-500" : "text-muted-foreground", className)} />
}

/** The business's documents (the classic interface's): its folder as a tree,
    files open as tabs, text edited and saved in place (⌘S), Markdown, images
    and PDFs shown, files added by dropping them, ⌘P to go to a file. */
export function Documents({ realm, onAsk, start }: { realm: string; onAsk: (text: string) => void; start?: { path: string; at: number } | null }) {
  const base_ = `realms/${enc(realm)}/files`
  const [dirs, setDirs] = useState<Record<string, Listing>>({})
  const [expanded, setExpanded] = useState<Set<string>>(new Set([""]))
  const [folder, setFolder] = useState("")
  const [tabs, setTabs] = useState<Tab[]>([])
  const [active, setActive] = useState<string | null>(null)
  const [making, setMaking] = useState<"file" | "folder" | null>(null)
  const [quick, setQuick] = useState(false)
  const [drop, setDrop] = useState(false)
  const [warned, setWarned] = useState<string | null>(null)
  const upload = useRef<HTMLInputElement>(null)

  const load = useCallback(async (path: string) => {
    try {
      const d = await api<Listing>(`${base_}?path=${enc(path)}`)
      setDirs((x) => ({ ...x, [path]: d }))
    } catch (e) { toast.error((e as Error).message) }
  }, [base_])
  useEffect(() => { setDirs({}); setTabs([]); setActive(null); setFolder(""); setExpanded(new Set([""])); load("") }, [realm, load])
  // Opened from another page on a file (Company's "Fill in"): that file,
  // its folder unfolded.
  useEffect(() => {
    if (!start) return
    const dir = parent(start.path)
    setExpanded((x) => new Set([...x, ...dir.split("/").map((_, i, a) => a.slice(0, i + 1).join("/")), ""]))
    for (const d of ["", ...dir.split("/").map((_, i, a) => a.slice(0, i + 1).join("/"))]) if (d || !dirs[""]) load(d)
    open(start.path)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [start])

  // What the agents write appears without a reload.
  useEffect(() => {
    const t = setInterval(() => { if (document.visibilityState === "visible") for (const p of expanded) load(p) }, 15000)
    return () => clearInterval(t)
  }, [expanded, load])

  const tab = tabs.find((t) => t.path === active) ?? null
  const patch = (path: string, p: Partial<Tab>) => setTabs((ts) => ts.map((t) => (t.path === path ? { ...t, ...p } : t)))

  async function open(path: string) {
    setActive(path)
    setFolder(parent(path))
    if (tabs.some((t) => t.path === path)) return
    const kind = kindOf(path)
    setTabs((ts) => [...ts, { path, kind }])
    try {
      if (kind === "text") {
        const r = await api<{ text: string; version: string }>(`${base_}/text?path=${enc(path)}`)
        patch(path, { text: r.text, saved: r.text, version: r.version, preview: MD.test(path) })
      } else if (kind === "image") {
        patch(path, { url: URL.createObjectURL(await (await raw(`${base_}/raw?path=${enc(path)}`)).blob()) })
      } else if (kind === "pdf") {
        patch(path, { url: (await api<{ url: string }>(`${base_}/preview`, { path })).url })
      }
    } catch (e) { patch(path, { error: (e as Error).message }) }
  }
  function close(path: string) {
    const t = tabs.find((x) => x.path === path)
    // Unsaved changes: the first × warns, the second closes.
    if (t && t.text !== t.saved && warned !== path) { setWarned(path); toast.warning(`${base(path)} is not saved: ⌘S to save, or × again to close it without.`); return }
    setWarned(null)
    if (t?.url?.startsWith("blob:")) URL.revokeObjectURL(t.url)
    const rest = tabs.filter((x) => x.path !== path)
    setTabs(rest)
    if (active === path) setActive(rest.at(-1)?.path ?? null)
  }
  function openFolder(path: string) {
    setActive(null)
    setFolder(path)
    setExpanded((x) => new Set([...x, ...path.split("/").map((_, i, a) => a.slice(0, i + 1).join("/")), ""]))
    load(path)
  }
  function toggle(path: string) {
    setExpanded((x) => {
      const n = new Set(x)
      if (n.has(path)) n.delete(path)
      else { n.add(path); load(path) }
      return n
    })
  }

  async function save(t: Tab, force = false) {
    if (t.text === undefined) return
    try {
      let version = t.version
      if (force) version = (await api<{ version: string }>(`${base_}/version?path=${enc(t.path)}`)).version
      const r = await fetchSave(t.path, t.text, version ?? "")
      patch(t.path, { saved: t.text, version: r.version, changed: false })
      toast.success(`${base(t.path)} saved.`)
    } catch (e) {
      if ((e as Error).message === "changed") patch(t.path, { changed: true })
      else toast.error((e as Error).message)
    }
  }
  async function fetchSave(path: string, text: string, version: string) {
    const r = await raw(`${base_}/text`, { method: "PUT", body: JSON.stringify({ path, text, version }) }).catch((e: Error) => {
      if (/changed since you opened it/.test(e.message)) throw new Error("changed")
      throw e
    })
    return (await r.json()) as { version: string }
  }
  async function reload(t: Tab) {
    const r = await api<{ text: string; version: string }>(`${base_}/text?path=${enc(t.path)}`)
    patch(t.path, { text: r.text, saved: r.text, version: r.version, changed: false })
  }
  // An open file an agent changed: taken in when it has no edit of the
  // founder's, said when it has.
  useEffect(() => {
    if (!tab || tab.kind !== "text" || tab.version === undefined) return
    const t = setInterval(async () => {
      if (document.visibilityState !== "visible") return
      try {
        const v = (await api<{ version: string }>(`${base_}/version?path=${enc(tab.path)}`)).version
        if (v === tab.version) return
        if (tab.text === tab.saved) await reload(tab)
        else patch(tab.path, { changed: true })
      } catch { /* gone or unreadable: the next save says so */ }
    }, 4000)
    return () => clearInterval(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab?.path, tab?.version, tab?.text === tab?.saved])

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return
      if (e.key === "s" && tab?.kind === "text") { e.preventDefault(); save(tab) }
      if (e.key === "p") { e.preventDefault(); setQuick(true) }
      if (e.key === "l" && tab) { e.preventDefault(); ask(tab) }
    }
    addEventListener("keydown", key)
    return () => removeEventListener("keydown", key)
  })

  // ⌘L: ask the team about this file, or the lines selected in it.
  async function ask(t: Tab) {
    if (t.kind === "text" && t.text !== t.saved) await save(t)
    const sel = t.kind === "text" && !t.preview ? selection() : null
    const lang = t.path.match(/\.([a-z0-9]+)$/i)?.[1] ?? ""
    if (sel) {
      onAsk(`About \`${t.path}\` (line${sel.to > sel.from ? `s ${sel.from}–${sel.to}` : ` ${sel.from}`}):\n\`\`\`${lang}\n${sel.text.length > 4000 ? `${sel.text.slice(0, 4000)}\n…` : sel.text}\n\`\`\`\n`)
    } else onAsk(`About \`${t.path}\`: `)
  }

  async function add(files: FileList | File[], dir: string) {
    const list = [...files]
    const big = list.filter((f) => f.size > MAX_UPLOAD)
    if (big.length) toast.error(`${big.map((f) => f.name).join(", ")}: too large (at most 50 MB).`)
    let added = 0
    for (const f of list.filter((x) => x.size <= MAX_UPLOAD)) {
      try { await raw(`${base_}/raw?dir=${enc(dir)}&name=${enc(f.name)}`, { method: "PUT", body: f }); added++ }
      catch (e) { toast.error(`${f.name}: ${(e as Error).message}`) }
    }
    if (added) toast.success(added === 1 ? "Added 1 file." : `Added ${added} files.`)
    load(dir)
  }
  async function make(name: string) {
    const kind = making
    setMaking(null)
    if (!name.trim() || !kind) return
    try {
      const made = await api<{ path: string }>(`${base_}/new`, { dir: folder, name: name.trim(), folder: kind === "folder" })
      await load(folder)
      if (kind === "folder") openFolder(made.path)
      else open(made.path)
    } catch (e) { toast.error((e as Error).message) }
  }
  async function trash(path: string) {
    try {
      await api(`${base_}/trash`, { path })
      toast.success(`${base(path)} moved to the Trash.`)
      setTabs((ts) => ts.filter((x) => x.path !== path))
      if (active === path) setActive(null)
      load(parent(path))
    } catch (e) { toast.error((e as Error).message) }
  }
  const reveal = (path: string, finder: boolean) => api(`${base_}/open`, { path, reveal: finder }).catch((e: Error) => toast.error(e.message))

  const root = dirs[""]
  return (
    <div className="flex min-w-0 flex-1">
      <aside className="flex w-64 shrink-0 flex-col border-r">
        <div className="flex items-center gap-0.5 px-3 pt-3 pb-2">
          <h2 className="flex-1 text-sm font-semibold">Documents</h2>
          <Button size="icon-sm" variant="ghost" title="Go to a file (⌘P)" aria-label="Go to a file" onClick={() => setQuick(true)}><Search /></Button>
          <Button size="icon-sm" variant="ghost" title="New file" aria-label="New file" onClick={() => setMaking("file")}><FilePlus /></Button>
          <Button size="icon-sm" variant="ghost" title="New folder" aria-label="New folder" onClick={() => setMaking("folder")}><FolderPlus /></Button>
          <Button size="icon-sm" variant="ghost" title="Add files" aria-label="Add files" onClick={() => upload.current?.click()}><Upload /></Button>
          <input ref={upload} type="file" multiple hidden onChange={(e) => { if (e.target.files) add(e.target.files, folder); e.target.value = "" }} />
        </div>
        {making && <NewName kind={making} where={folder || realm} onDone={make} />}
        <div className="flex-1 overflow-y-auto px-1.5 pb-3 text-[13px]">
          <button onClick={() => openFolder("")} className={cn("flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 font-medium hover:bg-accent/60", !active && folder === "" && "bg-accent")}>
            <FolderOpen className="size-4 text-sky-500" /> <span className="capitalize">{realm}</span>
          </button>
          <Tree path="" depth={0} dirs={dirs} expanded={expanded} active={active} folder={active ? null : folder} onToggle={toggle} onFile={open} onFolder={openFolder} />
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        {tabs.length > 0 && (
          <div className="flex shrink-0 items-center gap-0.5 overflow-x-auto border-b px-2 pt-1.5">
            {tabs.map((t) => (
              <div key={t.path} className={cn("group flex h-8 shrink-0 items-center gap-1.5 rounded-t-lg border border-b-0 pr-1 pl-2.5 text-[13px]", t.path === active ? "bg-surface text-foreground" : "border-transparent text-muted-foreground hover:text-foreground")}>
                <button onClick={() => open(t.path)} title={t.path} className="flex items-center gap-1.5"><Icon name={t.path} /> {base(t.path)}{t.text !== t.saved && <span className="size-1.5 rounded-full bg-amber-500" aria-label="Not saved" />}</button>
                <button onClick={() => close(t.path)} aria-label={`Close ${base(t.path)}`} className="grid size-5 place-items-center rounded opacity-60 hover:bg-accent hover:opacity-100"><X className="size-3" /></button>
              </div>
            ))}
          </div>
        )}
        {tab ? (
          <FileView t={tab} onText={(text) => patch(tab.path, { text })} onPreview={(p) => patch(tab.path, { preview: p })}
            onSave={() => save(tab)} onOverwrite={() => save(tab, true)} onReload={() => reload(tab)}
            onApp={() => reveal(tab.path, false)} onFinder={() => reveal(tab.path, true)} onTrash={() => trash(tab.path)} onAsk={() => ask(tab)} />
        ) : (
          <div className={cn("relative min-h-0 flex-1 overflow-y-auto", drop && "bg-sky-500/5")}
            onDragOver={(e) => { if (e.dataTransfer.types.includes("Files")) { e.preventDefault(); setDrop(true) } }}
            onDragLeave={() => setDrop(false)}
            onDrop={(e) => { e.preventDefault(); setDrop(false); if (e.dataTransfer.files.length) add(e.dataTransfer.files, folder) }}>
            <FolderView realm={realm} path={folder} d={dirs[folder]} root={root} onFile={open} onFolder={openFolder} onFinder={() => reveal(folder, true)} onAdd={() => upload.current?.click()} />
            {drop && <div className="pointer-events-none absolute inset-3 grid place-items-center rounded-2xl border-2 border-dashed border-sky-500/60 text-sm text-sky-600 dark:text-sky-300">Drop to add to {folder || realm}</div>}
          </div>
        )}
      </section>
      <QuickOpen realm={realm} open={quick} onOpenChange={setQuick} onPick={(p) => { setQuick(false); open(p) }} />
    </div>
  )
}

function Tree({ path, depth, dirs, expanded, active, folder, onToggle, onFile, onFolder }: {
  path: string; depth: number; dirs: Record<string, Listing>; expanded: Set<string>; active: string | null; folder: string | null
  onToggle: (p: string) => void; onFile: (p: string) => void; onFolder: (p: string) => void
}) {
  const d = dirs[path]
  if (!d) return <p className="py-1 text-xs text-muted-foreground" style={{ paddingLeft: 22 + depth * 14 }}>…</p>
  return (
    <ul>
      {d.entries.map((e) => {
        const p = join(path, e.name)
        const open = expanded.has(p)
        return (
          <li key={p}>
            <div className={cn("flex items-center rounded-md hover:bg-accent/60", (active === p || folder === p) && "bg-accent")} style={{ paddingLeft: 4 + depth * 14 }}>
              {e.dir
                ? <button onClick={() => onToggle(p)} aria-label={open ? `Fold ${e.name}` : `Unfold ${e.name}`} className="grid size-5 place-items-center text-muted-foreground"><ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} /></button>
                : <span className="size-5" />}
              <button onClick={() => (e.dir ? onFolder(p) : onFile(p))} className="flex min-w-0 flex-1 items-center gap-1.5 py-1 pr-1.5 text-left" title={p}>
                <Icon name={e.name} dir={e.dir} open={open} /> <span className="truncate">{e.name}</span>
              </button>
            </div>
            {e.dir && open && <Tree path={p} depth={depth + 1} dirs={dirs} expanded={expanded} active={active} folder={folder} onToggle={onToggle} onFile={onFile} onFolder={onFolder} />}
          </li>
        )
      })}
    </ul>
  )
}

function NewName({ kind, where, onDone }: { kind: "file" | "folder"; where: string; onDone: (name: string) => void }) {
  const [v, setV] = useState("")
  return (
    <div className="mx-2 mb-2 flex items-center gap-1.5 rounded-md border bg-surface px-2 py-1">
      {kind === "folder" ? <Folder className="size-4 text-sky-500" /> : <File className="size-4 text-muted-foreground" />}
      <input autoFocus value={v} onChange={(e) => setV(e.target.value)} aria-label={kind === "folder" ? "New folder name" : "New file name"}
        placeholder={kind === "folder" ? "Folder name" : "notes.md"} className="min-w-0 flex-1 bg-transparent text-[13px] outline-none"
        onKeyDown={(e) => { if (e.key === "Enter") onDone(v); if (e.key === "Escape") onDone("") }} onBlur={() => setTimeout(() => onDone(""), 150)} />
      <span className="truncate text-[11px] text-muted-foreground">in {where}</span>
    </div>
  )
}

function FolderView({ realm, path, d, root, onFile, onFolder, onFinder, onAdd }: {
  realm: string; path: string; d?: Listing; root?: Listing; onFile: (p: string) => void; onFolder: (p: string) => void; onFinder: () => void; onAdd: () => void
}) {
  const [q, setQ] = useState("")
  const parts = path ? path.split("/") : []
  const shown = (d?.entries ?? []).filter((e) => !q || e.name.toLowerCase().includes(q.toLowerCase()))
  return (
    <div className="mx-auto max-w-4xl px-8 pt-8 pb-16">
      <p className="text-sm text-muted-foreground">
        {root?.own_root ? <>The files in <code className="font-mono text-xs" title={root.root}>{root.root.replace(/^\/Users\/[^/]+/, "~")}</code>. The agents read and work with them in this business's tasks.</>
          : "This business's own folder. Add the files the agents should work with; what they make appears here too."}
      </p>
      <div className="mt-5 flex items-center gap-2">
        <nav className="flex min-w-0 flex-1 items-center gap-1 text-sm" aria-label="Folder">
          <button onClick={() => onFolder("")} className={cn("capitalize hover:underline", !parts.length && "font-semibold")}>{realm}</button>
          {parts.map((p, i) => (
            <span key={i} className="flex items-center gap-1">
              <span className="text-muted-foreground">/</span>
              <button onClick={() => onFolder(parts.slice(0, i + 1).join("/"))} className={cn("hover:underline", i === parts.length - 1 && "font-semibold")}>{p}</button>
            </span>
          ))}
        </nav>
        <label className="flex h-8 items-center gap-1.5 rounded-lg bg-muted px-2.5 text-sm text-muted-foreground">
          <Search className="size-3.5" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter this folder" aria-label="Filter this folder" className="w-40 bg-transparent text-foreground outline-none" />
        </label>
        <Button size="sm" variant="outline" onClick={onAdd}><Upload /> Add files</Button>
        <Button size="sm" variant="outline" onClick={onFinder}><Folder /> Finder</Button>
      </div>

      {!path && !!root?.made.length && (
        <section className="mt-7">
          <h2 className="mb-2 flex items-center gap-1.5 text-sm font-semibold"><Sparkles className="size-4 text-muted-foreground" /> Made by the agents</h2>
          <div className="grid gap-2 sm:grid-cols-2">
            {root.made.map((m) => (
              <button key={`${m.path}${m.run}`} onClick={() => onFile(m.path)} className="flex items-start gap-2.5 rounded-xl border bg-bubble/40 px-3 py-2.5 text-left hover:bg-accent/60">
                <Icon name={m.path} className="mt-0.5" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{base(m.path)}</span>
                  <span className="block truncate text-xs text-muted-foreground">{m.edited ? "Edited" : "Written"} in {m.run_title ?? "a task"} · {ago(m.at)}</span>
                </span>
              </button>
            ))}
          </div>
        </section>
      )}

      <div className="mt-7 overflow-hidden rounded-xl border">
        {!d ? <p className="px-4 py-3 text-sm text-muted-foreground">Loading…</p> : shown.length ? shown.map((e) => {
          const p = join(path, e.name)
          return (
            <button key={p} onClick={() => (e.dir ? onFolder(p) : onFile(p))} className="flex w-full items-center gap-3 border-b px-4 py-2 text-left text-sm last:border-0 hover:bg-accent/50">
              <Icon name={e.name} dir={e.dir} />
              <span className="min-w-0 flex-1 truncate">{e.name}</span>
              <span className="w-16 text-right text-xs text-muted-foreground tabular-nums">{e.dir ? "" : size(e.size)}</span>
              <span className="w-20 text-right text-xs text-muted-foreground">{e.modified ? ago(e.modified) : ""}</span>
            </button>
          )
        }) : (
          <div className="flex flex-col items-center px-6 py-12 text-center">
            <Folder className="size-7 text-muted-foreground/60" />
            <p className="mt-3 text-sm font-medium">{q ? "Nothing here matches." : path ? "This folder is empty" : "No documents yet"}</p>
            {!q && <p className="mt-1 max-w-sm text-sm text-muted-foreground">Drop files here, or add them. The agents can read them, sum them up, compare them, and make new ones.</p>}
          </div>
        )}
      </div>
    </div>
  )
}

function FileView({ t, onText, onPreview, onSave, onOverwrite, onReload, onApp, onFinder, onTrash, onAsk }: {
  t: Tab; onText: (s: string) => void; onPreview: (p: boolean) => void; onSave: () => void; onOverwrite: () => void; onReload: () => void
  onApp: () => void; onFinder: () => void; onTrash: () => void; onAsk: () => void
}) {
  const [sure, setSure] = useState(false)
  const [pos, setPos] = useState({ line: 1, col: 1 })
  const dirty = t.text !== t.saved
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {t.changed && (
        <div className="flex shrink-0 items-center gap-2 border-b bg-amber-500/10 px-4 py-2 text-xs text-amber-700 dark:text-amber-300">
          <span className="flex-1">The file changed since you opened it{dirty ? ", and you have changes here" : ""}.</span>
          <Button size="xs" variant="outline" onClick={onReload}>Take the new version</Button>
          {dirty && <Button size="xs" variant="outline" onClick={onOverwrite}>Keep mine</Button>}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        {t.error ? <p className="p-6 text-sm text-red-500">{t.error}</p>
          : t.kind === "image" ? (t.url ? <div className="grid min-h-full place-items-center p-6"><img src={t.url} alt={base(t.path)} className="max-h-[75vh] max-w-full rounded-lg border" /></div> : null)
          : t.kind === "pdf" ? (t.url ? <iframe src={t.url} title={base(t.path)} className="size-full min-h-[70vh] border-0" /> : null)
          : t.kind === "other" ? (
            <div className="grid min-h-full place-items-center p-6 text-center">
              <div><File className="mx-auto size-8 text-muted-foreground/60" /><p className="mt-3 text-sm">{base(t.path)} is not shown here.</p>
                <Button className="mt-3" size="sm" variant="outline" onClick={onApp}><AppWindow /> Open with the Mac's app</Button></div>
            </div>
          ) : t.text === undefined ? <p className="p-6 text-sm text-muted-foreground">Loading…</p>
          : t.preview ? <div className="mx-auto max-w-3xl px-8 py-8 text-sm leading-relaxed"><Rich text={t.text} /></div>
          : (
            <div className="h-full">
              <Suspense fallback={<p className="p-6 text-sm text-muted-foreground">Loading the editor…</p>}>
                <CodeEditor key={t.path} path={t.path} value={t.text} onChange={onText} onCaret={(line, col) => setPos({ line, col })} />
              </Suspense>
            </div>
          )}
      </div>
      <div className="flex h-9 shrink-0 items-center gap-3 border-t px-3 text-xs text-muted-foreground">
        <span className="truncate font-mono" title={t.path}>{t.path}</span>
        {t.kind === "text" && <>
          <span>{lang(t.path)}</span>
          {!t.preview && <span className="tabular-nums">Ln {pos.line}, Col {pos.col}</span>}
          <span className={dirty ? "text-amber-600 dark:text-amber-400" : ""}>{dirty ? "Edited · ⌘S to save" : "Saved"}</span>
        </>}
        <span className="flex-1" />
        {t.kind === "text" && dirty && <Button size="xs" onClick={onSave}>Save</Button>}
        {t.kind === "text" && MD.test(t.path) && <Button size="xs" variant="ghost" onClick={() => onPreview(!t.preview)}>{t.preview ? <><Pencil /> Edit</> : <><Eye /> Preview</>}</Button>}
        <Button size="xs" variant="ghost" title="Ask the team about this file, or the selected lines (⌘L)" onClick={onAsk}><Sparkles /> Ask about it</Button>
        <Button size="icon-xs" variant="ghost" title="Open with the Mac's app" aria-label="Open with the Mac's app" onClick={onApp}><AppWindow /></Button>
        <Button size="icon-xs" variant="ghost" title="Show in the Finder" aria-label="Show in the Finder" onClick={onFinder}><Folder /></Button>
        {sure
          ? <><Button size="xs" variant="destructive" onClick={() => { setSure(false); onTrash() }}>Move to the Trash</Button><Button size="xs" variant="ghost" onClick={() => setSure(false)}>Keep</Button></>
          : <Button size="icon-xs" variant="ghost" title="Move to the Trash" aria-label="Move to the Trash" className="hover:text-red-500" onClick={() => setSure(true)}><Trash2 /></Button>}
      </div>
    </div>
  )
}

/** ⌘P: a file by its name, among all the business's files. */
function QuickOpen({ realm, open, onOpenChange, onPick }: { realm: string; open: boolean; onOpenChange: (o: boolean) => void; onPick: (p: string) => void }) {
  const [all, setAll] = useState<{ files: string[]; truncated: boolean } | null>(null)
  const [q, setQ] = useState("")
  const [i, setI] = useState(0)
  useEffect(() => {
    if (!open) return
    setQ(""); setI(0)
    api<{ files: string[]; truncated: boolean }>(`realms/${enc(realm)}/files/all`).then(setAll, (e: Error) => toast.error(e.message))
  }, [open, realm])
  const words = q.toLowerCase().split(/\s+/).filter(Boolean)
  const hits = (all?.files ?? []).filter((f) => words.every((w) => f.toLowerCase().includes(w)))
    .sort((a, b) => Number(!words.some((w) => base(b).toLowerCase().includes(w))) - Number(!words.some((w) => base(a).toLowerCase().includes(w))) || a.length - b.length)
    .slice(0, 12)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="top-[18%] translate-y-0 gap-0 p-0 sm:max-w-lg" showCloseButton={false}>
        <DialogTitle className="sr-only">Go to a file</DialogTitle>
        <div className="flex items-center gap-2 border-b px-3">
          <Search className="size-4 text-muted-foreground" />
          <input autoFocus value={q} onChange={(e) => { setQ(e.target.value); setI(0) }} placeholder="Go to a file" aria-label="Go to a file"
            className="h-11 flex-1 bg-transparent text-sm outline-none"
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setI((x) => Math.min(x + 1, hits.length - 1)) }
              if (e.key === "ArrowUp") { e.preventDefault(); setI((x) => Math.max(x - 1, 0)) }
              if (e.key === "Enter" && hits[i]) onPick(hits[i])
            }} />
        </div>
        <ul className="max-h-80 overflow-y-auto p-1.5">
          {!all ? <li className="px-3 py-2 text-sm text-muted-foreground">Loading…</li> : hits.length ? hits.map((f, n) => (
            <li key={f}>
              <button onClick={() => onPick(f)} onMouseEnter={() => setI(n)} className={cn("flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm", n === i && "bg-accent")}>
                <Icon name={f} /> <span className="truncate">{base(f)}</span> <span className="truncate text-xs text-muted-foreground">{parent(f)}</span>
              </button>
            </li>
          )) : <li className="px-3 py-2 text-sm text-muted-foreground">No file matches.</li>}
          {all?.truncated && <li className="px-3 py-1.5 text-xs text-muted-foreground">Only the first 5,000 files are searched.</li>}
        </ul>
      </DialogContent>
    </Dialog>
  )
}
