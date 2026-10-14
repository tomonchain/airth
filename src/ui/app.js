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
  propose_skill: "spark", test_skill: "flask", north_star: "star", use_skill: "books",
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
  if ((m = h.match(/^#\/r\/([^/]+)\/objectives/))) return { view: "objectives", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)\/skills/))) return { view: "skills", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/r\/([^/]+)/))) return { view: "new", realm: decodeURIComponent(m[1]) };
  if ((m = h.match(/^#\/owner(?:\/([a-z]+))?/))) return { view: "owner", tab: m[1] };
  if (h.startsWith("#/inbox")) return { view: "inbox" };
  if (h.startsWith("#/usage")) return { view: "usage" };
  if (h.startsWith("#/realms/new")) return { view: "new-realm" };
  if (h.startsWith("#/welcome")) return { view: "welcome" };
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
    return go(`#/r/${enc(r)}`);
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

// ---------- sidebar ----------

let realmMenuOpen = false;

function renderSidebar() {
  renderRealmSwitch();
  renderNav();
  renderRunList();
  renderFoot();
  updateTitle();
}

function renderNav() {
  const unread = app.info ? app.info.unread : 0;
  const v = app.route.view;
  const r = app.realm && enc(app.realm);
  $("#side-nav").replaceChildren(
    h("button", { class: "nav-link", type: "button", onclick: openPalette, title: "Search chats, pages, realms and actions" },
      icon("search"), h("span", { class: "grow" }, "Search"), h("kbd", { class: "nav-kbd" }, modKey + "K")),
    navLink(r ? `#/r/${r}` : null, "pencil", "New chat", v === "new"),
    navLink(r ? `#/r/${r}/documents` : null, "folder", "Documents", v === "documents"),
    navLink("#/inbox", "inbox", "Inbox", v === "inbox", unread > 0 && h("span", { class: "count" }, String(unread))),
    navLink(r ? `#/r/${r}/objectives` : null, "clock", "Objectives", v === "objectives"),
    navLink(r ? `#/r/${r}/skills` : null, "spark", "Skills", v === "skills"),
    navLink("#/usage", "chart", "Usage", v === "usage"),
    nsSide());
}

/** Which date group a run falls in, ChatGPT style. */
function dateGroup(iso) {
  const d = new Date(iso);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  if (d >= today) return "Today";
  const days = (today - d) / 86400000;
  if (days <= 1) return "Yesterday";
  if (days <= 7) return "Previous 7 days";
  if (days <= 30) return "Previous 30 days";
  return "Older";
}

/** One discreet mark per run: only what deserves attention. */
function runMark(r) {
  if (r.needs_approval) return h("span", { class: "dot awaiting", title: "Needs your approval" });
  if (r.live) return h("span", { class: "run-progress", title: "Running" }, r.plan_total > 0 && `${r.plan_done}/${r.plan_total}`, h("span", { class: "dot running" }));
  if (r.status === "failed") return h("span", { class: "dot failed", title: "Failed" });
  if (r.status === "stopped" || r.status === "interrupted") return h("span", { class: "dot stopped", title: r.status });
  return changeBadge(r);
}

function renderRunList() {
  if (app.renaming) return; // a run's name is being edited in place
  const list = $("#run-list");
  list.replaceChildren();
  if (app.realm && !app.runs.length) list.append(h("div", { class: "run-empty" }, "No chats yet."));
  const runs = [...app.runs].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at));
  const focus = focusOn();
  let group = null;
  for (const r of runs) {
    const g = dateGroup(r.updated_at);
    if (g !== group) { group = g; list.append(h("div", { class: "section-label" }, g)); }
    const active = app.route.view === "run" && app.route.id === r.id;
    const status = r.needs_approval ? "needs approval" : r.live ? "running" : r.status;
    // In focus mode, runs that did not serve the North Star fade (never those that need you).
    const shadowed = focus && !r.focus && !r.needs_approval && !active;
    list.append(
      h("button", { class: "run-item" + (active ? " active" : "") + (shadowed ? " shadowed" : "") + (focus && r.focus ? " on-goal" : ""),
          type: "button", title: `${runName(r)}\n${status} · ${ago(r.updated_at)}${r.focus ? " · ★ North Star" : ""}\nDouble-click to rename`,
          onclick: () => go(`#/r/${enc(app.realm)}/run/${enc(r.id)}`),
          ondblclick: (e) => { e.preventDefault(); editInPlace(e.currentTarget, r.id, runName(r)); } },
        focus && r.focus && h("span", { class: "ns-star small", "aria-hidden": "true" }, "★"),
        h("span", { class: "obj" }, runName(r)),
        runMark(r)));
  }
}

/** A run's display name: the name the owner gave it, else the first line of its objective. */
function runName(r) {
  return (r.title || "").trim() || String(r.objective || "").split("\n")[0].trim() || "(untitled chat)";
}

async function renameRun(id, title) {
  try {
    const res = await api(`${runPath(app.realm, id)}`, { title }, "PATCH");
    const run = app.runs.find((r) => r.id === id);
    if (run) run.title = res.title;
    if (app.run && app.route.id === id) app.run.title = res.title;
    flash(res.title ? "Renamed" : "Name cleared");
  } catch (e) {
    flash(e.message, true);
  }
}

/** Turn a run's name into a text box, in the sidebar or the run header. Enter saves, Esc cancels. */
function editInPlace(el, id, current) {
  if (!el || app.renaming) return;
  app.renaming = id;
  const input = h("input", { type: "text", class: "title-input", value: current, "aria-label": "Chat name", maxlength: "120" });
  el.replaceWith(input);
  input.focus();
  input.select();
  let done = false;
  const finish = async (save) => {
    if (done) return;
    done = true;
    const value = input.value.trim();
    if (save && value !== current) await renameRun(id, value);
    app.renaming = null;
    renderRunList();
    renderRun();
    updateTitle();
  };
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { e.preventDefault(); finish(true); }
    else if (e.key === "Escape") { e.preventDefault(); finish(false); }
  });
  // Clicks in the box must not reach the run item (which would navigate).
  input.addEventListener("click", (e) => e.stopPropagation());
  input.addEventListener("blur", () => finish(true));
}

function renderFoot() {
  const foot = $("#sidebar-foot");
  const d = app.realmDetail;
  const name = (app.ownerName || "").trim();
  foot.replaceChildren(
    updateWidget(),
    d && !d.model && h("button", { class: "side-warn", type: "button", title: d.model_error || "", onclick: () => go(`#/r/${enc(app.realm)}/settings/models`) }, "⚠ No model configured"),
    app.info && !app.info.sandbox && h("div", { class: "side-warn", title: "Commands are not confined to the realm on this platform." }, "⚠ No command sandbox"),
    h("div", { class: "me" },
      h("button", { class: "me-btn" + (app.route.view === "owner" ? " active" : ""), type: "button", onclick: () => go("#/owner") },
        pictureMark("avatar", "owner/avatar", app.ownerAvatar, name),
        h("span", { class: "me-name" }, name || "Your profile")),
      themeToggle(),
      app.realm && h("button", { class: "icon-action" + (app.route.view === "settings" ? " active" : ""), type: "button", title: "Realm settings", "aria-label": "Realm settings",
        onclick: () => go(`#/r/${enc(app.realm)}/settings`) }, icon("gear"))));
}

function navLink(hash, iconName, label, active, extra) {
  return h("button", { class: "nav-link" + (active ? " active" : ""), type: "button", disabled: !hash, onclick: () => hash && go(hash) },
    icon(iconName), h("span", { class: "grow" }, label), extra);
}

function changeBadge(r) {
  if (r.change === "adopted") return h("span", { class: "badge ok" }, "adopted");
  if (r.change === "discarded") return h("span", { class: "badge" }, "discarded");
  if (r.checks_passed === true) return h("span", { class: "badge ok" }, "✓ checks");
  if (r.checks_passed === false) return h("span", { class: "badge err" }, "✗ checks");
  return null;
}

function renderRealmSwitch() {
  const el = $("#realm-switch");
  el.replaceChildren();
  const elsewhere = app.realms.filter((r) => r.name !== app.realm && r.needs_approval > 0);
  const current = app.realms.find((r) => r.name === app.realm);
  const button = h("button", { class: "realm-button", type: "button", "aria-haspopup": "true", "aria-expanded": String(realmMenuOpen),
      title: current && current.description ? current.description : "",
      onclick: (e) => { e.stopPropagation(); realmMenuOpen = !realmMenuOpen; renderRealmSwitch(); } },
    pictureMark("realm-mark", `${realmPath(app.realm || "")}/image`, current && current.image, app.realm),
    h("span", { class: "realm-name" }, app.realm || "No realm"),
    elsewhere.length > 0 && h("span", { class: "needs-dot", title: "Approvals waiting in other realms" }),
    h("span", { class: "chev" }, icon("chevrons")));
  el.append(button);
  if (elsewhere.length)
    el.append(h("button", { class: "realm-alert", type: "button", onclick: () => go(`#/r/${enc(elsewhere[0].name)}`) },
      `${elsewhere.map((r) => r.name).join(", ")} waiting for approval →`));
  if (!realmMenuOpen) return;
  const close = () => { realmMenuOpen = false; };
  const d = app.realmDetail;
  const menu = h("div", { class: "realm-menu", role: "menu" }, h("div", { class: "menu-label" }, "Realms"));
  for (const r of app.realms) {
    menu.append(h("button", { class: "realm-option" + (r.name === app.realm ? " active" : "") + (r.archived ? " archived" : ""), type: "button", role: "menuitem",
        onclick: () => { close(); go(`#/r/${enc(r.name)}`); } },
      pictureMark("realm-mark small", `${realmPath(r.name)}/image`, r.image, r.name),
      h("span", { class: "grow" }, r.name, r.archived && h("span", { class: "muted" }, " · archived")),
      r.needs_approval > 0 ? h("span", { class: "needs" }, r.needs_approval) : r.live > 0 ? h("span", { class: "dot running" }) : r.name === app.realm ? h("span", { class: "check" }, "✓") : null));
  }
  menu.append(
    h("div", { class: "menu-sep" }),
    app.realm && h("button", { class: "realm-option", type: "button", role: "menuitem", onclick: () => { close(); go(`#/r/${enc(app.realm)}/settings`); } }, icon("gear"), h("span", { class: "grow" }, "Realm settings")),
    h("button", { class: "realm-option", type: "button", role: "menuitem", onclick: () => { close(); go("#/realms/new"); } }, icon("plus"), h("span", { class: "grow" }, "New realm")),
    h("div", { class: "menu-sep" }),
    h("div", { class: "menu-foot" },
      h("span", { class: "logo", "aria-hidden": "true" }),
      h("span", { class: "grow" }, "airth", app.info && h("span", { class: "muted" }, " v" + app.info.version)),
      d && h("span", { class: "muted mono", title: "Model for this realm" }, d.model || "no model")));
  el.append(menu);
}

document.addEventListener("click", () => {
  if (realmMenuOpen) { realmMenuOpen = false; renderRealmSwitch(); }
});

function updateTitle() {
  const n = app.realms.reduce((acc, r) => acc + (r.needs_approval || 0), 0);
  const run = app.route.view === "run" && app.run && runName({ title: app.run.title, objective: app.run.state.objective });
  const page = run ? (run.length > 48 ? run.slice(0, 47) + "…" : run) + " · Airth" : "Airth";
  document.title = (n ? `(${n}) ` : "") + page;
}

// ---------- main ----------

function renderMain() {
  const main = $("#main");
  ideStash();
  main.replaceChildren();
  runEls = null;
  if (app.unauthorized) {
    main.append(h("div", { class: "center-note" },
      h("h1", null, "Open Airth from your terminal"),
      h("p", null, "Run ", h("code", null, "airth ui"), " and use the link it prints. The link carries a private key that lets this page control the agent.")));
    return;
  }
  switch (app.route.view) {
    case "new": return renderNew(main);
    case "run": return renderRunShell(main);
    case "settings": return renderSettings(main);
    case "owner": return renderOwner(main);
    case "documents": return renderDocuments(main);
    case "objectives": return renderObjectives(main);
    case "skills": return renderSkills(main);
    case "inbox": return renderInbox(main);
    case "usage": return renderUsage(main);
    case "new-realm": return renderNewRealm(main);
    case "welcome": return renderWelcome(main);
  }
}

const SpeechRec = window.SpeechRecognition || window.webkitSpeechRecognition;

/** Dictate into a text box: speech becomes text you can edit before sending. */
function micButton(ta) {
  const label = "Dictate";
  if (!SpeechRec) {
    return h("button", { class: "mic", type: "button", title: label, "aria-label": label, onclick: () => {
      ta.focus();
      flash(isMac ? "Press the 🎤 key (or Fn twice) to dictate with macOS." : "Use your system's dictation to speak.");
    } }, icon("mic"));
  }
  let rec = null;
  const btn = h("button", { class: "mic", type: "button", title: label, "aria-label": label, onclick: () => {
    if (rec) { rec.stop(); return; }
    rec = new SpeechRec();
    rec.lang = navigator.language || "fr-FR";
    rec.interimResults = true;
    rec.continuous = true;
    const base = ta.value.trim() ? ta.value.replace(/\s*$/, " ") : "";
    rec.onresult = (e) => {
      let text = "";
      for (const r of e.results) text += r[0].transcript;
      ta.value = base + text;
      ta.dispatchEvent(new Event("input"));
    };
    rec.onerror = (e) => { if (e.error !== "aborted" && e.error !== "no-speech") flash("Dictation: " + e.error, true); };
    rec.onend = () => { rec = null; btn.classList.remove("on"); ta.focus(); };
    try { rec.start(); btn.classList.add("on"); } catch (e) { rec = null; flash("Dictation unavailable: " + e.message, true); }
  } }, icon("mic"));
  return btn;
}

// ---------- model picker ----------

const modelUi = { realm: null, options: null, loading: null, open: null };

function loadModels(force) {
  if (!app.realm) return Promise.resolve(null);
  if (!force && modelUi.realm === app.realm && modelUi.options) return Promise.resolve(modelUi.options);
  if (!force && modelUi.loading && modelUi.realm === app.realm) return modelUi.loading;
  modelUi.realm = app.realm;
  modelUi.loading = api(`${realmPath(app.realm)}/models${force ? "?refresh=1" : ""}`)
    .then((o) => (modelUi.options = o))
    .catch((e) => { flash(e.message, true); return null; })
    .finally(() => { modelUi.loading = null; });
  return modelUi.loading;
}

const shortModel = (choice) => {
  if (!choice || choice === "auto") return "Auto";
  const m = choice.split("/").slice(1).join("/");
  return m.length > 24 ? m.slice(0, 23) + "…" : m;
};

document.addEventListener("mousedown", (e) => {
  if (modelUi.open && !modelUi.open.contains(e.target)) { modelUi.open.remove(); modelUi.open = null; }
});

/** "Auto ▾": which model answers this request. get/set read and store the choice.
 *  `auto` renames the first entry ({label, sub}), e.g. "None" for an optional model. */
function modelPicker({ get, set, auto }) {
  const label = h("span", { class: "mp-label" });
  const btn = h("button", { class: "model-pill", type: "button", title: "Which model answers", "aria-haspopup": "true" }, icon("brain"), label, h("span", { class: "mp-chev" }, "▾"));
  const refresh = () => {
    label.textContent = get() === "auto" && auto ? auto.label : shortModel(get());
    btn.title = get() === "auto" ? "Which model answers" : get();
    btn.classList.toggle("chosen", get() !== "auto");
  };
  const wrap = h("span", { class: "mp" }, btn);
  btn.addEventListener("click", async (e) => {
    e.stopPropagation();
    if (modelUi.open) { modelUi.open.remove(); modelUi.open = null; return; }
    const menu = h("div", { class: "mp-menu", role: "menu" }, working("Loading models…"));
    wrap.append(menu);
    modelUi.open = menu;
    const opts = await loadModels(false);
    if (modelUi.open !== menu) return;
    const q = h("input", { type: "search", class: "mp-search", placeholder: "Search, or type provider/model", "aria-label": "Search models" });
    const list = h("div", { class: "mp-list" });
    const pick = (choice) => { set(choice); refresh(); menu.remove(); modelUi.open = null; };
    const item = (choice, text, sub, on) => h("button", { type: "button", class: "mp-item" + (on ? " on" : ""), role: "menuitem", onclick: () => pick(choice) },
      h("span", { class: "grow" }, text, sub && h("span", { class: "mp-sub" }, sub)), on && icon("check"));
    const draw = () => {
      const s = q.value.trim().toLowerCase();
      const cur = get();
      const groups = [];
      groups.push(auto
        ? item("auto", auto.label, auto.sub, cur === "auto")
        : item("auto", "Auto", opts && opts.default ? `realm default · ${opts.default}` : "the realm's model and routing", cur === "auto"));
      for (const p of (opts && opts.providers) || []) {
        if (!p.configured) {
          if (!s || p.name.toLowerCase().includes(s)) groups.push(h("div", { class: "mp-group off" }, p.name,
            h("a", { href: `#/r/${enc(app.realm)}/settings`, onclick: () => { menu.remove(); modelUi.open = null; } }, p.key ? `add ${p.key}` : "set up")));
          continue;
        }
        let models = p.models.filter((m) => !s || `${p.id}/${m}`.toLowerCase().includes(s) || p.name.toLowerCase().includes(s));
        if (p.default_model && !models.includes(p.default_model) && (!s || p.default_model.includes(s))) models.unshift(p.default_model);
        if (!models.length) continue;
        const shown = s ? models.slice(0, 30) : models.slice(-8).reverse();
        groups.push(h("div", { class: "mp-group" }, p.name, p.error && h("span", { class: "mp-sub warn", title: p.error }, "list unavailable")),
          shown.map((m) => item(`${p.id}/${m}`, m, m === p.default_model ? "default" : null, cur === `${p.id}/${m}`)));
      }
      if (s.includes("/") && !groups.some((g) => g.textContent === s)) groups.push(item(q.value.trim(), `Use ${q.value.trim()}`, "custom", false));
      list.replaceChildren(...groups.flat());
    };
    q.addEventListener("input", draw);
    q.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape") { menu.remove(); modelUi.open = null; }
      if (ev.key === "Enter" && q.value.includes("/")) pick(q.value.trim());
    });
    menu.replaceChildren(q, list,
      h("button", { type: "button", class: "mp-refresh", onclick: async () => { list.replaceChildren(working("Refreshing…")); await loadModels(true); if (modelUi.open === menu) { menu.remove(); modelUi.open = null; btn.click(); } } }, icon("refresh"), "Refresh lists"));
    draw();
    setTimeout(() => q.focus(), 0);
  });
  refresh();
  return { el: wrap, refresh };
}

const modelChoices = cappedStore("airth-models", 300);
// One-time cleanup of the per-key format used before the capped store.
try { for (const k of Object.keys(localStorage)) if (k.startsWith("airth-model:")) localStorage.removeItem(k); } catch {}
function storedModel(key) { return modelChoices.get(key) || null; }
function storeModel(key, v) { modelChoices.set(key, v); }

/** A host name for display; never throws on a bad address (registry entries are not vetted). */
const hostOf = (u) => { try { return new URL(u).host; } catch { return String(u || ""); } };

/** A small map kept in this browser, capped: the least recently written entries go first. */
function cappedStore(name, cap) {
  let map = {};
  try { map = JSON.parse(localStorage.getItem(name) || "{}") || {}; } catch { map = {}; }
  return {
    get: (k) => map[k],
    set(k, v) {
      delete map[k];
      if (v != null && v !== "") map[k] = v;
      const keys = Object.keys(map);
      for (const old of keys.slice(0, Math.max(0, keys.length - cap))) delete map[old];
      try { localStorage.setItem(name, JSON.stringify(map)); } catch {}
    },
  };
}

/** Unsent text, per message box (a run, or a realm's new-run screen), kept in this browser. */
const drafts = cappedStore("airth-drafts", 300);
function saveDraft(key, text) {
  if (!key) return;
  drafts.set(key, text.trim() ? text : "");
}

function composer({ placeholder, buttonLabel, onSubmit, rows = 1, disabled = false, note, draftKey, model }) {
  const ta = h("textarea", { rows, placeholder, disabled, "aria-label": placeholder });
  if (draftKey && drafts.get(draftKey)) {
    ta.value = drafts.get(draftKey);
    // Fit the box to the restored text once it is on screen.
    setTimeout(() => { if (ta.scrollHeight) { ta.style.height = "auto"; ta.style.height = Math.min(ta.scrollHeight, 200) + "px"; } }, 0);
  }
  const enterSends = () => window.airthPrefs && window.airthPrefs.get("send") === "enter";
  // Blocked: you may write (a draft) but not send yet, e.g. while the run works.
  let blocked = false;
  const sync = () => { btn.disabled = ta.disabled || blocked || !ta.value.trim(); };
  const btn = h("button", { class: "send", type: "button", disabled, "aria-label": buttonLabel, title: `${buttonLabel} (${enterSends() ? "↵" : modKey + "↵"})` }, icon("send"));
  const errorEl = h("div", { class: "error-banner", hidden: !app.error }, app.error || "");
  const submit = async () => {
    const text = ta.value.trim();
    if (!text || btn.disabled) return;
    btn.disabled = true;
    try {
      await onSubmit(text);
      ta.value = "";
      saveDraft(draftKey, "");
      app.error = null;
      errorEl.hidden = true;
    } catch (e) {
      app.error = e.message;
      errorEl.textContent = e.message;
      errorEl.hidden = false;
    } finally {
      sync();
    }
  };
  ta.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || e.isComposing) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod || (enterSends() && !e.shiftKey && !e.altKey)) { e.preventDefault(); submit(); }
  });
  ta.addEventListener("input", () => {
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, 200) + "px";
    sync();
    saveDraft(draftKey, ta.value);
  });
  btn.disabled = disabled || !ta.value.trim();
  btn.addEventListener("click", submit);
  const picker = model ? modelPicker(model) : null;
  const el = h("div", { class: "composer" }, errorEl, h("div", { class: "composer-box" }, ta, picker && picker.el, micButton(ta), btn), note && h("div", { class: "note" }, note));
  return { el, ta, btn, errorEl, picker, block: (b) => { blocked = b; sync(); } };
}

function page(main, title, sub, ...children) {
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner" },
    h("h1", null, title), sub && h("p", { class: "sub" }, sub), ...children)));
}

// ---------- North Star ----------

const NS_MARK = { todo: "", doing: "", done: "✓", dropped: "–" };
const NS_NEXT = { todo: "doing", doing: "done", done: "todo", dropped: "todo" };
const nsUi = { open: false, editing: false, confirmClear: false };

/** The current realm's North Star, if the owner set one. */
const northStar = () => (app.realmDetail && app.realmDetail.north_star) || null;

/** Focus mode (per realm, on by default once there is a North Star): what does not serve it is shadowed. */
function focusOn() {
  if (!northStar()) return false;
  let saved = null;
  try { saved = localStorage.getItem("airth-focus:" + app.realm); } catch {}
  return saved !== "0";
}

function toggleFocus() {
  const on = !focusOn();
  try { localStorage.setItem("airth-focus:" + app.realm, on ? "1" : "0"); } catch {}
  renderSidebar();
  if (app.route.view === "new") renderMain();
  flash(on ? "Focus on: what does not serve the North Star is shadowed" : "Focus off");
}

function nsBar(ns) {
  const fill = h("span");
  // Set through the CSSOM: the page's CSP blocks inline style attributes.
  fill.style.width = (ns.total ? Math.round((100 * ns.done) / ns.total) : 0) + "%";
  return h("div", { class: "plan-bar ns-bar", "aria-hidden": "true" }, fill);
}

/** The goal, always in sight at the top of the sidebar. */
function nsSide() {
  const ns = northStar();
  if (!ns) return null;
  return h("div", { class: "ns-side" },
    h("button", { class: "ns-side-goal", type: "button", title: ns.goal + (ns.total ? `\n${ns.done} of ${ns.total} milestones done` : ""), onclick: () => go(`#/r/${enc(app.realm)}`) },
      h("span", { class: "ns-star", "aria-hidden": "true" }, "★"),
      h("span", { class: "grow ns-side-text" }, ns.goal),
      ns.total > 0 && h("span", { class: "progress-mini" }, `${ns.done}/${ns.total}`)),
    ns.total > 0 && nsBar(ns),
    h("label", { class: "ns-focus", title: "Shadow the chats that do not serve the North Star" },
      h("input", { type: "checkbox", checked: focusOn(), onchange: toggleFocus }), h("span", null, "Focus")));
}

async function startNorthStarRun(what, realm = app.realm) {
  try {
    const r = await api(`${realmPath(realm)}/northstar/${what}`, {});
    await refreshRuns();
    go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
  } catch (e) { flash(e.message, true); }
}

/** The North Star card of the realm's home page: goal, progress, milestones, and how Airth pushes it. */
function northStarCard() {
  const el = h("div", { id: "ns-wrap", class: "ns-wrap" });
  drawNorthStar(el);
  return el;
}

function drawNorthStar(el) {
  const d = app.realmDetail;
  if (!d) return el.replaceChildren();
  const ns = d.north_star;
  const realm = app.realm;
  const noModel = !!d.model_error;
  const change = async (path, body, method, msg) => {
    try {
      app.realmDetail = await api(`${realmPath(realm)}/northstar${path}`, body, method);
      drawNorthStar(el);
      renderSidebar();
      if (msg) flash(msg);
    } catch (e) { flash(e.message, true); }
  };

  if (!ns && !nsUi.open) {
    return el.replaceChildren(h("button", { class: "ns-empty", type: "button", onclick: () => { nsUi.open = true; drawNorthStar(el); } },
      h("span", { class: "ns-star", "aria-hidden": "true" }, "★"),
      h("span", null, h("span", { class: "strong" }, "Set a North Star"), " · write the one goal for this realm. Airth then asks you a few questions, helps you sharpen it and define the milestones, and pushes it forward every day.")));
  }
  if (!ns || nsUi.editing) {
    const goal = h("input", { type: "text", value: ns ? ns.goal : "", placeholder: "The one goal this realm works toward, e.g. “100 kit sales by December”" });
    const why = h("input", { type: "text", value: ns ? ns.why : "", placeholder: "Why it matters, or what success looks like (optional)" });
    const save = async () => {
      if (!goal.value.trim()) return goal.focus();
      nsUi.editing = false;
      nsUi.open = false;
      const isNew = !ns;
      await change("", { goal: goal.value, why: why.value }, "PUT", ns ? "North Star saved" : null);
      // A new North Star: Airth starts the conversation that defines it.
      if (isNew && northStar()) {
        if (noModel) flash("North Star set. Add a model in the realm settings to define it with Airth.");
        else { flash("North Star set. Airth has a few questions for you."); startNorthStarRun("brainstorm"); }
      }
    };
    const cancel = () => { nsUi.editing = false; nsUi.open = false; nsUi.confirmClear = false; drawNorthStar(el); };
    for (const input of [goal, why]) input.addEventListener("keydown", (e) => { if (e.key === "Enter") save(); else if (e.key === "Escape") cancel(); });
    el.replaceChildren(h("div", { class: "ns-card editing" },
      h("div", { class: "ns-head" }, h("span", { class: "ns-star", "aria-hidden": "true" }, "★"), h("span", { class: "ns-label grow" }, "North Star")),
      goal, why,
      h("div", { class: "row-end" },
        ns && (nsUi.confirmClear
          ? h("button", { class: "btn danger", type: "button", onclick: () => { nsUi.editing = false; nsUi.confirmClear = false; change("", undefined, "DELETE", "North Star removed"); } }, "Really remove it?")
          : h("button", { class: "btn danger ghost", type: "button", onclick: () => { nsUi.confirmClear = true; drawNorthStar(el); } }, "Remove…")),
        h("span", { class: "grow" }),
        h("button", { class: "btn ghost", type: "button", onclick: cancel }, "Cancel"),
        h("button", { class: "btn primary", type: "button", onclick: save }, ns ? "Save" : "Set it and define it with Airth"))));
    goal.focus();
    return;
  }

  const s = ns.schedule;
  const has = ns.milestones.length > 0;
  const milestones = has
    ? h("ol", { class: "ns-milestones" }, ns.milestones.map((m) => h("li", { class: "ns-m " + m.status + (m.id === ns.next ? " next" : "") },
        h("button", { class: "mark", type: "button", title: `${m.status}: click to mark ${NS_NEXT[m.status]}`, "aria-label": `${m.title}: ${m.status}`,
          onclick: () => change(`/milestones/${enc(m.id)}`, { status: NS_NEXT[m.status] }, "PATCH") }, NS_MARK[m.status]),
        h("div", { class: "grow" },
          h("div", { class: "ns-m-title" }, m.title, m.id === ns.next && h("span", { class: "chip running" }, "next")),
          m.done_when && h("div", { class: "muted small" }, "Done when: " + m.done_when),
          m.note && h("div", { class: "muted small" }, m.note)),
        m.runs.length > 0 && h("a", { class: "muted small ns-runs", href: `#/r/${enc(realm)}/run/${enc(m.runs[m.runs.length - 1])}`, title: "The latest chat that moved it" },
          `${m.runs.length} chat${m.runs.length === 1 ? "" : "s"}`))))
    : h("p", { class: "muted small ns-hint" }, "No milestones yet. Define them with Airth: it asks you a few questions, proposes a sharper goal and the steps to it, and records them once you agree.");

  const every = h("input", { type: "text", class: "ns-every", value: s ? s.every : "weekdays 09:00", placeholder: "weekdays 09:00 · every 3h", "aria-label": "When Airth advances it", spellcheck: "false" });
  every.addEventListener("change", () => { if (s) change("/schedule", { every: every.value }, undefined, "Schedule saved"); });
  const daily = has && h("div", { class: "ns-daily" },
    h("label", { class: "toggle small", title: "An objective: the chats it starts may record North Star progress without asking; anything else follows this realm's rules" },
      h("input", { type: "checkbox", checked: !!s, onchange: (e) => change("/schedule", { every: e.target.checked ? every.value : null }, undefined, e.target.checked ? "Airth will push it forward on its own" : "Stopped") }),
      h("span", null, "Airth pushes it forward on its own")),
    every,
    s && s.running ? h("a", { class: "chip running", href: `#/r/${enc(realm)}/run/${enc(s.last_run)}` }, "working on it")
      : s && s.next_at && h("span", { class: "muted small" }, "next " + when(s.next_at)),
    s && !s.running && s.last_run && h("a", { class: "muted small", href: `#/r/${enc(realm)}/run/${enc(s.last_run)}` }, "last: " + (s.last_status || "run")));

  el.replaceChildren(h("div", { class: "ns-card" },
    h("div", { class: "ns-head" },
      h("span", { class: "ns-star", "aria-hidden": "true" }, "★"),
      h("span", { class: "ns-label grow" }, "North Star", ns.total > 0 && h("span", { class: "muted" }, ` · ${ns.done} of ${ns.total} milestones`)),
      h("button", { class: "icon-action", type: "button", title: "Edit the North Star", "aria-label": "Edit the North Star", onclick: () => { nsUi.editing = true; drawNorthStar(el); } }, icon("pencil"))),
    h("div", { class: "ns-goal" }, ns.goal),
    ns.why && h("div", { class: "ns-why" }, ns.why),
    ns.total > 0 && nsBar(ns),
    milestones,
    h("div", { class: "ns-actions" },
      has
        ? h("button", { class: "btn primary", type: "button", disabled: noModel, title: "A chat on the next milestone: the most valuable step Airth can complete and verify now", onclick: () => startNorthStarRun("push") }, "Push it forward now")
        : h("button", { class: "btn primary", type: "button", disabled: noModel, onclick: () => startNorthStarRun("brainstorm") }, "Define it with Airth"),
      has && h("button", { class: "btn", type: "button", disabled: noModel, title: "Airth asks what changed and reworks the goal and milestones with you", onclick: () => startNorthStarRun("brainstorm") }, "Refine with Airth")),
    daily));
}

/** Keep the progress live on the realm's home page (runs record it as they go). */
async function refreshNorthStar() {
  if (!app.realm || app.unauthorized) return;
  const before = JSON.stringify(northStar());
  try { await refreshRealmDetail(); } catch { return; }
  if (JSON.stringify(northStar()) === before) return;
  renderSidebar();
  const el = $("#ns-wrap");
  if (el && !nsUi.editing && !nsUi.open) drawNorthStar(el);
}

// ---------- what Airth can do ----------

/** What Airth can take on, by area, in words; examples fill the message box. */
const CAN_DO = [
  { caps: ["read", "write"], icon: "folder", title: "Your files",
    text: "Reads, sorts, writes and edits files in this realm's folder, and never outside it.",
    examples: ["Sort the files in this folder by topic", "Summarize every document in this folder"] },
  { caps: ["apps"], icon: "calendar", title: "Your Mac apps",
    text: "Calendar, Reminders, Notes, Mail (it reads and prepares drafts; it never sends), Safari tabs and the clipboard.",
    examples: ["What's on my calendar tomorrow?", "Summarize today's unread emails", "Add a reminder to call the bank on Friday"] },
  { caps: ["network"], icon: "world", title: "The web and your services",
    text: "Looks things up on public websites, and calls the services you connect (Notion, Linear, Stripe…) without ever seeing their keys.",
    examples: ["Compare the prices of three competitors", "Find the best-rated plumbers near me"], services: true },
  { caps: ["execute"], icon: "terminal", title: "Commands and code",
    text: "Runs commands in a sandbox: scripts, file conversions, tests, code changes you review before they count.",
    examples: ["Convert the images in this folder to WebP", "Run the tests and fix what fails"] },
  { caps: ["memory"], icon: "brain", title: "Memory and goals",
    text: "Remembers what matters in this realm, and keeps the North Star's milestones up to date.",
    examples: ["Remember that I prefer meetings in the morning", "What should I do next for the North Star?"] },
  { caps: [], icon: "clock", title: "Work on its own",
    text: "Objectives start work for you: on a schedule, when files change, or from a webhook. A brief every morning, a weekly review…",
    link: ["Set up an objective", (r) => `#/r/${enc(r)}/objectives`] },
  { caps: ["skill"], icon: "spark", title: "New skills",
    text: "When it hits a limit, it can build and test a small tool; nothing is installed until you say so. Or add ready-made skills.",
    link: ["Browse skills", (r) => `#/r/${enc(r)}/skills`] },
];
const CAN_DO_RULE = { allow: ["Automatic", "rule-allow"], ask: ["Asks you first", "rule-ask"], deny: ["Off", "rule-deny"] };

let canDo = null;

/** The presentation of what Airth can do in the current realm. */
async function openCanDo(onExample) {
  if (canDo) return;
  const d = app.realmDetail;
  const realm = app.realm;
  if (!d || !realm) return;
  // The strictest rule of an area says what to expect.
  const ruleOf = (caps) => {
    const rules = caps.map((c) => d.policy[c]).filter(Boolean);
    if (!rules.length) return null;
    return rules.includes("deny") ? (rules.every((r) => r === "deny") ? "deny" : "ask") : rules.includes("ask") ? "ask" : "allow";
  };
  const close = () => {
    if (!canDo) return;
    const el = canDo;
    canDo = null;
    el.classList.add("closing");
    setTimeout(() => el.remove(), 140);
    document.removeEventListener("keydown", onKey);
  };
  const onKey = (e) => { if (e.key === "Escape") close(); };
  const services = h("div", { class: "cando-services muted small" });
  const cards = CAN_DO.map((a) => {
    const rule = ruleOf(a.caps);
    const [word, cls] = rule ? CAN_DO_RULE[rule] : [];
    return h("div", { class: "cando-card" + (rule === "deny" ? " off" : "") },
      h("div", { class: "cando-top" },
        h("span", { class: "welcome-icon" }, icon(a.icon)),
        h("div", { class: "grow strong" }, a.title),
        rule && h("a", { class: "chip " + cls, href: `#/r/${enc(realm)}/settings/permissions`, title: "Change in Permissions", onclick: close }, word)),
      h("p", { class: "muted small" }, a.text),
      a.services && services,
      a.examples && h("div", { class: "cando-examples" }, a.examples.map((x) =>
        h("button", { class: "suggestion", type: "button", disabled: rule === "deny", onclick: () => { close(); onExample && onExample(x); } }, x))),
      a.link && h("a", { class: "cando-more", href: a.link[1](realm), onclick: close }, a.link[0], " ›"));
  });
  const tools = app.info && h("details", { class: "cando-tools" },
    h("summary", null, "Technical details: every tool"),
    h("div", null, app.info.tools.map((t) => {
      const rule = d.policy[t.capability];
      return h("div", { class: "tool-row" },
        h("code", null, t.name),
        rule ? h("span", { class: "chip rule-" + rule }, POLICY_WORDS[rule]) : h("span"),
        h("span", { class: "d" }, t.description.split(". ")[0] + "."));
    })));
  canDo = h("div", { class: "pal-overlay cando-overlay", onmousedown: (e) => { if (e.target === canDo) close(); } },
    h("div", { class: "pal cando", role: "dialog", "aria-modal": "true", "aria-label": "What Airth can do" },
      h("div", { class: "cando-head" },
        h("div", { class: "grow" },
          h("div", { class: "eyebrow" }, realm),
          h("h2", null, "What can Airth do for you?"),
          h("p", { class: "muted" }, "Describe an outcome in plain words. Airth makes a plan, uses the tools below, shows you each step, and asks before anything this realm does not allow on its own.")),
        h("button", { class: "icon-btn", type: "button", "aria-label": "Close", title: "Close (Esc)", onclick: close }, "×")),
      h("div", { class: "cando-body" },
        h("div", { class: "cando-grid" }, cards),
        h("div", { class: "cando-promise" },
          h("div", { class: "strong" }, "Always"),
          h("ul", null,
            h("li", null, "It works only in this realm: other realms' files, memory and keys stay out of reach."),
            h("li", null, "It never sees your keys and passwords; the runtime adds them for it."),
            h("li", null, "You can stop Airth at any time, and review a change before it counts."))),
        tools)));
  document.body.append(canDo);
  document.addEventListener("keydown", onKey);
  // Connected services, for the web card.
  try {
    const c = await api(`${realmPath(realm)}/connectors`);
    const on = (c.connectors || []).filter((x) => x.connected).map((x) => x.connector.name);
    services.replaceChildren(on.length
      ? h("span", null, "Connected here: ", h("b", null, on.join(", ")))
      : h("span", null, "No service connected yet. ", h("a", { href: `#/r/${enc(realm)}/skills`, onclick: () => { skillUi.tab = "connectors"; close(); } }, "Connect one"), "."));
  } catch { services.remove(); }
}

// ---------- new run ----------

const SUGGESTIONS = {
  code: ["Identify and implement one small improvement", "Find and fix a bug", "Add tests for an untested module", "Review error messages and make them clearer"],
  other: ["Organize the files in this realm", "Summarize what's in this realm", "Draft a plan for this week", "Write a checklist for an upcoming task"],
  northStar: ["What is blocking the next milestone?", "Plan this week around the next milestone", "Review our progress toward the North Star: what should we cut?", "What is the smallest step that moves the North Star today?"],
};

const POLICY_WORDS = { allow: "automatic", ask: "needs approval", deny: "disabled" };

function renderNew(main) {
  const d = app.realmDetail;
  const realm = app.realms.find((r) => r.name === app.realm);
  const modelError = d && d.model_error;
  const c = composer({
    placeholder: `Describe an objective for ${app.realm}…`,
    draftKey: `new:${app.realm}`,
    model: {
      get: () => storedModel(`new:${app.realm}`) || "auto",
      set: (v) => storeModel(`new:${app.realm}`, v),
    },
    buttonLabel: "Start",
    rows: 3,
    // Without a realm model you can still pick one in the model menu.
    disabled: !!(realm && realm.archived),
    note: d && d.isolation === "worktree"
      ? "Airth works in its own copy of the repository. You review the change, and it reaches your checkout only when you adopt it."
      : "Every step that needs approval waits for you.",
    onSubmit: async (objective) => {
      const model = storedModel(`new:${app.realm}`) || "auto";
      const { id } = await api(`${realmPath(app.realm)}/runs`, { objective, model });
      storeModel(`run:${app.realm}/${id}`, model);
      await refreshRuns();
      go(`#/r/${enc(app.realm)}/run/${enc(id)}`);
    },
  });
  if (modelError) {
    c.errorEl.replaceChildren(
      h("span", { title: modelError }, "Airth has no model yet, so it cannot start a chat. "),
      h("a", { href: "#/welcome", onclick: () => { welcomeUi.step = WELCOME_STEPS.indexOf("model"); welcomeUi.setup = null; } }, "Connect Claude"),
      " or ",
      h("a", { href: `#/r/${enc(app.realm)}/settings/models` }, "choose another provider"), ".");
    c.errorEl.hidden = false;
  }
  const isCode = d && d.own_root && d.checks.length > 0;
  const ns = northStar();
  const suggestions = ns && ns.milestones.length && focusOn() ? SUGGESTIONS.northStar : isCode ? SUGGESTIONS.code : SUGGESTIONS.other;
  const fill = (text) => { c.ta.value = text; c.ta.dispatchEvent(new Event("input")); c.ta.focus(); };
  const prefill = app.prefill;
  app.prefill = null;
  const tools = d
    ? h("button", { class: "cando-link", type: "button", onclick: () => openCanDo(fill) }, icon("compass"), "What can Airth do for you?")
    : null;
  const hour = new Date().getHours();
  const hello = hour < 5 ? "Good night" : hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
  const first = (app.ownerName || "").trim().split(/\s+/)[0];
  main.append(
    h("div", { class: "new-view" },
      h("div", { class: "new-inner" },
        h("div", { class: "eyebrow" }, app.realm || ""),
        h("h1", null, first ? `${hello}, ${first}` : "What should Airth work on?"),
        h("p", { class: "sub", title: d ? d.root : "" }, first ? "What should Airth work on?" : d ? d.root : ""),
        tools,
        northStarCard(),
        c.el,
        h("div", { class: "suggestions" },
          suggestions.map((s) => h("button", { class: "suggestion", type: "button", onclick: () => { c.ta.value = s; c.ta.dispatchEvent(new Event("input")); c.ta.focus(); } }, s))))));
  if (prefill) fill(prefill);
  c.ta.focus();
}

// ---------- run view ----------

let runEls = null;

function renderRunShell(main) {
  const header = h("header", { class: "run-header" });
  const transcript = h("div", { class: "transcript" }, working("Loading…"));
  const scroller = h("div", { class: "scroller" }, transcript);
  const c = composer({
    placeholder: "Send a follow-up…",
    draftKey: `run:${app.realm}/${app.route.id}`,
    model: {
      // What you picked for this run, else what it was started with.
      get: () => storedModel(`run:${app.realm}/${app.route.id}`) || (app.run && app.run.state.model_choice) || "auto",
      set: (v) => storeModel(`run:${app.realm}/${app.route.id}`, v),
    },
    buttonLabel: "Send",
    disabled: true,
    onSubmit: (message) => sendFollowUp(message, storedModel(`run:${app.realm}/${app.route.id}`) || (app.run && app.run.state.model_choice) || "auto"),
  });
  const toBottom = h("button", { class: "to-bottom", type: "button", title: "Scroll to the latest", "aria-label": "Scroll to the latest",
    onclick: () => scroller.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" }) }, icon("down"));
  const wrap = h("div", { class: "composer-wrap" }, toBottom, c.el);
  scroller.addEventListener("scroll", () => syncToBottom(), { passive: true });
  const side = h("aside", { class: "run-side", "aria-label": "Plan and details" });
  const layout = h("div", { class: "run-layout" + (sideOpen() ? "" : " side-closed") },
    h("div", { class: "run-center" }, header, scroller, wrap), side);
  main.append(layout);
  runEls = { header, transcript, scroller, composer: c, wrap, side, layout, toBottom, seen: null, steps: null };
}

/** Show the "↓" button when the latest messages are out of sight. */
function syncToBottom() {
  if (!runEls) return;
  const sc = runEls.scroller;
  runEls.toBottom.classList.toggle("show", sc.scrollHeight - sc.scrollTop - sc.clientHeight > 240);
}

/** Whether the right column is shown: the owner's last choice, else open on wide windows. */
function sideOpen() {
  let saved = null;
  try { saved = localStorage.getItem("airth-side"); } catch {}
  return saved === null ? window.innerWidth >= 1100 : saved === "1";
}

function toggleSide() {
  const open = runEls.layout.classList.toggle("side-closed") === false;
  try { localStorage.setItem("airth-side", open ? "1" : "0"); } catch {}
  renderRun();
}

/** The "Airth is working" line: a breathing orb and shimmering text. */
function working(label) {
  return h("div", { class: "working" }, h("span", { class: "orb", "aria-hidden": "true" }), h("span", { class: "shimmer" }, label));
}

/** Animate only what is new since the last render: the transcript is rebuilt on every poll. */
function animateNew(nodes) {
  const first = runEls.seen === null;
  const seen = first ? new Set() : runEls.seen;
  for (const top of nodes) {
    for (const n of [top, ...top.querySelectorAll("[data-k]")]) {
      const k = n.dataset && n.dataset.k;
      if (!k) continue;
      if (!first && !seen.has(k)) n.classList.add("enter");
      seen.add(k);
    }
  }
  runEls.seen = seen;
}

const keyed = (node, k) => { node.dataset.k = k; return node; };

async function sendFollowUp(message, model) {
  await api(`${runPath(app.realm, app.route.id)}/messages`, model ? { message, model } : { message });
  app.version = null;
  app.diff = null;
  await Promise.all([pollRun(), refreshRuns()]);
}

function liveStatus(d) {
  if (!d.live) return d.interrupted ? "interrupted" : d.state.status;
  if (d.stopping) return "stopping";
  if (d.pending) return "awaiting";
  if (d.state.change && d.state.change.checks && !d.state.change.checks.finished) return "checking";
  return "running";
}

const STATUS_LABEL = { awaiting: "awaiting approval", stopping: "stopping…", checking: "running checks" };

function renderRun() {
  const d = app.run;
  if (!d || !runEls) return;
  const s = d.state;
  const status = liveStatus(d);

  const canStop = d.live && !d.elsewhere && !d.stopping;
  const open = !runEls.layout.classList.contains("side-closed");
  const hasPlan = s.plan && s.plan.length > 0;
  const closedSteps = hasPlan ? s.plan.filter((p) => p.status === "done" || p.status === "skipped").length : 0;
  const statusClass = status === "stopping" ? "stopped" : status === "checking" ? "running" : status;
  const ctx = [
    h("span", { class: "rh-state " + statusClass }, STATUS_LABEL[status] || status),
    h("span", null, "updated " + ago(s.updated_at)),
    s.origin && h("span", { class: "rh-tag", title: s.origin.trigger }, icon("clock"), s.origin.objective),
    (s.north_star || (app.runs.find((r) => r.id === s.id) || {}).focus) && h("span", { class: "rh-tag ns", title: "This chat serves the realm's North Star" }, "★ North Star"),
    s.skill && h("a", { class: "rh-tag", href: `#/r/${enc(app.realm)}/skills` }, icon("spark"), "developing " + s.skill),
  ].filter(Boolean);
  if (!app.renaming) runEls.header.replaceChildren(
    h("span", { class: "rh-dot dot " + (d.pending ? "awaiting" : statusClass), title: STATUS_LABEL[status] || status }),
    h("div", { class: "run-title" },
      h("h1", { class: "renamable", title: `${s.objective}\n\nClick to rename`, tabindex: "0",
        onclick: (e) => editInPlace(e.currentTarget, s.id, runName({ title: d.title, objective: s.objective })),
        onkeydown: (e) => { if (e.key === "Enter") editInPlace(e.currentTarget, s.id, runName({ title: d.title, objective: s.objective })); } },
        runName({ title: d.title, objective: s.objective })),
      h("div", { class: "run-meta rh-ctx" }, ctx.flatMap((n, i) => (i ? [h("span", { class: "rh-sep", "aria-hidden": "true" }, "·"), n] : [n])))),
    h("div", { class: "rh-actions" },
      canStop && h("button", { class: "btn rh-stop", type: "button", onclick: stopRun, title: "Stop after the current step" }, icon("stop"), "Stop"),
      // A named toggle (not another panel icon): "Plan 2/5", or "Details" without a plan.
      h("button", { class: "btn ghost side-toggle" + (open ? " on" : ""), type: "button", onclick: toggleSide,
        title: open ? "Hide plan & details" : "Show plan & details", "aria-pressed": String(open) },
        icon(hasPlan ? "listCheck" : "panel"), hasPlan ? "Plan" : "Details",
        hasPlan && h("span", { class: "progress-mini" }, `${closedSteps}/${s.plan.length}`)),
      copyButton(() => s.objective, "Copy the objective")));
  runEls.side.replaceChildren(...sideNodes(d, status));

  const sc = runEls.scroller;
  const nearBottom = sc.scrollHeight - sc.scrollTop - sc.clientHeight < 80;
  const focusedId = document.activeElement && document.activeElement.id;
  const nodes = transcriptNodes(d);
  animateNew(nodes);
  runEls.transcript.replaceChildren(...nodes);
  if (focusedId === "deny-reason") $("#deny-reason")?.focus();
  if (nearBottom || app.version === null) sc.scrollTop = sc.scrollHeight;
  syncToBottom();
  updateTitle();

  const c = runEls.composer;
  const closed = s.change && s.change.status !== "open";
  // A missing realm model only blocks when nothing else was picked for the run.
  const picked = storedModel(`run:${app.realm}/${app.route.id}`) || s.model_choice;
  const noModel = !!(app.realmDetail && app.realmDetail.model_error) && (!picked || picked === "auto");
  const disabled = d.live || closed || noModel;
  runEls.wrap.classList.toggle("live", !!d.live && !d.pending);
  // While the run works you can already write your next message; it sends once it stops.
  c.ta.disabled = closed;
  c.block(disabled);
  if (c.picker) c.picker.refresh();
  c.ta.placeholder = d.live
    ? (d.elsewhere ? "This chat is in progress in another Airth process…" : "Airth is working… write your next message, it can be sent when it stops.")
    : closed ? `This chat's change was ${s.change.status}. Start a new chat to keep going.`
    : status === "interrupted" ? "This chat was interrupted. Send a message to resume it…" : "Send a follow-up…";
}

function transcriptNodes(d) {
  const s = d.state;
  const events = new Map();
  for (const e of s.events) if (e.tool_use_id) events.set(e.tool_use_id, e);
  const results = new Map();
  for (const m of s.messages)
    if (m.role === "user") for (const b of m.content) if (b.type === "tool_result") results.set(b.tool_use_id, b);

  // The call waiting for approval is the first unanswered call in the last turn.
  let awaitingId = null;
  if (d.pending) {
    const last = [...s.messages].reverse().find((m) => m.role === "assistant");
    const call = last && last.content.find((b) => b.type === "tool_use" && !events.has(b.id) && !results.has(b.id));
    awaitingId = call && call.id;
  }

  const nodes = [];
  // Consecutive thinking and tool calls fold into one activity line.
  let group = [];
  const flush = (last) => {
    if (group.length === 1) nodes.push(group[0].node);
    else if (group.length > 1) nodes.push(activityGroup(group, last && d.live));
    group = [];
  };
  const lastAt = s.messages.length - 1;
  const formAt = !d.live && lastAt >= 0 && s.messages[lastAt].role === "assistant"
    && numberedQuestions(s.messages[lastAt].content.filter((b) => b.type === "text").map((b) => b.text).join("\n")).length ? lastAt : -1;
  s.messages.forEach((m, mi) => {
    if (s.compaction && mi === s.compaction.upto) {
      flush(false);
      nodes.push(keyed(keyedDetails(`compact:${mi}`, { class: "thinking" },
        h("summary", null, "Earlier conversation summarized to save tokens"),
        h("div", { class: "prose", html: mdc(s.compaction.summary.replace(RUNTIME_PREFIX, "").trim()) })), `compact:${mi}`));
    }
    if (m.role === "user") {
      const texts = m.content.filter((b) => b.type === "text");
      if (texts.length) flush(false);
      texts.forEach((b, ti) => {
        if (b.text.startsWith(RUNTIME_PREFIX)) {
          const body = b.text.slice(RUNTIME_PREFIX.length).trim();
          // Long runtime texts are instructions for the model: folded, behind a plain-language line.
          nodes.push(keyed(body.length > 400
            ? keyedDetails(`rt:${mi}:${ti}`, { class: "runtime-brief" },
                h("summary", null, h("span", { class: "ns-star", "aria-hidden": "true" }, "★"), " ", runtimeLabel(body), h("span", { class: "muted" }, " · how Airth was asked to work")),
                h("div", { class: "prose", html: mdc(body) }))
            : h("div", { class: "msg runtime" }, h("div", { class: "who" }, "Airth"), h("div", { class: "prose", html: mdc(body) })), `rt:${mi}:${ti}`));
        } else {
          nodes.push(keyed(h("div", { class: "msg user" + (mi === 0 ? " objective" : "") },
            h("div", { class: "who" }, mi === 0 ? "Objective" : "You"),
            h("div", { class: "prose", html: mdc(b.text) })), `u:${mi}:${ti}`));
        }
      });
      return;
    }
    m.content.forEach((b, bi) => {
      if (b.type === "reasoning" && b.summary && b.summary.trim()) {
        group.push({ kind: "think", node: keyed(keyedDetails(`think:${mi}:${bi}`, { class: "thinking" },
          h("summary", null, "Thinking"),
          h("div", { class: "prose", html: mdc(b.summary) })), `think:${mi}:${bi}`) });
      } else if (b.type === "text" && b.text.trim()) {
        flush(false);
        // The questions of the latest answer are shown once, in the form below it.
        const text = mi === formAt ? withoutQuestions(b.text) : b.text;
        if (text.trim()) nodes.push(keyed(assistantMessage(text), `t:${mi}:${bi}`));
      } else if (b.type === "tool_use") {
        const ev = events.get(b.id), res = results.get(b.id);
        group.push({ kind: "tool", name: b.name, input: b.input || {},
          failed: (ev && (ev.status === "error" || ev.status === "denied")) || (!ev && res && res.is_error),
          node: keyed(toolCard(b, ev, res, d.live, b.id === awaitingId), "tool:" + b.id) });
      }
    });
  });
  flush(true);
  const answers = nodes.filter((n) => n.classList.contains("assistant"));
  if (answers.length) answers[answers.length - 1].classList.add("latest");

  const lastMsg0 = s.messages[s.messages.length - 1];
  if (!d.live && lastMsg0 && lastMsg0.role === "assistant") {
    const text = lastMsg0.content.filter((b) => b.type === "text").map((b) => b.text).join("\n");
    const questions = numberedQuestions(text);
    const key = `answers:${s.id}:${s.messages.length - 1}`;
    if (questions.length) nodes.push(keyed(answerForm(questions, key, speaksFrench(s)), key));
    else if (s.north_star && s.objective.startsWith("Define my North Star")) nodes.push(keyed(northStarReplies(s), "ns-replies:" + s.messages.length));
  }
  const checking = s.change && s.change.checks && !s.change.checks.finished && d.live;
  if (d.pending) nodes.push(keyed(approvalCard(d.pending), "approval:" + d.pending.id));
  else if (d.live && !checking) {
    const lastMsg = s.messages[s.messages.length - 1];
    let label = "Thinking…";
    if (d.stopping) label = "Stopping after the current step…";
    else if (lastMsg && lastMsg.role === "assistant") {
      const running = [...lastMsg.content].reverse().find((b) => b.type === "tool_use" && !events.has(b.id) && !results.has(b.id));
      label = running ? activeLabel(running.name, running.input || {}) : "Running…";
    }
    nodes.push(keyed(working(label), "working"));
  } else if (!d.live) {
    const status = d.interrupted ? "interrupted" : s.status;
    const last = s.messages[s.messages.length - 1];
    const asks = last && last.role === "assistant" && (numberedQuestions(last.content.filter((b) => b.type === "text").map((b) => b.text).join("\n")).length > 0
      || (s.north_star && s.objective.startsWith("Define my North Star") && !s.events.some((e) => e.action === "north_star" && e.status === "ok")));
    const text = {
      completed: asks ? "Your turn: Airth is waiting for your answer." : `Completed after ${s.iterations} iteration${s.iterations === 1 ? "" : "s"}.`,
      failed: "The chat failed.",
      stopped: "The chat stopped.",
      interrupted: "The chat was interrupted: the process exited before it finished.",
    }[status];
    const showReport = status !== "completed" && s.report;
    const closed = s.change && s.change.status !== "open";
    const canResume = status !== "completed" && !closed && !d.elsewhere;
    nodes.push(keyed(h("div", { class: "end " + status },
      h("div", { class: "grow" }, text, showReport && h("pre", null, s.report),
        canResume && h("div", { class: "muted small" }, "Resume to let Airth check where it stopped and carry on with its plan, or send a message to redirect it.")),
      canResume && h("button", { class: "btn primary", type: "button", disabled: !!(app.realmDetail && app.realmDetail.model_error), onclick: resumeRun }, "Resume")), "end:" + status));
  }
  if (s.change) nodes.push(keyed(changePanel(d), "change"));
  return nodes;
}

const RUNTIME_PREFIX = "[Airth runtime]";

/** What a long runtime instruction is about, for the owner. */
function runtimeLabel(body) {
  if (body.includes("define this realm's North Star")) return "Airth asks you a few questions, proposes a plan, and records it once you agree";
  if (body.includes("conversation grew long")) return "Earlier conversation summarized";
  return "Instructions Airth received";
}

/** The numbered questions at the end of an answer ("1. Where do you stand? ..."), if there are at least two. */
function numberedQuestions(text) {
  const items = [];
  for (const line of text.split("\n")) {
    const m = line.match(/^\s*(\d+)[.)]\s+(.+)$/);
    if (m && m[2].includes("?")) items.push(m[2].replace(/\*\*|__|`/g, "").trim());
  }
  return items.length >= 2 ? items : [];
}

/** Whether the conversation is in French (the app's own words are English, but what it sends for the owner follows their language). */
function speaksFrench(s) {
  // What the owner and Airth wrote, not Airth's own (English) runtime notes.
  const text = s.messages.slice(-4).flatMap((m) => m.content).filter((b) => b.type === "text" && !b.text.startsWith(RUNTIME_PREFIX)).map((b) => b.text).join(" ").toLowerCase();
  const hits = (text.match(/\b(je|tu|vous|est|les|des|une|pour|avec|que|qui|pas|mon|ton|c'est|qu'est-ce)\b/g) || []).length;
  const en = (text.match(/\b(the|you|is|are|and|for|with|what|your|this|that)\b/g) || []).length;
  return hits > en;
}

/** The text without its numbered questions (they move to the answer form). */
function withoutQuestions(text) {
  return text.split("\n").filter((line) => !(/^\s*\d+[.)]\s+/.test(line) && line.includes("?"))).join("\n").replace(/\n{3,}/g, "\n\n");
}

/** Answers typed in the question form, kept across redraws (the transcript is rebuilt on every poll). */
const answerDrafts = new Map();

/** One field per question, sent as a single follow-up. */
function answerForm(questions, key, fr) {
  const fields = questions.map((q, i) => {
    const ta = h("textarea", { rows: 1, placeholder: "Your answer (or leave empty)", "aria-label": q });
    ta.value = answerDrafts.get(`${key}:${i}`) || "";
    const fit = () => { ta.style.height = "auto"; ta.style.height = ta.scrollHeight + "px"; };
    ta.addEventListener("input", () => { answerDrafts.set(`${key}:${i}`, ta.value); fit(); });
    setTimeout(fit, 0);
    return { q, ta };
  });
  const send = async () => {
    const lines = fields.map((f, i) => `${i + 1}. ${f.q}\n→ ${f.ta.value.trim() || (fr ? "(pas de réponse)" : "(no answer)")}`);
    if (fields.every((f) => !f.ta.value.trim())) return fields[0].ta.focus();
    btn.disabled = true;
    try {
      await sendFollowUp(lines.join("\n\n"));
      fields.forEach((_, i) => answerDrafts.delete(`${key}:${i}`));
    } catch (e) { flash(e.message, true); btn.disabled = false; }
  };
  const btn = h("button", { class: "btn primary", type: "button", onclick: send }, "Send my answers");
  return h("div", { class: "answer-form" },
    h("div", { class: "answer-head" }, "Answer here, or reply in your own words below."),
    fields.map((f, i) => h("label", { class: "answer-q" }, h("span", { class: "answer-n" }, i + 1), h("span", { class: "grow" }, h("span", { class: "answer-text" }, f.q), f.ta))),
    h("div", { class: "row-end" }, btn));
}

/** Suggested replies under the latest answer of a run defining the North Star. */
function northStarReplies(s) {
  const fr = speaksFrench(s);
  const recorded = s.events.some((e) => e.action === "north_star" && e.status === "ok");
  if (recorded) return h("div", { class: "quick-replies" },
    h("button", { class: "chip-btn primary", type: "button", onclick: () => go(`#/r/${enc(app.realm)}`) }, "★ See my North Star"));
  return h("div", { class: "quick-replies" },
    h("button", { class: "chip-btn primary", type: "button", onclick: () => sendFollowUp(fr ? "C'est bon, enregistre-le tel que proposé." : "That looks good. Record it as proposed.").catch((e) => flash(e.message, true)) }, "Looks good, record it"),
    h("button", { class: "chip-btn", type: "button", onclick: () => { const ta = $(".composer textarea"); if (ta) { ta.value = fr ? "Je changerais : " : "I'd change: "; ta.focus(); ta.dispatchEvent(new Event("input")); } } }, "Change something…"));
}

/** Past tense for a finished activity line: [verb, one, many]. */
const DONE_VERBS = {
  read_file: ["read", "file", "files"],
  list_directory: ["listed", "folder", "folders"],
  write_file: ["wrote", "file", "files"],
  edit_file: ["edited", "file", "files"],
  execute_command: ["ran", "command", "commands"],
  http_request: ["sent", "request", "requests"],
  remember: ["saved", "memory", "memories"],
  propose_skill: ["proposed", "skill", "skills"],
  north_star: ["recorded", "North Star update", "North Star updates"],
};

/** "Read 4 files, ran 2 commands, updated the plan" */
function describeTools(tools) {
  const counts = new Map();
  for (const t of tools) counts.set(t.name, (counts.get(t.name) || 0) + 1);
  const parts = [...counts].map(([name, n]) => {
    if (name === "update_plan") return "updated the plan";
    const v = DONE_VERBS[name];
    if (v) return `${v[0]} ${n} ${n === 1 ? v[1] : v[2]}`;
    return `used ${name.replace(/_/g, " ")}` + (n > 1 ? ` ×${n}` : "");
  });
  const text = parts.join(", ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

const ACTIVE_VERBS = {
  read_file: "Reading", list_directory: "Listing", write_file: "Writing", edit_file: "Editing",
  execute_command: "Running", http_request: "Calling", remember: "Remembering", update_plan: "Updating the plan",
  north_star: "Recording North Star progress:",
};

/** What Airth is doing right now, e.g. "Running $ cargo test". */
function activeLabel(name, input) {
  if (name === "update_plan") return "Updating the plan…";
  const what = toolLabel(name, input);
  return `${ACTIVE_VERBS[name] || "Using " + name.replace(/_/g, " ")} ${what && what.length < 90 ? what : ""}`.trim() + "…";
}

/** Several steps in a row, folded into one line you can open. */
function activityGroup(items, live) {
  const tools = items.filter((i) => i.kind === "tool");
  const failed = tools.filter((t) => t.failed).length;
  const key = "act:" + items[0].node.dataset.k;
  const names = [...new Set(tools.map((t) => t.name))].slice(0, 4);
  return keyed(keyedDetails(key, { class: "activity" + (live ? " live" : "") },
    h("summary", null,
      h("span", { class: "act-icons", "aria-hidden": "true" }, names.map((n) => h("span", { class: "icon" }, toolIcon(n)))),
      h("span", { class: "act-label" }, tools.length ? describeTools(tools) : "Thought it through"),
      failed > 0 && h("span", { class: "chip denied" }, `${failed} not run`),
      h("span", { class: "act-count" }, `${items.length} steps`)),
    h("div", { class: "act-body" }, items.map((i) => i.node))), key);
}

/** An answer, with a copy button under it. */
function assistantMessage(text) {
  return h("div", { class: "msg assistant" },
    h("div", { class: "prose", html: mdc(text) }),
    h("div", { class: "msg-actions" }, copyButton(() => text, "Copy answer")));
}

function copyButton(getText, label) {
  const btn = h("button", { class: "copy-btn", type: "button", title: label, "aria-label": label }, icon("copy"));
  btn.addEventListener("click", () => copyText(getText(), btn));
  return btn;
}

async function copyText(text, btn) {
  try {
    await navigator.clipboard.writeText(text);
    if (btn) { btn.classList.add("done"); setTimeout(() => btn.classList.remove("done"), 1400); }
    flash("Copied");
  } catch {
    flash("Could not copy to the clipboard", true);
  }
}

// Code blocks rendered by md() carry a constant copy button; one listener serves them all.
document.addEventListener("click", (e) => {
  const btn = e.target.closest && e.target.closest(".code-copy");
  if (!btn) return;
  const code = btn.closest(".code").querySelector("code");
  copyText(code ? code.textContent : "", btn);
});

const STEP_MARK = { pending: "", in_progress: "", done: "✓", blocked: "!", skipped: "–" };

/** The right column: the plan, live, then what the run cost. */
function sideNodes(d, status) {
  const s = d.state;
  const plan = s.plan || [];
  const nodes = [];
  if (plan.length) {
    const closed = plan.filter((p) => p.status === "done" || p.status === "skipped").length;
    const pct = Math.round((100 * closed) / plan.length);
    // Set through the CSSOM: the page's CSP blocks inline style attributes.
    const fill = h("span");
    fill.style.width = pct + "%";
    // Flash the steps whose status just changed.
    const first = runEls.steps === null;
    const seen = first ? new Set() : runEls.steps;
    const steps = plan.map((p, i) => {
      const k = `${i}:${p.status}:${p.title}`;
      const changed = !first && !seen.has(k);
      seen.add(k);
      return h("li", { class: "step " + p.status + (changed ? " changed" : "") },
        h("span", { class: "mark", title: p.status.replace("_", " ") }, STEP_MARK[p.status] ?? "•"),
        h("div", { class: "grow" }, h("div", { class: "step-title" }, p.title), p.note && h("div", { class: "step-note" }, p.note)));
    });
    runEls.steps = seen;
    nodes.push(h("section", { class: "side-section" },
      h("div", { class: "side-head" }, h("span", null, "Plan"), h("span", { class: "progress-mini" }, `${closed}/${plan.length}`)),
      h("div", { class: "plan-bar", "aria-hidden": "true" }, fill),
      h("ol", { class: "plan-steps" + (d.live ? " live" : "") }, steps)));
  } else {
    nodes.push(h("section", { class: "side-section" },
      h("div", { class: "side-head" }, h("span", null, "Plan")),
      h("p", { class: "side-empty" }, d.live ? "Airth writes its plan here as soon as the work takes more than a couple of steps." : "No plan for this chat.")));
  }
  const row = (k, v, title) => v != null && v !== false && h("div", { class: "kv", title: title || "" }, h("span", { class: "k" }, k), h("span", { class: "v" }, v));
  nodes.push(h("section", { class: "side-section" },
    h("div", { class: "side-head" }, h("span", null, "Details")),
    row("Status", STATUS_LABEL[status] || status),
    s.origin && row("Started by", s.origin.objective, s.origin.trigger),
    row("Iterations", String(s.iterations)),
    row("Tool calls", String(s.events.length)),
    (s.input_tokens || s.output_tokens) ? row("Tokens", `${kfmt(s.input_tokens)} in · ${kfmt(s.output_tokens)} out`, `${s.input_tokens} in / ${s.output_tokens} out`) : null,
    s.usage && s.usage.length ? row("Model", s.usage[s.usage.length - 1].model) : null,
    s.tier ? row("Routing", s.tier === "fast" ? "fast model (simple ask)" : "main model", "With a fast connection, each ask is triaged: simple ones run on the fast model, the rest on the main one") : null,
    s.usage && s.usage.some((u) => u.cache_read) ? (() => {
      const input = s.usage.reduce((a, u) => a + u.input, 0);
      const cached = s.usage.reduce((a, u) => a + u.cache_read, 0);
      return row("From cache", `${input ? Math.round((100 * cached) / input) : 0}% of input`, `${cached} of ${input} input tokens read from the prompt cache`);
    })() : null,
    s.change && row("Change", s.change.status),
    row("Updated", ago(s.updated_at)),
    row("Chat", s.id)));
  return nodes;
}

async function resumeRun() {
  try {
    await api(`${runPath(app.realm, app.route.id)}/resume`, {});
    app.version = null;
    await Promise.all([pollRun(), refreshRuns()]);
  } catch (e) {
    flash(e.message, true);
  }
}

function keyedDetails(key, attrs, ...children) {
  const el = h("details", attrs, ...children);
  el.open = app.openKeys.has(key);
  el.addEventListener("toggle", () => (el.open ? app.openKeys.add(key) : app.openKeys.delete(key)));
  return el;
}

function toolLabel(name, input) {
  switch (name) {
    case "read_file": return input.path;
    case "list_directory": return (input.path || ".") + (input.recursive ? "  (recursive)" : "");
    case "write_file": return `${input.path}  · ${(input.content || "").split("\n").length} lines`;
    case "execute_command": return "$ " + String(input.command || "").split("\n")[0];
    case "remember": return input.text;
    case "edit_file": return input.path;
    case "http_request": return `${input.method || "GET"} ${input.integration ? input.integration + ": " : ""}${input.url || ""}`;
    case "update_plan": {
      const steps = input.steps || [];
      return `${steps.filter((p) => p.status === "done" || p.status === "skipped").length}/${steps.length} steps done`;
    }
    case "north_star":
      return input.action === "set_milestones"
        ? `${(input.milestones || []).length} milestones`
        : `${input.milestone || "?"}${input.status ? " → " + input.status : ""}${input.note ? ": " + input.note : ""}`;
    default: return JSON.stringify(input);
  }
}

function toolCard(b, event, result, live, awaiting) {
  let status, label;
  if (event) { status = event.status; label = event.status; }
  else if (result) { status = result.is_error ? "denied" : "ok"; label = result.is_error ? "not run" : "ok"; }
  else if (awaiting) { status = "awaiting"; label = "awaiting approval"; }
  else if (live) { status = "running"; label = "running"; }
  else { status = "denied"; label = "not run"; }
  const output = event ? event.result : result ? result.content : null;
  const input = b.input || {};

  const body = [];
  if (b.name === "execute_command") body.push(section("Command", h("pre", { class: "out" }, input.command)));
  else if (b.name === "write_file") body.push(section("New content · " + input.path, h("pre", { class: "out" }, input.content)));
  else if (b.name === "edit_file") body.push(section("Edit · " + input.path, diffView((input.old_text || "").split("\n").map((l) => "-" + l).concat((input.new_text || "").split("\n").map((l) => "+" + l)).join("\n"), "diff")));
  else if (b.name === "http_request") body.push(section("Request", h("pre", { class: "out" }, toolLabel(b.name, input) + (input.body ? "\n\n" + input.body : ""))));
  else if (!["read_file", "list_directory", "remember", "update_plan"].includes(b.name)) body.push(section("Input", h("pre", { class: "out" }, JSON.stringify(input, null, 2))));
  if (output != null) body.push(section("Result", h("pre", { class: "out" }, output || "(empty)")));

  return keyedDetails("tool:" + b.id, { class: "tool" },
    h("summary", null,
      h("span", { class: "icon" }, toolIcon(b.name)),
      h("span", { class: "label" }, h("span", { class: "name" }, b.name), toolLabel(b.name, input)),
      h("span", { class: "chip " + status }, label)),
    h("div", { class: "body" }, body));
}

const section = (title, ...children) => h("div", null, h("h4", null, title), ...children);

function diffView(text, kind) {
  const el = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    let cls = "";
    if (kind === "command") cls = line.startsWith("$ ") ? "cmd" : "hunk";
    else if (kind === "plain") cls = "cmd";
    else if (/^(\+\+\+|---)\s/.test(line) || /^(create|modify)\s/.test(line) || line.startsWith("diff --git")) cls = "file";
    else if (line.startsWith("@@")) cls = "hunk";
    else if (/^(index |new file|deleted file|similarity|rename )/.test(line)) cls = "meta";
    else if (line.startsWith("+")) cls = "add";
    else if (line.startsWith("-")) cls = "del";
    el.append(h("div", { class: cls }, line || " "));
  }
  return el;
}

/** A North Star or memory change, as text rather than code. */
function readablePreview(text) {
  if (text.startsWith("New North Star: ")) {
    const field = (prefix) => (text.split("\n").find((l) => l.startsWith(prefix)) || "").slice(prefix.length);
    const was = field("(was: ").replace(/\)$/, "");
    const why = field("Why: ");
    return h("div", { class: "preview-readable" },
      h("div", { class: "pr-label" }, "Now"), h("div", { class: "pr-old" }, was),
      h("div", { class: "pr-label" }, "Becomes"), h("div", { class: "pr-new" }, field("New North Star: ")),
      why && h("div", { class: "muted small" }, "Why: " + why));
  }
  return h("div", { class: "preview-readable" }, text.split("\n").filter((l) => l.trim()).map((l) =>
    /^\d+\. /.test(l) ? h("div", { class: "pr-item" }, l) : h("div", { class: "pr-line" }, l)));
}

function approvalCard(p) {
  const title = p.tool === "north_star" && p.preview.startsWith("New North Star:") ? "Change your North Star?"
    : { write_file: "Write this file?", edit_file: "Apply this edit?", execute_command: "Run this command?", remember: "Remember this?", http_request: "Send this request?", north_star: "Record this on the North Star?" }[p.tool] || `Allow ${p.tool}?`;
  const kind = p.tool === "execute_command" ? "command" : ["remember", "http_request", "north_star"].includes(p.tool) ? "plain" : "diff";
  const reason = h("input", {
    id: "deny-reason", type: "text", placeholder: "Reason for denying (optional; Airth sees it)",
    value: app.reason, autocomplete: "off",
    oninput: (e) => (app.reason = e.target.value),
    onkeydown: (e) => { if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) { e.preventDefault(); answer(false); } },
  });
  return h("div", { class: "approval", role: "region", "aria-label": "Approval required" },
    h("div", { class: "approval-head" },
      h("h3", null, title),
      h("span", { class: "chip rule-ask" }, p.capability)),
    p.warnings.map((w) => h("div", { class: "approval-warn" }, "⚠ " + w)),
    ["north_star", "remember"].includes(p.tool) ? readablePreview(p.preview) : diffView(p.preview, kind),
    h("div", { class: "approval-actions" },
      h("button", { class: "btn primary", type: "button", onclick: () => answer(true) }, "Approve ", h("kbd", null, modKey + "↵")),
      p.grantable && h("button", { class: "btn", type: "button", title: "Later requests like this one in this chat are approved without asking", onclick: () => answer(true, true) }, `Allow ${p.grant_label} for this chat`),
      h("button", { class: "btn danger", type: "button", onclick: () => answer(false) }, "Deny"),
      reason));
}

// ---------- change review ----------

let confirmDiscard = false;

function changePanel(d) {
  const s = d.state;
  const c = s.change;
  const checks = c.checks;
  const hasChanges = c.base !== c.head;
  const checksCurrent = checks && checks.commit === c.head;
  const checksRunning = checks && !checks.finished && d.live;
  const passed = checksCurrent && checks.finished && checks.results.every((r) => r.ok);
  const failed = checks && checks.results.find((r) => !r.ok);
  const open = c.status === "open";
  const canAdopt = open && hasChanges && !d.live && (passed || !d.checks_required);

  const head = h("div", { class: "change-head" },
    h("div", { class: "grow" },
      h("h3", null, "Change"),
      h("div", { class: "muted small" }, `branch ${c.branch} · from ${c.base.slice(0, 8)}`)),
    h("span", { class: "chip " + (c.status === "adopted" ? "completed" : c.status === "discarded" ? "stopped" : "running") }, c.status));

  const files = hasChanges
    ? h("div", { class: "files" }, c.files.map((f) => {
        const [st, ...rest] = f.split("\t");
        return h("div", { class: "file-row" }, h("span", { class: "st st-" + st[0] }, st[0]), h("span", null, rest.join(" → ")));
      }))
    : h("div", { class: "muted small pad" }, d.live ? "Changes appear here when Airth finishes." : "No files changed.");

  let checkList = null;
  if (checks) {
    checkList = h("div", { class: "checks" },
      h("div", { class: "checks-title" }, "Checks", !checksCurrent && h("span", { class: "muted small" }, " · ran on an older commit")),
      checks.results.map((r, i) =>
        keyedDetails(`check:${checks.started_at}:${i}`, { class: "check " + (r.ok ? "ok" : "err") },
          h("summary", null, h("span", { class: "mark" }, r.ok ? "✓" : "✗"), h("code", null, r.command), h("span", { class: "muted small" }, (r.duration_ms / 1000).toFixed(1) + "s")),
          h("pre", { class: "out" }, r.output || "(no output)"))),
      checksRunning && h("div", { class: "working small" }, h("span", { class: "spinner" }), "Running checks…"),
      !checksRunning && !checks.finished && h("div", { class: "muted small" }, "The checks did not finish."));
  } else if (open && hasChanges && d.checks_required && !d.live) {
    checkList = h("div", { class: "muted small pad" }, "Checks have not run on this change.");
  }

  const actions = h("div", { class: "change-actions" });
  if (open && hasChanges && !d.live) {
    actions.append(
      h("button", { class: "btn primary", type: "button", disabled: !canAdopt, title: canAdopt ? "Merge into your checkout" : "The checks must pass first",
        onclick: () => changeAction("adopt") }, "Adopt into checkout"),
      h("button", { class: "btn", type: "button", onclick: () => changeAction("checks") }, checks ? "Re-run checks" : "Run checks"),
      failed && h("button", { class: "btn", type: "button", onclick: () => askToFix(failed) }, "Ask Airth to fix"),
      confirmDiscard
        ? h("span", { class: "confirm" }, "Discard this change? ",
            h("button", { class: "btn danger", type: "button", onclick: () => { confirmDiscard = false; changeAction("discard"); } }, "Discard"),
            h("button", { class: "btn ghost", type: "button", onclick: () => { confirmDiscard = false; renderRun(); } }, "Keep"))
        : h("button", { class: "btn danger ghost", type: "button", onclick: () => { confirmDiscard = true; renderRun(); } }, "Discard…"));
  }
  if (hasChanges) {
    const shown = app.diff && app.diff.runId === s.id;
    actions.append(h("button", { class: "btn ghost", type: "button", onclick: toggleDiff }, shown ? "Hide diff" : "View diff"));
  }
  const err = app.changeError ? h("div", { class: "error-banner" }, app.changeError) : null;
  const diff = app.diff && app.diff.runId === s.id ? diffView(app.diff.text || "(empty diff)", "diff") : null;

  return h("div", { class: "change" }, head, files, checkList, err, actions.childNodes.length ? actions : null, diff);
}

async function toggleDiff() {
  if (app.diff && app.diff.runId === app.route.id) app.diff = null;
  else {
    try {
      const { diff } = await api(`${runPath(app.realm, app.route.id)}/diff`);
      app.diff = { runId: app.route.id, text: diff };
    } catch (e) { app.changeError = e.message; }
  }
  renderRun();
}

async function changeAction(action) {
  app.changeError = null;
  try {
    await api(`${runPath(app.realm, app.route.id)}/${action}`, {});
    app.version = null;
    if (action !== "checks") app.diff = null;
    await Promise.all([pollRun(), refreshRuns()]);
  } catch (e) {
    app.changeError = e.message;
    renderRun();
  }
}

function askToFix(failed) {
  const lines = failed.output.split("\n");
  const tail = lines.slice(-60).join("\n");
  sendFollowUp(`The check \`${failed.command}\` failed:\n\n\`\`\`\n${tail}\n\`\`\`\n\nFix the problem, then summarize what you changed.`)
    .catch((e) => { app.changeError = e.message; renderRun(); });
}

async function answer(approve, forRun) {
  const d = app.run;
  if (!d || !d.pending || app.busy) return;
  app.busy = true;
  try {
    await api(`${runPath(app.realm, app.route.id)}/approval`, {
      approval_id: d.pending.id, approve, for_run: !!forRun, reason: approve ? "" : app.reason,
    });
    app.reason = "";
    app.version = null;
    await pollRun();
  } catch (e) {
    app.error = e.message;
  } finally {
    app.busy = false;
  }
}

async function stopRun() {
  try {
    await api(`${runPath(app.realm, app.route.id)}/stop`, {});
    await pollRun();
  } catch (e) {
    app.error = e.message;
  }
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && app.route.view === "run" && app.run && app.run.pending) {
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag !== "TEXTAREA") { e.preventDefault(); answer(true); }
  }
});

// ---------- realm settings ----------

const CAP_HELP = {
  read: "Read files and list directories",
  write: "Create and change files",
  execute: "Run shell commands",
  memory: "Save facts to this realm's memory",
  network: "Call integrations and websites",
  skill: "Use skills you installed",
  apps: "Calendar, Reminders, Notes, Mail, Safari and the clipboard",
};
const CAP_NAME = {
  read: "Read files", write: "Change files", execute: "Run commands", memory: "Remember things",
  network: "Use the web and integrations", skill: "Use skills", apps: "Use your Mac apps",
};

const MODEL_TIERS = [
  { id: "main", name: "Main", help: "Answers requests; every request when there is no fast model.", auto: { label: "Default", sub: "Anthropic · claude-opus-5-5" } },
  { id: "fast", name: "Fast", help: "Simple requests. Optional.", auto: { label: "None", sub: "every request goes to the main model" } },
  { id: "fallback", name: "Fallback", help: "Only when the others fail (an outage, no credit). Optional.", auto: { label: "None", sub: "no fallback" } },
];
const SOURCES = { realm: "this realm", all: "all realms", env: "environment" };

/** The realm's main, fast and fallback models: a picker and an effort each. */
function modelSettings(d) {
  const m = d.models || {};
  const allRealms = h("input", { type: "checkbox" });
  const choiceOf = (t) => {
    const s = m[t] || {};
    if (!s.model && !(t === "main" && s.provider)) return "auto";
    const provider = s.provider || (t !== "main" && m.main && m.main.provider) || "anthropic";
    return `${provider}/${s.model || ""}`;
  };
  const save = async (tier, choice, effort) => {
    const [provider, ...rest] = choice === "auto" ? ["", ""] : choice.split("/");
    try {
      app.realmDetail = await api(`${realmPath(d.name)}/models`, {
        tier, provider, model: rest.join("/"), effort, all_realms: allRealms.checked,
      });
      flash(allRealms.checked ? "Saved for all realms" : "Saved");
      loadModels(true);
      renderMain();
      renderSidebar();
    } catch (e) { flash(e.message, true); }
  };
  const rows = MODEL_TIERS.map((t) => {
    const s = m[t.id] || {};
    const choice = choiceOf(t.id);
    const picker = modelPicker({ get: () => choice, set: (c) => save(t.id, c, s.effort || ""), auto: t.auto });
    const effort = h("select", { class: "pill-select", "aria-label": `${t.name} model effort`, title: "Reasoning effort: higher thinks more and costs more" },
      ["", "low", "medium", "high", "xhigh", "max"].map((e) => h("option", { value: e }, e ? `Effort: ${e}` : "Effort: default")));
    effort.value = s.effort || "";
    effort.disabled = choice === "auto" && t.id !== "main";
    effort.addEventListener("change", () => save(t.id, choice, effort.value));
    return h("div", { class: "policy-row" },
      h("div", null,
        h("div", { class: "strong" }, t.name, s.source && h("span", { class: "chip", title: "Where this setting comes from" }, SOURCES[s.source] || s.source)),
        h("div", { class: "muted small" }, t.help)),
      h("div", { class: "model-controls" }, picker.el, effort));
  });
  return h("div", { class: "card" },
    h("div", { class: "policy" }, rows),
    h("label", { class: "check-row" }, allRealms,
      h("span", null, "Apply my next change to all realms",
        h("span", { class: "muted small block" }, "It also replaces this realm's own setting. Otherwise a change applies to this realm only, and a realm's own setting wins over the shared one."))),
    field("In use", d.model ? h("span", { class: "value" }, d.model) : h("span", { class: "value warn" }, d.model_error || "not configured")));
}

const SETTINGS_TABS = [
  ["general", "General", "gear"],
  ["models", "Models", "brain"],
  ["permissions", "Permissions", "check"],
  ["memory", "Memory", "notes"],
  ["access", "Access", "plug"],
  ["advanced", "Advanced", "terminal"],
];
const settingsUi = { addIntegration: false };

/** A home-relative path: easier to read than the full one. */
const tildePath = (p) => (p || "").replace(/^\/Users\/[^/]+(?=\/|$)/, "~");

function renderSettings(main) {
  const d = app.realmDetail;
  if (!d) return page(main, "Realm settings", "Loading…");
  const tab = SETTINGS_TABS.find((t) => t[0] === app.route.tab) || SETTINGS_TABS[0];
  const save = async (patch, msg) => {
    try {
      app.realmDetail = await api(realmPath(d.name), patch, "PATCH");
      flash(msg || "Saved");
      await refreshRealms();
      renderMain();
      renderSidebar();
    } catch (e) { flash(e.message, true); }
  };
  const remove = async (path) => {
    try { app.realmDetail = await api(path, undefined, "DELETE"); renderMain(); } catch (e) { flash(e.message, true); }
  };
  const section = (title, sub, ...body) => [title && h("h2", null, title), sub && h("p", { class: "muted lead" }, sub), ...body];

  const general = () => {
    const desc = h("textarea", { rows: 2, placeholder: "What this realm is about (Airth reads this in every chat)" });
    desc.value = d.description;
    desc.addEventListener("change", () => save({ description: desc.value.trim() }, "Description saved"));
    return [
      h("div", { class: "card" },
        field("Picture", pictureEditor({ path: `${realmPath(d.name)}/image`, version: d.image, text: d.name,
          done: async (detail) => { app.realmDetail = detail; await refreshRealms(); renderMain(); renderSidebar(); } }), "Shown in the app only."),
        field("Description", desc),
        field("Workspace", h("span", { class: "value path", title: d.root }, tildePath(d.root)), d.own_root ? null : "A private folder in Airth's home."),
        field("Changes", h("span", { class: "value" }, d.isolation === "worktree"
          ? "Each chat works on its own copy (a git worktree); you adopt or discard what it changed."
          : "Chats work directly in the workspace."))),
      ...section("Your profile", null,
        h("div", { class: "card" },
          h("label", { class: "toggle" },
            h("input", { type: "checkbox", checked: d.share_owner_profile, onchange: (e) => save({ share_owner_profile: e.target.checked }) }),
            h("span", null, h("span", null, "Share your profile with this realm's chats"),
              h("span", { class: "muted small block" }, "Your name, what you do, your preferences and boundaries, from ", h("a", { href: "#/owner" }, "your profile"), "."))))),
    ];
  };

  const models = () => section(null,
    "Which models this realm's chats use. With a fast model, Airth sorts each new request: simple ones (questions, lookups, short texts) go to the fast model, which costs much less; the rest go to the main model.",
    modelSettings(d),
    h("p", { class: "muted small" }, "A model's API key goes under ", h("a", { href: `#/r/${enc(d.name)}/settings/access` }, "Access"), "."));

  const permissions = () => section(null,
    "What Airth may do in this realm. Only you can change this.",
    h("div", { class: "legend muted small" },
      h("span", null, h("b", { class: "rule-allow" }, "Allow"), " does it without asking"),
      h("span", null, h("b", { class: "rule-ask" }, "Ask"), " waits for your approval"),
      h("span", null, h("b", { class: "rule-deny" }, "Deny"), " never")),
    h("div", { class: "card" }, h("div", { class: "policy" }, Object.keys(CAP_HELP).map((cap) =>
      h("div", { class: "policy-row" },
        h("div", null, h("div", { class: "strong" }, CAP_NAME[cap] || cap), h("div", { class: "muted small" }, CAP_HELP[cap])),
        h("div", { class: "seg", role: "radiogroup", "aria-label": CAP_NAME[cap] || cap },
          ["allow", "ask", "deny"].map((rule) =>
            h("button", { type: "button", role: "radio", "aria-checked": String(d.policy[cap] === rule), class: d.policy[cap] === rule ? "on rule-" + rule : "",
              onclick: () => save({ policy: { [cap]: rule } }) }, rule[0].toUpperCase() + rule.slice(1)))))))));

  const memory = () => {
    const input = h("input", { type: "text", placeholder: "Something Airth should remember in this realm…" });
    const add = async () => {
      if (!input.value.trim()) return;
      try { app.realmDetail = await api(`${realmPath(d.name)}/memory`, { text: input.value }); renderMain(); flash("Remembered"); }
      catch (e) { flash(e.message, true); }
    };
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") add(); });
    return section(null, "What Airth remembers in this realm. Chats in other realms never see it.",
      h("div", { class: "card" },
        h("div", { class: "inline-form first" }, input, h("button", { class: "btn", type: "button", onclick: add }, "Add")),
        d.memory.length ? d.memory.map((m) =>
          h("div", { class: "list-row" },
            h("div", { class: "grow" }, m.text, h("div", { class: "muted small" }, (m.source === "owner" ? "added by you" : "learned in a chat") + " · " + ago(m.created_at))),
            h("button", { class: "icon-btn", type: "button", title: "Forget", "aria-label": "Forget", onclick: () => remove(`${realmPath(d.name)}/memory/${enc(m.id)}`) }, "×")))
          : h("div", { class: "empty-row muted small" }, "Nothing yet. Chats add what they learn; you can add facts here.")));
  };

  const access = () => {
    const key = h("input", { type: "text", placeholder: "NAME, e.g. GITHUB_TOKEN", autocomplete: "off", spellcheck: "false" });
    const val = h("input", { type: "password", placeholder: "value", autocomplete: "new-password" });
    const addSecret = async () => {
      try {
        app.realmDetail = await api(`${realmPath(d.name)}/secrets`, { key: key.value.trim().toUpperCase(), value: val.value });
        renderMain();
        flash("Key saved");
      } catch (e) { flash(e.message, true); }
    };
    val.addEventListener("keydown", (e) => { if (e.key === "Enter") addSecret(); });

    const integ = {
      name: h("input", { type: "text", placeholder: "name, e.g. github", autocomplete: "off", spellcheck: "false" }),
      url: h("input", { type: "text", placeholder: "base URL, e.g. https://api.github.com", autocomplete: "off", spellcheck: "false" }),
      header: h("input", { type: "text", placeholder: "header (optional), e.g. Authorization: Bearer [secret:GITHUB_TOKEN]", autocomplete: "off", spellcheck: "false" }),
      desc: h("input", { type: "text", placeholder: "what it is for (Airth reads this)" }),
    };
    const addIntegration = async () => {
      const headers = {};
      const hv = integ.header.value.trim();
      if (hv) {
        const i = hv.indexOf(":");
        if (i < 1) return flash("Headers look like “Name: value”", true);
        headers[hv.slice(0, i).trim()] = hv.slice(i + 1).trim();
      }
      try {
        app.realmDetail = await api(`${realmPath(d.name)}/integrations`, {
          name: integ.name.value.trim(), base_url: integ.url.value.trim(), description: integ.desc.value.trim(), headers,
        });
        settingsUi.addIntegration = false;
        renderMain();
        flash("Integration saved");
      } catch (e) { flash(e.message, true); }
    };

    return [
      ...section("Keys", "Given to this realm's commands as environment variables, and never shown to the model. Model keys (ANTHROPIC_API_KEY, OPENAI_API_KEY, XAI_API_KEY…) are used for this realm's models only.",
        h("div", { class: "card" },
          d.secrets.length ? d.secrets.map((s) =>
            h("div", { class: "list-row" },
              h("code", { class: "grow" }, s.key), s.provider && h("span", { class: "chip" }, "model"),
              h("button", { class: "icon-btn", type: "button", title: "Remove", "aria-label": "Remove " + s.key, onclick: () => remove(`${realmPath(d.name)}/secrets/${enc(s.key)}`) }, "×")))
            : h("div", { class: "empty-row muted small" }, "No keys yet."),
          h("div", { class: "inline-form" }, key, val, h("button", { class: "btn", type: "button", onclick: addSecret }, "Save")))),
      ...section("Integrations", "Services this realm's chats may call. A header can carry a key as [secret:NAME]: the runtime fills it in, only for requests under the base URL, and the model never sees it. Without an integration, runs reach only public websites, with no keys.",
        h("p", { class: "muted small" }, "For common services (Notion, Linear, GitHub, Stripe…), ",
          h("a", { href: `#/r/${enc(d.name)}/skills`, onclick: () => { skillUi.tab = "connectors"; } }, "Connectors"),
          " set this up in one step."),
        h("div", { class: "card" },
          d.integrations.length ? d.integrations.map((i) =>
            h("div", { class: "list-row" },
              h("div", { class: "grow" },
                h("div", null, h("span", { class: "strong" }, i.name), " ", h("code", { class: "path" }, i.base_url)),
                i.description && h("div", { class: "muted small" }, i.description),
                Object.entries(i.headers).map(([k, v]) => h("div", { class: "muted small" }, h("code", null, `${k}: ${v}`))),
                i.missing_secrets.length > 0 && h("div", { class: "warn small" }, "Missing key: " + i.missing_secrets.join(", "))),
              h("button", { class: "icon-btn", type: "button", title: "Remove", "aria-label": "Remove " + i.name, onclick: () => remove(`${realmPath(d.name)}/integrations/${enc(i.name)}`) }, "×")))
            : h("div", { class: "empty-row muted small" }, "No integrations."),
          settingsUi.addIntegration
            ? h("div", { class: "stack-form" },
                h("div", { class: "inline-form" }, integ.name, integ.url), integ.header, integ.desc,
                h("div", { class: "row-end" },
                  h("button", { class: "btn ghost", type: "button", onclick: () => { settingsUi.addIntegration = false; renderMain(); } }, "Cancel"),
                  h("button", { class: "btn primary", type: "button", onclick: addIntegration }, "Add integration")))
            : h("div", { class: "row-start" }, h("button", { class: "btn", type: "button", onclick: () => { settingsUi.addIntegration = true; renderMain(); } }, icon("plus"), "Add an integration")))),
    ];
  };

  const advanced = () => {
    const checks = h("textarea", { rows: 3, placeholder: "One command per line, e.g. cargo test" });
    checks.value = d.checks.join("\n");
    return [
      ...section("Checks", "Commands the runtime runs on every change before you can adopt it, in the chat's sandbox. Useful for code: tests, linters.",
        h("div", { class: "card" }, checks,
          h("div", { class: "row-end" }, h("button", { class: "btn", type: "button", onclick: () => save({ checks: checks.value.split("\n") }, "Checks saved") }, "Save checks")))),
      ...section("Realm", null,
        h("div", { class: "card" },
          h("div", { class: "policy-row" },
            h("div", null, h("div", { class: "strong" }, "Default realm"),
              h("div", { class: "muted small" }, d.default ? "Airth opens this realm when nothing else is chosen." : "The realm Airth opens when nothing else is chosen.")),
            d.default ? h("span", { class: "chip" }, "default") : h("button", { class: "btn", type: "button", onclick: () => save({ default: true }, "Default realm set") }, "Make default")),
          h("div", { class: "policy-row" },
            h("div", null, h("div", { class: "strong" }, d.archived ? "Archived" : "Archive"),
              h("div", { class: "muted small" }, "Hides the realm from the switcher and stops its objectives. Its chats, memory and files are kept; you can restore it.")),
            h("button", { class: "btn" + (d.archived ? "" : " danger"), type: "button", onclick: () => save({ archived: !d.archived }, d.archived ? "Restored" : "Archived") }, d.archived ? "Restore" : "Archive")))),
    ];
  };

  const body = { general, models, permissions, memory, access, advanced }[tab[0]]();
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner settings" },
    h("div", { class: "settings-head" },
      pictureMark("realm-mark large", `${realmPath(d.name)}/image`, d.image, d.name),
      h("div", null, h("h1", null, d.name), h("div", { class: "muted" }, "Realm settings"))),
    h("div", { class: "tabs", role: "tablist" }, SETTINGS_TABS.map(([id, label, ic]) =>
      h("button", { type: "button", role: "tab", "aria-selected": String(id === tab[0]), class: "tab" + (id === tab[0] ? " on" : ""),
        onclick: () => go(`#/r/${enc(d.name)}/settings/${id}`) }, icon(ic), label))),
    ...body)));
}

function field(label, control, help) {
  return h("div", { class: "field" }, h("label", null, label), h("div", null, control, help && h("div", { class: "muted small" }, help)));
}

let flashTimer = null;
function flash(text, isError) {
  let el = $("#flash");
  if (!el) { el = h("div", { id: "flash", role: "status" }); document.body.append(el); }
  el.textContent = text;
  el.className = isError ? "err show" : "show";
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => (el.className = ""), isError ? 5000 : 1800);
}

// ---------- pictures (profile, realms) ----------

/** Pictures are fetched with the API token, so they become blob URLs; cached per version. */
const pictureCache = new Map();
const pictureReady = new Map();
function pictureUrl(path, version) {
  const key = `${path}@${version}`;
  if (!pictureCache.has(key)) {
    pictureCache.set(key, fetch("/api/" + path, { headers: { "X-Airth-Token": token || "" } })
      .then((r) => (r.ok ? r.blob() : null))
      .then((b) => (b ? URL.createObjectURL(b) : null))
      .catch(() => null)
      .then((url) => { pictureReady.set(key, url); return url; }));
  }
  return pictureCache.get(key);
}

/** A round or square mark: the picture if there is one, else the initial. */
function pictureMark(cls, path, version, text) {
  const el = h("span", { class: cls }, (text || "?").slice(0, 1).toUpperCase());
  const show = (url) => { if (url) { el.classList.add("has-img"); el.replaceChildren(h("img", { src: url, alt: "" })); } };
  if (!version) return el;
  const ready = pictureReady.get(`${path}@${version}`);
  // Already loaded: shown at once, so redraws do not flicker.
  if (ready !== undefined) show(ready);
  else pictureUrl(path, version).then(show);
  return el;
}

/** Reduce a picked image to a 256×256 PNG, centered. */
async function squarePicture(file) {
  const bitmap = await createImageBitmap(file);
  const side = Math.min(bitmap.width, bitmap.height);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  canvas.getContext("2d").drawImage(bitmap, (bitmap.width - side) / 2, (bitmap.height - side) / 2, side, side, 0, 0, 256, 256);
  return new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("cannot read this image"))), "image/png"));
}

/** Choose or remove a picture. `path` is its API path; `done` gets the answer. */
function pictureEditor({ path, version, text, round, done }) {
  const input = h("input", { type: "file", accept: "image/png,image/jpeg,image/webp,image/gif", hidden: true });
  const send = async (method, body) => {
    const res = await fetch("/api/" + path, { method, body, headers: { "X-Airth-Token": token || "", "Content-Type": "image/png" } });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || res.statusText);
    return data;
  };
  input.addEventListener("change", async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    try { done(await send("PUT", await squarePicture(file))); flash("Picture saved"); }
    catch (e) { flash(e.message, true); }
    input.value = "";
  });
  return h("div", { class: "picture-edit" },
    pictureMark(round ? "avatar large" : "realm-mark large", path, version, text),
    h("button", { class: "btn", type: "button", onclick: () => input.click() }, version ? "Change picture" : "Add picture"),
    version && h("button", { class: "btn ghost", type: "button", onclick: async () => {
      try { done(await send("DELETE")); flash("Picture removed"); } catch (e) { flash(e.message, true); }
    } }, "Remove"),
    input);
}

// ---------- owner profile ----------

const OWNER_LISTS = [
  ["preferences", "Preferences", "How you like things done", "e.g. Answer in French\nShort, direct answers"],
  ["goals", "Goals", "Short- and long-term", "e.g. Launch my shop before December"],
  ["commitments", "Commitments", "What you are on the hook for", "e.g. A newsletter every Friday"],
  ["boundaries", "Boundaries", "Lines Airth must never cross", "e.g. Never email a client without my OK"],
];
const OWNER_TABS = [["profile", "Profile", "notes"], ["appearance", "Appearance", "sun"]];

async function renderOwner(main) {
  const tab = OWNER_TABS.find((t) => t[0] === app.route.tab) || OWNER_TABS[0];
  const inner = h("div", { class: "page-inner settings" },
    h("div", { class: "settings-head" },
      pictureMark("avatar large", "owner/avatar", app.ownerAvatar, app.ownerName),
      h("div", null, h("h1", null, app.ownerName || "Your profile"), h("div", { class: "muted" }, app.ownerName ? "Your profile" : "Tell Airth who you are"))),
    h("div", { class: "tabs", role: "tablist" }, OWNER_TABS.map(([id, label, ic]) =>
      h("button", { type: "button", role: "tab", "aria-selected": String(id === tab[0]), class: "tab" + (id === tab[0] ? " on" : ""),
        onclick: () => go(id === "profile" ? "#/owner" : `#/owner/${id}`) }, icon(ic), label))));
  main.append(h("div", { class: "page" }, inner));
  if (tab[0] === "appearance") {
    inner.append(h("p", { class: "muted lead" }, "Kept in this browser or app window. System follows macOS, day and night."), appearanceCard());
    return;
  }
  let owner;
  try { owner = await api("owner"); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "owner") return;
  const name = h("input", { type: "text", value: owner.name, placeholder: "Your name" });
  const about = h("textarea", { rows: 3, placeholder: "Who you are, what you do" });
  about.value = owner.about;
  const lists = OWNER_LISTS.map(([key, label, help, example]) => {
    const ta = h("textarea", { rows: 3, placeholder: example });
    ta.value = owner[key].join("\n");
    return { key, ta, el: field(label, ta, help + " · one per line") };
  });
  // Each field saves when you leave it, like the realm settings.
  const status = h("span", { class: "muted small" }, owner.updated_at ? "Saved · updated " + ago(owner.updated_at) : "Changes save on their own");
  const save = async () => {
    const body = { name: name.value.trim(), about: about.value.trim() };
    for (const l of lists) body[l.key] = l.ta.value.split("\n").map((x) => x.trim()).filter(Boolean);
    try {
      await api("owner", body, "PUT");
      const renamed = app.ownerName !== body.name;
      app.ownerName = body.name;
      renderFoot();
      status.textContent = "Saved";
      flash("Profile saved");
      if (renamed) $(".settings-head h1", inner).textContent = body.name || "Your profile";
    } catch (e) { flash(e.message, true); }
  };
  for (const el of [name, about, ...lists.map((l) => l.ta)]) el.addEventListener("change", save);
  const picture = field("Picture", pictureEditor({ path: "owner/avatar", version: owner.avatar, text: owner.name, round: true,
    done: (o) => { app.ownerAvatar = o.avatar; renderFoot(); renderMain(); } }), "Shown in the app only.");
  inner.append(
    h("p", { class: "muted lead" }, "What Airth knows about you. Realms that share your profile give it to every chat; you can turn that off in a realm's settings."),
    h("div", { class: "card" }, picture, field("Name", name), field("About", about), lists.map((l) => l.el)),
    h("div", { class: "row-end" }, status));
}

function appearanceCard() {
  const card = h("div", { class: "card" });
  const prefs = window.airthPrefs;
  const row = (title, help, value, options, onPick) => h("div", { class: "policy-row" },
    h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, help)),
    h("div", { class: "seg", role: "radiogroup", "aria-label": title },
      options.map(([v, label]) =>
        h("button", { type: "button", role: "radio", "aria-checked": String(value === v), class: value === v ? "on" : "",
          onclick: (e) => { onPick(v, e); draw(); } }, label))));
  const draw = () => {
    if (!prefs) return card.replaceChildren(h("div", { class: "muted small pad" }, "Reload the page to change the appearance."));
    card.replaceChildren(
      row("Theme", "Light, dark, or the same as your Mac", prefs.get("theme"),
        [["system", "System"], ["light", "Light"], ["dark", "Dark"]],
        (v, e) => { window.airthTheme.set(v, e); renderFoot(); }),
      row("Text size", "Conversations and the message box", prefs.get("text"),
        [["s", "Small"], ["m", "Default"], ["l", "Large"], ["xl", "Larger"]], (v) => prefs.set("text", v)),
      row("Reading width", "How wide conversations can get", prefs.get("width"),
        [["narrow", "Narrow"], ["normal", "Normal"], ["wide", "Wide"]], (v) => prefs.set("width", v)),
      row("Send with", "In the message box; the other one adds a new line", prefs.get("send"),
        [["mod", modKey + "↵"], ["enter", "↵"]], (v) => prefs.set("send", v)));
  };
  draw();
  return card;
}

// ---------- objectives ----------

function when(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  const mins = Math.round((d - Date.now()) / 60000);
  if (mins <= 0) return "due now";
  if (mins < 60) return `in ${mins} min`;
  if (mins < 24 * 60) return `in ${Math.round(mins / 60)} h · ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  return d.toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

function grantText(g) {
  return g.scope ? `${g.capability}: ${g.scope.replace(/^integration /, "")}` : `all ${g.capability}`;
}

let confirmDelete = null;

// ---------- documents ----------
//
// A small editor for the realm's files, in the spirit of a code editor: the
// files on the left, open files as tabs, an editor with syntax colors, ⌘S to
// save, ⌘P to go to a file, and ⌘L to ask Airth about the file or the
// selected lines. Tabs stay open (with unsaved changes) while you visit
// other pages.

const docsUi = { filter: "", ide: null, quick: null };
const DOC_TEXT = /\.(md|markdown|txt|text|csv|tsv|json|jsonl|ya?ml|toml|ini|cfg|conf|log|xml|html?|css|scss|js|mjs|cjs|ts|mts|jsx|tsx|py|rb|rs|go|java|kt|swift|c|h|cc|cpp|hpp|cs|m|php|sh|bash|zsh|fish|sql|r|lua|tex|srt|vtt|ics|vcf|eml|env|lock|plist|gitignore|dockerfile)$|(^|\/)(Makefile|Dockerfile|LICENSE|README|Procfile|Gemfile)$/i;
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
    h("span", { class: "grow" }, "This file changed on disk since you opened it, maybe in a chat. Your changes are not saved."),
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
        h("a", { href: `#/r/${enc(realm)}/run/${enc(m.run)}` }, m.run_title || "a chat"), " · ", ago(m.at)))))) : null;
  ide.el.body.replaceChildren(h("div", { class: "ide-pane ide-home" }, h("div", { class: "page-inner" },
    h("h1", null, "Documents"),
    home && h("p", { class: "sub" }, home.own_root
      ? h("span", null, "The files in ", h("code", { class: "path", title: home.root }, tildePath(home.root)), ". Airth reads and works with them in this realm's chats.")
      : "This realm's own folder. Add the files Airth should work with; what it makes appears here too.",
      d && d.isolation === "worktree" ? " What a chat changes appears here once you adopt it." : ""),
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

async function renderObjectives(main) {
  const realm = app.realm;
  page(main, "Objectives", "Work Airth starts on its own: on a schedule, when files change, or from a local webhook. Each time, Airth may do what this realm allows plus what you approve here; anything else waits for you, then is denied. You get a notification each time it finishes.");
  const inner = $(".page-inner", main);
  const listEl = h("div", null, h("div", { class: "muted small" }, "Loading…"));
  inner.append(listEl);
  let list;
  try { list = await api(`${realmPath(realm)}/objectives`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "objectives" || app.realm !== realm) return;
  const d = app.realmDetail;

  const refresh = (l) => { list = l; draw(); };
  const draw = () => listEl.replaceChildren(
    list.length ? h("div", { class: "objectives" }, list.map((o) => h("div", { class: "card objective" + (o.enabled ? "" : " off") + (focusOn() && o.id !== "north-star" ? " shadowed" : "") },
      h("div", { class: "objective-head" },
        h("div", { class: "grow" },
          h("div", { class: "strong" }, o.id),
          h("div", null, o.objective)),
        h("label", { class: "toggle small", title: o.enabled ? "Turn off" : "Turn on" },
          h("input", { type: "checkbox", checked: o.enabled, onchange: async (e) => {
            try { refresh(await api(`${realmPath(realm)}/objectives/${enc(o.id)}`, { enabled: e.target.checked }, "PATCH")); } catch (err) { flash(err.message, true); }
          } }), h("span", null, o.enabled ? "on" : "off"))),
      h("div", { class: "objective-meta" },
        h("span", { class: "chip" }, "◷ " + o.trigger_label),
        o.enabled && o.next_at && h("span", null, "next " + when(o.next_at)),
        o.running ? h("span", { class: "chip running" }, "running")
          : o.last_status && h("span", null, "last: ", o.last_run
            ? h("a", { href: `#/r/${enc(realm)}/run/${enc(o.last_run)}` }, o.last_status)
            : o.last_status, o.last_at && " · " + ago(o.last_at))),
      h("div", { class: "objective-meta muted small" },
        "Approved in advance: ", o.approvals.length ? o.approvals.map(grantText).join(", ") : "nothing beyond the realm's rules",
        " · waits ", String(o.approval_wait_minutes), " min for you",
        Object.keys(o.budget || {}).length ? " · budget: " + Object.entries(o.budget).map(([k, v]) => `${v} ${k.replace("max_", "")}`).join(", ") : ""),
      o.webhook_url && h("div", { class: "objective-meta" }, h("code", { class: "path small" }, "curl -X POST --data @payload.json " + o.webhook_url)),
      h("div", { class: "row-end" },
        h("button", { class: "btn", type: "button", disabled: o.running || !!(d && d.model_error), onclick: async () => {
          try { const r = await api(`${realmPath(realm)}/objectives/${enc(o.id)}/run`, {}); go(`#/r/${enc(realm)}/run/${enc(r.id)}`); } catch (e) { flash(e.message, true); }
        } }, "Run now"),
        h("button", { class: "btn danger", type: "button", onclick: async () => {
          if (confirmDelete !== o.id) { confirmDelete = o.id; return draw(); }
          confirmDelete = null;
          try { refresh(await api(`${realmPath(realm)}/objectives/${enc(o.id)}`, undefined, "DELETE")); } catch (e) { flash(e.message, true); }
        } }, confirmDelete === o.id ? "Really delete?" : "Delete")))))
      : h("div", { class: "card muted small pad" }, "No objectives yet."));
  draw();

  // New objective
  const f = {
    id: h("input", { type: "text", placeholder: "name, e.g. morning-triage", autocomplete: "off", spellcheck: "false" }),
    objective: h("textarea", { rows: 3, placeholder: "What Airth should achieve each time, e.g. “Triage new issues in the tracker and write today's summary to triage.md”" }),
    kind: h("select", null, ["schedule", "watch", "webhook", "manual"].map((k) => h("option", { value: k }, { schedule: "On a schedule", watch: "When files change", webhook: "From a webhook", manual: "Only when I start it" }[k]))),
    every: h("input", { type: "text", placeholder: "every 30m · daily 08:30 · weekdays 09:00 · mon,thu 18:00", value: "daily 08:30" }),
    path: h("input", { type: "text", placeholder: "folder or file in the realm, e.g. inbox" }),
    wait: h("input", { type: "number", min: 0, value: 10 }),
    minutes: h("input", { type: "number", min: 1, placeholder: "minutes" }),
    tokens: h("input", { type: "number", min: 1000, step: 1000, placeholder: "tokens" }),
    iterations: h("input", { type: "number", min: 1, placeholder: "model calls" }),
  };
  const grantOptions = [
    { capability: "write", label: "Change files" },
    { capability: "execute", label: "Run commands" },
    { capability: "memory", label: "Save to memory" },
    { capability: "network", label: "Any web request" },
    ...((d && d.integrations) || []).map((i) => ({ capability: "network", scope: "integration " + i.name, label: `Call ${i.name}` })),
  ].map((g) => ({ ...g, box: h("input", { type: "checkbox" }) }));
  const triggerField = h("div");
  const syncTrigger = () => triggerField.replaceChildren(
    f.kind.value === "schedule" ? f.every : f.kind.value === "watch" ? f.path
      : h("div", { class: "muted small" }, f.kind.value === "webhook" ? "You get a private URL to POST to from this machine; the request body is given to Airth as data." : "Start it from this page or with airth objective run."));
  f.kind.addEventListener("change", syncTrigger);
  syncTrigger();
  const num = (el) => (el.value.trim() ? Number(el.value) : undefined);
  const save = async () => {
    const body = {
      id: f.id.value.trim(), objective: f.objective.value.trim(),
      trigger: { kind: f.kind.value, every: f.every.value, path: f.path.value },
      approvals: grantOptions.filter((g) => g.box.checked).map((g) => ({ capability: g.capability, scope: g.scope || null })),
      approval_wait_minutes: num(f.wait) ?? 10,
      budget: { max_minutes: num(f.minutes), max_tokens: num(f.tokens), max_iterations: num(f.iterations) },
    };
    try {
      refresh(await api(`${realmPath(realm)}/objectives`, body));
      f.id.value = ""; f.objective.value = ""; delete f.id.dataset.touched;
      closeWiz();
      flash("Objective created");
    } catch (e) { flash(e.message, true); }
  };

  // A step-by-step wizard instead of one long form.
  const STEPS = ["What", "When", "Allowed", "Review"];
  let step = 0;
  const wiz = h("div", { class: "wizard" });
  const slug = (t) => t.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").split("-").slice(0, 4).join("-").slice(0, 40);
  f.objective.addEventListener("input", () => { if (!f.id.dataset.touched) f.id.value = slug(f.objective.value); });
  f.id.addEventListener("input", () => (f.id.dataset.touched = "1"));
  f.objective.rows = 4;
  const kv = (k, v) => h("div", { class: "kv" }, h("span", { class: "k" }, k), h("span", { class: "v" }, v));
  const tile = (k, ic, label, sub) => h("button", { type: "button", class: "tile" + (f.kind.value === k ? " on" : ""),
    onclick: () => { f.kind.value = k; syncTrigger(); drawWiz(0); } }, icon(ic), h("span", { class: "tile-label" }, label), h("span", { class: "tile-sub" }, sub));
  const PRESETS = [["Every morning", "daily 08:30"], ["Weekdays at 9", "weekdays 09:00"], ["Every 2 hours", "every 2h"], ["Mon & Thu 18:00", "mon,thu 18:00"]];
  const GRANT_ICONS = { write: "fileEdit", execute: "terminal", memory: "brain", network: "world" };
  const stepBody = () => {
    if (step === 0) return [
      h("div", { class: "wiz-q" }, "What should Airth achieve each time?"),
      f.objective,
      h("div", { class: "wiz-hint" }, "Write it like an instruction to a colleague: every time starts from it."),
      field("Name", f.id, "Shown in the list, and used by `airth objective run`.")];
    if (step === 1) return [
      h("div", { class: "wiz-q" }, "When should it run?"),
      h("div", { class: "tiles" },
        tile("schedule", "clock", "On a schedule", "e.g. every morning"),
        tile("watch", "folder", "When files change", "a folder or a file"),
        tile("webhook", "world", "From a webhook", "a private local URL"),
        tile("manual", "spark", "When I start it", "from here or the CLI")),
      f.kind.value === "schedule" && h("div", { class: "suggestions wiz-presets" }, PRESETS.map(([label, v]) =>
        h("button", { type: "button", class: "suggestion" + (f.every.value === v ? " on" : ""), onclick: () => { f.every.value = v; drawWiz(0); } }, label))),
      triggerField];
    if (step === 2) return [
      h("div", { class: "wiz-q" }, "What may it do without asking you?"),
      h("div", { class: "tiles compact" }, grantOptions.map((g) => {
        const t = h("button", { type: "button", class: "tile" + (g.box.checked ? " on" : ""),
          onclick: () => { g.box.checked = !g.box.checked; t.classList.toggle("on", g.box.checked); } },
          icon(GRANT_ICONS[g.capability] || "plug"), h("span", { class: "tile-label" }, g.label));
        return t;
      })),
      h("div", { class: "wiz-hint" }, "Anything else waits for you, then is denied. Commands still run in the realm's sandbox, and edits to Airth's security code are never approved in advance."),
      h("div", { class: "inline wiz-wait" }, "When it needs me, wait", f.wait, "minutes, then go on without it.")];
    const when = { schedule: f.every.value, watch: `when ${f.path.value || "files"} change`, webhook: "from a webhook", manual: "only when you start it" }[f.kind.value];
    return [
      h("div", { class: "wiz-q" }, "Ready to go"),
      h("div", { class: "review" },
        kv("Objective", f.objective.value.trim()), kv("Name", f.id.value.trim()), kv("Runs", when),
        kv("Approved", grantOptions.filter((g) => g.box.checked).map((g) => g.label).join(", ") || "nothing beyond the realm's rules"),
        kv("Waits for you", `${f.wait.value || 10} min`)),
      h("details", { class: "wiz-more" }, h("summary", null, "Limits each time (optional)"),
        h("div", { class: "inline" }, f.minutes, f.tokens, f.iterations), h("div", { class: "wiz-hint" }, "Leave empty for the defaults."))];
  };
  const valid = () => step !== 0 || (f.objective.value.trim() && f.id.value.trim());
  const drawWiz = (d) => {
    const fill = h("span");
    fill.style.width = `${((step + 1) / STEPS.length) * 100}%`;
    wiz.replaceChildren(
      h("div", { class: "wiz-steps" }, STEPS.map((s, i) => h("button", { type: "button", class: "wiz-step" + (i === step ? " on" : "") + (i < step ? " done" : ""),
        onclick: () => { if (i < step) { step = i; drawWiz(-1); } } }, h("span", { class: "wiz-dot" }, i < step ? icon("check") : String(i + 1)), h("span", null, s)))),
      h("div", { class: "wiz-bar" }, fill),
      h("div", { class: "wiz-pane" + (d > 0 ? " from-right" : d < 0 ? " from-left" : "") }, stepBody()),
      h("div", { class: "wiz-nav" },
        step > 0 ? h("button", { class: "btn ghost", type: "button", onclick: () => { step--; drawWiz(-1); } }, "Back")
          : h("button", { class: "btn ghost", type: "button", onclick: () => closeWiz() }, "Cancel"),
        step < STEPS.length - 1
          ? h("button", { class: "btn primary", type: "button", onclick: () => { if (!valid()) return flash("Describe the objective first", true); step++; drawWiz(1); } }, "Next")
          : h("button", { class: "btn primary", type: "button", onclick: save }, icon("check"), "Create objective")));
    if (step === 0 && d >= 0) setTimeout(() => f.objective.focus && f.objective.focus(), 60);
  };
  const wizWrap = h("div", { class: "wizard-wrap", hidden: true }, wiz);
  const openBtn = h("button", { class: "btn primary", type: "button", onclick: () => { step = 0; wizWrap.hidden = false; openBtn.hidden = true; drawWiz(1); } }, icon("plus"), "New objective");
  const closeWiz = () => { wizWrap.hidden = true; openBtn.hidden = false; };
  inner.append(h("div", { class: "row-start pad-top" }, openBtn), wizWrap);
}

// ---------- skills ----------

const SKILL_HELP = {
  proposed: "Proposed in a chat. Nothing is built yet.",
  developing: "Airth is building it in its own folder, with no access to the rest of the realm.",
  ready: "Built, and every test passed. Review it, then install it to enable it.",
  failed: "Built, but the evaluation failed.",
  installed: "Enabled: chats in this realm can use it.",
  rejected: "You declined it.",
};

const skillUi = { open: new Set(), files: {}, feedback: {}, tab: "built" };

const SKILL_TABS = [
  ["built", "Built by Airth", "spark", "Small programs with tests that become tools. Airth proposes them when it hits a limit; Airth develops each one in isolation and tests it; nothing is enabled until you install it."],
  ["connectors", "Connectors", "world", "Connect the services you use (Notion, Linear, GitHub, Stripe…) in one step: paste a token once, and chats in this realm can call the service's official API. The token never leaves for anywhere else, and the agent never sees it."],
  ["catalog", "Catalog", "books", "Ready-made Agent Skills (instructions, scripts, references) from Anthropic's public catalog. Install one and chats in this realm read it when a task matches."],
  ["mcp", "MCP servers", "plug", "Model Context Protocol servers give chats new tools (GitHub, databases, browsers…). Each runs in this realm's sandbox with its credentials, and every call follows the realm's skill rule."],
];

async function renderSkills(main) {
  const realm = app.realm;
  const tab = SKILL_TABS.find((t) => t[0] === skillUi.tab) || SKILL_TABS[0];
  page(main, "Skills", tab[3]);
  const inner = $(".page-inner", main);
  inner.insertBefore(h("div", { class: "tabs", role: "tablist" }, SKILL_TABS.map(([id, label, ic]) =>
    h("button", { type: "button", role: "tab", "aria-selected": String(id === tab[0]), class: "tab" + (id === tab[0] ? " on" : ""),
      onclick: () => { skillUi.tab = id; renderMain(); } }, icon(ic), label))), inner.querySelector(".sub"));
  if (tab[0] === "catalog") return renderCatalogTab(inner, realm);
  if (tab[0] === "mcp") return renderMcpTab(inner, realm);
  if (tab[0] === "connectors") return renderConnectorsTab(inner, realm);
  const listEl = h("div", null, h("div", { class: "muted small" }, "Loading…"));
  inner.append(listEl);
  let list;
  try { list = await api(`${realmPath(realm)}/skills`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "skills" || app.realm !== realm) return;
  const d = app.realmDetail;
  const noModel = !!(d && d.model_error);

  const act = async (name, action) => {
    try { list = await api(`${realmPath(realm)}/skills/${enc(name)}/${action}`, {}); draw(); flash({ install: "Installed", uninstall: "Uninstalled", evaluate: "Evaluated", reject: "Rejected" }[action]); }
    catch (e) { flash(e.message, true); }
  };
  const develop = async (name, feedback) => {
    try {
      const r = await api(`${realmPath(realm)}/skills/${enc(name)}/develop`, feedback ? { feedback } : {});
      go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
    } catch (e) { flash(e.message, true); }
  };
  const toggleFiles = async (name) => {
    if (skillUi.open.has(name)) { skillUi.open.delete(name); return draw(); }
    skillUi.open.add(name);
    try { skillUi.files[name] = await api(`${realmPath(realm)}/skills/${enc(name)}/files`); } catch (e) { flash(e.message, true); }
    draw();
  };

  const card = (sk) => {
    const ev = sk.evaluation;
    const fb = h("input", { type: "text", placeholder: "What to change (optional)…", value: skillUi.feedback[sk.name] || "", oninput: (e) => (skillUi.feedback[sk.name] = e.target.value) });
    const failedReport = ev ? [...ev.problems.map((p) => "problem: " + p), ...ev.results.filter((r) => !r.ok).map((r) => `${r.name}: ${r.detail}`)].join("\n") : "";
    return h("div", { class: "card objective" },
      h("div", { class: "objective-head" },
        h("div", { class: "grow" },
          h("div", null, h("span", { class: "strong mono" }, sk.name), " ", h("span", { class: "chip skill-" + sk.status }, sk.developing_now ? "developing…" : sk.status)),
          h("div", null, sk.description))),
      h("div", { class: "muted small" }, SKILL_HELP[sk.status]),
      sk.reason && h("div", { class: "objective-meta" }, "Why: ", sk.reason),
      h("div", { class: "objective-meta" },
        h("span", null, sk.proposed_by === "owner" ? "asked for by you" : "proposed by ", sk.proposed_by !== "owner" && h("a", { href: `#/r/${enc(realm)}/run/${enc(sk.proposed_by.replace("run:", ""))}` }, "a chat")),
        sk.dev_run && h("a", { href: `#/r/${enc(realm)}/run/${enc(sk.dev_run)}` }, "development chat"),
        sk.installed && h("span", null, "installed " + ago(sk.installed.at))),
      ev && h("div", { class: "eval" },
        h("div", { class: ev.problems.length || ev.results.some((r) => !r.ok) ? "warn small strong" : "ok small strong" }, "Evaluation: " + sk.summary + " · " + ago(ev.at)),
        ev.problems.map((p) => h("div", { class: "eval-row bad" }, "✗ " + p)),
        ev.results.map((r) => h("div", { class: "eval-row " + (r.ok ? "good" : "bad") }, (r.ok ? "✓ " : "✗ ") + r.name + ` (${(r.duration_ms / 1000).toFixed(1)}s)`,
          !r.ok && h("pre", { class: "out" }, r.detail)))),
      skillUi.open.has(sk.name) && h("div", { class: "files" }, (skillUi.files[sk.name] || []).map((f) =>
        h("details", { class: "tool", open: true }, h("summary", null, h("span", { class: "label" }, f.path)),
          h("div", { class: "body" }, h("pre", { class: "out" }, f.content == null ? "(binary file)" : f.content))))),
      ["ready", "failed", "installed"].includes(sk.status) && !sk.developing_now && h("div", { class: "inline-form" }, fb,
        h("button", { class: "btn", type: "button", disabled: noModel, onclick: () => develop(sk.name, fb.value.trim() || (sk.status === "failed" ? "The evaluation failed:\n" + failedReport + "\nFix it and make every test pass." : "")) }, sk.status === "failed" ? "Ask Airth to fix" : "Ask for changes")),
      h("div", { class: "row-end" },
        sk.dev_run || sk.evaluation ? h("button", { class: "btn", type: "button", onclick: () => toggleFiles(sk.name) }, skillUi.open.has(sk.name) ? "Hide files" : "Review files") : null,
        sk.status === "proposed" && h("button", { class: "btn primary", type: "button", disabled: noModel, onclick: () => develop(sk.name) }, "Develop it"),
        ["ready", "failed", "installed"].includes(sk.status) && !sk.developing_now && h("button", { class: "btn", type: "button", onclick: () => act(sk.name, "evaluate") }, "Re-run tests"),
        sk.status === "ready" && h("button", { class: "btn primary", type: "button", onclick: () => act(sk.name, "install") }, "Install"),
        sk.installed && h("button", { class: "btn", type: "button", onclick: () => act(sk.name, "uninstall") }, "Uninstall"),
        ["proposed", "ready", "failed"].includes(sk.status) && h("button", { class: "btn", type: "button", onclick: () => act(sk.name, "reject") }, "Reject"),
        h("button", { class: "btn danger", type: "button", onclick: async () => {
          if (confirmDelete !== "skill:" + sk.name) { confirmDelete = "skill:" + sk.name; return draw(); }
          confirmDelete = null;
          try { list = await api(`${realmPath(realm)}/skills/${enc(sk.name)}`, undefined, "DELETE"); draw(); } catch (e) { flash(e.message, true); }
        } }, confirmDelete === "skill:" + sk.name ? "Really delete?" : "Delete")));
  };
  const draw = () => listEl.replaceChildren(list.length ? h("div", { class: "objectives" }, list.map(card)) : h("div", { class: "card muted small pad" }, "No skills yet. Airth proposes them when it needs a capability; you can also ask for one below."));
  draw();

  const name = h("input", { type: "text", placeholder: "name, e.g. ical_events", autocomplete: "off", spellcheck: "false" });
  const desc = h("textarea", { rows: 3, placeholder: "What it should do: inputs, outputs, edge cases" });
  inner.append(h("h2", null, "Ask for a skill"),
    h("div", { class: "card" }, field("Name", name), field("What it does", desc),
      h("div", { class: "row-end" }, h("button", { class: "btn primary", type: "button", disabled: noModel, onclick: async () => {
        try {
          const r = await api(`${realmPath(realm)}/skills`, { name: name.value.trim(), description: desc.value.trim() });
          go(`#/r/${enc(realm)}/run/${enc(r.id)}`);
        } catch (e) { flash(e.message, true); }
      } }, "Develop it"))));
}

// ---------- skills: connectors tab ----------

/** Brand marks from Simple Icons (CC0, simpleicons.org), as constant path data. */
const LOGOS = {
  notion: [null, "M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L17.86 1.968c-.42-.326-.981-.7-2.055-.607L3.01 2.295c-.466.046-.56.28-.374.466zm.793 3.08v13.904c0 .747.373 1.027 1.214.98l14.523-.84c.841-.046.935-.56.935-1.167V6.354c0-.606-.233-.933-.748-.887l-15.177.887c-.56.047-.747.327-.747.933zm14.337.745c.093.42 0 .84-.42.888l-.7.14v10.264c-.608.327-1.168.514-1.635.514c-.748 0-.935-.234-1.495-.933l-4.577-7.186v6.952L12.21 19s0 .84-1.168.84l-3.222.186c-.093-.186 0-.653.327-.746l.84-.233V9.854L7.822 9.76c-.094-.42.14-1.026.793-1.073l3.456-.233l4.764 7.279v-6.44l-1.215-.139c-.093-.514.28-.887.747-.933zM1.936 1.035l13.31-.98c1.634-.14 2.055-.047 3.082.7l4.249 2.986c.7.513.934.653.934 1.213v16.378c0 1.026-.373 1.634-1.68 1.726l-15.458.934c-.98.047-1.448-.093-1.962-.747l-3.129-4.06c-.56-.747-.793-1.306-.793-1.96V2.667c0-.839.374-1.54 1.447-1.632"],
  linear: ["#5E6AD2", "M2.886 4.18A11.98 11.98 0 0 1 11.99 0C18.624 0 24 5.376 24 12.009c0 3.64-1.62 6.903-4.18 9.105L2.887 4.18ZM1.817 5.626l16.556 16.556q-.787.496-1.65.866L.951 7.277q.371-.863.866-1.65ZM.322 9.163l14.515 14.515q-1.066.26-2.195.322L0 11.358a12 12 0 0 1 .322-2.195m-.17 4.862l9.823 9.824a12.02 12.02 0 0 1-9.824-9.824Z"],
  stripe: ["#635BFF", "M13.976 9.15c-2.172-.806-3.356-1.426-3.356-2.409c0-.831.683-1.305 1.901-1.305c2.227 0 4.515.858 6.09 1.631l.89-5.494C18.252.975 15.697 0 12.165 0C9.667 0 7.589.654 6.104 1.872C4.56 3.147 3.757 4.992 3.757 7.218c0 4.039 2.467 5.76 6.476 7.219c2.585.92 3.445 1.574 3.445 2.583c0 .98-.84 1.545-2.354 1.545c-1.875 0-4.965-.921-6.99-2.109l-.9 5.555C5.175 22.99 8.385 24 11.714 24c2.641 0 4.843-.624 6.328-1.813c1.664-1.305 2.525-3.236 2.525-5.732c0-4.128-2.524-5.851-6.594-7.305z"],
  vercel: [null, "m12 1.608l12 20.784H0Z"],
  sentry: ["#7553FF", "M13.91 2.505c-.873-1.448-2.972-1.448-3.844 0L6.904 7.92a15.48 15.48 0 0 1 8.53 12.811h-2.221A13.3 13.3 0 0 0 5.784 9.814l-2.926 5.06a7.65 7.65 0 0 1 4.435 5.848H2.194a.365.365 0 0 1-.298-.534l1.413-2.402a5.2 5.2 0 0 0-1.614-.913L.296 19.275a2.18 2.18 0 0 0 .812 2.999a2.24 2.24 0 0 0 1.086.288h6.983a9.32 9.32 0 0 0-3.845-8.318l1.11-1.922a11.47 11.47 0 0 1 4.95 10.24h5.915a17.24 17.24 0 0 0-7.885-15.28l2.244-3.845a.37.37 0 0 1 .504-.13c.255.14 9.75 16.708 9.928 16.9a.365.365 0 0 1-.327.543h-2.287q.043.918 0 1.831h2.297a2.206 2.206 0 0 0 1.922-3.31z"],
  paypal: ["#0070E0", "M15.607 4.653H8.941L6.645 19.251H1.82L4.862 0h7.995c3.754 0 6.375 2.294 6.473 5.513c-.648-.478-2.105-.86-3.722-.86m6.57 5.546c0 3.41-3.01 6.853-6.958 6.853h-2.493L11.595 24H6.74l1.845-11.538h3.592c4.208 0 7.346-3.634 7.153-6.949a5.24 5.24 0 0 1 2.848 4.686M9.653 5.546h6.408c.907 0 1.942.222 2.363.541c-.195 2.741-2.655 5.483-6.441 5.483H8.714Z"],
  intercom: ["#286EFA", "M21 0H3a3 3 0 0 0-3 3v18a3 3 0 0 0 3 3h18c1.658 0 3-1.342 3-3V3a3 3 0 0 0-3-3m-5.801 4.399c0-.44.36-.8.802-.8c.44 0 .8.36.8.8v10.688a.802.802 0 0 1-1.602 0zM11.2 3.994a.8.8 0 0 1 1.6 0v11.602a.8.8 0 0 1-1.6 0zm-4 .405a.801.801 0 0 1 1.601 0v10.688a.801.801 0 0 1-1.601 0zM3.199 6A.801.801 0 0 1 4.8 6v7.195a.8.8 0 0 1-1.601 0zM20.52 18.202c-.123.105-3.086 2.593-8.52 2.593s-8.397-2.486-8.521-2.593a.8.8 0 0 1 1.039-1.218c.047.041 2.693 2.211 7.481 2.211c4.848 0 7.456-2.186 7.479-2.207a.8.8 0 0 1 1.128.086c.289.336.25.84-.086 1.128m.281-5.007a.802.802 0 0 1-1.602 0V6a.802.802 0 0 1 1.602 0z"],
  figma: ["#F24E1E", "M15.852 8.981h-4.588V0h4.588c2.476 0 4.49 2.014 4.49 4.49s-2.014 4.491-4.49 4.491M12.735 7.51h3.117c1.665 0 3.019-1.355 3.019-3.019s-1.355-3.019-3.019-3.019h-3.117zm0 1.471H8.148c-2.476 0-4.49-2.014-4.49-4.49S5.672 0 8.148 0h4.588v8.981zm-4.587-7.51c-1.665 0-3.019 1.355-3.019 3.019s1.354 3.02 3.019 3.02h3.117V1.471zm4.587 15.019H8.148c-2.476 0-4.49-2.014-4.49-4.49s2.014-4.49 4.49-4.49h4.588v8.98zM8.148 8.981c-1.665 0-3.019 1.355-3.019 3.019s1.355 3.019 3.019 3.019h3.117V8.981zM8.172 24c-2.489 0-4.515-2.014-4.515-4.49s2.014-4.49 4.49-4.49h4.588v4.441c0 2.503-2.047 4.539-4.563 4.539m-.024-7.51a3.023 3.023 0 0 0-3.019 3.019c0 1.665 1.365 3.019 3.044 3.019c1.705 0 3.093-1.376 3.093-3.068v-2.97zm7.704 0h-.098c-2.476 0-4.49-2.014-4.49-4.49s2.014-4.49 4.49-4.49h.098c2.476 0 4.49 2.014 4.49 4.49s-2.014 4.49-4.49 4.49m-.097-7.509c-1.665 0-3.019 1.355-3.019 3.019s1.355 3.019 3.019 3.019h.098c1.665 0 3.019-1.355 3.019-3.019s-1.355-3.019-3.019-3.019z"],
  webflow: ["#146EF5", "m24 4.515l-7.658 14.97H9.149l3.205-6.204h-.144C9.566 16.713 5.621 18.973 0 19.485v-6.118s3.596-.213 5.71-2.435H0V4.515h6.417v5.278l.144-.001l2.622-5.277h4.854v5.244h.144l2.72-5.244z"],
  cloudflare: ["#F38020", "M16.509 16.845c.147-.507.09-.971-.155-1.316c-.225-.316-.605-.499-1.062-.52l-8.66-.113a.16.16 0 0 1-.133-.07a.2.2 0 0 1-.02-.156a.24.24 0 0 1 .203-.156l8.736-.113c1.035-.049 2.16-.886 2.554-1.913l.499-1.302a.27.27 0 0 0 .014-.168a5.689 5.689 0 0 0-10.937-.584a2.58 2.58 0 0 0-1.794-.498a2.56 2.56 0 0 0-2.223 3.18A3.634 3.634 0 0 0 0 16.751q.002.264.035.527a.174.174 0 0 0 .17.148h15.98a.22.22 0 0 0 .204-.155zm2.757-5.564c-.077 0-.161 0-.239.011c-.056 0-.105.042-.127.098l-.337 1.174c-.148.507-.092.971.154 1.317c.225.316.605.498 1.062.52l1.844.113c.056 0 .105.026.133.07a.2.2 0 0 1 .021.156a.24.24 0 0 1-.204.156l-1.92.112c-1.042.049-2.159.887-2.553 1.914l-.141.358c-.028.072.021.142.099.142h6.597a.174.174 0 0 0 .17-.126a5 5 0 0 0 .175-1.28a4.74 4.74 0 0 0-4.734-4.727"],
  square: [null, "M4.01 0A4.01 4.01 0 0 0 0 4.01v15.98c0 2.21 1.8 4 4.01 4.01h15.98C22.2 24 24 22.2 24 19.99V4a4.01 4.01 0 0 0-4.01-4H4zm1.62 4.36h12.74c.7 0 1.26.57 1.26 1.27v12.74c0 .7-.56 1.27-1.26 1.27H5.63c-.7 0-1.26-.57-1.26-1.27V5.63a1.27 1.27 0 0 1 1.26-1.27m3.83 4.35a.73.73 0 0 0-.73.73v5.09c0 .4.32.72.72.72h5.1a.73.73 0 0 0 .73-.72V9.44a.73.73 0 0 0-.73-.73h-5.1Z"],
  supabase: ["#3FCF8E", "M11.9 1.036c-.015-.986-1.26-1.41-1.874-.637L.764 12.05C-.33 13.427.65 15.455 2.409 15.455h9.579l.113 7.51c.014.985 1.259 1.408 1.873.636l9.262-11.653c1.093-1.375.113-3.403-1.645-3.403h-9.642z"],
  gmail: ["#EA4335", "M24 5.457v13.909c0 .904-.732 1.636-1.636 1.636h-3.819V11.73L12 16.64l-6.545-4.91v9.273H1.636A1.636 1.636 0 0 1 0 19.366V5.457c0-2.023 2.309-3.178 3.927-1.964L5.455 4.64L12 9.548l6.545-4.91l1.528-1.145C21.69 2.28 24 3.434 24 5.457"],
};

function logo(id, name) {
  const l = LOGOS[id];
  if (!l) return h("span", { class: "conn-mark", "aria-hidden": "true" }, (name || "?").slice(0, 1).toUpperCase());
  const el = h("span", { class: "conn-mark logo-mark", "aria-hidden": "true",
    html: `<svg viewBox="0 0 24 24"><path fill="currentColor" d="${l[1]}"/></svg>` });
  if (l[0]) el.style.color = l[0];
  return el;
}

/** Official remote MCP servers, each checked to support sign-in with automatic app registration. */
const FEATURED = [
  ["notion", "Notion", "https://mcp.notion.com/mcp", "Search, read and write your pages and databases."],
  ["linear", "Linear", "https://mcp.linear.app/mcp", "Find, create and update issues and projects."],
  ["stripe", "Stripe", "https://mcp.stripe.com", "Payments, customers, invoices and balance."],
  ["vercel", "Vercel", "https://mcp.vercel.com", "Projects, deployments and their logs."],
  ["sentry", "Sentry", "https://mcp.sentry.dev/mcp", "Errors and issues across your projects."],
  ["paypal", "PayPal", "https://mcp.paypal.com/mcp", "Transactions, invoices and disputes."],
  ["canva", "Canva", "https://mcp.canva.com/mcp", "Find, create and export designs."],
  ["intercom", "Intercom", "https://mcp.intercom.com/mcp", "Conversations and contacts."],
  ["figma", "Figma", "https://mcp.figma.com/mcp", "Read files, frames and design context."],
  ["webflow", "Webflow", "https://mcp.webflow.com/mcp", "Sites, pages and CMS collections."],
  ["cloudflare", "Cloudflare", "https://mcp.cloudflare.com/mcp", "Workers, DNS and your account."],
  ["square", "Square", "https://mcp.squareup.com/mcp", "Orders, payments and catalog."],
  ["supabase", "Supabase", "https://mcp.supabase.com/mcp", "Databases, tables and SQL."],
].map(([id, name, url, description]) => ({ id, name, url, description }));

const connUi = { query: "", registry: null, searching: false, custom: false };

async function renderConnectorsTab(inner, realm) {
  const onTab = () => app.route.view === "skills" && skillUi.tab === "connectors" && app.realm === realm;
  const body = h("div", null, working("Loading connectors…"));
  inner.append(body);
  let servers = [];
  const waiting = new Map(); // name → sign-in URL, while the browser is away
  const load = async () => {
    try { servers = await api(`${realmPath(realm)}/mcp`); } catch (e) { flash(e.message, true); }
  };
  await load();
  if (!onTab()) return;
  const byName = (n) => servers.find((s) => s.name === n);
  const poll = async () => {
    if (!onTab() || !waiting.size) return;
    await load();
    for (const [name] of waiting) {
      const s = byName(name);
      if (s && (s.connected_at || s.error) && !s.connecting) {
        waiting.delete(name);
        flash(s.error ? `${name}: ${s.error}` : `${name} is connected`, !!s.error);
      }
    }
    draw();
    if (waiting.size) setTimeout(poll, 1500);
  };
  const connect = async (name, url, description) => {
    try {
      const res = await api(`${realmPath(realm)}/connect`, { name, url, description });
      if (res.sign_in) { waiting.set(name, res.sign_in); flash("Sign in in the page that just opened"); }
      else waiting.set(name, null);
      await load(); draw();
      setTimeout(poll, 1500);
    } catch (e) { flash(e.message, true); await load(); draw(); }
  };
  const disconnect = async (name) => {
    try { servers = await api(`${realmPath(realm)}/mcp/${enc(name)}`, undefined, "DELETE"); flash("Disconnected"); } catch (e) { flash(e.message, true); }
    draw();
  };
  const state = (name) => {
    const s = byName(name);
    if (waiting.has(name)) return "waiting";
    if (!s) return "none";
    if (s.connecting) return "waiting";
    if (s.tools.length && (s.signed_in || !s.url)) return "connected";
    if (s.error) return "error";
    return "none";
  };
  const card = (c) => {
    const st = state(c.id);
    const s = byName(c.id);
    const action = st === "connected"
      ? h("div", { class: "row-end" }, h("span", { class: "muted small grow" }, `${s.tools.length} tool${s.tools.length === 1 ? "" : "s"}`),
          h("button", { class: "btn danger ghost", type: "button", onclick: () => disconnect(c.id) }, "Disconnect"))
      : st === "waiting"
        ? h("div", { class: "row-end" }, h("span", { class: "working small grow" }, h("span", { class: "spinner" }), "Waiting for sign-in…"),
            waiting.get(c.id) && h("a", { class: "btn ghost", href: waiting.get(c.id), target: "_blank", rel: "noopener noreferrer" }, "Open it again"))
        : h("div", { class: "row-end" }, h("button", { class: "btn primary", type: "button", onclick: () => connect(c.id, c.url, c.description) }, "Connect"));
    return h("div", { class: "card cat-card conn-card" + (st === "connected" ? " installed" : "") },
      h("div", { class: "cat-head" }, logo(c.id, c.name),
        h("div", { class: "grow" }, h("div", { class: "strong" }, c.name), h("div", { class: "muted small mono" }, hostOf(c.url))),
        st === "connected" && h("span", { class: "chip ok" }, icon("check"), "connected")),
      h("div", { class: "cat-desc" }, c.description),
      st === "error" && s && h("div", { class: "warn small clamp" , title: s.error }, s.error),
      action);
  };
  const gmail = h("div", { class: "card cat-card conn-card" },
    h("div", { class: "cat-head" }, logo("gmail", "Gmail"), h("div", { class: "grow" }, h("div", { class: "strong" }, "Gmail & Google"), h("div", { class: "muted small" }, "through your Mac"))),
    h("div", { class: "cat-desc" }, "Google offers no one-click sign-in for apps like Airth yet. Add your Google account in macOS (Internet Accounts): chats read your calendar and prepare Gmail drafts with the Mac app tools."),
    h("div", { class: "row-end" }, h("a", { class: "btn", href: "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension" }, "Open Internet Accounts")));
  const search = h("input", { type: "search", class: "conn-search", placeholder: "Search connectors, or the MCP Registry…", value: connUi.query, "aria-label": "Search connectors" });
  let timer = null;
  search.addEventListener("input", () => {
    connUi.query = search.value;
    clearTimeout(timer);
    drawGrid();
    timer = setTimeout(searchRegistry, 450);
  });
  const searchRegistry = async () => {
    const q = connUi.query.trim();
    if (q.length < 2) { connUi.registry = null; return drawRegistry(); }
    connUi.searching = true; drawRegistry();
    try { connUi.registry = await api("registry?search=" + encodeURIComponent(q)); } catch (e) { connUi.registry = []; flash(e.message, true); }
    connUi.searching = false;
    if (onTab()) drawRegistry();
  };
  const grid = h("div", { class: "cat-grid" });
  const registryEl = h("div");
  const customEl = h("div");
  const drawGrid = () => {
    const q = connUi.query.trim().toLowerCase();
    const shown = FEATURED.filter((c) => !q || (c.name + " " + c.description).toLowerCase().includes(q));
    grid.replaceChildren(...shown.map(card), (!q || "gmail google".includes(q)) && gmail);
  };
  const slug = (t) => t.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 32) || "connector";
  const drawRegistry = () => {
    if (connUi.searching) return registryEl.replaceChildren(h("div", { class: "working small" }, h("span", { class: "spinner" }), "Searching the MCP Registry…"));
    if (!connUi.registry) return registryEl.replaceChildren();
    const featuredHosts = new Set(FEATURED.map((c) => hostOf(c.url)));
    const rows = connUi.registry.filter((r) => !featuredHosts.has(hostOf(r.url)));
    registryEl.replaceChildren(
      h("h2", null, "From the MCP Registry"),
      h("p", { class: "muted small" }, "Public servers listed in the official registry. They are not checked by Airth: connect the ones published by the company itself (look at the address)."),
      rows.length ? h("div", { class: "card" }, rows.map((r) => {
        const name = slug(r.title || r.name);
        const st = state(name);
        return h("div", { class: "list-row" },
          logo(null, r.title || r.name),
          h("div", { class: "grow" },
            h("div", null, h("span", { class: "strong" }, r.title || r.name), " ", h("span", { class: "muted small mono" }, hostOf(r.url))),
            r.description && h("div", { class: "muted small clamp" }, r.description)),
          st === "connected" ? h("span", { class: "chip ok" }, "connected")
            : st === "waiting" ? h("span", { class: "working small" }, h("span", { class: "spinner" }))
            : h("button", { class: "btn", type: "button", onclick: () => connect(name, r.url, r.description || "") }, "Connect"));
      })) : h("div", { class: "card muted small pad" }, "No connectable server found."));
  };
  const drawCustom = () => {
    if (!connUi.custom) {
      return customEl.replaceChildren(h("button", { class: "btn ghost", type: "button", onclick: () => { connUi.custom = true; drawCustom(); } }, icon("plus"), "Custom connector"));
    }
    const name = h("input", { type: "text", placeholder: "name, e.g. crm", autocomplete: "off", spellcheck: "false" });
    const url = h("input", { type: "text", class: "mono", placeholder: "https://mcp.example.com/mcp", autocomplete: "off", spellcheck: "false" });
    name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
    customEl.replaceChildren(h("div", { class: "card" },
      field("Name", name), field("Server URL", url, "A remote MCP server (Streamable HTTP). If it asks to sign in, its page opens."),
      h("div", { class: "row-end" },
        h("button", { class: "btn ghost", type: "button", onclick: () => { connUi.custom = false; drawCustom(); } }, "Cancel"),
        h("button", { class: "btn primary", type: "button", onclick: () => connect(name.value.trim(), url.value.trim(), "") }, "Connect"))));
  };
  const keys = h("details", { class: "conn-keys" }, h("summary", null, "Other services, with an API key (Lemon Squeezy, Qonto, Brevo, GitHub…)"));
  keys.addEventListener("toggle", () => { if (keys.open && keys.children.length === 1) renderKeyConnectors(keys, realm); });
  const draw = () => { drawGrid(); drawRegistry(); };
  body.replaceChildren(
    h("div", { class: "conn-bar" }, h("span", { class: "i" }, icon("search")), search),
    grid, registryEl,
    h("div", { class: "row-start pad-top" }, customEl),
    h("p", { class: "muted small pad-top" },
      "Connect opens the service's sign-in page; Airth keeps the access in this realm, out of reach of commands, and renews it on its own. ",
      "Chats use a connector's tools under the realm's skill rule (they appear as mcp__name__tool)."),
    keys);
  draw(); drawCustom();
  if (connUi.query.trim().length >= 2 && !connUi.registry) searchRegistry();
  if (servers.some((s) => s.connecting)) { for (const s of servers) if (s.connecting) waiting.set(s.name, null); setTimeout(poll, 1500); }
}

const CONNECTOR_ORDER = ["Productivity", "Product", "Development", "Communication", "Business", "Marketing", "Design"];

/** The API-key connectors (decision 19), under the OAuth ones. */
async function renderKeyConnectors(inner, realm) {
  const body = h("div", null, working("Loading connectors…"));
  inner.append(body);
  let data;
  try { data = await api(`${realmPath(realm)}/connectors`); } catch (e) { body.replaceChildren(); return flash(e.message, true); }
  const onTab = () => app.route.view === "skills" && skillUi.tab === "connectors" && app.realm === realm;
  if (!onTab()) return;
  let open = null; // the connector whose token form is open
  let busy = null;
  const send = async (id, path, payload, method) => {
    busy = id; draw();
    try {
      data = await api(path, payload, method);
      if (data.check && data.check.id === id) flash(data.check.ok ? (data.check.message || "Connected") : "Saved, but the check failed", !data.check.ok);
      else if (method === "DELETE") flash("Disconnected");
      open = null;
    } catch (e) { flash(e.message, true); }
    busy = null;
    if (onTab()) draw();
  };
  const card = (s) => {
    const c = s.connector;
    const failed = data.check && data.check.id === c.id && !data.check.ok ? data.check.message : null;
    let actions;
    if (c.kind === "mac") {
      actions = h("div", { class: "stack" },
        h("div", { class: "muted small" }, "Mac apps in this realm: ", h("strong", null, POLICY_WORDS[data.apps_rule] || data.apps_rule)),
        h("div", { class: "row-end" },
          h("a", { class: "btn ghost", href: c.docs, target: "_blank", rel: "noopener noreferrer" }, "How it works"),
          h("a", { class: "btn", href: c.setup_url }, "Open Internet Accounts")));
    } else if (busy === c.id) {
      actions = h("div", { class: "row-end" }, h("span", { class: "working small" }, h("span", { class: "spinner" }), "Checking…"));
    } else if (s.connected) {
      actions = h("div", { class: "row-end" },
        h("button", { class: "btn ghost", type: "button", onclick: () => send(c.id, `${realmPath(realm)}/connectors/${enc(c.id)}/check`, {}) }, icon("refresh"), "Check"),
        h("button", { class: "btn danger ghost", type: "button", onclick: () => send(c.id, `${realmPath(realm)}/connectors/${enc(c.id)}`, undefined, "DELETE") }, "Disconnect"));
    } else if (open === c.id) {
      const token = h("input", { type: "password", placeholder: "Paste the token", autocomplete: "off", spellcheck: "false", "aria-label": `${c.name} token` });
      const go = () => send(c.id, `${realmPath(realm)}/connectors/${enc(c.id)}`, { token: token.value });
      token.addEventListener("keydown", (e) => { if (e.key === "Enter") go(); if (e.key === "Escape") { open = null; draw(); } });
      setTimeout(() => token.focus && token.focus(), 0);
      actions = h("div", { class: "stack conn-form" },
        h("div", { class: "muted small" }, c.token_help, " ", h("a", { href: c.token_url, target: "_blank", rel: "noopener noreferrer" }, "Get a token ↗")),
        token,
        h("div", { class: "row-end" },
          h("button", { class: "btn ghost", type: "button", onclick: () => { open = null; draw(); } }, "Cancel"),
          h("button", { class: "btn primary", type: "button", onclick: go }, "Connect")));
    } else {
      actions = h("div", { class: "row-end" },
        h("a", { class: "btn ghost", href: c.docs, target: "_blank", rel: "noopener noreferrer" }, "API docs"),
        h("button", { class: "btn primary", type: "button", onclick: () => { open = c.id; draw(); } }, "Connect"));
    }
    return h("div", { class: "card cat-card" + (s.connected ? " installed" : "") },
      h("div", { class: "cat-head" },
        h("span", { class: "conn-mark", "aria-hidden": "true" }, c.name.slice(0, 1)),
        h("div", { class: "grow" }, h("div", { class: "strong" }, c.name), h("div", { class: "muted small" }, c.category)),
        s.connected && h("span", { class: "chip ok" }, icon("check"), "connected")),
      h("div", { class: "cat-desc" }, c.description),
      failed && h("div", { class: "warn small" }, failed),
      actions);
  };
  const draw = () => {
    const sorted = [...data.connectors].filter((s) => s.connector.kind !== "mac").sort((a, b) =>
      (b.connected - a.connected) || (CONNECTOR_ORDER.indexOf(a.connector.category) - CONNECTOR_ORDER.indexOf(b.connector.category)) || a.connector.name.localeCompare(b.connector.name));
    body.replaceChildren(
      h("div", { class: "cat-grid" }, sorted.map(card)),
      h("p", { class: "muted small pad-top" },
        "A token is saved as one of this realm's credentials and only ever sent to that service's official API. Chats call a connector by its name with http_request, under the realm's network rule. ",
        "Need something else? Add any API under Realm settings → Integrations, or an MCP server."));
  };
  draw();
}

// ---------- skills: catalog & MCP tabs ----------

/** Re-render the Skills page shortly, while something runs in the background. */
function refreshSkillsSoon(realm, tab) {
  setTimeout(() => {
    if (app.route.view === "skills" && app.realm === realm && skillUi.tab === tab && !palette) renderMain();
  }, 1500);
}

async function renderCatalogTab(inner, realm, refresh = false) {
  const body = h("div", null, working("Loading the catalog…"));
  inner.append(body);
  let cat, mine;
  try {
    [cat, mine] = await Promise.all([api("catalog" + (refresh ? "?refresh=1" : "")), api(`${realmPath(realm)}/catalog`)]);
  } catch (e) { body.replaceChildren(); return flash(e.message, true); }
  if (app.route.view !== "skills" || skillUi.tab !== "catalog" || app.realm !== realm) return;
  const installed = new Map(mine.installed.map((s) => [s.name, s]));
  const busy = mine.installing.length > 0 || cat.refreshing;
  const act = async (path, method) => {
    try { await api(path, method === "DELETE" ? undefined : {}, method); } catch (e) { flash(e.message, true); }
    renderMain();
  };
  const card = (s) => {
    const mineS = installed.get(s.name);
    const installing = mine.installing.includes(s.name);
    const error = mine.errors[s.name];
    return h("div", { class: "card cat-card" + (mineS ? " installed" : "") },
      h("div", { class: "cat-head" },
        h("span", { class: "cat-icon" }, icon("books")),
        h("div", { class: "grow" },
          h("div", { class: "strong" }, s.title || s.name),
          h("div", { class: "muted small mono" }, s.name)),
        mineS && h("span", { class: "chip ok" }, icon("check"), "installed")),
      h("div", { class: "cat-desc clamp3", title: s.description }, s.description || "No description."),
      h("div", { class: "objective-meta" },
        h("span", null, `${s.files} file${s.files === 1 ? "" : "s"}`),
        s.skipped > 0 && h("span", { title: "Binaries and very large files are left out" }, `${s.skipped} left out`),
        h("a", { href: `${cat.source}/tree/main/skills/${encodeURIComponent(s.name)}`, target: "_blank", rel: "noopener noreferrer" }, "source")),
      error && h("div", { class: "warn small" }, "Install failed: " + error),
      h("div", { class: "row-end" },
        installing ? h("span", { class: "working small" }, h("span", { class: "spinner" }), "Installing…")
          : mineS ? h("button", { class: "btn danger ghost", type: "button", onclick: () => act(`${realmPath(realm)}/catalog/${enc(s.name)}`, "DELETE") }, "Uninstall")
          : h("button", { class: "btn primary", type: "button", onclick: () => act(`${realmPath(realm)}/catalog/${enc(s.name)}/install`, "POST") }, icon("download"), "Install")));
  };
  body.replaceChildren(
    h("div", { class: "cat-bar" },
      h("span", { class: "muted small" }, "From ", h("a", { href: cat.source, target: "_blank", rel: "noopener noreferrer" }, cat.source.replace("https://", "")),
        cat.fetched_at ? ` · updated ${ago(cat.fetched_at)}` : ""),
      cat.refreshing ? h("span", { class: "working small" }, h("span", { class: "spinner" }), "Refreshing…")
        : h("button", { class: "btn ghost", type: "button", onclick: () => { body.remove(); renderCatalogTab(inner, realm, true); } }, icon("refresh"), "Refresh")),
    cat.error && h("div", { class: "error-banner" }, "Could not reach the catalog: " + cat.error),
    cat.skills.length
      ? h("div", { class: "cat-grid" }, cat.skills.map(card))
      : h("div", { class: "card muted small pad" }, cat.refreshing ? "Fetching the catalog from GitHub…" : "The catalog is empty."),
    h("p", { class: "muted small pad-top" },
      "Chats in this realm see each installed skill's name and description in their use_skill tool, and read its instructions when a task matches. " +
      "It is third-party guidance: it never outranks your objective or boundaries, and its scripts only run once copied into the workspace, with your approval."));
  if (busy) refreshSkillsSoon(realm, "catalog");
}

async function renderMcpTab(inner, realm) {
  const body = h("div", null, working("Loading MCP servers…"));
  inner.append(body);
  let list;
  try { list = await api(`${realmPath(realm)}/mcp`); } catch (e) { body.replaceChildren(); return flash(e.message, true); }
  if (app.route.view !== "skills" || skillUi.tab !== "mcp" || app.realm !== realm) return;
  const d = app.realmDetail;
  const rule = d && d.policy ? d.policy.skill : "ask";
  const onTab = () => app.route.view === "skills" && skillUi.tab === "mcp" && app.realm === realm;
  const listEl = h("div");
  // Only the list refreshes (while servers connect), so the form below keeps what you type.
  const reload = async () => {
    try { list = await api(`${realmPath(realm)}/mcp`); } catch (e) { return flash(e.message, true); }
    if (!onTab()) return;
    draw();
    if (list.some((s) => s.connecting)) setTimeout(() => { if (onTab()) reload(); }, 1500);
  };
  const call = async (path, payload, method) => {
    try { await api(path, payload, method); } catch (e) { flash(e.message, true); }
    reload();
  };
  const server = (s) => {
    const status = s.connecting ? h("span", { class: "chip running" }, "connecting…")
      : s.error ? h("span", { class: "chip failed" }, "error")
      : s.tools.length ? h("span", { class: "chip ok" }, `${s.tools.length} tool${s.tools.length === 1 ? "" : "s"}`)
      : h("span", { class: "chip" }, "not connected");
    return h("div", { class: "card objective" + (s.enabled ? "" : " off") },
      h("div", { class: "objective-head" },
        h("span", { class: "cat-icon" }, icon("plug")),
        h("div", { class: "grow" },
          h("div", null, h("span", { class: "strong mono" }, s.name), " ", status),
          s.description && h("div", { class: "muted small" }, s.description)),
        h("label", { class: "toggle small", title: s.enabled ? "Chats can use it" : "Hidden from chats" },
          h("input", { type: "checkbox", checked: s.enabled, onchange: (e) => call(`${realmPath(realm)}/mcp/${enc(s.name)}`, { enabled: e.target.checked }, "PATCH") }),
          h("span", null, s.enabled ? "on" : "off"))),
      h("code", { class: "path small mcp-cmd" }, "$ " + s.command),
      (s.server_info || s.connected_at) && h("div", { class: "objective-meta" },
        s.server_info && h("span", null, s.server_info), s.connected_at && h("span", null, "connected " + ago(s.connected_at))),
      s.error && h("pre", { class: "out small" }, s.error),
      s.tools.length > 0 && h("details", { class: "mcp-tools" },
        h("summary", { class: "muted small" }, `Tools chats get (${s.tools.length})`),
        s.tools.map((t) => h("div", { class: "kv mcp-tool", title: t.description },
          h("code", { class: "k" }, t.agent_name), h("span", { class: "v" }, t.description || "")))),
      h("div", { class: "row-end" },
        h("button", { class: "btn", type: "button", disabled: s.connecting, onclick: () => call(`${realmPath(realm)}/mcp/${enc(s.name)}/connect`, {}) },
          icon("refresh"), s.tools.length ? "Reconnect" : "Connect"),
        h("button", { class: "btn danger ghost", type: "button", onclick: () => {
          if (confirmDelete !== "mcp:" + s.name) { confirmDelete = "mcp:" + s.name; return draw(); }
          confirmDelete = null;
          call(`${realmPath(realm)}/mcp/${enc(s.name)}`, undefined, "DELETE");
        } }, confirmDelete === "mcp:" + s.name ? "Really remove?" : "Remove")));
  };
  const f = {
    name: h("input", { type: "text", placeholder: "name, e.g. github", autocomplete: "off", spellcheck: "false" }),
    command: h("input", { type: "text", class: "mono", placeholder: "npx -y @modelcontextprotocol/server-memory", autocomplete: "off", spellcheck: "false" }),
    desc: h("input", { type: "text", placeholder: "what it is for (optional)" }),
  };
  f.name.addEventListener("input", () => (f.name.value = f.name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
  const add = async () => {
    try {
      await api(`${realmPath(realm)}/mcp`, { name: f.name.value.trim(), command: f.command.value.trim(), description: f.desc.value.trim() });
      f.name.value = ""; f.command.value = ""; f.desc.value = "";
      flash("Added; connecting…");
    } catch (e) { return flash(e.message, true); }
    reload();
  };
  const draw = () => listEl.replaceChildren(
    list.length ? h("div", { class: "objectives" }, list.map(server)) : h("div", { class: "card muted small pad" }, "No MCP server in this realm yet."));
  draw();
  body.replaceChildren(
    listEl,
    h("h2", null, "Add a server"),
    h("p", { class: "muted" },
      "A stdio server: the command that starts it. Airth starts it in this realm's sandbox, lists its tools, and stops it after each call. ",
      "Calls follow the realm's skill rule (now: ", h("strong", null, POLICY_WORDS[rule] || rule), "). ",
      "If it needs a token, add it under Realm settings → Access: the server gets it as an environment variable."),
    h("div", { class: "card" },
      field("Name", f.name, "Lowercase; tools appear to chats as mcp__name__tool."),
      field("Command", f.command, "e.g. npx -y @modelcontextprotocol/server-filesystem .  ·  uvx mcp-server-git"),
      field("Description", f.desc),
      h("div", { class: "row-end" }, h("button", { class: "btn primary", type: "button", onclick: add }, icon("plug"), "Add and connect"))));
  if (list.some((s) => s.connecting)) setTimeout(() => { if (onTab()) reload(); }, 1500);
}

// ---------- command palette & shortcuts ----------

let palette = null;

/** Everything the palette can do, grouped; runs and realms come from what the app already loaded. */
function paletteItems() {
  const r = app.realm && enc(app.realm);
  const dark = window.airthTheme && window.airthTheme.current() === "dark";
  const items = [];
  const add = (group, label, lead, action, extra = {}) => items.push({ group, label, lead, action, ...extra });
  if (r) add("Actions", "New chat", icon("pencil"), () => go(`#/r/${r}`), { keys: modKey + "⇧O" });
  add("Actions", dark ? "Switch to light mode" : "Switch to dark mode", icon(dark ? "sun" : "moon"),
    () => { window.airthTheme && window.airthTheme.set(dark ? "light" : "dark"); renderFoot(); });
  add("Actions", "Show or hide the sidebar", icon("sidebar"), toggleSidebar, { keys: modKey + "B" });
  if (app.route.view === "run" && runEls) add("Actions", "Show or hide plan & details", icon("panel"), toggleSide);
  if (app.route.view === "run" && runEls && app.run) add("Actions", "Rename this chat", icon("rename"),
    () => setTimeout(() => editInPlace(runEls && runEls.header.querySelector("h1"), app.route.id, runName({ title: app.run.title, objective: app.run.state.objective })), 160));
  const ns = northStar();
  if (r && ns) {
    if (ns.milestones.length) add("North Star", "Push the North Star forward now", h("span", { class: "i ns-star" }, "★"), () => startNorthStarRun("push"), { hint: ns.goal });
    add("North Star", ns.milestones.length ? "Rework the milestones with Airth" : "Brainstorm milestones with Airth", h("span", { class: "i ns-star" }, "★"), () => startNorthStarRun("brainstorm"));
    add("North Star", focusOn() ? "Turn focus mode off" : "Turn focus mode on", h("span", { class: "i ns-star" }, "★"), toggleFocus, { hint: "shadow what does not serve the North Star" });
  } else if (r) {
    add("North Star", "Set a North Star for this realm", h("span", { class: "i ns-star" }, "★"), () => { nsUi.open = true; go(`#/r/${r}`); });
  }
  add("Go to", "Inbox", icon("inbox"), () => go("#/inbox"));
  add("Go to", "Usage", icon("chart"), () => go("#/usage"));
  if (app.realm) add("Go to", "What can Airth do for you?", icon("compass"), () => {
    const show = () => openCanDo((text) => {
      const ta = $(".new-view textarea");
      if (ta) { ta.value = text; ta.dispatchEvent(new Event("input")); ta.focus(); }
    });
    if (app.route.view === "new") show(); else { go(`#/r/${enc(app.realm)}`); setTimeout(show, 300); }
  });
  add("Go to", "Welcome guide", icon("compass"), () => { welcomeUi.step = 0; welcomeUi.setup = null; go("#/welcome"); });
  if (r) {
    add("Go to", "Documents", icon("folder"), () => go(`#/r/${r}/documents`));
    add("Go to", "Objectives", icon("clock"), () => go(`#/r/${r}/objectives`));
    add("Go to", "Skills", icon("spark"), () => go(`#/r/${r}/skills`));
    add("Go to", "Connectors (Notion, Linear, GitHub…)", icon("world"), () => { skillUi.tab = "connectors"; go(`#/r/${r}/skills`); });
    add("Go to", "Realm settings", icon("gear"), () => go(`#/r/${r}/settings`));
  }
  add("Go to", "Your profile", icon("notes"), () => go("#/owner"));
  add("Go to", "Appearance", icon("sun"), () => go("#/owner/appearance"));
  const runs = [...app.runs].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at));
  for (const run of runs) {
    const status = run.needs_approval ? "awaiting" : run.live ? "running" : run.status;
    add(`Chats in ${app.realm}`, runName(run), h("span", { class: "i" }, h("span", { class: "dot " + status })),
      () => go(`#/r/${enc(app.realm)}/run/${enc(run.id)}`), { hint: `${run.live ? "running" : run.status} · ${ago(run.updated_at)}`, run: true });
  }
  for (const realm of app.realms) {
    if (realm.name === app.realm || realm.archived) continue;
    add("Switch realm", realm.name, h("span", { class: "realm-mark small" }, realm.name.slice(0, 1).toUpperCase()),
      () => go(`#/r/${enc(realm.name)}`), { hint: realm.description });
  }
  return items;
}

function openPalette() {
  if (palette || app.unauthorized) return;
  const input = h("input", { type: "text", class: "pal-input", placeholder: "Search chats, pages, realms, actions…", "aria-label": "Search", autocomplete: "off", spellcheck: "false" });
  const list = h("div", { class: "pal-list", role: "listbox" });
  const overlay = h("div", { class: "pal-overlay", onmousedown: (e) => { if (e.target === overlay) closePalette(); } },
    h("div", { class: "pal", role: "dialog", "aria-modal": "true", "aria-label": "Command palette" },
      h("div", { class: "pal-search" }, icon("search"), input, h("kbd", null, "esc")), list));
  const items = paletteItems();
  let shown = [];
  let sel = 0;
  const run = (it) => { closePalette(); it.action(); };
  const draw = () => {
    const q = input.value.trim().toLowerCase();
    if (q) {
      const words = q.split(/\s+/);
      shown = items.filter((it) => { const text = `${it.label} ${it.hint || ""} ${it.group}`.toLowerCase(); return words.every((w) => text.includes(w)); });
    } else {
      // Without a query: actions, pages, the 8 latest runs, other realms.
      let runs = 0;
      shown = items.filter((it) => !it.run || runs++ < 8);
    }
    shown = shown.slice(0, 80);
    sel = Math.max(0, Math.min(sel, shown.length - 1));
    list.replaceChildren();
    if (!shown.length) list.append(h("div", { class: "pal-empty" }, "Nothing matches."));
    let group = null;
    shown.forEach((it, i) => {
      if (it.group !== group) { group = it.group; list.append(h("div", { class: "pal-group" }, group)); }
      list.append(h("button", { type: "button", class: "pal-item" + (i === sel ? " sel" : ""), role: "option", "aria-selected": String(i === sel),
          onmousemove: () => { if (sel !== i) { sel = i; draw(); } }, onclick: () => run(it) },
        it.lead, h("span", { class: "pal-label" }, it.label), it.hint && h("span", { class: "pal-hint" }, it.hint), it.keys && h("kbd", null, it.keys)));
    });
    const cur = list.querySelector(".pal-item.sel");
    if (cur) cur.scrollIntoView({ block: "nearest" });
  };
  input.addEventListener("input", () => { sel = 0; draw(); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); sel = Math.min(sel + 1, shown.length - 1); draw(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); sel = Math.max(sel - 1, 0); draw(); }
    else if (e.key === "Enter") { e.preventDefault(); if (shown[sel]) run(shown[sel]); }
    else if (e.key === "Escape") { e.preventDefault(); closePalette(); }
  });
  palette = { overlay, prevFocus: document.activeElement };
  document.body.append(overlay);
  draw();
  input.focus();
}

function closePalette() {
  if (!palette) return;
  const { overlay, prevFocus } = palette;
  palette = null;
  overlay.classList.add("closing");
  setTimeout(() => overlay.remove(), 140);
  if (prevFocus && prevFocus.focus && document.contains(prevFocus)) prevFocus.focus();
}

/** Phone-sized windows: the sidebar opens over the page, on demand. */
const narrow = () => window.matchMedia("(max-width: 760px)").matches;

function sidebarHidden() {
  if (narrow()) return true;
  try { return localStorage.getItem("airth-sidebar") === "hidden"; } catch { return false; }
}

// Crossing the phone-size boundary: closed on a phone, the owner's choice otherwise.
window.matchMedia("(max-width: 760px)").addEventListener("change", () => {
  const root = $("#app");
  if (root) root.classList.toggle("side-hidden", sidebarHidden());
});

// On a phone, a tap outside the open sidebar closes it.
document.addEventListener("click", (e) => {
  const root = $("#app");
  if (!narrow() || !root || root.classList.contains("side-hidden")) return;
  if (e.target.closest && (e.target.closest(".sidebar") || e.target.closest(".side-open"))) return;
  root.classList.add("side-hidden");
});

// In the Mac app the page is the title bar: an empty spot of the top strip
// moves the window, and a double click zooms it, as in other Mac apps.
const TITLE_BAR = 44;
const INTERACTIVE = "button, a, input, textarea, select, label, summary, [contenteditable], [role=button], .renamable";
function onTitleBar(e) {
  if (document.documentElement.dataset.shell !== "mac" || !window.ipc) return false;
  if (e.button !== 0 || e.clientY > TITLE_BAR) return false;
  return !(e.target.closest && e.target.closest(INTERACTIVE));
}
document.addEventListener("mousedown", (e) => {
  if (!onTitleBar(e)) return;
  e.preventDefault(); // no text selection from a title bar click
  if (e.detail === 1) window.ipc.postMessage("drag");
});
document.addEventListener("dblclick", (e) => {
  if (onTitleBar(e)) window.ipc.postMessage("zoom");
});

function toggleSidebar() {
  const hidden = !$("#app").classList.contains("side-hidden");
  $("#app").classList.toggle("side-hidden", hidden);
  if (!narrow()) try { localStorage.setItem("airth-sidebar", hidden ? "hidden" : "shown"); } catch {}
}

document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  const key = e.key.toLowerCase();
  if (mod && !e.shiftKey && !e.altKey && key === "k") { e.preventDefault(); palette ? closePalette() : openPalette(); }
  else if (mod && e.shiftKey && key === "o") { e.preventDefault(); if (app.realm) go(`#/r/${enc(app.realm)}`); }
  else if (mod && !e.shiftKey && !e.altKey && key === "b") { e.preventDefault(); toggleSidebar(); }
  else if (e.key === "Escape" && realmMenuOpen) { realmMenuOpen = false; renderRealmSwitch(); }
});

// ---------- usage ----------

const usageUi = { days: 30 };

/** 1234 → 1.2k, 4567890 → 4.57M: token counts at a glance. */
function tok(n) {
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 1 : 2) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(n >= 1e4 ? 0 : 1) + "k";
  return String(n);
}

const total = (t) => t.input + t.output;
const cachePct = (t) => (t.input ? Math.round((100 * t.cache_read) / t.input) : 0);
const tokDetail = (t) => `${tok(t.input)} in · ${tok(t.output)} out · ${t.calls} call${t.calls === 1 ? "" : "s"}`;

async function renderUsage(main) {
  page(main, "Usage", "Tokens your chats exchanged with the model, across every realm. Input includes what the provider served from its prompt cache, which costs far less than fresh input.");
  const inner = $(".page-inner", main);
  const period = h("div", { class: "seg", role: "radiogroup", "aria-label": "Period" }, [7, 30, 90].map((d) =>
    h("button", { type: "button", role: "radio", "aria-checked": String(usageUi.days === d), class: usageUi.days === d ? "on" : "",
      onclick: () => { usageUi.days = d; renderMain(); } }, `${d} days`)));
  const body = h("div", null, h("div", { class: "working" }, h("span", { class: "orb" }), h("span", { class: "shimmer" }, "Adding up…")));
  inner.append(h("div", { class: "usage-bar" }, period), body);
  let r;
  try { r = await api(`usage?days=${usageUi.days}`); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "usage") return;

  const stat = (label, value, detail) => h("div", { class: "stat" },
    h("div", { class: "stat-label" }, label), h("div", { class: "stat-value" }, value), h("div", { class: "stat-detail" }, detail));
  const stats = h("div", { class: "stats" },
    stat(`Last ${r.days} days`, tok(total(r.window)), tokDetail(r.window)),
    stat("Today", tok(total(r.today)), tokDetail(r.today)),
    stat("This month", tok(total(r.month)), tokDetail(r.month)),
    stat("From cache", cachePct(r.window) + "%", `${tok(r.window.cache_read)} of ${tok(r.window.input)} input tokens`));

  if (!r.window.calls) {
    body.replaceChildren(stats, h("div", { class: "card muted small pad" }, "No model calls in this period."));
    return;
  }

  // Daily chart: fresh input, cached input and output, stacked.
  const max = Math.max(1, ...r.by_day.map((d) => total(d)));
  const every = Math.ceil(r.by_day.length / 7);
  const cols = r.by_day.map((d, i) => {
    const seg = (cls, n) => {
      const el = h("span", { class: "seg-" + cls });
      el.style.height = (100 * n) / max + "%";
      return el;
    };
    const date = new Date(d.date + "T12:00:00");
    const label = date.toLocaleDateString([], { day: "numeric", month: "short" });
    const bar = h("div", { class: "bar" }, seg("out", d.output), seg("cache", d.cache_read), seg("in", d.input - d.cache_read));
    bar.style.animationDelay = Math.min(i * 18, 600) + "ms";
    return h("div", { class: "col" + (i === r.by_day.length - 1 ? " today" : ""),
        title: `${label}\n${tok(total(d))} tokens\n${tok(d.input - d.cache_read)} fresh input · ${tok(d.cache_read)} cached · ${tok(d.output)} output\n${d.calls} calls` },
      bar, h("div", { class: "col-label" }, i % every === 0 || i === r.by_day.length - 1 ? label : ""));
  });
  const chart = h("div", { class: "card chart-card" },
    h("div", { class: "chart-head" },
      h("span", { class: "strong" }, "Per day"),
      h("span", { class: "legend" },
        h("span", null, h("i", { class: "seg-in" }), "fresh input"),
        h("span", null, h("i", { class: "seg-cache" }), "cached input"),
        h("span", null, h("i", { class: "seg-out" }), "output"))),
    h("div", { class: "chart" }, cols));

  const shareList = (title, items, linkTo) => h("div", { class: "card share-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, title)),
    items.map((s, i) => {
      const pct = (100 * total(s)) / total(r.window);
      const fill = h("span");
      fill.style.width = Math.max(pct, 1) + "%";
      fill.style.animationDelay = i * 60 + "ms";
      const nameEl = linkTo ? h("a", { href: linkTo(s) }, s.name) : h("span", null, s.name === "unrecorded" ? "before tracking" : s.name);
      return h("div", { class: "share", title: tokDetail(s) + ` · ${cachePct(s)}% cached` },
        h("div", { class: "share-top" }, nameEl, h("span", { class: "muted" }, `${tok(total(s))} · ${Math.round(pct)}%`)),
        h("div", { class: "share-bar" }, fill));
    }));

  const top = h("div", { class: "card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, "Hungriest chats")),
    r.top_runs.map((run) => h("button", { class: "list-row link", type: "button", onclick: () => go(`#/r/${enc(run.realm)}/run/${enc(run.id)}`) },
      h("div", { class: "grow" }, h("div", { class: "clamp" }, run.objective || "(no objective)"), h("div", { class: "muted small" }, `${run.realm} · ${tokDetail(run)} · ${cachePct(run)}% cached`)),
      h("span", { class: "strong" }, tok(total(run))))));

  body.replaceChildren(stats, chart,
    h("div", { class: "shares" },
      shareList("By realm", r.by_realm, (s) => `#/r/${enc(s.name)}`),
      shareList("By model", r.by_model)),
    top);
}

// ---------- inbox ----------

const KIND_MARK = { completed: "✓", failed: "✗", stopped: "■", approval: "?", review: "⧗" };

async function renderInbox(main) {
  page(main, "Inbox", "What Airth did on its own, and what waits for you.");
  const inner = $(".page-inner", main);
  let notes, summary;
  try { [notes, summary] = await Promise.all([api("inbox"), api("summary?hours=24")]); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "inbox") return;
  const open = (realm, run) => go(`#/r/${enc(realm)}${run ? "/run/" + enc(run) : ""}`);

  inner.append(h("h2", null, "Last 24 hours"));
  if (!summary.length) inner.append(h("div", { class: "card muted small pad" }, "Nothing happened."));
  for (const r of summary) {
    const line = (mark, l, extra) => h("button", { class: "inbox-row link", type: "button", onclick: () => open(r.realm, l.id) },
      h("span", { class: "inbox-mark" }, mark),
      h("div", { class: "grow" }, 
        h("div", { class: "inbox-title" }, l.objective, l.origin && h("span", { class: "muted small" }, " · " + l.origin)),
        (extra || l.report) && h("div", { class: "inbox-detail" }, extra || l.report)));
    const sections = [
      r.to_review.length > 0 && h("div", { class: "inbox-section" },
        h("div", { class: "inbox-section-label" }, "To review"),
        r.to_review.map((l) => line("⧗", l, "Change waiting for your review"))),
      r.unfinished.length > 0 && h("div", { class: "inbox-section" },
        h("div", { class: "inbox-section-label" }, "Can resume"),
        r.unfinished.map((l) => line("↻", l, `${l.status}: can be resumed`))),
      r.recent.length > 0 && h("div", { class: "inbox-section" },
        h("div", { class: "inbox-section-label" }, "Recent"),
        r.recent.map((l) => line("✓", l)))
    ].filter(Boolean);
    
    inner.append(h("div", { class: "card inbox-card" },
      h("div", { class: "inbox-realm-head" }, h("span", { class: "strong" }, r.realm),
        h("span", { class: "muted small" }, `${r.runs} chat${r.runs === 1 ? "" : "s"} · ${r.completed} completed · ${r.failed} failed · ${r.stopped} stopped`)),
      sections.length > 0 ? h("div", { class: "inbox-content" }, sections)
        : h("div", { class: "muted small pad" }, "No activity"),
      r.objectives.filter((o) => o.enabled).length > 0 && h("div", { class: "inbox-objectives" },
        "Objectives: ", r.objectives.filter((o) => o.enabled).map((o) => o.id + (o.next_at ? ` (${when(o.next_at)})` : "")).join(" · "))));
  }

  inner.append(h("h2", null, "Notifications"));
  inner.append(notes.length ? h("div", { class: "card inbox-notifications" }, notes.slice(0, 100).map((n) =>
    h("button", { class: "inbox-notification" + (n.read ? "" : " unread"), type: "button", onclick: () => open(n.realm, n.run) },
      h("span", { class: "inbox-kind " + n.kind }, KIND_MARK[n.kind] || "•"),
      h("div", { class: "grow" },
        h("div", { class: "inbox-notif-title" }, n.title),
        n.body && h("div", { class: "inbox-notif-body" }, n.body),
        h("div", { class: "inbox-notif-meta" }, n.realm + " · " + ago(n.created_at))))))
    : h("div", { class: "card muted small pad" }, "No notifications."));
  if (notes.some((n) => !n.read)) {
    api("inbox/read", {}).then(() => { if (app.info) app.info.unread = 0; renderSidebar(); }).catch(() => {});
  }
}

// ---------- new realm ----------

function renderNewRealm(main) {
  const first = app.realms.length === 0;
  const name = h("input", { type: "text", placeholder: "personal, acme, freelance…", autocomplete: "off", spellcheck: "false" });
  const desc = h("input", { type: "text", placeholder: "e.g. My job at Acme Corp" });
  const root = h("input", { type: "text", placeholder: "Leave empty for a private folder, or e.g. ~/code/project", spellcheck: "false" });
  const iso = h("select", null,
    h("option", { value: "" }, "Automatic (a worktree per chat for git repositories)"),
    h("option", { value: "none" }, "None: work directly in the folder"),
    h("option", { value: "worktree" }, "Worktree per chat"));
  const star = h("input", { type: "text", placeholder: "Optional: the one goal of this realm, e.g. “100 kit sales by December”" });
  const err = h("div", { class: "error-banner", hidden: true });
  const create = async () => {
    try {
      const r = await api("realms", { name: name.value.trim(), description: desc.value, root: root.value, isolation: iso.value || null });
      await refreshRealms();
      go(`#/r/${enc(r.name)}`);
      // With a North Star, Airth starts defining it with the owner at once.
      if (star.value.trim()) {
        app.realmDetail = await api(`${realmPath(r.name)}/northstar`, { goal: star.value, why: "" }, "PUT");
        if (app.realmDetail.model) { flash("Realm created. Airth has a few questions about your North Star."); await startNorthStarRun("brainstorm", r.name); }
        else { flash("Realm created. Add a model in its settings to define the North Star with Airth."); renderMain(); renderSidebar(); }
      }
    } catch (e) { err.textContent = e.message; err.hidden = false; }
  };
  name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
  page(main, first ? "Welcome to Airth" : "New realm",
    first
      ? "Airth keeps each area of your life in its own realm, with its own files, memory, credentials and permissions. Nothing crosses between realms. Create your first one."
      : "A realm is an isolated area of your life: its own files, memory, credentials and permissions.",
    h("div", { class: "card" },
      field("Name", name, "Lowercase letters, digits and dashes."),
      field("Description", desc, "Airth sees this, so it knows what the realm is for."),
      field("Folder", root, "The directory Airth works in. Other realms can never read it."),
      field("Isolation", iso),
      field("North Star", star, "Airth will ask you a few questions to sharpen it and define the milestones with you.")),
    err,
    h("div", { class: "row-end spaced" }, h("button", { class: "btn primary", type: "button", onclick: create }, "Create realm")));
  name.focus();
}

// ---------- welcome guide ----------

/** The first launch: what Airth is, then the few things it needs, one at a time. */
const WELCOME_STEPS = ["hello", "you", "model", "realm", "tips"];
const welcomeUi = { step: 0, setup: null, realm: null, star: "" };

async function renderWelcome(main) {
  if (!welcomeUi.setup) {
    try { welcomeUi.setup = await api("setup"); } catch { welcomeUi.setup = { realms: app.realms.length, model_ready: false }; }
    if (app.route.view !== "welcome") return;
  }
  const s = welcomeUi.setup;
  const step = WELCOME_STEPS[welcomeUi.step] || "hello";
  const next = () => { welcomeUi.step = Math.min(welcomeUi.step + 1, WELCOME_STEPS.length - 1); renderMain(); };
  const back = () => { welcomeUi.step = Math.max(welcomeUi.step - 1, 0); renderMain(); };
  const err = h("div", { class: "error-banner", hidden: true });
  const fail = (e) => { err.textContent = e.message; err.hidden = false; };
  const busy = async (btn, work) => {
    btn.disabled = true;
    try { await work(); } catch (e) { fail(e); } finally { btn.disabled = false; }
  };
  const actions = (...items) => h("div", { class: "welcome-actions" },
    welcomeUi.step > 0 ? h("button", { class: "btn ghost", type: "button", onclick: back }, "Back") : h("span"),
    h("div", { class: "row-start" }, ...items));
  const skip = (label) => h("button", { class: "btn ghost", type: "button", onclick: next }, label || "Skip");

  let body;
  if (step === "hello") {
    const point = (ic, title, text) => h("div", { class: "welcome-point" }, h("span", { class: "welcome-icon" }, icon(ic)), h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, text)));
    body = [
      h("div", { class: "logo welcome-logo", "aria-hidden": "true" }),
      h("h1", null, "Welcome to Airth"),
      h("p", { class: "lead" }, "Your personal agent. It works on your Mac with your files, apps and services, and asks you before anything that matters."),
      h("div", { class: "welcome-points" },
        point("folder", "Realms", "Each area of your life (work, a project, home) gets its own files, memory, keys and rules. Nothing crosses between them."),
        point("send", "Chats", "Ask for something; Airth plans it, does it, and shows you each step. You approve what it may not do on its own."),
        point("star", "North Star", "Give a realm one goal. Airth helps you break it into milestones and keeps moving it forward.")),
      h("p", { class: "muted small" }, "Setting up takes about two minutes."),
      actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Get started")),
    ];
  } else if (step === "you") {
    const name = h("input", { type: "text", placeholder: "Your first name", value: app.ownerName || "" });
    const about = h("textarea", { rows: 3, placeholder: "e.g. Freelance designer in Lyon. I sell kits online on the side." });
    const go_on = h("button", { class: "btn primary", type: "button" }, "Continue");
    go_on.onclick = () => busy(go_on, async () => {
      if (name.value.trim() || about.value.trim()) {
        const owner = await api("owner");
        const keep = Object.fromEntries(OWNER_LISTS.map(([k]) => [k, owner[k]]));
        await api("owner", { ...keep, name: name.value.trim() || owner.name, about: about.value.trim() || owner.about }, "PUT");
        app.ownerName = name.value.trim() || owner.name;
        renderFoot();
      }
      next();
    });
    api("owner").then((o) => { if (!name.value) name.value = o.name || ""; if (!about.value) about.value = o.about || ""; }).catch(() => {});
    body = [
      h("h1", null, "A little about you"),
      h("p", { class: "lead" }, "Airth uses this to write and decide the way you would. It stays on this Mac; you choose which realms see it."),
      h("label", { class: "welcome-label" }, "Name", name),
      h("label", { class: "welcome-label" }, "What you do", about),
      err,
      actions(skip(), go_on),
    ];
    setTimeout(() => name.focus());
  } else if (step === "model") {
    if (s.model_ready) {
      body = [
        h("h1", null, "Airth can think"),
        h("p", { class: "lead" }, h("span", { class: "ok" }, "✓ "), "A model is set up. You can change it, or add a cheaper fast model, in each realm's settings under Models."),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else {
      const key = h("input", { type: "password", placeholder: "sk-ant-…", autocomplete: "off", spellcheck: "false" });
      const save = h("button", { class: "btn primary", type: "button" }, "Check and save");
      save.onclick = () => busy(save, async () => {
        welcomeUi.setup = await api("setup/key", { key: key.value });
        flash("Key saved");
        next();
      });
      key.addEventListener("keydown", (e) => { if (e.key === "Enter") save.click(); });
      body = [
        h("h1", null, "Connect a model"),
        h("p", { class: "lead" }, "Airth thinks with Claude, through your own Anthropic account: you pay Anthropic directly for what you use."),
        h("ol", { class: "welcome-steps" },
          h("li", null, "Open ", h("a", { href: "https://console.anthropic.com/settings/keys", target: "_blank", rel: "noopener" }, "console.anthropic.com/settings/keys"), " and sign in."),
          h("li", null, "Create a key, copy it, and paste it here.")),
        h("label", { class: "welcome-label" }, "Anthropic API key", key),
        h("p", { class: "muted small" }, "Airth checks the key, then keeps it on this Mac only (in ~/.airth, readable by you alone) for all your realms. The model never sees it. Prefer OpenAI, Grok, DeepSeek, Mistral, OpenRouter or a local Ollama model? Skip this and pick one in a realm's settings."),
        err,
        actions(skip("Later"), save),
      ];
      setTimeout(() => key.focus());
    }
  } else if (step === "realm") {
    if (welcomeUi.realm || s.realms > 0) {
      const n = welcomeUi.realm ? 1 : s.realms;
      body = [
        h("h1", null, welcomeUi.realm ? `“${welcomeUi.realm}” is ready` : "Your realms are ready"),
        h("p", { class: "lead" }, welcomeUi.realm ? "Your first realm exists, with a private folder of its own." : `You already have ${n} realm${n > 1 ? "s" : ""}. Add more anytime from the realm menu, top left.`),
        actions(h("button", { class: "btn primary", type: "button", onclick: next }, "Continue")),
      ];
    } else {
      const name = h("input", { type: "text", value: "personal", autocomplete: "off", spellcheck: "false" });
      name.addEventListener("input", () => (name.value = name.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-")));
      const desc = h("input", { type: "text", placeholder: "e.g. My personal life: admin, health, side projects" });
      const star = h("input", { type: "text", placeholder: "Optional, e.g. “Run a half marathon in March”" });
      const create = h("button", { class: "btn primary", type: "button" }, "Create realm");
      create.onclick = () => busy(create, async () => {
        const r = await api("realms", { name: name.value.trim(), description: desc.value.trim() });
        welcomeUi.realm = r.name;
        welcomeUi.star = star.value.trim();
        await refreshRealms();
        renderSidebar();
        next();
      });
      body = [
        h("h1", null, "Your first realm"),
        h("p", { class: "lead" }, "A realm is one area of your life. Start with one; add others (work, a client, a project) whenever you like."),
        h("label", { class: "welcome-label" }, "Name", name, h("span", { class: "muted small" }, "Lowercase letters, digits and dashes.")),
        h("label", { class: "welcome-label" }, "What it is for", desc, h("span", { class: "muted small" }, "Airth reads this in every chat of the realm.")),
        h("label", { class: "welcome-label" }, "North Star", star, h("span", { class: "muted small" }, "The one goal of this realm. Airth will ask you a few questions to sharpen it.")),
        err,
        actions(create),
      ];
      setTimeout(() => desc.focus());
    }
  } else {
    const tip = (ic, title, text) => h("div", { class: "welcome-point" }, h("span", { class: "welcome-icon" }, icon(ic)), h("div", null, h("div", { class: "strong" }, title), h("div", { class: "muted small" }, text)));
    const mac = document.documentElement.dataset.shell === "mac";
    const done = h("button", { class: "btn primary", type: "button" }, welcomeUi.star && s.model_ready ? "Define my North Star" : "Start using Airth");
    done.onclick = () => busy(done, async () => {
      try { localStorage.setItem("airth-welcomed", "1"); } catch {}
      const realm = welcomeUi.realm || preferredRealm();
      const star = welcomeUi.star;
      Object.assign(welcomeUi, { step: 0, setup: null, realm: null, star: "" });
      if (!realm) return go("#/realms/new");
      go(`#/r/${enc(realm)}`);
      if (star) {
        app.realmDetail = await api(`${realmPath(realm)}/northstar`, { goal: star, why: "" }, "PUT");
        if (app.realmDetail.model) await startNorthStarRun("brainstorm", realm);
        else { flash("Connect a model in the realm's settings to define the North Star with Airth."); renderMain(); renderSidebar(); }
      }
    });
    body = [
      h("h1", null, "You're set"),
      h("p", { class: "lead" }, "A few things worth knowing:"),
      h("div", { class: "welcome-points" },
        tip("pencil", "New chat", "Ask for anything in plain words: “sort my downloads”, “draft a reply to Marc”, “plan my week”."),
        tip("inbox", "Inbox", "When Airth needs your OK, or finishes something, it lands here (and in a notification)."),
        tip("clock", "Objectives", "Recurring work: every morning, when a file changes, or on a webhook."),
        tip("world", "Skills & connectors", "Connect Notion, Linear, Stripe… or install skills, per realm."),
        tip("search", modKey + "K", "Find a chat, a page or an action from anywhere in the app."),
        mac && tip("window", "⌥⇧A", "Shows or hides Airth from any app. Closing the window keeps Airth in the menu bar.")),
      err,
      actions(done),
    ];
  }

  main.append(h("div", { class: "page welcome" },
    h("div", { class: "welcome-inner" },
      h("div", { class: "welcome-dots", "aria-label": `Step ${welcomeUi.step + 1} of ${WELCOME_STEPS.length}` },
        WELCOME_STEPS.map((_, i) => h("span", { class: i === welcomeUi.step ? "on" : i < welcomeUi.step ? "done" : "" }))),
      h("div", { class: "welcome-card" }, ...body),
      welcomeUi.step < WELCOME_STEPS.length - 1 && h("button", { class: "welcome-skip", type: "button", onclick: () => {
        try { localStorage.setItem("airth-welcomed", "1"); } catch {}
        Object.assign(welcomeUi, { step: 0, setup: null, realm: null, star: "" });
        const r = preferredRealm();
        go(r ? `#/r/${enc(r)}` : "#/realms/new");
      } }, "Skip the guide"))));
}

// ---------- polling ----------

let runTimer = null;

async function pollRun() {
  clearTimeout(runTimer);
  if (app.route.view !== "run" || app.unauthorized) return;
  const { realm, id } = app.route;
  let next = 3000;
  try {
    const q = app.version != null && app.run && app.run.live ? `?since=${app.version}` : "";
    const d = await api(runPath(realm, id) + q);
    if (app.route.view !== "run" || app.route.id !== id) return;
    const same = !d.unchanged && app.run && !d.live && !app.run.live &&
      d.state.updated_at === app.run.state.updated_at;
    if (!d.unchanged && !same) {
      const wasLive = app.run && app.run.live;
      app.run = d;
      if (wasLive && !d.live) app.diff = null;
      renderRun();
    }
    app.version = d.version;
    if (d.live || (app.run && app.run.live)) next = 600;
    if (app.pollFailures) { app.pollFailures = 0; flash("Back in touch with Airth"); }
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
    // A run already on screen stays there through a short outage (a restart,
    // an update): say it once, then retry, more slowly each time.
    app.pollFailures = (app.pollFailures || 0) + 1;
    if (app.run && runEls) {
      if (app.pollFailures === 1) flash("Lost touch with Airth; retrying…", true);
      next = Math.min(1000 * 2 ** app.pollFailures, 15000);
    } else if (runEls) {
      runEls.transcript.replaceChildren(h("div", { class: "end failed" }, e.message));
    }
  }
  runTimer = setTimeout(pollRun, next);
}

async function refreshRuns() {
  if (!app.realm) return;
  try {
    app.runs = await api(`${realmPath(app.realm)}/runs`);
    renderSidebar();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

function updateKey(info) {
  const u = info && info.update;
  return u ? [u.phase, u.step, u.available && u.available.commit, u.error].join("|") : "";
}

function updateWidget() {
  const u = app.info && app.info.update;
  if (app.restarting) return h("div", { class: "update restarting" }, h("span", { class: "spinner" }), "Restarting Airth…");
  if (!u || !u.available) return null;
  const a = u.available;
  const what = `${a.count} change${a.count === 1 ? "" : "s"}`;
  const changes = h("ul", { class: "update-changes" }, a.changes.slice(0, 8).map((c) => h("li", null, c)));
  if (u.phase === "ready")
    return h("div", { class: "update ready" },
      h("div", { class: "strong" }, "New update available"),
      h("details", null, h("summary", { class: "muted small" }, `${a.short} · ${what} · ${a.release ? "signature verified" : "tests passed"}`), changes),
      h("button", { class: "btn primary", type: "button", onclick: restartToUpdate }, "Restart to update"));
  if (u.phase === "preparing" || u.phase === "checking")
    return h("div", { class: "update" }, h("span", { class: "spinner" }),
      h("div", null, h("div", null, "Preparing update ", h("code", null, a.short)), h("div", { class: "muted small" }, (u.step || "starting") + "…")));
  if (u.phase === "failed")
    return h("div", { class: "update failed" },
      h("div", { class: "strong" }, `Update ${a.short} not installed`),
      h("details", null, h("summary", { class: "muted small" }, a.release ? "It could not be downloaded or verified" : "Its build or tests failed"), h("pre", { class: "out small" }, u.error || "")),
      h("button", { class: "btn", type: "button", onclick: () => api("update/check", {}).then(refreshInfo) }, "Try again"));
  return null;
}

async function restartToUpdate() {
  const live = app.runs.filter((r) => r.live).length;
  if (live && !app.confirmRestart) {
    app.confirmRestart = true;
    flash(`${live} chat${live === 1 ? " is" : "s are"} in progress: they will stop (and can be resumed). Click again to restart.`);
    return;
  }
  app.confirmRestart = false;
  const before = app.info.version;
  try { await api("update/restart", {}); } catch (e) { return flash(e.message, true); }
  app.restarting = true;
  renderSidebar();
  for (let i = 0; i < 300; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    try {
      const info = await api("info");
      if (info.version !== before) { location.reload(); return; }
    } catch {}
  }
  app.restarting = false;
  flash("Airth did not come back; check the terminal or `airth service status`.", true);
  renderSidebar();
}

async function refreshInfo() {
  try {
    const before = app.info && app.info.unread;
    const beforeUpdate = updateKey(app.info);
    app.info = await api("info");
    if (app.info.unread !== before || updateKey(app.info) !== beforeUpdate) renderSidebar();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

async function refreshRealms() {
  try {
    app.realms = await api("realms");
    renderRealmSwitch();
    updateTitle();
  } catch (e) {
    if (e instanceof AuthError) unauthorized();
  }
}

async function refreshRealmDetail() {
  if (!app.realm) return;
  try {
    app.realmDetail = await api(realmPath(app.realm));
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
    app.realmDetail = null;
  }
}

function unauthorized() {
  app.unauthorized = true;
  renderMain();
}

async function boot() {
  $("#side-close").append(icon("sidebar"));
  $("#side-open").append(icon("sidebar"));
  $("#side-close").addEventListener("click", toggleSidebar);
  $("#side-open").addEventListener("click", toggleSidebar);
  if (sidebarHidden()) $("#app").classList.add("side-hidden");
  window.addEventListener("hashchange", onRoute);
  if (app.unauthorized) return renderMain();
  try {
    app.info = await api("info");
  } catch (e) {
    if (e instanceof AuthError) return unauthorized();
  }
  try { const o = await api("owner"); app.ownerName = o.name; app.ownerAvatar = o.avatar; } catch {}
  await refreshRealms();
  await onRoute();
  // Background refreshes pause while the window is hidden, and catch up when it shows.
  setInterval(() => { if (!document.hidden) { refreshRealms(); refreshRuns(); } }, 2000);
  setInterval(() => { if (document.hidden) return; refreshInfo(); if (app.route.view === "new" || focusOn()) refreshNorthStar(); }, 5000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) { refreshRealms(); refreshRuns(); refreshInfo(); } });
}

boot();
