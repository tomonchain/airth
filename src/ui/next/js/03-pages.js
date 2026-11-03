// The pages. Each one is built here as its milestone ships it (decision
// 57); until then a page says where its features are today.

function renderPanel() {
  const panel = $("#panel");
  panel.classList.toggle("flush", ["needs-you", "routines"].includes(next.route.page) && !next.unauthorized);
  if (next.unauthorized) {
    panel.replaceChildren(h("div", { class: "column" },
      h("h1", { class: "headline" }, "Open Airth from its link"),
      h("p", { class: "lede" }, "This page needs Airth's key: open the link airth ui printed on your Mac, or pair your phone again (airth phone).")));
    return;
  }
  const page = next.route.page;
  if (page === "hq") return panel.replaceChildren(hqPage());
  if (page === "needs-you" && next.realm) return panel.replaceChildren(needsPage());
  if (page === "team" && next.realm) return panel.replaceChildren(teamPage());
  if (page === "routines" && next.realm) return panel.replaceChildren(routinesPage());
  if (page === "business" && next.realm) return panel.replaceChildren(businessPage());
  if (page === "calendar" && next.realm) return panel.replaceChildren(calendarPage());
  if (page === "tasks" && next.realm) return panel.replaceChildren(tasksPage());
  if (page === "hire" && next.realm) return panel.replaceChildren(devPage("Hire an agent", "hire"));
  if (page === "new-business") return panel.replaceChildren(devPage("New business", "new-business"));
  if (page === "workshop") return panel.replaceChildren(devPage("Workshop", "workshop"));
  if (page === "settings") return panel.replaceChildren(settingsPage());
  panel.replaceChildren(devPage("This page", "page"));
}

// What each part not built yet will do: two or three lines, never fake
// data, never a button that does nothing.
const IN_DEVELOPMENT = {
  "hire": ["Hire an agent", "Choose a role and its department, write its job, and see its character before you hire it (decision 65).",
    "Until then, an agent is a file in agents/ in the business folder."],
  "new-business": ["Create a business", "Start a business from a template: its departments, its roles and their routines.",
    "Until then, a business is created in the classic interface, from Settings."],
  "workshop": ["Workshop", "Where Airth itself is developed: its own runs, their checks and changes, and the security scenarios (decision 67).",
    "Until then, they are in the classic interface, and the scenarios run with airth scenarios."],
  "connectors": ["Connectors, keys and models", "The services each business may call, with which credentials, and the models its agents use.",
    "Until then, they are in the classic interface."],
  "mailboxes": ["Mailboxes", "One mailbox per role, on the business's domain: what it received, what it sent with your approval (decision 70)."],
  "routine-edit": ["Editing a routine", "Change a block's parameters here, checked before they are saved, then dry run it (decision 69).",
    "Until then, a routine is edited in its file, routines/<name>.json."],
  "page": ["This page", "It is not part of Airth yet."],
};

/** One part not built yet, as the same panel everywhere. */
function inDevelopment(key) {
  const [title, ...lines] = IN_DEVELOPMENT[key];
  return h("section", { class: "in-dev", "aria-label": `${title}, in development` },
    h("span", { class: "in-dev-tag" }, "In development"),
    h("strong", {}, title),
    lines.map((l) => h("p", {}, l)));
}

function devPage(title, key) {
  return h("div", { class: "column" },
    h("header", { class: "page-head" },
      h("div", { class: "label" }, next.realm || "Airth"),
      h("h1", { class: "headline" }, title)),
    inDevelopment(key));
}

/** Settings: what is built elsewhere for now, and the classic interface. */
function settingsPage() {
  return h("div", { class: "column" },
    h("header", { class: "page-head" }, h("div", { class: "label" }, "Airth"), h("h1", { class: "headline" }, "Settings")),
    h("section", { class: "card classic" },
      h("strong", {}, "Classic interface"),
      h("p", {}, "Everything that has not moved here yet: connectors, keys, models, the Workshop, documents and usage."),
      h("a", { class: "button", href: "/" }, "Open the classic interface")),
    inDevelopment("connectors"),
    inDevelopment("mailboxes"));
}

/** Today, as the page's headline says it: "Monday 2 November". */
function today() {
  return new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" });
}
