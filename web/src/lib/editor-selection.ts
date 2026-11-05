// What is selected in the Documents editor, kept apart from the editor so
// the page asks for it (⌘L) without loading CodeMirror.
type Sel = { text: string; from: number; to: number }
let read: (() => Sel | null) | null = null

export const setSelectionReader = (f: (() => Sel | null) | null) => { read = f }

/** The selected text and its first and last line, or null. */
export const selection = (): Sel | null => (read ? read() : null)
