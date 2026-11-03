// Tasks (docs/design/mockups/tasks.html, decision 64): the founder's orders,
// each worked as one agent, grouped by what they wait for. The order box is
// the command bar's, larger.

const tasks = { data: {} };

const TASK_GROUPS = [["waiting", "Waiting for you"], ["review", "To review"], ["running", "Running"], ["done", "Done"]];

function tasksPage() {
  const realm = next.realm;
  const data = loaded(tasks.data, "tasks");
  return h("div", { class: "column tasks" },
    h("header", { class: "tasks-head" },
      h("h1", { class: "headline" }, "Give the order.", h("br", {}), "Approve the result."),
      h("p", { class: "lede" }, "Your agents work on tasks in parallel. Whatever commits the business comes back to you first.")),
    h("form", { class: "order big", "aria-label": "Give an order",
        onsubmit: (e) => { e.preventDefault(); routeOrder($("#task-text").value); } },
      h("label", { for: "task-text", class: "sr-only" }, "What needs doing?"),
      h("textarea", { id: "task-text", rows: 2, placeholder: "What needs doing? Start with an agent's name to choose it." }, orders.text),
      h("div", { class: "order-row" },
        h("button", { type: "submit", class: "primary", disabled: orders.busy ? true : null }, orders.busy ? "Routing…" : "Send")),
      (orders.route || orders.note) && orderCard()),
    !data ? h("p", { class: "lede" }, "Loading…")
      : data.error ? h("p", { class: "note bad" }, data.error)
      : TASK_GROUPS.every(([k]) => !data[k].length)
        ? h("div", { class: "empty" }, h("strong", {}, "No task yet"), h("span", {}, "Give an order above: it becomes a task of the agent whose job it is."))
        : h("section", { class: "task-groups", "aria-label": "Tasks" },
            TASK_GROUPS.filter(([k]) => data[k].length).map(([k, title]) =>
              h("div", { class: "task-group" },
                h("h2", {}, `${title} · ${data[k].length}`),
                data[k].map((t) => taskRow(realm, k, t))))));
}

function taskRow(realm, group, t) {
  const href = group === "waiting" ? pageHash("needs-you") : `/#/r/${enc(realm)}/run/${enc(t.id)}`;
  return h("a", { class: `task ${group}`, href },
    h("span", { class: "task-dot", "aria-hidden": "true" }),
    h("span", { class: "task-order" }, t.order),
    h("span", { class: "task-line" }, group === "waiting" ? "Waits for you in Needs you" : t.line || t.status),
    h("span", { class: "agent-mark", title: t.agent, "aria-label": t.agent, style: `--agent: ${agentColour(t.agent)}` }));
}
