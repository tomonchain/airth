// Search (⌘K), from the classic interface (the founder's choice): pages,
// businesses, the open business's tasks and the theme, filtered as you type.
// What it lists comes from what Airth already serves; it acts by moving to
// a page, or switching the theme.

const search = { open: false, query: "", sel: 0, tasks: null, prevFocus: null };

/** Everything search can reach, grouped. */
function searchItems() {
  const items = [];
  const add = (group, label, go, hint) => items.push({ group, label, go, hint });
  const theme = window.airthTheme;
  const dark = theme ? theme.current() === "dark" : true;
  add("Actions", dark ? "Switch to light mode" : "Switch to dark mode", () => { if (theme) theme.set(dark ? "light" : "dark"); renderRail(); });
  add("Go to", "HQ", "#/hq");
  if (next.realm) PAGES.forEach((p) => add("Go to", p.title, pageHash(p.id), next.realm));
  PINNED.forEach((p) => add("Go to", p.title, pageHash(p.id)));
  add("Go to", "Classic interface", "/classic");
  add("Go to", "Your profile", "/classic#/owner");
  for (const group of ["waiting", "review", "running", "done"]) {
    for (const t of (search.tasks && search.tasks[group]) || []) {
      add(`Tasks in ${next.realm}`, t.order, group === "waiting" ? pageHash("needs-you") : `/classic#/r/${enc(next.realm)}/run/${enc(t.id)}`, t.agent);
    }
  }
  for (const r of next.realms || []) {
    if (r.archived || r.name === next.realm) continue;
    add("Switch business", r.name, pageHash(next.route.page === "hq" ? "needs-you" : next.route.page, r.name), r.description);
  }
  return items;
}

/** The items matching every word of the query; without one, all of them. */
function searchMatches() {
  const words = search.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return searchItems().filter((it) => {
    const text = `${it.label} ${it.hint || ""} ${it.group}`.toLowerCase();
    return words.every((w) => text.includes(w));
  }).slice(0, 60);
}

function openSearch() {
  if (search.open || next.unauthorized) return;
  Object.assign(search, { open: true, query: "", sel: 0, prevFocus: document.activeElement });
  // The open business's tasks, fresh each time search opens.
  if (next.realm) {
    api(`realms/${enc(next.realm)}/tasks`).then((d) => { search.tasks = d; renderSearch(); }).catch(() => {});
  }
  renderSearch();
}

function closeSearch() {
  search.open = false;
  renderSearch();
  if (search.prevFocus && search.prevFocus.focus) search.prevFocus.focus();
}

function chooseSearch(it) {
  closeSearch();
  if (typeof it.go === "function") it.go();
  else location.href = it.go;
}

function renderSearch() {
  const old = $("#search");
  if (old) old.remove();
  if (!search.open) return;
  const shown = searchMatches();
  search.sel = Math.max(0, Math.min(search.sel, shown.length - 1));
  const input = h("input", { type: "text", class: "search-input", placeholder: "Search pages, tasks, businesses…", "aria-label": "Search",
    autocomplete: "off", spellcheck: "false", value: search.query,
    oninput: (e) => { search.query = e.target.value; search.sel = 0; renderSearch(); },
    onkeydown: (e) => {
      if (e.key === "ArrowDown") { e.preventDefault(); search.sel = Math.min(search.sel + 1, shown.length - 1); renderSearch(); }
      else if (e.key === "ArrowUp") { e.preventDefault(); search.sel = Math.max(search.sel - 1, 0); renderSearch(); }
      else if (e.key === "Enter" && shown[search.sel]) { e.preventDefault(); chooseSearch(shown[search.sel]); }
      else if (e.key === "Escape") { e.preventDefault(); closeSearch(); }
    } });
  let group = null;
  const rows = [];
  shown.forEach((it, i) => {
    if (it.group !== group) { group = it.group; rows.push(h("div", { class: "search-group" }, group)); }
    rows.push(h("button", { type: "button", class: i === search.sel ? "search-item sel" : "search-item", role: "option",
        "aria-selected": i === search.sel ? "true" : "false", onclick: () => chooseSearch(it) },
      h("span", { class: "search-label" }, it.label), it.hint && h("span", { class: "search-hint" }, it.hint)));
  });
  const overlay = h("div", { id: "search", class: "search-overlay", onmousedown: (e) => { if (e.target === overlay) closeSearch(); } },
    h("div", { class: "search", role: "dialog", "aria-modal": "true", "aria-label": "Search" },
      h("div", { class: "search-bar" }, h("span", { class: "glyph", svg: icon("search", 16) }), input, h("kbd", {}, "esc")),
      h("div", { class: "search-list", role: "listbox" }, rows.length ? rows : h("p", { class: "search-empty" }, "Nothing matches."))));
  document.body.append(overlay);
  input.focus();
  if (input.setSelectionRange) input.setSelectionRange(search.query.length, search.query.length);
}

document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "k") {
    e.preventDefault();
    if (search.open) closeSearch(); else openSearch();
  }
});
