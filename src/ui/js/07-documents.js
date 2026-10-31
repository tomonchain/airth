// ---------- documents ----------
//
// A small editor for the realm's files, in the spirit of a code editor: the
// files on the left, open files as tabs, an editor with syntax colors, ⌘S to
// save, ⌘P to go to a file, and ⌘L to ask Airth about the file or the
// selected lines. Tabs stay open (with unsaved changes) while you visit
// other pages.

const docsUi = { filter: "", ide: null, quick: null };
const DOC_IMAGE = /\.(png|jpe?g|gif|webp|svg|heic)$/i;
const DOC_PDF = /\.pdf$/i;
const DOC_MD = /\.(md|markdown)$/i;
const DOC_MAX_UPLOAD = 50 << 20;
const DOC_LANG = [
  [/\.(m?js|cjs|jsx)$/i, "JavaScript"], [/\.(ts|mts|tsx)$/i, "TypeScript"], [/\.jsonl?$/i, "JSON"], [DOC_MD, "Markdown"],
  [/\.py$/i, "Python"], [/\.rs$/i, "Rust"], [/\.s?css$/i, "CSS"], [/\.html?$/i, "HTML"], [/\.ya?ml$/i, "YAML"],
  [/\.sql$/i, "SQL"], [/\.(xml|svg|plist)$/i, "XML"], [/\.go$/i, "Go"], [/\.java$/i, "Java"], [/\.kt$/i, "Kotlin"],
  [/\.(c|h)$/i, "C"], [/\.(cc|cpp|hpp)$/i, "C++"], [/\.cs$/i, "C#"], [/\.php$/i, "PHP"], [/\.(sh|bash|zsh)$/i, "Shell"],
  [/\.toml$/i, "TOML"], [/\.swift$/i, "Swift"], [/\.rb$/i, "Ruby"], [/\.csv$/i, "CSV"],
];

const docHash = (realm, path) => `#/r/${enc(realm)}/documents` + (path ? "/" + enc(path) : "");
const docParent = (path) => path.split("/").slice(0, -1).join("/");
const docName = (path) => path.split("/").pop();
const docIconName = (name, dir) => (dir ? "folder" : DOC_IMAGE.test(name) ? "photo" : "file");
const docLang = (path) => (DOC_LANG.find(([re]) => re.test(path)) || [null, "Plain text"])[1];
const docKind = (path) => (DOC_PDF.test(path) ? "pdf" : DOC_IMAGE.test(path) ? "image" : "text");

function fmtSize(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1 << 20) return `${Math.round(n / 1024)} KB`;
  return `${(n / (1 << 20)).toFixed(n < 10 << 20 ? 1 : 0)} MB`;
}

/** A document's bytes, fetched with the API token. */
async function docBlob(realm, path) {
  const res = await fetch(`/api/${realmPath(realm)}/files/raw?path=${enc(path)}`, { headers: { "X-Airth-Token": token || "" } });
  if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error || res.statusText);
  return res.blob();
}

/** Add files to a folder of the realm's documents; never replaces one. */
async function uploadDocs(realm, dir, files) {
  const list = [...files];
  const big = list.filter((f) => f.size > DOC_MAX_UPLOAD);
  if (big.length) flash(`${big.map((f) => f.name).join(", ")}: too large (at most 50 MB)`, true);
  let added = 0;
  for (const f of list.filter((f) => f.size <= DOC_MAX_UPLOAD)) {
    try {
      const res = await fetch(`/api/${realmPath(realm)}/files/raw?dir=${enc(dir)}&name=${enc(f.name)}`,
        { method: "PUT", headers: { "X-Airth-Token": token || "" }, body: f });
      if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error || res.statusText);
      added++;
    } catch (e) { flash(`${f.name}: ${e.message}`, true); }
  }
  if (added) flash(added === 1 ? "Added 1 file" : `Added ${added} files`);
  return added;
}

async function openDoc(realm, path, reveal) {
  try { await api(`${realmPath(realm)}/files/open`, { path, reveal: !!reveal }); }
  catch (e) { flash(e.message, true); }
}

/** The editor's code, loaded the first time a file opens. */
let editorLoading = null;
function loadEditor() {
  if (window.AirthEditor) return Promise.resolve(window.AirthEditor);
  if (!editorLoading) {
    editorLoading = new Promise((resolve, reject) => {
      const s = h("script", { src: "/vendor/editor.js" });
      s.onload = () => resolve(window.AirthEditor);
      s.onerror = () => { editorLoading = null; reject(new Error("The editor could not load")); };
      document.head.append(s);
    });
  }
  return editorLoading;
}

/** The documents state of `realm`: open tabs, folders, the tree. */
function ideState(realm) {
  if (!docsUi.ide || docsUi.ide.realm !== realm) {
    ideStash();
    docsUi.ide = { realm, tabs: [], active: null, folder: "", open: new Set([""]), dirs: new Map(), home: null, el: null };
  }
  return docsUi.ide;
}

const ideTab = (path) => docsUi.ide && docsUi.ide.tabs.find((t) => t.path === path);
const ideActive = () => docsUi.ide && docsUi.ide.active && ideTab(docsUi.ide.active);
const ideMounted = () => !!(docsUi.ide && docsUi.ide.el && document.contains(docsUi.ide.el.root));
const ideDirty = () => !!docsUi.ide && docsUi.ide.tabs.some((t) => t.dirty);

/** Before the page is replaced: keep each editor's state, free the views. */
function ideStash() {
  const ide = docsUi.ide;
  if (!ide) return;
  for (const t of ide.tabs) {
    if (t.editor) { t.state = t.editor.state(); t.editor.destroy(); t.editor = null; }
  }
  ide.el = null;
}

window.addEventListener("beforeunload", (e) => { if (ideDirty()) { e.preventDefault(); e.returnValue = ""; } });

/** The entries of a folder, from the server (kept until a reload). */
async function ideDir(path, fresh) {
  const ide = docsUi.ide;
  if (!fresh && ide.dirs.has(path)) return ide.dirs.get(path);
  const data = await api(`${realmPath(ide.realm)}/files?path=${enc(path)}`);
  ide.dirs.set(path, data.entries);
  if (path === "") ide.home = data;
  return data.entries;
}

/** Whether `path` is a folder, a file, or nothing (null). */
async function ideKindOf(path) {
  if (!path) return "dir";
  if (ideTab(path)) return "file";
  try {
    const entries = await ideDir(docParent(path));
    const e = entries.find((x) => x.name === docName(path));
    return e ? (e.dir ? "dir" : "file") : null;
  } catch { return null; }
}

async function renderDocuments(main) {
  const realm = app.realm;
  const ide = ideState(realm);
  const tree = h("div", { class: "ide-tree", role: "tree", "aria-label": "Files" });
  const fileInput = h("input", { type: "file", multiple: true, hidden: true, onchange: async (e) => {
    if (await uploadDocs(realm, ideTargetFolder(), e.target.files)) ideRefresh();
    e.target.value = "";
  } });
  const newRow = h("div", { class: "ide-new", hidden: true });
  const side = h("aside", { class: "ide-side" },
    h("div", { class: "ide-side-head" },
      h("a", { class: "ide-root strong", href: docHash(realm, ""), title: "All the documents" }, realm),
      h("span", { class: "grow" }),
      h("button", { class: "ide-tool", type: "button", title: "Go to file (⌘P)", "aria-label": "Go to file", onclick: () => ideQuickOpen() }, icon("search")),
      h("button", { class: "ide-tool", type: "button", title: "New file", "aria-label": "New file", onclick: () => ideNew(false) }, icon("filePlus")),
      h("button", { class: "ide-tool", type: "button", title: "New folder", "aria-label": "New folder", onclick: () => ideNew(true) }, icon("folder")),
      h("button", { class: "ide-tool", type: "button", title: "Add files from your Mac", "aria-label": "Add files", onclick: () => fileInput.click() }, icon("plus")),
      h("button", { class: "ide-tool", type: "button", title: "Reload the files", "aria-label": "Reload the files", onclick: () => ideRefresh() }, icon("refresh")),
      fileInput),
    newRow, tree);
  const tabs = h("div", { class: "ide-tabs", role: "tablist", "aria-label": "Open files" });
  const body = h("div", { class: "ide-body" });
  const status = h("div", { class: "ide-status" });
  const root = h("div", { class: "ide" }, side, h("section", { class: "ide-main" }, tabs, body, status));
  main.append(root);
  ide.el = { root, tree, tabs, body, status, newRow };

  // Files dropped anywhere on the documents land in the current folder.
  let depth = 0;
  const hasFiles = (e) => e.dataTransfer && [...e.dataTransfer.types].includes("Files");
  root.addEventListener("dragenter", (e) => { if (hasFiles(e)) { depth++; root.classList.add("dropping"); } });
  root.addEventListener("dragleave", () => { if (--depth <= 0) { depth = 0; root.classList.remove("dropping"); } });
  root.addEventListener("dragover", (e) => { if (hasFiles(e)) e.preventDefault(); });
  root.addEventListener("drop", async (e) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth = 0;
    root.classList.remove("dropping");
    const files = [...e.dataTransfer.files].filter((f) => f.size > 0 || f.type);
    if (!files.length) return flash("Drop files, not folders", true);
    if (await uploadDocs(realm, ideTargetFolder(), files)) ideRefresh();
  });

  try { await ideDir("", true); } catch (e) { body.replaceChildren(h("div", { class: "doc-none" }, h("p", null, e.message))); return; }
  if (!ideMounted() || docsUi.ide !== ide) return;
  await ideRoute();
}

/** Where new and added files go: the folder shown, or the open file's. */
function ideTargetFolder() {
  const ide = docsUi.ide;
  return ide.active ? docParent(ide.active) : ide.folder;
}

/** Follow the address: a folder shows its files, a file opens in a tab. */
async function ideRoute() {
  const ide = docsUi.ide;
  const path = app.route.path || "";
  const kind = await ideKindOf(path);
  if (!ideMounted()) return;
  if (kind === "file") {
    for (let p = docParent(path); ; p = docParent(p)) { ide.open.add(p); if (!p) break; }
    await ideOpenTab(path);
  } else {
    if (kind === null) flash(`${path} is no longer there`, true);
    ide.active = null;
    ide.folder = kind === "dir" ? path : "";
    for (let p = ide.folder; ; p = docParent(p)) { ide.open.add(p); if (!p) break; }
  }
  await ideDrawAll();
}

async function ideDrawAll() {
  await ideDrawTree();
  ideDrawTabs();
  await ideDrawBody();
  ideDrawStatus();
}

/** Reload what the folders hold (after an upload, a new file, a chat). */
async function ideRefresh() {
  const ide = docsUi.ide;
  ide.dirs.clear();
  try { await ideDir("", true); } catch {}
  await ideDrawAll();
}

async function ideDrawTree() {
  const ide = docsUi.ide;
  if (!ideMounted()) return;
  const rows = [];
  const walk = async (dir, depth) => {
    let entries;
    try { entries = await ideDir(dir); } catch { return; }
    for (const e of entries) {
      const path = dir ? `${dir}/${e.name}` : e.name;
      const open = e.dir && ide.open.has(path);
      const tab = !e.dir && ideTab(path);
      rows.push(h("button", {
        class: "ide-row" + (path === ide.active ? " active" : "") + (e.dir && path === ide.folder && !ide.active ? " here" : ""),
        type: "button", role: "treeitem", title: path,
        "aria-expanded": e.dir ? String(open) : null,
        onclick: () => {
          if (!e.dir) return go(docHash(ide.realm, path));
          if (open) ide.open.delete(path); else ide.open.add(path);
          ideDrawTree();
        },
        ondblclick: () => { if (e.dir) go(docHash(ide.realm, path)); },
      },
        h("span", { class: "ide-chev" + (open ? " open" : "") }, e.dir ? icon("chevron") : null),
        h("span", { class: "doc-icon" + (e.dir ? " dir" : "") }, icon(docIconName(e.name, e.dir))),
        h("span", { class: "ide-row-name" }, e.name),
        tab && tab.dirty && h("span", { class: "ide-dot", title: "Unsaved changes" })));
      rows[rows.length - 1].style.setProperty("--depth", depth);
      if (open) await walk(path, depth + 1);
    }
  };
  await walk("", 0);
  if (!ideMounted()) return;
  ide.el.tree.replaceChildren(rows.length ? rows : h("div", { class: "muted small pad" }, "No files yet. Drop some here."));
}

function ideDrawTabs() {
  const ide = docsUi.ide;
  if (!ideMounted()) return;
  ide.el.tabs.replaceChildren(
    h("button", { class: "ide-tab home" + (ide.active ? "" : " active"), type: "button", role: "tab", title: "The folder's files",
      "aria-selected": String(!ide.active), onclick: () => go(docHash(ide.realm, ide.folder)) }, icon("folder")),
    ide.tabs.map((t) => {
      const same = ide.tabs.filter((o) => docName(o.path) === docName(t.path)).length > 1;
      return h("div", { class: "ide-tab" + (t.path === ide.active ? " active" : "") + (t.dirty ? " dirty" : ""), role: "tab",
        "aria-selected": String(t.path === ide.active), title: t.path,
        onmousedown: (e) => { if (e.button === 1) { e.preventDefault(); ideCloseTab(t.path); } },
        onclick: () => go(docHash(ide.realm, t.path)) },
        h("span", { class: "doc-icon" }, icon(docIconName(t.path, false))),
        h("span", { class: "ide-tab-name" }, docName(t.path), same && h("span", { class: "muted" }, " · " + (docParent(t.path) || ide.realm))),
        h("button", { class: "ide-tab-x", type: "button", "aria-label": `Close ${docName(t.path)}`,
          title: t.dirty ? "Unsaved changes" : "Close", onclick: (e) => { e.stopPropagation(); ideCloseTab(t.path); } }, "×"));
    }));
  const active = $(".ide-tab.active", ide.el.tabs);
  if (active) active.scrollIntoView({ block: "nearest", inline: "nearest" });
}

/** Open `path` in a tab (once) and show it. */
async function ideOpenTab(path) {
  const ide = docsUi.ide;
  let t = ideTab(path);
  if (!t) {
    t = { path, kind: docKind(path), mode: DOC_MD.test(path) ? "preview" : "edit", dirty: false, loading: true };
    ide.tabs.push(t);
    ide.active = path;
    ideDrawTabs();
    await ideLoad(t);
  }
  ide.active = path;
}

/** Fetch a tab's content: the text and its version, or the image. */
async function ideLoad(t) {
  const ide = docsUi.ide;
  t.loading = true;
  t.error = null;
  try {
    if (t.kind === "text") {
      const r = await api(`${realmPath(ide.realm)}/files/text?path=${enc(t.path)}`);
      t.text = r.text;
      t.version = r.version;
      t.state = null;
      if (t.editor) { t.editor.destroy(); t.editor = null; }
    } else if (t.kind === "image") {
      if (t.url) URL.revokeObjectURL(t.url);
      t.url = URL.createObjectURL(await docBlob(ide.realm, t.path));
    }
  } catch (e) { t.error = e.message; }
  t.loading = false;
}

function ideCloseTab(path) {
  const ide = docsUi.ide;
  const t = ideTab(path);
  if (!t) return;
  if (t.dirty && !t.confirmClose) {
    t.confirmClose = true;
    flash(`${docName(path)} has unsaved changes: close it again to throw them away`, true);
    setTimeout(() => { t.confirmClose = false; }, 4000);
    return;
  }
  if (t.editor) t.editor.destroy();
  if (t.url) URL.revokeObjectURL(t.url);
  const i = ide.tabs.indexOf(t);
  ide.tabs.splice(i, 1);
  if (ide.active !== path) { ideDrawTabs(); ideDrawTree(); return; }
  const next = ide.tabs[i] || ide.tabs[i - 1];
  go(docHash(ide.realm, next ? next.path : docParent(path)));
}

async function ideDrawBody() {
  const ide = docsUi.ide;
  if (!ideMounted()) return;
  const body = ide.el.body;
  // The editor is made again from its state (text, cursor, undo history);
  // only one lives at a time.
  for (const t of ide.tabs) {
    if (t.editor) { t.state = t.editor.state(); t.editor.destroy(); t.editor = null; }
  }
  const t = ideActive();
  if (!t) return ideDrawHome();
  if (t.loading) { body.replaceChildren(h("div", { class: "muted small pad" }, "Loading…")); return; }
  const banner = t.conflict && h("div", { class: "ide-banner" },
    h("span", { class: "grow" }, "This file changed on disk since you opened it, maybe in a task. Your changes are not saved."),
    h("button", { class: "btn small", type: "button", onclick: () => ideReload(t) }, "Load theirs"),
    h("button", { class: "btn small danger", type: "button", onclick: () => ideSave(t, true) }, "Keep mine"));
  if (t.error) {
    body.replaceChildren(h("div", { class: "doc-none" }, icon("file"), h("p", null, t.error),
      h("button", { class: "btn", type: "button", onclick: () => openDoc(ide.realm, t.path, false) }, "Open it on your Mac")));
    return;
  }
  if (t.kind === "image") {
    body.replaceChildren(h("div", { class: "doc-image ide-pane" }, h("img", { src: t.url, alt: docName(t.path) })));
    return;
  }
  if (t.kind === "pdf") {
    // The browser's PDF viewer needs a page of its own: a short-lived link.
    let link;
    try { link = await api(`${realmPath(ide.realm)}/files/preview`, { path: t.path }); }
    catch (e) { body.replaceChildren(h("div", { class: "doc-none" }, h("p", null, e.message))); return; }
    if (ideActive() === t) body.replaceChildren(h("iframe", { class: "doc-pdf ide-pane", src: link.url, title: docName(t.path) }));
    return;
  }
  if (t.mode === "preview") {
    const text = t.editor ? t.editor.text() : t.state ? t.state.doc.toString() : t.text;
    body.replaceChildren(banner, h("div", { class: "ide-pane ide-md" }, h("div", { class: "prose doc-md", html: md(text) })));
    return;
  }
  const host = h("div", { class: "ide-editor" });
  body.replaceChildren(banner, host);
  let Editor;
  try { Editor = await loadEditor(); } catch (e) { host.replaceChildren(h("div", { class: "doc-none" }, h("p", null, e.message))); return; }
  if (ideActive() !== t || !document.contains(host)) return;
  t.editor = Editor.create(host, {
    path: t.path,
    text: t.text,
    state: t.state,
    onChange: () => {
      const dirty = t.editor.text() !== t.text;
      if (dirty !== t.dirty) { t.dirty = dirty; ideDrawTabs(); ideDrawTree(); }
      ideDrawStatus();
    },
    onSelection: () => ideDrawStatus(),
    onSave: () => ideSave(t),
    onAsk: () => ideAsk(t),
  });
  t.state = null;
  if (t.line) { t.editor.goToLine(t.line); t.line = null; } else t.editor.focus();
}

/** No file open: the folder's files, and what Airth made lately. */
function ideDrawHome() {
  const ide = docsUi.ide;
  const realm = ide.realm;
  const path = ide.folder;
  const d = app.realmDetail;
  const entries = ide.dirs.get(path) || [];
  const home = ide.home;
  const parts = path ? path.split("/") : [];
  const listEl = h("div", { class: "doc-list" });
  const drawList = () => {
    const q = docsUi.filter.trim().toLowerCase();
    const shown = entries.filter((e) => !q || e.name.toLowerCase().includes(q));
    listEl.replaceChildren(shown.length ? shown.map((e) => {
      const rel = path ? `${path}/${e.name}` : e.name;
      return h("button", { class: "doc-row", type: "button", onclick: () => go(docHash(realm, rel)) },
        h("span", { class: "doc-icon" + (e.dir ? " dir" : "") }, icon(docIconName(e.name, e.dir))),
        h("span", { class: "doc-name grow" }, e.name),
        h("span", { class: "muted small doc-size" }, e.dir ? "" : fmtSize(e.size)),
        h("span", { class: "muted small doc-date" }, e.modified ? ago(e.modified) : ""));
    }) : h("div", { class: "doc-empty" },
      q ? h("p", { class: "muted" }, "Nothing here matches.")
        : [icon("folder"), h("p", { class: "strong" }, path ? "This folder is empty" : "No documents yet"),
          h("p", { class: "muted small" }, "Drop files here, or add them with +. Airth can read them, sum them up, compare them, and make new ones.")]));
  };
  const made = !path && home && home.made.length ? h("section", { class: "doc-made" },
    h("h2", null, "Made by Airth"),
    h("div", { class: "doc-made-grid" }, home.made.map((m) => h("div", { class: "doc-made-card" },
      h("a", { class: "doc-made-open", href: docHash(realm, m.path) },
        icon(m.edited ? "fileEdit" : "filePlus"),
        h("span", { class: "grow" }, h("span", { class: "doc-name" }, docName(m.path)),
          docParent(m.path) && h("span", { class: "muted small doc-in" }, docParent(m.path)))),
      h("div", { class: "muted small doc-from" }, m.edited ? "edited in " : "written in ",
        h("a", { href: `#/r/${enc(realm)}/run/${enc(m.run)}` }, m.run_title || "a task"), " · ", ago(m.at)))))) : null;
  ide.el.body.replaceChildren(h("div", { class: "ide-pane ide-home" }, h("div", { class: "page-inner" },
    h("h1", null, "Documents"),
    home && h("p", { class: "sub" }, home.own_root
      ? h("span", null, "The files in ", h("code", { class: "path", title: home.root }, tildePath(home.root)), ". Airth reads and works with them in this business's tasks.")
      : "This business's own folder. Add the files Airth should work with; what it makes appears here too.",
      d && d.isolation === "worktree" ? " What a task changes appears here once you adopt it." : ""),
    h("div", { class: "doc-bar" },
      h("nav", { class: "doc-crumbs", "aria-label": "Folder" },
        h("a", { href: docHash(realm, ""), class: parts.length ? "" : "here" }, icon("folder"), realm),
        parts.map((p, i) => [h("span", { class: "doc-sep", "aria-hidden": "true" }, "/"),
          h("a", { href: docHash(realm, parts.slice(0, i + 1).join("/")), class: i === parts.length - 1 ? "here" : "" }, p)])),
      h("div", { class: "doc-tools" },
        h("input", { type: "search", class: "doc-filter", placeholder: "Filter this folder", value: docsUi.filter, "aria-label": "Filter this folder",
          oninput: (e) => { docsUi.filter = e.target.value; drawList(); } }),
        h("button", { class: "btn", type: "button", title: "Show this folder in the Finder", onclick: () => openDoc(realm, path, true) }, icon("folder"), "Finder"))),
    made, listEl)));
  drawList();
}

function ideDrawStatus() {
  const ide = docsUi.ide;
  if (!ideMounted()) return;
  const t = ideActive();
  if (!t) {
    ide.el.status.replaceChildren(h("span", { class: "muted" }, "⌘P go to a file"));
    return;
  }
  const pos = t.editor && t.editor.cursor();
  const sel = t.editor && t.editor.selection();
  const trash = h("button", { class: "ide-st-btn danger", type: "button", title: "Move to the Trash", onclick: async () => {
    if (!t.confirmTrash) { t.confirmTrash = true; trash.textContent = "Click again to move it to the Trash"; setTimeout(() => { t.confirmTrash = false; ideDrawStatus(); }, 4000); return; }
    try { await api(`${realmPath(ide.realm)}/files/trash`, { path: t.path }); } catch (e) { return flash(e.message, true); }
    t.dirty = false;
    flash("Moved to the Trash");
    ideCloseTab(t.path);
    ideRefresh();
  } }, icon("trash"));
  ide.el.status.replaceChildren(
    h("span", { class: "ide-st-path", title: t.path }, t.path),
    t.kind === "text" && h("span", { class: "muted" }, docLang(t.path)),
    pos && h("span", { class: "muted" }, sel ? `${sel.to - sel.from + 1} line${sel.to > sel.from ? "s" : ""} selected` : `Ln ${pos.line}, Col ${pos.col}`),
    t.kind === "text" && h("span", { class: t.dirty ? "ide-st-dirty" : "muted" }, t.saving ? "Saving…" : t.dirty ? "Edited · ⌘S to save" : "Saved"),
    h("span", { class: "grow" }),
    t.kind === "text" && DOC_MD.test(t.path) && h("button", { class: "ide-st-btn", type: "button", onclick: () => ideMode(t, t.mode === "preview" ? "edit" : "preview") },
      icon(t.mode === "preview" ? "pencil" : "notes"), t.mode === "preview" ? "Edit" : "Preview"),
    h("button", { class: "ide-st-btn", type: "button", title: "Open with the Mac's app", onclick: () => openDoc(ide.realm, t.path, false) }, icon("window")),
    h("button", { class: "ide-st-btn", type: "button", title: "Show in the Finder", onclick: () => openDoc(ide.realm, t.path, true) }, icon("folder")),
    trash,
    h("button", { class: "ide-st-btn ask", type: "button", title: "Ask Airth about this file, or the selected lines (⌘L)", onclick: () => ideAsk(t) },
      icon("spark"), sel ? "Ask about the selection" : "Ask Airth", h("kbd", null, "⌘L")));
}

function ideMode(t, mode) {
  if (t.editor) { t.state = t.editor.state(); t.editor.destroy(); t.editor = null; }
  t.mode = mode;
  ideDrawBody().then(ideDrawStatus);
}

/** Save the active text: refused if the file changed meanwhile, unless `force`. */
async function ideSave(t, force) {
  const ide = docsUi.ide;
  if (t.kind !== "text" || t.saving) return;
  const text = t.editor ? t.editor.text() : t.state ? t.state.doc.toString() : t.text;
  let version = t.version;
  if (force) {
    try { version = (await api(`${realmPath(ide.realm)}/files/version?path=${enc(t.path)}`)).version; } catch (e) { return flash(e.message, true); }
  }
  t.saving = true;
  ideDrawStatus();
  try {
    const res = await fetch(`/api/${realmPath(ide.realm)}/files/text`, {
      method: "PUT", headers: { "X-Airth-Token": token || "", "Content-Type": "application/json" },
      body: JSON.stringify({ path: t.path, text, version }),
    });
    const data = await res.json().catch(() => ({}));
    if (res.status === 409 && data.changed) {
      t.conflict = true;
      flash("Not saved: the file changed on disk", true);
    } else if (!res.ok) {
      flash(data.error || res.statusText, true);
    } else {
      t.version = data.version;
      t.text = text;
      t.dirty = false;
      t.conflict = false;
      if (force) ideDrawBody();
    }
  } catch (e) { flash(e.message, true); }
  t.saving = false;
  if (ideMounted()) { ideDrawStatus(); ideDrawTabs(); ideDrawTree(); if (t.conflict) ideDrawBody(); }
}

/** Throw the editor's changes away and load the file as it is on disk. */
async function ideReload(t) {
  t.dirty = false;
  t.conflict = false;
  await ideLoad(t);
  await ideDrawAll();
}

/** Every few seconds: the open file changed on disk (a chat, another app)? */
async function ideWatch() {
  const ide = docsUi.ide;
  const t = ideActive();
  if (!t || t.kind !== "text" || t.saving || t.conflict || t.loading || !ideMounted() || document.hidden) return;
  let version;
  try { version = (await api(`${realmPath(ide.realm)}/files/version?path=${enc(t.path)}`)).version; } catch { return; }
  if (version === t.version || ideActive() !== t || t.saving) return;
  if (t.dirty) { t.conflict = true; ideDrawBody(); return; }
  // Unchanged here: show the new text, keeping the cursor.
  try {
    const r = await api(`${realmPath(ide.realm)}/files/text?path=${enc(t.path)}`);
    t.text = r.text;
    t.version = r.version;
    if (t.editor) t.editor.setText(r.text);
    else { t.state = null; ideDrawBody(); }
    flash(`${docName(t.path)} changed on disk: updated`);
  } catch {}
}
setInterval(ideWatch, 3000);

/** Start a chat about the file, or the selected lines (saved first). */
async function ideAsk(t) {
  if (t.dirty) await ideSave(t);
  const sel = t.editor && t.editor.selection();
  const lang = (t.path.match(/\.([a-z0-9]+)$/i) || [])[1] || "";
  app.prefill = sel
    ? `About \`${t.path}\` (line${sel.to > sel.from ? `s ${sel.from}–${sel.to}` : ` ${sel.from}`}):\n\`\`\`${lang}\n${sel.text.length > 4000 ? sel.text.slice(0, 4000) + "\n…" : sel.text}\n\`\`\`\n`
    : `About \`${t.path}\`: `;
  go(`#/r/${enc(docsUi.ide.realm)}`);
}

/** New file or folder: a name box at the top of the tree. */
function ideNew(folder) {
  const ide = docsUi.ide;
  const dir = ideTargetFolder();
  const input = h("input", { type: "text", class: "ide-new-input", placeholder: folder ? "Folder name" : "File name, e.g. notes.md", "aria-label": folder ? "New folder name" : "New file name" });
  const close = () => { ide.el.newRow.hidden = true; ide.el.newRow.replaceChildren(); };
  input.addEventListener("keydown", async (e) => {
    if (e.key === "Escape") return close();
    if (e.key !== "Enter" || !input.value.trim()) return;
    let made;
    try { made = await api(`${realmPath(ide.realm)}/files/new`, { dir, name: input.value, folder }); }
    catch (err) { return flash(err.message, true); }
    close();
    ide.dirs.delete(dir);
    if (folder) { ide.open.add(made.path); ide.folder = made.path; }
    go(docHash(ide.realm, made.path));
    if (folder) ideRefresh();
  });
  input.addEventListener("blur", () => setTimeout(close, 150));
  ide.el.newRow.hidden = false;
  ide.el.newRow.replaceChildren(h("span", { class: "doc-icon" + (folder ? " dir" : "") }, icon(folder ? "folder" : "file")),
    h("span", { class: "muted small" }, (dir || ide.realm) + "/"), input);
  input.focus();
}

/** ⌘P: find a file by name and open it. */
async function ideQuickOpen() {
  const ide = docsUi.ide;
  if (!ide || docsUi.quick) return;
  const input = h("input", { type: "text", class: "pal-input", placeholder: "Go to a file by name…", "aria-label": "File name" });
  const list = h("div", { class: "pal-list", role: "listbox" });
  let files = [];
  let shown = [];
  let at = 0;
  const close = () => { if (!docsUi.quick) return; docsUi.quick.remove(); docsUi.quick = null; };
  const pick = (p) => { close(); go(docHash(ide.realm, p)); };
  // Letters in order, closer together and in the name first.
  const score = (p, q) => {
    const s = p.toLowerCase();
    let i = 0, last = -1, gaps = 0;
    for (const c of q) {
      const j = s.indexOf(c, i);
      if (j < 0) return -1;
      if (last >= 0) gaps += j - last - 1;
      last = j; i = j + 1;
    }
    const name = docName(s);
    return 1000 - gaps - p.length / 10 + (name.includes(q) ? 500 : 0) + (name.startsWith(q) ? 200 : 0);
  };
  const draw = () => {
    const q = input.value.trim().toLowerCase().replace(/\s+/g, "");
    shown = (q ? files.map((p) => [p, score(p, q)]).filter(([, s]) => s >= 0).sort((a, b) => b[1] - a[1]).map(([p]) => p)
      : [...ide.tabs.map((t) => t.path), ...files.filter((p) => !ideTab(p))]).slice(0, 50);
    at = Math.min(at, Math.max(0, shown.length - 1));
    list.replaceChildren(shown.length ? shown.map((p, i) => h("button", { class: "pal-item" + (i === at ? " sel" : ""), type: "button", role: "option",
      onmousemove: () => { if (at !== i) { at = i; draw(); } }, onclick: () => pick(p) },
      h("span", { class: "doc-icon" }, icon(docIconName(p, false))),
      h("span", { class: "grow pal-label" }, docName(p), docParent(p) && h("span", { class: "muted small" }, "  " + docParent(p))))) :
      h("div", { class: "muted small pad" }, files.length ? "No file matches." : "Loading…"));
    const on = $(".pal-item.sel", list);
    if (on) on.scrollIntoView({ block: "nearest" });
  };
  input.addEventListener("input", () => { at = 0; draw(); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { e.preventDefault(); close(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); at = Math.min(at + 1, shown.length - 1); draw(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); at = Math.max(at - 1, 0); draw(); }
    else if (e.key === "Enter" && shown[at]) { e.preventDefault(); pick(shown[at]); }
  });
  const el = h("div", { class: "pal-overlay", onmousedown: (e) => { if (e.target === el) close(); } },
    h("div", { class: "pal", role: "dialog", "aria-modal": "true", "aria-label": "Go to file" }, h("div", { class: "pal-search" }, icon("search"), input), list));
  docsUi.quick = el;
  document.body.append(el);
  input.focus();
  draw();
  try {
    const r = await api(`${realmPath(ide.realm)}/files/all`);
    files = r.files;
    if (docsUi.quick === el) draw();
  } catch (e) { close(); flash(e.message, true); }
}

// ⌘S and ⌘P anywhere on the documents (the editor has its own ⌘S and ⌘L).
document.addEventListener("keydown", (e) => {
  if (app.route.view !== "documents" || !ideMounted() || !(e.metaKey || e.ctrlKey) || e.altKey) return;
  const k = e.key.toLowerCase();
  if (k === "p" && !e.shiftKey) { e.preventDefault(); ideQuickOpen(); }
  else if (k === "s" && ideActive()) { e.preventDefault(); ideSave(ideActive()); }
});
