"use strict";
// The new interface (decision 57): plain scripts joined in name order by
// build.rs, served at /next next to the current interface, same API.

const $ = (sel, root = document) => root.querySelector(sel);
const enc = encodeURIComponent;

/** Build an element. Strings become text nodes: content is never parsed as
 *  HTML. `svg` takes constant icon markup only, never data. */
// Components (decision 72): Preact with htm, vendored in /next/vendor.js.
// `html` writes a component's markup; `h` below is the older DOM builder,
// used by the pages not moved to components yet.
const html = htm.bind(preact.h);
const { useState, useEffect, useLayoutEffect, useRef } = preactHooks;

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
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  moon: '<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>',
  profile: '<circle cx="12" cy="8" r="4"/><path d="M4 21c0-4.4 3.6-8 8-8s8 3.6 8 8"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  // Needs you: what an item is, and what the founder answers.
  alert: '<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><path d="M12 9v4M12 17h.01"/>',
  shield: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M12 8v4M12 16h.01"/>',
  approve: '<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
  hand: '<path d="M18 11V6a2 2 0 0 0-4 0v4M14 10V4a2 2 0 0 0-4 0v6M10 10.5V6a2 2 0 0 0-4 0v8l-1.5-1.5a2 2 0 0 0-3 2.6L6 20a5 5 0 0 0 4 2h3a6 6 0 0 0 6-6v-5a2 2 0 0 0-4 0"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  doc: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6M8 13h8M8 17h5"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  pencil: '<path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z"/><path d="m13.5 6.5 4 4"/>',
  retry: '<path d="M3 12a9 9 0 0 1 15.5-6.3L21 8M21 3v5h-5M21 12a9 9 0 0 1-15.5 6.3L3 16M3 21v-5h5"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>',
  arrow: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  up: '<path d="m6 15 6-6 6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>',
  filter: '<path d="M3 5h18l-7 8v6l-4 2v-8z"/>',
};
const icon = (name, size = 20, stroke = 1.8) =>
  `<svg aria-hidden="true" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round">${ICON_PATHS[name]}</svg>`;

/** A business's mark: its first letter. */
const initial = (name) => (String(name || "?").trim()[0] || "?").toUpperCase();
