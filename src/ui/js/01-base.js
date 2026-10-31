"use strict";

// ---------- small helpers ----------

const $ = (sel, root = document) => root.querySelector(sel);

/** Build a DOM element. Strings become text nodes, so content is never parsed as HTML. */
function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === false || v == null) continue;
    if (k === "class") el.className = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "html") el.innerHTML = v; // only ever used with md(), which escapes, and the constant ICON markup
    else el.setAttribute(k, v === true ? "" : v);
  }
  // Conditional children (`cond && node`, maps of maps) may nest and may be
  // false, true, null or undefined: none of those is ever shown as text.
  for (const c of children.flat(Infinity)) {
    if (c == null || typeof c === "boolean") continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

// The DOM's own append/prepend/replaceChildren turn null, undefined and
// booleans into text ("null", "false"). The app passes conditional children
// to them everywhere (`cond && node`, helpers that return null), so filter
// those out once, here, the same way h() does.
if (typeof Element !== "undefined") {
  for (const name of ["append", "prepend", "replaceChildren"]) {
    const native = Element.prototype[name];
    Element.prototype[name] = function (...items) {
      return native.apply(this, items.flat(Infinity).filter((c) => c != null && typeof c !== "boolean"));
    };
  }
}

/** Icons: Tabler Icons (MIT, tabler.io/icons), path data embedded because the page's CSP
 *  allows no external assets. Drawn with a 1.5 stroke for a light, refined line. Constant
 *  markup, never built from data. */
const TB = (d) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg>`;
const ICON = {
  send: TB("M12 5v14m6-8l-6-6m-6 6l6-6"),
  mic: TB("M9 5a3 3 0 0 1 3-3a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3a3 3 0 0 1-3-3zM5 10a7 7 0 0 0 14 0M8 21h8m-4-4v4"),
  stop: '<svg viewBox="0 0 24 24"><path fill="currentColor" d="M17 4H7a3 3 0 0 0-3 3v10a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3V7a3 3 0 0 0-3-3"/></svg>',
  pencil: TB("M7 7H6a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2v-1M20.385 6.585a2.1 2.1 0 0 0-2.97-2.97L9 12v3h3zM16 5l3 3"),
  rename: TB("M4 20h4L18.5 9.5a2.828 2.828 0 1 0-4-4L4 16zm9.5-13.5l4 4"),
  inbox: TB("M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zM4 13h3l3 3h4l3-3h3"),
  clock: TB("M3 12a9 9 0 1 0 18 0a9 9 0 0 0-18 0M12 7v5l3 3"),
  spark: TB("M16 18a2 2 0 0 1 2 2a2 2 0 0 1 2-2a2 2 0 0 1-2-2a2 2 0 0 1-2 2m0-12a2 2 0 0 1 2 2a2 2 0 0 1 2-2a2 2 0 0 1-2-2a2 2 0 0 1-2 2M9 18a6 6 0 0 1 6-6a6 6 0 0 1-6-6a6 6 0 0 1-6 6a6 6 0 0 1 6 6"),
  gear: TB("M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 0 0 1.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 0 0-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 0 0-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 0 0-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 0 0-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 0 0 1.066-2.573c-.94-1.543.826-3.31 2.37-2.37c1 .608 2.296.07 2.572-1.065M9 12a3 3 0 1 0 6 0a3 3 0 0 0-6 0"),
  panel: TB("M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zm11-2v16"),
  sidebar: TB("M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zm5-2v16"),
  chevrons: TB("m8 9l4-4l4 4m0 6l-4 4l-4-4"),
  plus: TB("M12 5v14m-7-7h14"),
  sun: TB("M8 12a4 4 0 1 0 8 0a4 4 0 1 0-8 0m-5 0h1m8-9v1m8 8h1m-9 8v1M5.6 5.6l.7.7m12.1-.7l-.7.7m0 11.4l.7.7m-12.1-.7l-.7.7"),
  moon: TB("M12 3h.393a7.5 7.5 0 0 0 7.92 12.446A9 9 0 1 1 12 2.992z"),
  chart: TB("M3 13a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1zm12-4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1h-4a1 1 0 0 1-1-1zM9 5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-4a1 1 0 0 1-1-1zM4 20h14"),
  copy: TB("M7 9.667A2.667 2.667 0 0 1 9.667 7h8.666A2.667 2.667 0 0 1 21 9.667v8.666A2.667 2.667 0 0 1 18.333 21H9.667A2.667 2.667 0 0 1 7 18.333zM4.012 16.737A2 2 0 0 1 3 15V5c0-1.1.9-2 2-2h10c.75 0 1.158.385 1.5 1"),
  search: TB("M3 10a7 7 0 1 0 14 0a7 7 0 1 0-14 0m18 11l-6-6"),
  down: TB("M12 5v14m6-6l-6 6m-6-6l6 6"),
  chevron: TB("M6 9l6 6l6 -6"),
  plug: TB("m7 12l5 5l-1.5 1.5a3.536 3.536 0 1 1-5-5zm10 0l-5-5l1.5-1.5a3.536 3.536 0 1 1 5 5zM3 21l2.5-2.5m13-13L21 3m-11 8l-2 2m5 1l-2 2"),
  books: TB("M5 5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1zm4 0a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-2a1 1 0 0 1-1-1zM5 8h4m0 8h4m.803-11.44l2.184-.53c.562-.135 1.133.19 1.282.732l3.695 13.418a1.02 1.02 0 0 1-.634 1.219l-.133.041l-2.184.53c-.562.135-1.133-.19-1.282-.732L13.036 5.82a1.02 1.02 0 0 1 .634-1.219zM14 9l4-1m-2 8l3.923-.98"),
  download: TB("M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2M7 11l5 5l5-5m-5-7v12"),
  trash: TB("M4 7h16m-10 4v6m4-6v6M5 7l1 12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-12M9 7V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v3"),
  code: TB("M7 8l-4 4l4 4m10-8l4 4l-4 4M14 4l-4 16"),
  external: TB("M12 6H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6m-7 1l9-9m-5 0h5v5"),
  gitMerge: TB("M5 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0M5 6a2 2 0 1 0 4 0a2 2 0 1 0-4 0M15 12a2 2 0 1 0 4 0a2 2 0 1 0-4 0M7 8v8m0-8a4 4 0 0 0 4 4h4"),
  gitPull: TB("M4 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0M4 6a2 2 0 1 0 4 0a2 2 0 1 0-4 0M16 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0M6 8v8m5-10h5a2 2 0 0 1 2 2v8m-4-7l-3-3l3-3"),
  alert: TB("M12 9v4m-1.637-9.409L2.257 17.125a1.914 1.914 0 0 0 1.636 2.871h16.214a1.914 1.914 0 0 0 1.636-2.87L13.637 3.59a1.914 1.914 0 0 0-3.274 0zM12 16h.01"),
  play: TB("M7 4v16l13-8z"),
  archive: TB("M3 6a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8m-9 4h4"),
  refresh: TB("M20 11A8.1 8.1 0 0 0 4.5 9M4 5v4h4m-4 4a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4"),
  check: TB("m5 12l5 5L20 7"),
  x: TB("M18 6L6 18M6 6l12 12"),
  dots: TB("M4 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0m7 0a1 1 0 1 0 2 0a1 1 0 1 0-2 0m7 0a1 1 0 1 0 2 0a1 1 0 1 0-2 0"),
  // Tools
  file: TB("M14 3v4a1 1 0 0 0 1 1h4M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2M9 9h1m-1 4h6m-6 4h6"),
  photo: TB("M15 8h.01M3 6a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3v12a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3zm0 10l5-5c.928-.893 2.072-.893 3 0l5 5m-2-2l1-1c.928-.893 2.072-.893 3 0l3 3"),
  folder: TB("m5 19l2.757-7.351A1 1 0 0 1 8.693 11H21a1 1 0 0 1 .986 1.164l-.996 5.211A2 2 0 0 1 19.026 19za2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v2"),
  filePlus: TB("M14 3v4a1 1 0 0 0 1 1h4M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2m-5-10v6m-3-3h6"),
  fileEdit: TB("M14 3v4a1 1 0 0 0 1 1h4M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2m-7-3l5-5a1.414 1.414 0 0 0-2-2l-5 5v2z"),
  terminal: TB("m8 9l3 3l-3 3m5 0h3M3 6a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"),
  brain: TB("M15.5 13a3.5 3.5 0 0 0-3.5 3.5v1a3.5 3.5 0 0 0 7 0v-1.8M8.5 13a3.5 3.5 0 0 1 3.5 3.5v1a3.5 3.5 0 0 1-7 0v-1.8m12.5.3a3.5 3.5 0 0 0 0-7H17m2 .3V6.5a3.5 3.5 0 0 0-7 0M6.5 16a3.5 3.5 0 0 1 0-7H7m-2 .3V6.5a3.5 3.5 0 0 1 7 0v10"),
  world: TB("M3 12a9 9 0 1 0 18 0a9 9 0 0 0-18 0m.6-3h16.8M3.6 15h16.8M11.5 3a17 17 0 0 0 0 18m1-18a17 17 0 0 1 0 18"),
  listCheck: TB("M3.5 5.5L5 7l2.5-2.5m-4 7L5 13l2.5-2.5m-4 7L5 19l2.5-2.5M11 6h9m-9 6h9m-9 6h9"),
  calendar: TB("M4 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zm12-4v4M8 3v4m-4 4h16m-9 4h1m0 0v3"),
  checkbox: TB("m9 11l3 3l8-8m0 6v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h9"),
  notes: TB("M5 5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2zm4 2h6m-6 4h6m-6 4h4"),
  mail: TB("M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zm0 0l9 6l9-6"),
  compass: TB("m8 16l2-6l6-2l-2 6zM3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0m9-9v2m0 14v2m-9-9h2m14 0h2"),
  clipboard: TB("M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2a2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2m0 7h6m-6 4h6"),
  window: TB("M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zm3 1h.01M9 8h.01"),
  tool: TB("M7 10h3V7L6.5 3.5a6 6 0 0 1 8 8l6 6a2 2 0 0 1-3 3l-6-6a6 6 0 0 1-8-8z"),
  flask: TB("M9 3h6m-5 6h4m-4-6v6L6 20a.7.7 0 0 0 .5 1h11a.7.7 0 0 0 .5-1L14 9V3"),
  star: TB("m12 17.75l-6.172 3.245l1.179-6.873l-5-4.867l6.9-1l3.086-6.253l3.086 6.253l6.9 1l-5 4.867l1.179 6.873z"),
};

/** The icon of each tool the agent can call (MCP tools get a plug). */
const TOOL_ICONS = {
  read_file: "file", list_directory: "folder", write_file: "filePlus", edit_file: "fileEdit", execute_command: "terminal",
  remember: "brain", http_request: "world", update_plan: "listCheck", calendar: "calendar", reminders: "checkbox",
  notes: "notes", mail_draft: "mail", safari: "compass", clipboard: "clipboard", open_on_mac: "window",
  propose_skill: "spark", test_skill: "flask", use_skill: "books",
};
const toolIcon = (name) => icon(TOOL_ICONS[name] || (name.startsWith("mcp__") ? "plug" : "tool"));

/** Day/night: theme.js (loaded in <head>) owns the choice; this is the toggle. */
function themeToggle() {
  const dark = window.airthTheme && window.airthTheme.current() === "dark";
  const label = dark ? "Switch to light mode" : "Switch to dark mode";
  return h("button", { class: "icon-action theme-toggle", type: "button", title: label, "aria-label": label,
    onclick: (e) => { window.airthTheme && window.airthTheme.set(dark ? "light" : "dark", e); renderFoot(); } },
    icon(dark ? "sun" : "moon"));
}
const icon = (name) => h("span", { class: "i", "aria-hidden": "true", html: ICON[name] });

const esc = (s) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function inline(s) {
  return s
    .split(/(`[^`\n]+`)/g)
    .map((part) => {
      if (/^`[^`\n]+`$/.test(part)) return "<code>" + esc(part.slice(1, -1)) + "</code>";
      return esc(part)
        .replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
        .replace(/(^|[\s(])\*([^*\n]+)\*(?=[\s).,:;!?]|$)/g, "$1<em>$2</em>")
        .replace(/~~([^~\n]+)~~/g, "<del>$1</del>")
        .replace(
          /\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g,
          '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>'
        )
        // Bare links (only after a space or an opening bracket, so never inside the anchors
        // above); they stop before trailing punctuation and escaped quotes.
        .replace(
          /(^|[\s(])(https?:\/\/(?:(?!&quot;|&#39;|&gt;)[^\s<])+?)(?=[.,:;!?)]*(?:[\s<]|$|&quot;|&#39;|&gt;))/g,
          '$1<a href="$2" target="_blank" rel="noopener noreferrer">$2</a>'
        );
    })
    .join("");
}

const CODE_COPY = '<button class="code-copy" type="button" aria-label="Copy code">Copy</button>';

/** Minimal Markdown → HTML. All text is escaped before any markup is added. */
function md(src) {
  const lines = String(src || "").replace(/\r\n/g, "\n").split("\n");
  const out = [];
  const isBullet = (l) => /^\s*[-*]\s+/.test(l);
  const isNum = (l) => /^\s*\d+[.)]\s+/.test(l);
  const isFence = (l) => /^\s*```/.test(l);
  const isHeading = (l) => /^#{1,6}\s+/.test(l);
  const isRule = (l) => /^\s*([-*_])(\s*\1){2,}\s*$/.test(l);
  const isQuote = (l) => /^\s*>/.test(l);
  const isRow = (l) => /^\s*\|.*\|\s*$/.test(l);
  const isTableStart = (i) => isRow(lines[i]) && i + 1 < lines.length && /^\s*\|(\s*:?-{2,}:?\s*\|)+\s*$/.test(lines[i + 1]);
  const cells = (l) => l.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
  const startsBlock = (i) => isFence(lines[i]) || isHeading(lines[i]) || isBullet(lines[i]) || isNum(lines[i]) ||
    isRule(lines[i]) || isQuote(lines[i]) || isTableStart(i);
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (isFence(line)) {
      const lang = (line.match(/^\s*```\s*([\w+#.-]*)/) || [])[1] || "";
      const buf = [];
      i++;
      while (i < lines.length && !isFence(lines[i])) buf.push(lines[i++]);
      i++;
      out.push(`<div class="code"><div class="code-head"><span>${esc(lang || "code")}</span>${CODE_COPY}</div><pre><code>${esc(buf.join("\n"))}</code></pre></div>`);
    } else if (isTableStart(i)) {
      const head = cells(line);
      const align = cells(lines[i + 1]).map((c) => (c.endsWith(":") ? (c.startsWith(":") ? "center" : "right") : ""));
      const td = (tag, c, k) => `<${tag}${align[k] ? ` class="al-${align[k]}"` : ""}>${inline(c)}</${tag}>`;
      i += 2;
      const rows = [];
      while (i < lines.length && isRow(lines[i])) rows.push(cells(lines[i++]));
      out.push('<div class="table-wrap"><table><thead><tr>' + head.map((c, k) => td("th", c, k)).join("") + "</tr></thead><tbody>" +
        rows.map((r) => "<tr>" + head.map((_, k) => td("td", r[k] || "", k)).join("") + "</tr>").join("") + "</tbody></table></div>");
    } else if (isRule(line)) {
      out.push("<hr>");
      i++;
    } else if (isQuote(line)) {
      const buf = [];
      while (i < lines.length && isQuote(lines[i])) buf.push(lines[i++].replace(/^\s*>\s?/, ""));
      out.push("<blockquote>" + md(buf.join("\n")) + "</blockquote>");
    } else if (isHeading(line)) {
      const m = line.match(/^(#{1,6})\s+(.*)$/);
      const level = Math.min(m[1].length + 1, 4);
      out.push(`<h${level}>${inline(m[2])}</h${level}>`);
      i++;
    } else if (isBullet(line) || isNum(line)) {
      const ordered = isNum(line);
      const items = [];
      while (i < lines.length && (isBullet(lines[i]) || isNum(lines[i]) || /^\s{2,}\S/.test(lines[i]))) {
        const l = lines[i];
        if (isBullet(l) || isNum(l)) items.push(l.replace(/^\s*(?:[-*]|\d+[.)])\s+/, ""));
        else items[items.length - 1] += " " + l.trim();
        i++;
      }
      const tag = ordered ? "ol" : "ul";
      const item = (t) => {
        const task = t.match(/^\[( |x|X)\]\s+(.*)$/);
        return task ? `<li class="task${task[1] === " " ? "" : " checked"}">${inline(task[2])}</li>` : "<li>" + inline(t) + "</li>";
      };
      out.push(`<${tag}>` + items.map(item).join("") + `</${tag}>`);
    } else if (!line.trim()) {
      i++;
    } else {
      const buf = [];
      while (i < lines.length && lines[i].trim() && !startsBlock(i)) buf.push(lines[i++]);
      if (!buf.length) buf.push(lines[i++]);
      out.push("<p>" + buf.map(inline).join("<br>") + "</p>");
    }
  }
  return out.join("");
}

/** md() with a cache: the transcript is rebuilt on every poll, its texts rarely change. */
const mdCache = new Map();
function mdc(src) {
  let html = mdCache.get(src);
  if (html === undefined) {
    html = md(src);
    if (mdCache.size > 600) mdCache.clear();
    mdCache.set(src, html);
  }
  return html;
}

function ago(iso) {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 45) return "just now";
  if (s < 3600) return Math.round(s / 60) + "m ago";
  if (s < 86400) return Math.round(s / 3600) + "h ago";
  return Math.round(s / 86400) + "d ago";
}

const kfmt = (n) => (n >= 1e6 ? (n / 1e6).toFixed(1) + "M" : n >= 1e3 ? (n / 1e3).toFixed(1) + "k" : String(n));
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
const modKey = isMac ? "⌘" : "Ctrl";

// ---------- API ----------

let token = null;
try { token = sessionStorage.getItem("airth-token"); } catch {}
{
  const m = location.hash.match(/token=([0-9a-f]+)/);
  if (m) {
    token = m[1];
    try { sessionStorage.setItem("airth-token", token); } catch {}
    history.replaceState(null, "", "#/");
  }
}

class AuthError extends Error {}

async function api(path, body, method) {
  method = method || (body === undefined ? "GET" : "POST");
  const res = await fetch("/api/" + path, {
    method,
    headers: { "X-Airth-Token": token || "", "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 401) throw new AuthError("unauthorized");
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

const enc = encodeURIComponent;
const realmPath = (r) => `realms/${enc(r)}`;
const runPath = (r, id) => `realms/${enc(r)}/runs/${enc(id)}`;

// ---------- app state ----------

const app = {
  info: null,
  realms: [],
  realm: null, // current realm name
  realmDetail: null, // settings of the current realm
  runs: [],
  showArchived: false, // the sidebar lists archived chats instead
  route: { view: "home" },
  run: null, // last full response for the current run
  version: null,
  diff: null, // { runId, text } once loaded
  openKeys: new Set(),
  reason: "",
  error: null,
  changeError: null,
  busy: false,
  unauthorized: !token,
};

function parseRoute() {
  const h = location.hash;
  let m;
  if ((m = h.match(/^#\/r\/([^/]+)\/run\/([^/?#]+)/))) return { view: "run", realm: decodeURIComponent(m[1]), id: decodeURIComponent(m[2]) };
  if ((m = h.match(/^#\/r\/([^/]+)\/settings(?:\/([a-z]+))?/))) return { view: "settings", realm: decodeURIComponent(m[1]), tab: m[2] };
  if ((m = h.match(/^#\/r\/([^/]+)\/documents(?:\/([^?#]*))?/))) return { view: "documents", realm: decodeURIComponent(m[1]), path: decodeURIComponent(m[2] || "") };
  if ((m = h.match(/^#\/r\/([^/]+)\/skills/))) return { view: "skills", realm: decodeURIComponent(m[1]) };
  // Today was the business's page before Steering (decision 37).
  if ((m = h.match(/^#\/r\/([^/]+)\/(?:steering|today)/))) return { view: "steering", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)\/(routines|pipelines)/))) return { view: m[2], realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)/))) return { view: "new", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/owner(?:\/([a-z]+))?/))) return { view: "owner", tab: m[1] };
  if (h.startsWith("#/usage")) return { view: "usage" };
  if (h.startsWith("#/realms/new")) return { view: "new-realm" };
  if (h.startsWith("#/welcome")) return { view: "welcome" };
  // A new chat in the realm open last (the Mac app's File → New chat).
  if (h.startsWith("#/chat")) return { view: "home", chat: true };
  return { view: "home" };
}

function go(hash) {
  if (location.hash === hash) onRoute();
  else location.hash = hash;
}

/** The realm to show when the URL does not name one. */
function preferredRealm() {
  const live = app.realms.filter((r) => !r.archived);
  const pick = (name) => live.find((r) => r.name === name);
  let saved = null;
  try { saved = localStorage.getItem("airth-realm"); } catch {}
  return (pick(app.realm) || pick(saved) || pick(app.info && app.info.realm) || live.find((r) => r.default) || live[0] || {}).name || null;
}

/** Whether the owner finished or skipped the welcome guide in this window. */
function welcomed() {
  try { return localStorage.getItem("airth-welcomed") === "1"; } catch { return false; }
}

async function onRoute() {
  // On a phone, going somewhere closes the sidebar.
  if (narrow()) $("#app").classList.add("side-hidden");
  const before = app.route;
  app.route = parseRoute();
  if (docsUi.quick) { docsUi.quick.remove(); docsUi.quick = null; }
  if (before.view !== app.route.view || before.path !== app.route.path) docsUi.filter = "";
  // Within the documents, the page stays: only the open file or folder changes.
  if (before.view === "documents" && app.route.view === "documents" && app.route.realm === app.realm && ideMounted()) {
    renderSidebar();
    return ideRoute();
  }
  app.run = null;
  app.version = null;
  app.diff = null;
  app.error = null;
  app.changeError = null;
  app.reason = "";
  confirmDiscard = false;
  if (app.route.view === "home") {
    const r = preferredRealm();
    if (!r) return go(welcomed() ? "#/realms/new" : "#/welcome");
    return go(app.route.chat ? `#/r/${enc(r)}` : `#/r/${enc(r)}/steering`);
  }
  // The guide has the whole window; the sidebar comes back after.
  $("#app").classList.toggle("welcoming", app.route.view === "welcome");
  if (app.route.realm && app.route.realm !== app.realm) await selectRealm(app.route.realm);
  renderMain();
  renderSidebar();
  animateRoute();
  if (app.route.view === "run") pollRun();
}

/** A soft fade between pages (only on navigation, not on re-renders). */
function animateRoute() {
  const main = $("#main");
  main.classList.remove("route-enter");
  void main.offsetWidth; // restart the animation
  main.classList.add("route-enter");
}

async function selectRealm(name) {
  app.realm = name;
  app.runs = [];
  app.realmDetail = null;
  try { localStorage.setItem("airth-realm", name); } catch {}
  await Promise.all([refreshRuns(), refreshRealmDetail()]);
}

