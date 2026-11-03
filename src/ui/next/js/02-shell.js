// The shell: header with the business switcher, the icon rail, the panel.

const next = {
  realms: [],
  realm: null, // the business open now
  route: { page: "hq" },
  menuOpen: false,
  unauthorized: !token,
};

// The pages of a business, in the rail's order; Workshop and Settings are
// pinned at the bottom.
const PAGES = [
  { id: "needs-you", title: "Needs you", icon: "needs" },
  { id: "team", title: "Team", icon: "team" },
  { id: "routines", title: "Routines", icon: "routines" },
  { id: "tasks", title: "Tasks", icon: "tasks" },
  { id: "calendar", title: "Calendar", icon: "calendar" },
  { id: "business", title: "Business", icon: "business" },
];
const PINNED = [
  { id: "workshop", title: "Workshop", icon: "workshop" },
  { id: "settings", title: "Settings", icon: "settings" },
];

function parseRoute() {
  const m = location.hash.match(/^#\/b\/([^/]+)\/([a-z-]+)/);
  if (m) return { page: m[2], realm: decodeURIComponent(m[1]) };
  const p = location.hash.match(/^#\/(workshop|settings|new-business)/);
  if (p) return { page: p[1] };
  return { page: "hq" };
}

/** Where a rail entry goes: this interface's page (decision 72). */
const railHref = (page, realm = next.realm) => pageHash(page, realm);

const pageHash = (page, realm = next.realm) =>
  PINNED.some((p) => p.id === page) || ["hq", "new-business"].includes(page) || !realm ? `#/${page}` : `#/b/${enc(realm)}/${page}`;

function renderTop() {
  const top = $("#top");
  const inBusiness = next.route.page !== "hq" && next.realm;
  const page = [...PAGES, ...PINNED].find((p) => p.id === next.route.page);
  const switcher = h("button", { class: "switcher", type: "button", "aria-haspopup": "true", "aria-expanded": next.menuOpen ? "true" : "false",
      onclick: (e) => { e.stopPropagation(); next.menuOpen = !next.menuOpen; renderTop(); } },
    h("span", { class: "mark", "aria-hidden": "true" }, inBusiness ? initial(next.realm) : "A"),
    inBusiness ? next.realm : "Airth HQ",
    h("span", { class: "glyph dim", svg: icon("updown", 14, 2) }));
  const menu = next.menuOpen && h("div", { class: "menu", role: "menu" },
    h("div", { class: "label" }, "Switch to"),
    h("a", { href: "#/hq", role: "menuitem", "aria-current": next.route.page === "hq" ? "page" : null }, h("span", { class: "mark", "aria-hidden": "true" }, "A"), "Airth HQ"),
    next.realms.filter((r) => !r.archived).map((r) =>
      h("a", { href: pageHash("needs-you", r.name), role: "menuitem", "aria-current": inBusiness && r.name === next.realm ? "page" : null },
        h("span", { class: "mark", "aria-hidden": "true" }, initial(r.name)), r.name)));
  top.replaceChildren(
    h("div", { class: "top-left" },
      h("div", { class: "anchor" }, switcher, menu),
      h("span", { class: "where" }, inBusiness ? (page ? page.title : "") : "All businesses")),
    inBusiness && next.route.page !== "tasks" && orderBox(),
    next.route.page === "hq"
      ? h("a", { class: "primary", href: "#/new-business" }, h("span", { class: "glyph", svg: icon("plus", 14, 2.4) }), "New business")
      : null);
}

function renderRail() {
  const link = (p) => {
    const current = next.route.page === p.id;
    const disabled = !next.realm && PAGES.includes(p);
    const waiting = p.id === "needs-you" && needs.realm === next.realm && needs.items.length;
    const el = h("a", { href: disabled ? "#/hq" : railHref(p.id), "aria-label": waiting ? `${p.title}, ${needs.items.length} waiting` : p.title, title: p.title,
        "aria-current": current ? "page" : null, svg: icon(p.icon) });
    if (waiting) el.append(h("span", { class: "dot", "aria-hidden": "true" }));
    return el;
  };
  $("#rail").replaceChildren(
    h("button", { type: "button", class: "rail-button", "aria-label": "Search (⌘K)", title: "Search (⌘K)", svg: icon("search"), onclick: openSearch }),
    h("a", { href: "#/hq", "aria-label": "HQ", title: "HQ", "aria-current": next.route.page === "hq" ? "page" : null, svg: icon("hq") }),
    PAGES.map(link),
    h("div", { class: "gap" }),
    PINNED.map(link),
    themeToggle(),
    // The founder's profile lives in the classic interface until it moves.
    h("a", { href: "/classic#/owner", "aria-label": "Your profile", title: "Your profile", svg: icon("profile") }));
}

/** Light or dark, kept in this browser and shared with the classic
    interface (/theme.js). */
function themeToggle() {
  const theme = window.airthTheme;
  const dark = theme ? theme.current() === "dark" : true;
  const label = dark ? "Switch to light mode" : "Switch to dark mode";
  return h("button", { type: "button", class: "rail-button", "aria-label": label, title: label, svg: icon(dark ? "sun" : "moon"),
    onclick: (e) => { if (theme) theme.set(dark ? "light" : "dark", e); renderRail(); } });
}

function renderAll() {
  renderTop();
  renderRail();
  renderPanel();
}

async function onRoute() {
  next.route = parseRoute();
  next.menuOpen = false;
  if (next.route.realm) next.realm = next.route.realm;
  // Needs you's dot in the rail, for the business open now.
  if (next.realm && needs.realm !== next.realm && !next.unauthorized) loadNeeds();
  try { if (next.realm) localStorage.setItem("airth-next-realm", next.realm); } catch {}
  renderAll();
}
