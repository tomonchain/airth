// ---------- what Airth can do ----------

/** What Airth can take on, by area, in words; examples fill the message box. */
const CAN_DO = [
  { caps: ["read", "write"], icon: "folder", title: "Your files",
    text: "Reads, sorts, writes and edits files in this realm's folder, and never outside it.",
    examples: ["Sort the files in this folder by topic", "Summarize every document in this folder"] },
  { caps: ["apps"], icon: "calendar", title: "Your Mac apps",
    text: "Calendar, Reminders, Notes, Mail (it reads and prepares drafts; it never sends), Safari tabs and the clipboard.",
    examples: ["What's on my calendar tomorrow?", "Summarize today's unread emails", "Add a reminder to call the bank on Friday"] },
  { caps: ["network"], icon: "world", title: "The web and your services",
    text: "Looks things up on public websites, and calls the services you connect (Notion, Linear, Stripe…) without ever seeing their keys.",
    examples: ["Compare the prices of three competitors", "Find the best-rated plumbers near me"], services: true },
  { caps: ["execute"], icon: "terminal", title: "Commands and code",
    text: "Runs commands in a sandbox: scripts, file conversions, tests, code changes you review before they count.",
    examples: ["Convert the images in this folder to WebP", "Run the tests and fix what fails"] },
  { caps: ["memory"], icon: "brain", title: "Memory and goals",
    text: "Remembers what matters in this realm.",
    examples: ["Remember that I prefer meetings in the morning", "What do you remember about this realm?"] },
  { caps: [], icon: "clock", title: "Work on its own",
    text: "Routines start work for you: on a schedule, when files change, or from a webhook. A brief every morning, a weekly review…",
    link: ["Set up a routine", (r) => `#/r/${enc(r)}/routines`] },
  { caps: ["skill"], icon: "spark", title: "New skills",
    text: "When it hits a limit, it can build and test a small tool; nothing is installed until you say so. Or add ready-made skills.",
    link: ["Browse skills", (r) => `#/r/${enc(r)}/skills`] },
];
const CAN_DO_RULE = { allow: ["Automatic", "rule-allow"], ask: ["Asks you first", "rule-ask"], deny: ["Off", "rule-deny"] };

let canDo = null;

/** The presentation of what Airth can do in the current realm. */
async function openCanDo(onExample) {
  if (canDo) return;
  const d = app.realmDetail;
  const realm = app.realm;
  if (!d || !realm) return;
  // The strictest rule of an area says what to expect.
  const ruleOf = (caps) => {
    const rules = caps.map((c) => d.policy[c]).filter(Boolean);
    if (!rules.length) return null;
    return rules.includes("deny") ? (rules.every((r) => r === "deny") ? "deny" : "ask") : rules.includes("ask") ? "ask" : "allow";
  };
  const close = () => {
    if (!canDo) return;
    const el = canDo;
    canDo = null;
    el.classList.add("closing");
    setTimeout(() => el.remove(), 140);
    document.removeEventListener("keydown", onKey);
  };
  const onKey = (e) => { if (e.key === "Escape") close(); };
  const services = h("div", { class: "cando-services muted small" });
  const cards = CAN_DO.map((a) => {
    const rule = ruleOf(a.caps);
    const [word, cls] = rule ? CAN_DO_RULE[rule] : [];
    return h("div", { class: "cando-card" + (rule === "deny" ? " off" : "") },
      h("div", { class: "cando-top" },
        h("span", { class: "welcome-icon" }, icon(a.icon)),
        h("div", { class: "grow strong" }, a.title),
        rule && h("a", { class: "chip " + cls, href: `#/r/${enc(realm)}/settings/permissions`, title: "Change in Permissions", onclick: close }, word)),
      h("p", { class: "muted small" }, a.text),
      a.services && services,
      a.examples && h("div", { class: "cando-examples" }, a.examples.map((x) =>
        h("button", { class: "suggestion", type: "button", disabled: rule === "deny", onclick: () => { close(); onExample && onExample(x); } }, x))),
      a.link && h("a", { class: "cando-more", href: a.link[1](realm), onclick: close }, a.link[0], " ›"));
  });
  const tools = app.info && h("details", { class: "cando-tools" },
    h("summary", null, "Technical details: every tool"),
    h("div", null, app.info.tools.map((t) => {
      const rule = d.policy[t.capability];
      return h("div", { class: "tool-row" },
        h("code", null, t.name),
        rule ? h("span", { class: "chip rule-" + rule }, POLICY_WORDS[rule]) : h("span"),
        h("span", { class: "d" }, t.description.split(". ")[0] + "."));
    })));
  canDo = h("div", { class: "pal-overlay cando-overlay", onmousedown: (e) => { if (e.target === canDo) close(); } },
    h("div", { class: "pal cando", role: "dialog", "aria-modal": "true", "aria-label": "What Airth can do" },
      h("div", { class: "cando-head" },
        h("div", { class: "grow" },
          h("div", { class: "eyebrow" }, realm),
          h("h2", null, "What can Airth do for you?"),
          h("p", { class: "muted" }, "Describe an outcome in plain words. Airth makes a plan, uses the tools below, shows you each step, and asks before anything this realm does not allow on its own.")),
        h("button", { class: "icon-btn", type: "button", "aria-label": "Close", title: "Close (Esc)", onclick: close }, "×")),
      h("div", { class: "cando-body" },
        h("div", { class: "cando-grid" }, cards),
        h("div", { class: "cando-promise" },
          h("div", { class: "strong" }, "Always"),
          h("ul", null,
            h("li", null, "It works only in this realm: other realms' files, memory and keys stay out of reach."),
            h("li", null, "It never sees your keys and passwords; the runtime adds them for it."),
            h("li", null, "You can stop Airth at any time, and review a change before it counts."))),
        tools)));
  document.body.append(canDo);
  document.addEventListener("keydown", onKey);
  // Connected services, for the web card.
  try {
    const c = await api(`${realmPath(realm)}/connectors`);
    const on = (c.connectors || []).filter((x) => x.connected).map((x) => x.connector.name);
    services.replaceChildren(on.length
      ? h("span", null, "Connected here: ", h("b", null, on.join(", ")))
      : h("span", null, "No service connected yet. ", h("a", { href: `#/r/${enc(realm)}/skills`, onclick: () => { skillUi.tab = "connectors"; close(); } }, "Connect one"), "."));
  } catch { services.remove(); }
}

