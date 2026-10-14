// The code editor of the app's Documents (CodeMirror 6). build.sh bundles
// this file into ../vendor/editor.js, which defines window.AirthEditor.
// Colors come from the app's stylesheet (.tok-* classes and CSS variables),
// so the editor follows the light and dark themes.

import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { indentWithTab } from "@codemirror/commands";
import { syntaxHighlighting, StreamLanguage, indentUnit } from "@codemirror/language";
import { classHighlighter } from "@lezer/highlight";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { yaml } from "@codemirror/lang-yaml";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";
import { go } from "@codemirror/lang-go";
import { java } from "@codemirror/lang-java";
import { cpp } from "@codemirror/lang-cpp";
import { php } from "@codemirror/lang-php";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { swift } from "@codemirror/legacy-modes/mode/swift";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";

const LANGUAGES = [
  [/\.(m?js|cjs)$/i, () => javascript()],
  [/\.jsx$/i, () => javascript({ jsx: true })],
  [/\.(ts|mts|cts)$/i, () => javascript({ typescript: true })],
  [/\.tsx$/i, () => javascript({ jsx: true, typescript: true })],
  [/\.(json|jsonl)$/i, () => json()],
  [/\.(md|markdown)$/i, () => markdown()],
  [/\.py$/i, () => python()],
  [/\.rs$/i, () => rust()],
  [/\.(s?css)$/i, () => css()],
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
];

function languageFor(path) {
  const found = LANGUAGES.find(([re]) => re.test(path));
  return found ? found[1]() : [];
}

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px", backgroundColor: "var(--bg)", color: "var(--text)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.6" },
  ".cm-content": { caretColor: "var(--text)", padding: "8px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--text)" },
  ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--faint)", border: "none" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text)" },
  ".cm-activeLine": { backgroundColor: "var(--hover)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection":
    { backgroundColor: "var(--brand-soft) !important" },
  ".cm-selectionMatch": { backgroundColor: "var(--accent-soft)" },
  "&.cm-focused .cm-matchingBracket": { backgroundColor: "var(--brand-soft)", outline: "1px solid var(--border-strong)" },
  ".cm-foldPlaceholder": { backgroundColor: "var(--surface)", border: "1px solid var(--border)", color: "var(--muted)" },
  ".cm-panels": { backgroundColor: "var(--sidebar)", color: "var(--text)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--border)" },
  ".cm-searchMatch": { backgroundColor: "var(--warn-soft)", outline: "1px solid var(--border-strong)" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--brand-soft)" },
  ".cm-tooltip": { backgroundColor: "var(--panel)", border: "1px solid var(--border-strong)", color: "var(--text)" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--accent-soft)", color: "var(--text)" },
});

/**
 * An editor in `parent`. Options: text, path (picks the language),
 * state (to restore one), onChange(), onSave(), onAsk() and onSelection().
 */
function create(parent, options) {
  const o = options || {};
  // A tab coming back: its text, cursor and undo history as they were.
  if (o.state) return wrap(new EditorView({ parent, state: o.state }));
  const shortcuts = [indentWithTab];
  if (o.onSave) shortcuts.push({ key: "Mod-s", preventDefault: true, run: () => { o.onSave(); return true; } });
  if (o.onAsk) shortcuts.push({ key: "Mod-l", preventDefault: true, run: () => { o.onAsk(); return true; } });
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: o.text || "",
      extensions: [
        basicSetup,
        keymap.of(shortcuts),
        syntaxHighlighting(classHighlighter),
        indentUnit.of(/\.(go|mk)$|Makefile$/.test(o.path || "") ? "\t" : /\.(rs|py|java|kt|swift|c|h|cpp|hpp|cs|php)$/i.test(o.path || "") ? "    " : "  "),
        languageFor(o.path || ""),
        theme,
        EditorView.lineWrapping,
        EditorView.updateListener.of((u) => {
          if (u.docChanged && o.onChange) o.onChange();
          if (u.selectionSet && o.onSelection) o.onSelection();
        }),
      ],
    }),
  });
  return wrap(view);
}

function wrap(view) {
  return {
    view,
    /** Everything to restore the editor later (see `state` in create). */
    state: () => view.state,
    text: () => view.state.doc.toString(),
    /** Replace the whole text (e.g. the file changed on disk); keeps the cursor if it can. */
    setText(text) {
      const head = Math.min(view.state.selection.main.head, text.length);
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, selection: { anchor: head } });
    },
    /** The selected text and its lines (1-based), or null. */
    selection() {
      const r = view.state.selection.main;
      if (r.empty) return null;
      const doc = view.state.doc;
      // Selected down to the start of a line: that line is not part of it.
      const end = doc.lineAt(r.to);
      const to = end.from === r.to && r.to > r.from ? doc.lineAt(r.to - 1) : end;
      return { text: doc.sliceString(r.from, r.to).replace(/\n$/, ""), from: doc.lineAt(r.from).number, to: to.number };
    },
    /** Where the cursor is: line and column, 1-based. */
    cursor() {
      const head = view.state.selection.main.head;
      const line = view.state.doc.lineAt(head);
      return { line: line.number, col: head - line.from + 1, lines: view.state.doc.lines };
    },
    goToLine(n) {
      const line = view.state.doc.line(Math.max(1, Math.min(n, view.state.doc.lines)));
      view.dispatch({ selection: { anchor: line.from }, scrollIntoView: true });
      view.focus();
    },

    focus() { view.focus(); },
    destroy() { view.destroy(); },
  };
}

window.AirthEditor = { create, languages: LANGUAGES.map(([re]) => re) };
