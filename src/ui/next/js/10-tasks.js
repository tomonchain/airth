// Tasks (docs/design/mockups/tasks.html, decision 64): the founder's orders,
// each worked as one agent, grouped by what they wait for. The order box is
// the command bar's, larger.

const tasks = { data: {} };

const TASK_GROUPS = [["waiting", "Waiting for you"], ["review", "To review"], ["running", "Running"], ["done", "Done"]];

/** Tasks, in the Reading layout (decision 72). */
function TasksPage() {
  const realm = next.realm;
  const data = loaded(tasks.data, "tasks");
  const submit = (e) => { e.preventDefault(); routeOrder(e.target.querySelector("textarea").value); };
  return html`<${Reading} header=${{ label: realm, title: "Tasks",
      summary: "Give the order, approve the result. Your agents work on tasks in parallel; whatever commits the business comes back to you first." }}>
    <form class="order big" aria-label="Give an order" onSubmit=${submit}>
      <label for="task-text" class="sr-only">What needs doing?</label>
      <textarea id="task-text" rows="2" placeholder="What needs doing? Start with an agent's name to choose it.">${orders.text}</textarea>
      <div class="order-row">
        <button type="submit" class="primary" disabled=${orders.busy}>${orders.busy ? "Routing…" : "Send"}</button>
      </div>
      ${(orders.route || orders.note) && html`<${Legacy} node=${orderCard()} />`}
    </form>
    ${!data ? html`<p class="lede">Loading…</p>`
      : data.error ? html`<p class="note bad">${data.error}</p>`
      : TASK_GROUPS.every(([k]) => !data[k].length)
        ? html`<div class="empty"><strong>No task yet</strong><span>Give an order above: it becomes a task of the agent whose job it is.</span></div>`
        : html`<section class="task-groups" aria-label="Tasks">${TASK_GROUPS.filter(([k]) => data[k].length).map(([k, title]) => html`
            <div class="task-group"><h2>${title} · ${data[k].length}</h2>${data[k].map((t) => html`<${TaskRow} realm=${realm} group=${k} t=${t} />`)}</div>`)}</section>`}
  </${Reading}>`;
}

function TaskRow({ realm, group, t }) {
  const href = group === "waiting" ? pageHash("needs-you") : `/classic#/r/${enc(realm)}/run/${enc(t.id)}`;
  return html`<a class=${`task ${group}`} href=${href}>
    <span class="task-dot" aria-hidden="true"></span>
    <span class="task-order">${t.order}</span>
    <span class="task-line">${group === "waiting" ? "Waits for you in Needs you" : t.line || t.status}</span>
    <span class="agent-mark" title=${t.agent} aria-label=${t.agent} style=${`--agent: ${agentColour(t.agent)}`}></span>
  </a>`;
}
