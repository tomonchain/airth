// The Documents editor (decision 76): CodeMirror 6, with the languages the
// classic interface's editor had. Loaded when a text file opens. Its
// colours are the theme's, through the .tok-* classes in index.css.

import { useEffect, useRef } from "react"
import { indentWithTab } from "@codemirror/commands"
import { indentUnit, StreamLanguage, syntaxHighlighting } from "@codemirror/language"
import { EditorState, type Extension } from "@codemirror/state"
import { EditorView, keymap } from "@codemirror/view"
import { classHighlighter } from "@lezer/highlight"
import { basicSetup } from "codemirror"
import { cpp } from "@codemirror/lang-cpp"
import { css } from "@codemirror/lang-css"
import { go } from "@codemirror/lang-go"
import { html } from "@codemirror/lang-html"
import { java } from "@codemirror/lang-java"
import { javascript } from "@codemirror/lang-javascript"
import { json } from "@codemirror/lang-json"
import { markdown } from "@codemirror/lang-markdown"
import { php } from "@codemirror/lang-php"
import { python } from "@codemirror/lang-python"
import { rust } from "@codemirror/lang-rust"
import { sql } from "@codemirror/lang-sql"
import { xml } from "@codemirror/lang-xml"
import { yaml } from "@codemirror/lang-yaml"
import { ruby } from "@codemirror/legacy-modes/mode/ruby"
import { shell } from "@codemirror/legacy-modes/mode/shell"
import { swift } from "@codemirror/legacy-modes/mode/swift"
import { toml } from "@codemirror/legacy-modes/mode/toml"
import { setSelectionReader } from "@/lib/editor-selection"

const LANGUAGES: [RegExp, () => Extension][] = [
  [/\.(m?js|cjs)$/i, () => javascript()],
  [/\.jsx$/i, () => javascript({ jsx: true })],
  [/\.(ts|mts|cts)$/i, () => javascript({ typescript: true })],
  [/\.tsx$/i, () => javascript({ jsx: true, typescript: true })],
  [/\.(json|jsonl)$/i, () => json()],
  [/\.(md|markdown)$/i, () => markdown()],
  [/\.py$/i, () => python()],
  [/\.rs$/i, () => rust()],
  [/\.s?css$/i, () => css()],
  [/\.html?$/i, () => html()],
  [/\.ya?ml$/i, () => yaml()],
  [/\.sql$/i, () => sql()],
  [/\.(xml|svg|plist)$/i, () => xml()],
  [/\.go$/i, () => go()],
  [/\.(java|kt)$/i, () => java()],
  [/\.(c|h|cc|cpp|hpp|cs|m)$/i, () => cpp()],
  [/\.php$/i, () => php()],
  [/(\.(sh|bash|zsh)|(^|\/)(\.?[a-z]*rc|Makefile))$/i, () => StreamLanguage.define(shell)],
  [/\.toml$/i, () => StreamLanguage.define(toml)],
  [/\.swift$/i, () => StreamLanguage.define(swift)],
  [/\.rb$/i, () => StreamLanguage.define(ruby)],
]

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "12.5px", backgroundColor: "transparent", color: "var(--foreground)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-mono, ui-monospace, monospace)", lineHeight: "1.6" },
  ".cm-content": { caretColor: "var(--foreground)", padding: "12px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--foreground)" },
  ".cm-gutters": { backgroundColor: "transparent", color: "var(--muted-foreground)", border: "none", paddingLeft: "8px" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--foreground)" },
  ".cm-activeLine": { backgroundColor: "color-mix(in oklch, var(--accent) 45%, transparent)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection":
    { backgroundColor: "color-mix(in oklch, var(--color-sky-500) 28%, transparent) !important" },
  ".cm-selectionMatch": { backgroundColor: "color-mix(in oklch, var(--color-sky-500) 14%, transparent)" },
  "&.cm-focused .cm-matchingBracket": { backgroundColor: "color-mix(in oklch, var(--color-sky-500) 22%, transparent)" },
  ".cm-foldPlaceholder": { backgroundColor: "var(--muted)", border: "1px solid var(--border)", color: "var(--muted-foreground)" },
  ".cm-panels": { backgroundColor: "var(--popover)", color: "var(--foreground)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--border)" },
  ".cm-searchMatch": { backgroundColor: "color-mix(in oklch, var(--color-amber-500) 25%, transparent)" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "color-mix(in oklch, var(--color-sky-500) 30%, transparent)" },
  ".cm-tooltip": { backgroundColor: "var(--popover)", border: "1px solid var(--border)", color: "var(--foreground)" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--accent)", color: "var(--foreground)" },
})

export default function CodeEditor({ path, value, onChange, onCaret }: {
  path: string; value: string; onChange: (text: string) => void; onCaret: (line: number, col: number) => void
}) {
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)
  const change = useRef(onChange)
  const caret = useRef(onCaret)
  change.current = onChange
  caret.current = onCaret

  // One editor per file: its own undo history, made once.
  useEffect(() => {
    const lang = LANGUAGES.find(([re]) => re.test(path))?.[1]() ?? []
    const v = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup, keymap.of([indentWithTab]), indentUnit.of("  "), lang, syntaxHighlighting(classHighlighter), theme,
          EditorView.updateListener.of((u) => {
            if (u.docChanged) change.current(u.state.doc.toString())
            if (u.selectionSet || u.docChanged) {
              const head = u.state.selection.main.head
              const line = u.state.doc.lineAt(head)
              caret.current(line.number, head - line.from + 1)
            }
          }),
        ],
      }),
    })
    view.current = v
    // Documents asks the open editor for the selected lines (⌘L).
    const read = () => {
      const r = v.state.selection.main
      if (r.empty) return null
      const doc = v.state.doc
      return { text: doc.sliceString(r.from, r.to), from: doc.lineAt(r.from).number, to: doc.lineAt(r.to).number }
    }
    setSelectionReader(read)
    v.focus()
    return () => { setSelectionReader(null); v.destroy(); view.current = null }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path])

  // A new version from outside (an agent's change taken in): replace the text.
  useEffect(() => {
    const v = view.current
    if (v && v.state.doc.toString() !== value) v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } })
  }, [value])

  return <div ref={host} className="h-full min-h-full" />
}
