// ---------- realm settings ----------

const CAP_HELP = {
  read: "Read files and list directories",
  write: "Create and change files",
  execute: "Run shell commands",
  memory: "Save facts to this realm's memory",
  network: "Call integrations and websites",
  skill: "Use skills you installed",
};
const CAP_NAME = {
  read: "Read files", write: "Change files", execute: "Run commands", memory: "Remember things",
  network: "Use the web and integrations", skill: "Use skills",
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
  ["repos", "Repositories", "code"],
  ["memory", "Memory", "notes"],
  ["access", "Access", "plug"],
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
        field("Changes", h("span", { class: "value" }, (d.repos || []).length
          ? `${d.repos.length} ${d.repos.length === 1 ? "repository" : "repositories"}: each chat works on its own copy, and you review what it changed.`
          : "Chats work directly in the workspace."),
          h("a", { href: `#/r/${enc(d.name)}/settings/repos` }, "Repositories"))),
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
    return [
      ...section("Realm", null,
        h("div", { class: "card" },
          h("div", { class: "policy-row" },
            h("div", null, h("div", { class: "strong" }, "Default realm"),
              h("div", { class: "muted small" }, d.default ? "Airth opens this realm when nothing else is chosen." : "The realm Airth opens when nothing else is chosen.")),
            d.default ? h("span", { class: "chip" }, "default") : h("button", { class: "btn", type: "button", onclick: () => save({ default: true }, "Default realm set") }, "Make default")),
          h("div", { class: "policy-row" },
            h("div", null, h("div", { class: "strong" }, d.archived ? "Archived" : "Archive"),
              h("div", { class: "muted small" }, "Hides the realm from the switcher and stops its routines. Its chats, memory and files are kept; you can restore it.")),
            h("button", { class: "btn" + (d.archived ? "" : " danger"), type: "button", onclick: () => save({ archived: !d.archived }, d.archived ? "Restored" : "Archived") }, d.archived ? "Restore" : "Archive")))),
    ];
  };

  const repos = () => repositoriesTab(d);
  const body = { general: () => [...general(), ...advanced()], models, permissions, repos, memory, access }[tab[0]]();
  main.append(h("div", { class: "page" }, h("div", { class: "page-inner settings" },
    h("div", { class: "settings-head" },
      pictureMark("realm-mark large", `${realmPath(d.name)}/image`, d.image, d.name),
      h("div", null, h("h1", null, d.name), h("div", { class: "muted" }, "Realm settings"))),
    h("div", { class: "tabs", role: "tablist" }, SETTINGS_TABS.map(([id, label, ic]) =>
      h("button", { type: "button", role: "tab", "aria-selected": String(id === tab[0]), class: "tab" + (id === tab[0] ? " on" : ""),
        onclick: () => go(`#/r/${enc(d.name)}/settings/${id}`) }, icon(ic), label))),
    ...body)));
}

const repoUi = { adding: false, busy: false, confirm: null };

/** Settings → Repositories: the git repositories this realm's chats work in. */
function repositoriesTab(d) {
  const list = d.repos || [];
  const path = `${realmPath(d.name)}/repos`;
  const apply = (r) => { app.realmDetail = { ...app.realmDetail, repos: r.repos }; renderMain(); };
  const change = async (name, patch, msg) => {
    try { apply(await api(`${path}/${enc(name)}`, patch, "PATCH")); flash(msg); } catch (e) { flash(e.message, true); }
  };
  const anyPr = list.some((r) => r.delivery === "pull_request");
  const card = (r) => {
    const checks = h("textarea", { rows: Math.max(2, r.checks.length), placeholder: "One command per line, e.g. cargo test", spellcheck: "false" });
    checks.value = r.checks.join("\n");
    checks.addEventListener("change", () => change(r.name, { checks: checks.value.split("\n") }, "Checks saved"));
    const del = !r.main && h("button", { class: "c-act " + (repoUi.confirm === r.name ? "sure" : "danger"), type: "button",
      title: "Forget this repository (its files stay)", "aria-label": "Forget this repository",
      onclick: async () => {
        if (repoUi.confirm !== r.name) { repoUi.confirm = r.name; renderMain(); setTimeout(() => { if (repoUi.confirm === r.name) { repoUi.confirm = null; renderMain(); } }, 3000); return; }
        repoUi.confirm = null;
        try { apply(await api(`${path}/${enc(r.name)}`, undefined, "DELETE")); flash("Repository forgotten; its files stay"); } catch (e) { flash(e.message, true); }
      } }, repoUi.confirm === r.name ? "Forget?" : icon("trash"));
    return h("div", { class: "card repo-card" },
      h("div", { class: "repo-head" },
        h("div", { class: "routine-icon", "aria-hidden": "true" }, icon(r.github ? "gitPull" : "code")),
        h("div", { class: "grow" },
          h("div", { class: "repo-title" }, h("span", { class: "strong" }, r.name),
            r.main && h("span", { class: "chip" }, "realm folder"),
            r.branch && h("span", { class: "muted small" }, "on " + r.branch)),
          h("div", { class: "muted small repo-path", title: r.path }, tildePath(r.path),
            r.github && [" · ", h("a", { href: `https://github.com/${r.github}`, target: "_blank", rel: "noopener noreferrer" }, r.github)])),
        del),
      h("div", { class: "repo-row" },
        h("div", { class: "repo-k" }, "Changes"),
        h("div", { class: "seg" },
          [["adopt", "Adopt here"], ["pull_request", "Pull requests"]].map(([v, label]) =>
            h("button", { type: "button", class: r.delivery === v ? "on" : "", disabled: v === "pull_request" && !r.github,
              title: v === "pull_request" && !r.github ? "Only for a repository on GitHub" : "",
              onclick: () => r.delivery !== v && change(r.name, { delivery: v }, v === "adopt" ? "You adopt changes here" : "Changes become pull requests") }, label)))),
      h("div", { class: "repo-row" },
        h("div", { class: "repo-k" }, "Checks"),
        h("div", { class: "grow" }, checks, h("div", { class: "muted small" }, "Run on every change, in the chat's sandbox, before you review it."))));
  };

  // Connect another repository: a URL to clone, or a folder on this Mac.
  const source = h("input", { type: "text", placeholder: "https://github.com/owner/name or ~/Code/project", spellcheck: "false", autocomplete: "off" });
  const name = h("input", { type: "text", placeholder: "name (optional)", spellcheck: "false", autocomplete: "off" });
  const dest = h("input", { type: "text", placeholder: "where to clone it (optional)", spellcheck: "false", autocomplete: "off" });
  const connect = async () => {
    if (!source.value.trim() || repoUi.busy) return;
    repoUi.busy = true; btn.disabled = true; btn.replaceChildren(h("span", { class: "spinner" }), /:\/\/|^git@/.test(source.value) ? "Cloning…" : "Connecting…");
    try {
      apply(await api(path, { source: source.value, name: name.value, path: dest.value }));
      repoUi.adding = false;
      flash("Repository connected");
    } catch (e) { flash(e.message, true); btn.disabled = false; btn.replaceChildren(icon("plus"), "Connect"); }
    repoUi.busy = false;
  };
  const btn = h("button", { class: "btn primary", type: "button", onclick: connect }, icon("plus"), "Connect");
  source.addEventListener("keydown", (e) => { if (e.key === "Enter") connect(); });
  const adder = repoUi.adding
    ? h("div", { class: "card repo-add" },
        h("div", { class: "strong" }, "Connect a repository"),
        h("div", { class: "muted small" }, "A URL is cloned next to the realm's folder (or where you say); a folder is used where it is. Airth detects its checks."),
        source, h("div", { class: "inline" }, name, dest),
        h("div", { class: "row-end" }, h("button", { class: "btn ghost", type: "button", onclick: () => { repoUi.adding = false; renderMain(); } }, "Cancel"), btn))
    : null;
  if (repoUi.adding) setTimeout(() => source.focus(), 30);

  return [
    h("div", { class: "page-head repo-intro" },
      h("p", { class: "muted lead" }, "The git repositories this realm's chats work in. A chat picks one, works on its own copy, and you review what it changed. Memory and routines stay shared across them."),
      !repoUi.adding && h("button", { class: "btn primary", type: "button", onclick: () => { repoUi.adding = true; renderMain(); } }, icon("plus"), "Connect")),
    adder,
    list.length ? h("div", { class: "repos" }, list.map(card))
      : !repoUi.adding && h("div", { class: "routine-empty" }, h("div", { class: "routine-icon big", "aria-hidden": "true" }, icon("code")),
          h("div", { class: "strong" }, "No repositories"), h("p", { class: "muted small" }, "Connect one to have Airth work on code: each chat gets its own copy, checked before you review it.")),
    anyPr && !d.github_token && h("div", { class: "error-banner" }, "Pull requests need a GitHub token: add GITHUB_TOKEN under ",
      h("a", { href: `#/r/${enc(d.name)}/settings/access` }, "Access"), " (a fine-grained token for these repositories, with Contents and Pull requests)."),
  ];
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
const OWNER_TABS = [["profile", "Profile", "notes"], ["appearance", "Appearance", "sun"], ["phone", "Phone", "send"]];

/** A QR code from the server's modules ("1" dark), drawn as a grid: no image, no script. */
function qrCode(qr, label) {
  const grid = h("div", { class: "qr", role: "img", "aria-label": label, style: `grid-template-columns: repeat(${qr.width}, 1fr)` });
  for (const c of qr.cells) grid.append(h("span", { class: c === "1" ? "on" : "" }));
  return h("div", { class: "qr-frame" }, grid);
}

/** The phone (decision 37): Tailscale brings the app to the owner's devices; this pairs one. */
async function phoneTab(inner) {
  let d;
  try { d = await api("phone"); } catch (e) { return flash(e.message, true); }
  if (app.route.view !== "owner") return;
  const again = () => { if (app.route.view === "owner") renderMain(); };
  const host = h("input", { type: "text", placeholder: "mac.tail1234.ts.net", spellcheck: "false", autocomplete: "off" });
  const allow = h("button", { class: "btn primary", type: "button", onclick: async () => {
    try { await api("phone/hosts", { host: host.value }); flash("Name allowed"); again(); } catch (e) { flash(e.message, true); }
  } }, "Allow");
  inner.append(
    h("p", { class: "muted lead" }, "Use Airth from your phone, at home or away. It stays on this Mac: Tailscale brings it to your own devices only, encrypted, and nothing is opened to the internet."),
    h("div", { class: "card pad" },
      h("ol", { class: "welcome-steps" },
        h("li", null, "Install Tailscale on this Mac and on your phone, and sign in to the same account on both."),
        h("li", null, "On this Mac, in a terminal: ", h("code", null, "tailscale serve --bg 7457"), ". It prints the Mac's address, like https://mac.tail1234.ts.net."),
        h("li", null, "Give that name below, then scan the code with your phone's camera.")),
      h("div", { class: "row-start" }, host, allow)),
    d.hosts.map((x) => h("div", { class: "card pad phone-pair" },
      qrCode(x.qr, `Pairing code for ${x.host}`),
      h("div", null,
        h("div", { class: "strong" }, x.host),
        h("p", { class: "muted small" }, "Scan it with the phone's camera, with Tailscale on. It carries Airth's key: whoever scans it controls Airth, so keep it to yourself."),
        h("button", { class: "btn ghost", type: "button", onclick: async () => {
          try { await api(`phone/hosts/${enc(x.host)}`, undefined, "DELETE"); flash(`${x.host} forgotten`); again(); } catch (e) { flash(e.message, true); }
        } }, "Forget this name")))));
}

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
  if (tab[0] === "phone") return phoneTab(inner);
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

// ---------- times ----------

function when(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  const mins = Math.round((d - Date.now()) / 60000);
  if (mins <= 0) return "due now";
  if (mins < 60) return `in ${mins} min`;
  if (mins < 24 * 60) return `in ${Math.round(mins / 60)} h · ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  return d.toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

let confirmDelete = null;

