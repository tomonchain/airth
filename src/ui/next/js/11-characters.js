// Agent characters (decision 65, docs/design/characters): drawn by code as
// inline SVG from the agent's seed (its shape and small traits), its
// department (its colour, a token, never a status colour), its accessory
// and its state. Ported from the reference chars.py; no image file.

const CH_SHAPES = {
  round: { body: '<ellipse cx="60" cy="70" rx="38" ry="35"/>', eye: 62, top: 35, belly: [60, 84, 22, 14], feet: 104, armY: 76, armDx: 40 },
  bean: { body: '<rect x="28" y="24" width="64" height="80" rx="32"/>', eye: 54, top: 24, belly: [60, 82, 20, 15], feet: 104, armY: 72, armDx: 34 },
  block: { body: '<rect x="25" y="34" width="70" height="70" rx="24"/>', eye: 60, top: 34, belly: [60, 84, 22, 13], feet: 104, armY: 76, armDx: 37 },
};
const CH_INK = "#0F1011";
const CH_DEPTS = ["leadership", "marketing", "sales", "support", "product", "operations", "finance"];
const CH_STATES = { idle: "Idle", working: "Working", needs: "Needs you", incident: "Incident", asleep: "Asleep" };

function chAccessory(kind, s, state, dark) {
  if (kind === "pen") return `<g transform="translate(${60 + s.armDx - 2} ${s.armY - 18}) rotate(28)"><g class="acc${state === "working" ? " scribble" : ""}"><rect x="-3.5" y="0" width="7" height="30" rx="2" fill="#ECEDEE"/><rect x="-3.5" y="0" width="7" height="6" rx="2" fill="#8FA6FF"/><path d="M-3.5 30 L0 38 L3.5 30 Z" fill="#ECEDEE"/><path d="M-1.2 34.5 L0 38 L1.2 34.5 Z" fill="${CH_INK}"/></g></g>`;
  const t = s.top;
  if (kind === "lens") return `<g class="acc${state === "working" ? " scan" : ""}"><path d="M60 ${t + 2} C60 ${t - 10} 66 ${t - 14} 70 ${t - 18}" fill="none" stroke="${dark}" stroke-width="4" stroke-linecap="round"/><circle cx="72" cy="${t - 21}" r="7.5" fill="#ECEDEE"/><circle cx="72" cy="${t - 21}" r="4.5" fill="#8FA6FF"/><circle cx="70.5" cy="${t - 22.5}" r="1.6" fill="#fff"/></g>`;
  if (kind === "book") return `<g transform="translate(60 ${s.armY + 12})"><g class="acc${state === "working" ? " flip" : ""}"><rect x="-17" y="-11" width="34" height="22" rx="3" fill="#ECEDEE"/><rect x="-1.2" y="-11" width="2.4" height="22" fill="#C9BDFB"/><path d="M-12 -4 H-5 M-12 1 H-5 M5 -4 H12 M5 1 H12" stroke="#8B929B" stroke-width="1.6" stroke-linecap="round"/></g></g>`;
  return "";
}

function chEyes(s, state, gap) {
  const y = s.eye, l = 48 - gap, r = 72 + gap;
  if (state === "asleep") return `<g fill="none" stroke="${CH_INK}" stroke-width="3" stroke-linecap="round"><path d="M${l - 6} ${y} q6 5 12 0"/><path d="M${r - 6} ${y} q6 5 12 0"/></g>`;
  if (state === "incident") return `<g fill="none" stroke="${CH_INK}" stroke-width="2.6" stroke-linecap="round"><path d="M${l - 4} ${y - 5} l8 8 m0 -8 l-8 8"/><path d="M${r - 4} ${y - 5} l8 8 m0 -8 l-8 8"/></g>`;
  const up = state === "needs" ? -2 : 0;
  return `<g class="eyes"><ellipse cx="${l}" cy="${y}" rx="9" ry="10" fill="#fff"/><ellipse cx="${r}" cy="${y}" rx="9" ry="10" fill="#fff"/>`
    + `<g class="pupils${state === "working" ? " look" : ""}"><circle cx="${l + 1}" cy="${y + 1 + up}" r="4.6" fill="${CH_INK}"/><circle cx="${r + 1}" cy="${y + 1 + up}" r="4.6" fill="${CH_INK}"/>`
    + `<circle cx="${l + 2.5}" cy="${y - 1 + up}" r="1.4" fill="#fff"/><circle cx="${r + 2.5}" cy="${y - 1 + up}" r="1.4" fill="#fff"/></g></g>`;
}

function chMouth(s, state) {
  const y = s.eye + 14;
  if (state === "asleep") return `<ellipse cx="60" cy="${y + 1}" rx="3" ry="2.2" fill="${CH_INK}" fill-opacity=".6"/>`;
  if (state === "incident") return `<path d="M53 ${y + 3} q3.5 -4 7 0 q3.5 4 7 0" fill="none" stroke="${CH_INK}" stroke-width="2.4" stroke-linecap="round"/>`;
  if (state === "needs") return `<ellipse cx="60" cy="${y + 1}" rx="4" ry="3.6" fill="${CH_INK}"/>`;
  return `<path d="M54 ${y} q6 5 12 0" fill="none" stroke="${CH_INK}" stroke-width="2.6" stroke-linecap="round"/>`;
}

function chArms(s, dark, state) {
  const y = s.armY, dx = s.armDx;
  const left = `<ellipse cx="${60 - dx}" cy="${y}" rx="6" ry="10" fill="${dark}" transform="rotate(18 ${60 - dx} ${y})"/>`;
  const right = state === "needs"
    ? `<g class="wave" style="transform-origin:${60 + dx - 6}px ${y - 6}px;transform-box:view-box"><ellipse cx="${60 + dx - 3}" cy="${y - 17}" rx="6" ry="12" fill="${dark}" transform="rotate(22 ${60 + dx - 3} ${y - 17})"/></g>`
    : `<ellipse cx="${60 + dx}" cy="${y}" rx="6" ry="10" fill="${dark}" transform="rotate(-18 ${60 + dx} ${y})"/>`;
  return left + right;
}

function chOverlay(s, state) {
  const t = s.top;
  if (state === "needs") return `<g class="bubble"><rect x="86" y="${t - 24}" width="26" height="22" rx="8" fill="#F5B353"/><path d="M92 ${t - 3} l-4 7 l9 -7 Z" fill="#F5B353"/><text x="99" y="${t - 8}" text-anchor="middle" font-family="Inter, sans-serif" font-size="14" font-weight="700" fill="#2A1404">!</text></g>`;
  if (state === "asleep") return '<g class="zzz" font-family="Inter, sans-serif" font-weight="700" fill="#A3A9B1"><text class="z1" x="88" y="34" font-size="12">z</text><text class="z2" x="97" y="22" font-size="15">z</text></g>';
  if (state === "incident") {
    const star = '<path d="M0 -5 L1.5 -1.5 L5 0 L1.5 1.5 L0 5 L-1.5 1.5 L-5 0 L-1.5 -1.5 Z" fill="#F5B353"/>';
    return `<g class="stars" style="transform-origin:60px ${t - 8}px"><g transform="translate(40 ${t - 8})">${star}</g><g transform="translate(80 ${t - 10})">${star}</g><g transform="translate(60 ${t - 16}) scale(.7)">${star}</g></g>`;
  }
  return "";
}

/** An agent as an SVG string: `a` has look (8 hex), department, accessory, state. */
function character(a, size = 72, still = false) {
  const seed = /^[0-9a-f]{8}$/.test(a.look || "") ? a.look : "00000000";
  const shape = ["round", "bean", "block"][parseInt(seed.slice(0, 2), 16) % 3];
  const gap = (parseInt(seed[2], 16) % 3) - 1; // eyes a little closer or wider
  const s = CH_SHAPES[shape];
  const state = CH_STATES[a.state] ? a.state : "idle";
  const dept = CH_DEPTS.includes(a.department) ? a.department : "none";
  const color = `var(--dept-${dept})`, dark = `var(--dept-${dept}-dark)`;
  const [bx, by, brx, bry] = s.belly;
  const behind = a.accessory === "lens";
  const body = '<g class="body">'
    + (behind ? chAccessory(a.accessory, s, state, dark) : "")
    + `<g fill="${dark}"><ellipse cx="47" cy="${s.feet}" rx="9" ry="5"/><ellipse cx="73" cy="${s.feet}" rx="9" ry="5"/></g>`
    + chArms(s, dark, state)
    + `<g fill="${color}">${s.body}</g>`
    + `<ellipse cx="${bx}" cy="${by}" rx="${brx}" ry="${bry}" fill="#fff" fill-opacity=".18"/>`
    + chEyes(s, state, gap) + chMouth(s, state)
    + (behind ? "" : chAccessory(a.accessory, s, state, dark))
    + "</g>";
  const shadow = '<ellipse class="shadow" cx="60" cy="111" rx="30" ry="4" fill="#000" fill-opacity=".35"/>';
  const label = `${a.title || a.name || "Agent"}, ${CH_STATES[state].toLowerCase()}`.replace(/"/g, "");
  return `<svg class="ch st-${state}${still ? " still" : ""}" role="img" aria-label="${label}" width="${size}" height="${size}" viewBox="0 0 120 120" style="overflow: visible">${shadow}${body}${chOverlay(s, state)}</svg>`;
}
