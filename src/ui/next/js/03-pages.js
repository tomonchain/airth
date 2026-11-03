// The pages (decision 72): every page is a component in one of the three
// layouts (13-ui.js). Pages not moved yet go through Legacy as they are.

function renderPanel() {
  const panel = $("#panel");
  panel.classList.toggle("flush", ["needs-you", "routines"].includes(next.route.page) && !next.unauthorized);
  preact.render(html`<${Page} />`, panel);
}

function Page() {
  if (next.unauthorized) {
    return html`<${Reading} header=${{ title: "Open Airth from its link",
      summary: "This page needs Airth's key: open the link airth ui printed on your Mac, or pair your phone again (airth phone)." }} />`;
  }
  const page = next.route.page;
  const realm = next.realm;
  if (page === "business" && realm) return html`<${BusinessPage} />`;
  if (page === "hire" && realm) return html`<${DevPage} title="Hire an agent" part="hire" />`;
  if (page === "new-business") return html`<${DevPage} title="New business" part="new-business" />`;
  if (page === "workshop") return html`<${DevPage} title="Workshop" part="workshop" />`;
  if (page === "settings") return html`<${SettingsPage} />`;
  const legacy = { hq: hqPage, "needs-you": needsPage, team: teamPage, routines: routinesPage, calendar: calendarPage, tasks: tasksPage }[page];
  if (legacy && (page === "hq" || realm)) return html`<${Legacy} node=${legacy()} />`;
  return html`<${DevPage} title="This page" part="page" />`;
}

/** A page whose part is not built yet: its header and what it will do. */
function DevPage({ title, part }) {
  return html`<${Reading} header=${{ label: next.realm || "Airth", title }}>
    <${InDevelopment} part=${part} />
  </${Reading}>`;
}

/** Settings: what has not moved yet, and the classic interface. */
function SettingsPage() {
  return html`<${Reading} header=${{ label: "Airth", title: "Settings" }}>
    <section class="card classic">
      <strong>Classic interface</strong>
      <p>Everything that has not moved here yet: connectors, keys, models, documents and usage.</p>
      <a class="button" href="/classic">Open the classic interface</a>
    </section>
    <${InDevelopment} part="connectors" />
    <${InDevelopment} part="mailboxes" />
  </${Reading}>`;
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

/** InDevelopment for the pages still built by the DOM builder (Routines),
    until they move to components. */
function inDevelopment(part) {
  const [title, ...lines] = IN_DEVELOPMENT[part];
  return h("section", { class: "in-dev", "aria-label": `${title}, in development` },
    h("span", { class: "in-dev-tag" }, "In development"), h("strong", {}, title), lines.map((l) => h("p", {}, l)));
}

/** Today, as the page's headline says it: "Monday 2 November". */
function today() {
  return new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" });
}
