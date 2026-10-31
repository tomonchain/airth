// ---------- skills ----------

const skillUi = { tab: "connectors" };

const SKILL_TABS = [
  ["connectors", "Connectors", "world", "Connect the services you use (Notion, Linear, GitHub, Stripe…) in one step: paste a token once, and tasks in this business can call the service's official API. The token never leaves for anywhere else, and the agent never sees it."],
  ["catalog", "Catalog", "books", "Ready-made Agent Skills (instructions, scripts, references) from Anthropic's public catalog. Install one and tasks in this business read it when a task matches."],
  ["mcp", "MCP servers", "plug", "Model Context Protocol servers give tasks new tools (GitHub, databases, browsers…). Each runs in this business's sandbox with its credentials, and every call follows the business's skill rule."],
];

async function renderSkills(main) {
  const realm = app.realm;
  const tab = SKILL_TABS.find((t) => t[0] === skillUi.tab) || SKILL_TABS[0];
  page(main, "Connectors & skills", tab[3]);
  const inner = $(".page-inner", main);
  inner.insertBefore(h("div", { class: "tabs", role: "tablist" }, SKILL_TABS.map(([id, label, ic]) =>
    h("button", { type: "button", role: "tab", "aria-selected": String(id === tab[0]), class: "tab" + (id === tab[0] ? " on" : ""),
      onclick: () => { skillUi.tab = id; renderMain(); } }, icon(ic), label))), inner.querySelector(".sub"));
  if (tab[0] === "catalog") return renderCatalogTab(inner, realm);
  if (tab[0] === "mcp") return renderMcpTab(inner, realm);
  return renderConnectorsTab(inner, realm);
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
    grid.replaceChildren(...shown.map(card));
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
      "Connect opens the service's sign-in page; Airth keeps the access in this business, out of reach of commands, and renews it on its own. ",
      "Tasks use a connector's tools under the business's skill rule (they appear as mcp__name__tool)."),
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
    if (busy === c.id) {
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
    const sorted = [...data.connectors].sort((a, b) =>
      (b.connected - a.connected) || (CONNECTOR_ORDER.indexOf(a.connector.category) - CONNECTOR_ORDER.indexOf(b.connector.category)) || a.connector.name.localeCompare(b.connector.name));
    body.replaceChildren(
      h("div", { class: "cat-grid" }, sorted.map(card)),
      h("p", { class: "muted small pad-top" },
        "A token is saved as one of this business's credentials and only ever sent to that service's official API. Tasks call a connector by its name with http_request, under the business's network rule. ",
        "Need something else? Add any API under Business settings → Integrations, or an MCP server."));
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
      "Tasks in this business see each installed skill's name and description in their use_skill tool, and read its instructions when a task matches. " +
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
        h("label", { class: "toggle small", title: s.enabled ? "Tasks can use it" : "Hidden from tasks" },
          h("input", { type: "checkbox", checked: s.enabled, onchange: (e) => call(`${realmPath(realm)}/mcp/${enc(s.name)}`, { enabled: e.target.checked }, "PATCH") }),
          h("span", null, s.enabled ? "on" : "off"))),
      h("code", { class: "path small mcp-cmd" }, "$ " + s.command),
      (s.server_info || s.connected_at) && h("div", { class: "objective-meta" },
        s.server_info && h("span", null, s.server_info), s.connected_at && h("span", null, "connected " + ago(s.connected_at))),
      s.error && h("pre", { class: "out small" }, s.error),
      s.tools.length > 0 && h("details", { class: "mcp-tools" },
        h("summary", { class: "muted small" }, `Tools tasks get (${s.tools.length})`),
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
    list.length ? h("div", { class: "objectives" }, list.map(server)) : h("div", { class: "card muted small pad" }, "No MCP server in this business yet."));
  draw();
  body.replaceChildren(
    listEl,
    h("h2", null, "Add a server"),
    h("p", { class: "muted" },
      "A stdio server: the command that starts it. Airth starts it in this business's sandbox, lists its tools, and stops it after each call. ",
      "Calls follow the business's skill rule (now: ", h("strong", null, POLICY_WORDS[rule] || rule), "). ",
      "If it needs a token, add it under Business settings → Access: the server gets it as an environment variable."),
    h("div", { class: "card" },
      field("Name", f.name, "Lowercase; tools appear to tasks as mcp__name__tool."),
      field("Command", f.command, "e.g. npx -y @modelcontextprotocol/server-filesystem .  ·  uvx mcp-server-git"),
      field("Description", f.desc),
      h("div", { class: "row-end" }, h("button", { class: "btn primary", type: "button", onclick: add }, icon("plug"), "Add and connect"))));
  if (list.some((s) => s.connecting)) setTimeout(() => { if (onTab()) reload(); }, 1500);
}

