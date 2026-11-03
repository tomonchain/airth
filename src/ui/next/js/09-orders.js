// The command bar (decision 64), in the header of every business page: an
// order goes to the agent named at its start, else the one routing chooses,
// shown with why before it starts; the founder can change it. It runs
// unwatched: what commits or needs a permission waits in Needs you.

const orders = { text: "", route: null, busy: false, note: null };

function orderBox() {
  return h("form", { class: "order", role: "search", "aria-label": "Give an order",
      onsubmit: (e) => { e.preventDefault(); routeOrder($("#order-text").value); } },
    h("label", { for: "order-text", class: "sr-only" }, "Give an order"),
    h("input", { id: "order-text", type: "text", autocomplete: "off", value: orders.text,
      placeholder: "Give an order, or start with an agent's name: “writer: …”" }),
    h("button", { type: "submit", class: "primary", disabled: orders.busy ? true : null }, orders.busy ? "Routing…" : "Send"),
    (orders.route || orders.note) && orderCard());
}

function orderCard() {
  const close = () => { orders.route = orders.note = null; renderTop(); };
  if (orders.note) return h("div", { class: "order-card", role: "status" }, h("p", {}, orders.note),
    h("div", { class: "order-row" }, h("a", { href: pageHash("tasks") }, "See Tasks"), h("button", { type: "button", class: "plain", onclick: close }, "Close")));
  const r = orders.route;
  const agents = r.agents || [];
  return h("div", { class: "order-card", role: "dialog", "aria-label": "Who takes the order" },
    r.error ? h("p", { class: "note bad" }, r.error) : h("p", {}, h("strong", {}, r.agent), ` · ${r.why}`),
    agents.length ? h("div", { class: "order-row" },
      h("label", { for: "order-agent", class: "sr-only" }, "Agent"),
      h("select", { id: "order-agent" }, agents.map((a) => h("option", { value: a, selected: a === r.agent ? true : null }, a))),
      h("button", { type: "button", class: "primary", onclick: () => startOrder($("#order-agent").value) }, "Start"),
      h("button", { type: "button", class: "plain", onclick: close }, "Cancel"))
      : h("p", { class: "note" }, "This business has no agent yet: hire one first."),
    h("p", { class: "note" }, "It runs unwatched: what commits the business, or needs your permission, waits in Needs you."));
}

async function routeOrder(text) {
  if (!text.trim()) return;
  Object.assign(orders, { text, busy: true, route: null, note: null });
  renderTop();
  try {
    orders.route = await api(`realms/${enc(next.realm)}/orders/route`, { text });
  } catch (e) {
    orders.route = { error: e.message, agents: [] };
  }
  orders.busy = false;
  renderTop();
}

async function startOrder(agent) {
  const text = (orders.route && orders.route.order) || orders.text;
  try {
    await api(`realms/${enc(next.realm)}/orders`, { text, agent });
    Object.assign(orders, { text: "", route: null, note: `Started: ${agent} works on “${text}”. It shows in Tasks; what needs you comes to Needs you.` });
  } catch (e) {
    orders.route = { ...orders.route, error: e.message };
  }
  renderTop();
}
