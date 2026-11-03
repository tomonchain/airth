// The pages. Each one is built here as its milestone ships it (decision
// 57); until then a page says where its features are today.

function renderPanel() {
  const panel = $("#panel");
  panel.classList.toggle("flush", ["needs-you", "routines"].includes(next.route.page) && !next.unauthorized);
  if (next.unauthorized) {
    panel.replaceChildren(h("div", { class: "column" },
      h("h1", { class: "headline" }, "Open Airth first"),
      h("p", { class: "lede" }, "This page uses the key of the current interface. Open ", h("a", { href: "/" }, "Airth"), ", then come back here from its menu.")));
    return;
  }
  const page = next.route.page;
  if (page === "hq") return panel.replaceChildren(hqPage());
  if (page === "needs-you" && next.realm) return panel.replaceChildren(needsPage());
  if (page === "team" && next.realm) return panel.replaceChildren(teamPage());
  if (page === "routines" && next.realm) return panel.replaceChildren(routinesPage());
  if (page === "business" && next.realm) return panel.replaceChildren(businessPage());
  if (page === "calendar" && next.realm) return panel.replaceChildren(calendarPage());
  const known = [...PAGES, ...PINNED].find((p) => p.id === page);
  panel.replaceChildren(soonPage(known ? known.title : "This page"));
}

function hqPage() {
  const live = next.realms.filter((r) => !r.archived);
  return h("div", { class: "column hq" },
    h("header", { class: "page-head" },
      h("div", { class: "date" }, today()),
      h("h1", { class: "headline" }, live.length === 1 ? "One business." : `${live.length} businesses.`)),
    live.length
      ? h("div", { class: "card rows" }, live.map((r) =>
          h("a", { class: "row", href: pageHash("needs-you", r.name) },
            h("span", { class: "mark big", "aria-hidden": "true" }, initial(r.name)),
            h("span", { class: "grow" },
              h("span", { class: "name" }, r.name),
              h("span", { class: "sub" }, r.description || "No description yet")))))
      : h("div", { class: "empty" },
          h("strong", {}, "No business yet"),
          h("span", {}, "Create one with New business: its pages will show here.")),
    h("div", { class: "empty" },
      h("strong", {}, "Health, vitals and finance"),
      h("span", {}, "Each business's health, its vitals and its finance will show here, one section per business.")));
}

function soonPage(title) {
  return h("div", { class: "column" },
    h("header", { class: "page-head" },
      h("div", { class: "label" }, next.realm || "Airth"),
      h("h1", { class: "headline" }, title)),
    h("div", { class: "empty" },
      h("strong", {}, `${title} moves here soon`),
      h("span", {}, "Until then, it is in the ", h("a", { href: "/" }, "current interface"), ".")));
}

/** Today, as the page's headline says it: "Monday 2 November". */
function today() {
  return new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" });
}
