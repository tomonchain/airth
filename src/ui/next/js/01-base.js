"use strict";
// The new interface (decision 57): plain scripts joined in name order by
// build.rs, served at /next next to the current interface, same API.

const $ = (sel, root = document) => root.querySelector(sel);
const enc = encodeURIComponent;

/** Build an element. Strings become text nodes: content is never parsed as
 *  HTML. `svg` takes constant icon markup only, never data. */
function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === false || v == null) continue;
    if (k === "class") el.className = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "svg") el.innerHTML = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat(Infinity)) {
    if (c == null || typeof c === "boolean") continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

// The DOM's own append and replaceChildren turn arrays, null and booleans
// into text. Pages pass lists and conditional children, so those are
// flattened and dropped here once, as h() does.
for (const name of ["append", "replaceChildren"]) {
  const native = Element.prototype[name];
  Element.prototype[name] = function (...items) {
    return native.apply(this, items.flat(Infinity).filter((c) => c != null && typeof c !== "boolean"));
  };
}

// The key, from the link Airth gives (`/#token=…`), kept for this tab, and
// on the phone for the device (decision 37). The classic interface keeps it
// the same way, so both use one key.
let token = null;
try {
  token = sessionStorage.getItem("airth-token") || localStorage.getItem("airth-token");
} catch {}
{
  const m = location.hash.match(/token=([0-9a-f]+)/);
  if (m) {
    token = m[1];
    try {
      sessionStorage.setItem("airth-token", token);
      if (location.hostname.endsWith(".ts.net")) localStorage.setItem("airth-token", token);
    } catch {}
    history.replaceState(null, "", "#/hq");
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

/** Icons from docs/design/mockups (stroke 1.8, 24 px grid). Constant markup. */
const ICON_PATHS = {
  hq: '<path d="M3 10.5 12 3l9 7.5V21H3z"/>',
  needs: '<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.5 5h13L22 12v7H2v-7z"/>',
  team: '<circle cx="9" cy="8" r="3"/><path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6"/><circle cx="17" cy="9" r="2.5"/><path d="M15.5 14.2c2.9.4 5 2.8 5 5.8"/>',
  routines: '<rect x="3" y="3" width="7" height="7" rx="2"/><rect x="14" y="14" width="7" height="7" rx="2"/><path d="M10 6.5h4a3 3 0 0 1 3 3V14"/>',
  tasks: '<path d="M3 6l2 2 3-3M3 13l2 2 3-3M12 6h9M12 13h9M12 20h9"/>',
  calendar: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M16 3v4M8 3v4M3 10h18"/>',
  business: '<path d="M4 21V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v16M16 9h2a2 2 0 0 1 2 2v10M3 21h18M8 7h4M8 11h4M8 15h4"/>',
  workshop: '<path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.4-.6-.6-2.4z"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1"/>',
  updown: '<path d="m7 9 5-5 5 5M7 15l5 5 5-5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
};
const icon = (name, size = 20, stroke = 1.8) =>
  `<svg aria-hidden="true" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round">${ICON_PATHS[name]}</svg>`;

/** A business's mark: its first letter. */
const initial = (name) => (String(name || "?").trim()[0] || "?").toUpperCase();
