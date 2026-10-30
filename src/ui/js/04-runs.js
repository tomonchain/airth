// ---------- new run ----------

const SUGGESTIONS = {
  code: ["Identify and implement one small improvement", "Find and fix a bug", "Add tests for an untested module", "Review error messages and make them clearer"],
  other: ["Organize the files in this realm", "Summarize what's in this realm", "Draft a plan for this week", "Write a checklist for an upcoming task"],
};

const POLICY_WORDS = { allow: "automatic", ask: "needs approval", deny: "disabled" };

function renderNew(main) {
  const d = app.realmDetail;
  const realm = app.realms.find((r) => r.name === app.realm);
  const modelError = d && d.model_error;
  const c = composer({
    placeholder: `Describe an objective for ${app.realm}…`,
    draftKey: `new:${app.realm}`,
    model: {
      get: () => storedModel(`new:${app.realm}`) || "auto",
      set: (v) => storeModel(`new:${app.realm}`, v),
    },
    repo: {},
    buttonLabel: "Start",
    rows: 3,
    // Without a realm model you can still pick one in the model menu.
    disabled: !!(realm && realm.archived),
    note: d && (d.isolation === "worktree" || (d.repos || []).length)
      ? "In a repository, Airth works in its own copy. You review the change before it reaches your checkout."
      : "Every step that needs approval waits for you.",
    onSubmit: async (objective) => {
      const model = storedModel(`new:${app.realm}`) || "auto";
      const { id } = await api(`${realmPath(app.realm)}/runs`, { objective, model, repo: chosenRepo() || null });
      storeModel(`run:${app.realm}/${id}`, model);
      await refreshRuns();
      go(`#/r/${enc(app.realm)}/run/${enc(id)}`);
    },
  });
  if (modelError) {
    c.errorEl.replaceChildren(
      h("span", { title: modelError }, "Airth has no model yet, so it cannot start a chat. "),
      h("a", { href: "#/welcome", onclick: () => { welcomeUi.step = WELCOME_STEPS.indexOf("model"); welcomeUi.setup = null; } }, "Connect Claude"),
      " or ",
      h("a", { href: `#/r/${enc(app.realm)}/settings/models` }, "choose another provider"), ".");
    c.errorEl.hidden = false;
  }
  const isCode = d && d.own_root && d.checks.length > 0;
  const suggestions = isCode ? SUGGESTIONS.code : SUGGESTIONS.other;
  const fill = (text) => { c.ta.value = text; c.ta.dispatchEvent(new Event("input")); c.ta.focus(); };
  const prefill = app.prefill;
  app.prefill = null;
  const tools = d
    ? h("button", { class: "cando-link", type: "button", onclick: () => openCanDo(fill) }, icon("compass"), "What can Airth do for you?")
    : null;
  const hour = new Date().getHours();
  const hello = hour < 5 ? "Good night" : hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
  const first = (app.ownerName || "").trim().split(/\s+/)[0];
  main.append(
    h("div", { class: "new-view" },
      h("div", { class: "new-inner" },
        h("div", { class: "eyebrow" }, app.realm || ""),
        h("h1", null, first ? `${hello}, ${first}` : "What should Airth work on?"),
        h("p", { class: "sub", title: d ? d.root : "" }, first ? "What should Airth work on?" : d ? d.root : ""),
        tools,
        c.el,
        h("div", { class: "suggestions" },
          suggestions.map((s) => h("button", { class: "suggestion", type: "button", onclick: () => { c.ta.value = s; c.ta.dispatchEvent(new Event("input")); c.ta.focus(); } }, s))))));
  if (prefill) fill(prefill);
  c.ta.focus();
}

// ---------- run view ----------

let runEls = null;

function renderRunShell(main) {
  const header = h("header", { class: "run-header" });
  const transcript = h("div", { class: "transcript" }, working("Loading…"));
  const scroller = h("div", { class: "scroller" }, transcript);
  const c = composer({
    placeholder: "Send a follow-up…",
    draftKey: `run:${app.realm}/${app.route.id}`,
    model: {
      // What you picked for this run, else what it was started with.
      get: () => storedModel(`run:${app.realm}/${app.route.id}`) || (app.run && app.run.state.model_choice) || "auto",
      set: (v) => storeModel(`run:${app.realm}/${app.route.id}`, v),
    },
    buttonLabel: "Send",
    disabled: true,
    onSubmit: (message) => sendFollowUp(message, storedModel(`run:${app.realm}/${app.route.id}`) || (app.run && app.run.state.model_choice) || "auto"),
  });
  const toBottom = h("button", { class: "to-bottom", type: "button", title: "Scroll to the latest", "aria-label": "Scroll to the latest",
    onclick: () => scroller.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" }) }, icon("down"));
  const wrap = h("div", { class: "composer-wrap" }, toBottom, c.el);
  scroller.addEventListener("scroll", () => syncToBottom(), { passive: true });
  const side = h("aside", { class: "run-side", "aria-label": "Plan and details" });
  const layout = h("div", { class: "run-layout" + (sideOpen() ? "" : " side-closed") },
    h("div", { class: "run-center" }, header, scroller, wrap), sideResizer(), side);
  const width = sideWidth();
  if (width) layout.style.setProperty("--side-w", width + "px");
  main.append(layout);
  runEls = { header, transcript, scroller, composer: c, wrap, side, layout, toBottom, seen: null, steps: null };
}

/** The right column's width the owner chose by dragging its edge (none: the default). */
function sideWidth() {
  try { return Number(localStorage.getItem("airth.sideWidth")) || 0; } catch { return 0; }
}

/** The right column's left edge: drag it to make the column wider or narrower. */
function sideResizer() {
  const el = h("div", { class: "side-resize", role: "separator", "aria-orientation": "vertical", title: "Drag to resize, double-click to reset" });
  el.addEventListener("pointerdown", (e) => {
    const layout = el.parentElement;
    el.setPointerCapture(e.pointerId);
    layout.classList.add("resizing");
    const right = layout.getBoundingClientRect().right;
    const move = (ev) => {
      const w = Math.round(Math.min(640, Math.max(260, right - ev.clientX)));
      layout.style.setProperty("--side-w", w + "px");
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      layout.classList.remove("resizing");
      const w = parseInt(layout.style.getPropertyValue("--side-w"), 10);
      try { if (w) localStorage.setItem("airth.sideWidth", String(w)); } catch { /* not kept */ }
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up, { once: true });
  });
  el.addEventListener("dblclick", () => {
    el.parentElement.style.removeProperty("--side-w");
    try { localStorage.removeItem("airth.sideWidth"); } catch { /* nothing kept */ }
  });
  return el;
}

/** Show the "↓" button when the latest messages are out of sight. */
function syncToBottom() {
  if (!runEls) return;
  const sc = runEls.scroller;
  runEls.toBottom.classList.toggle("show", sc.scrollHeight - sc.scrollTop - sc.clientHeight > 240);
}

/** Whether the right column is shown: the owner's last choice, else open on wide windows. */
function sideOpen() {
  let saved = null;
  try { saved = localStorage.getItem("airth-side"); } catch {}
  return saved === null ? window.innerWidth >= 1100 : saved === "1";
}

function toggleSide() {
  const open = runEls.layout.classList.toggle("side-closed") === false;
  try { localStorage.setItem("airth-side", open ? "1" : "0"); } catch {}
  renderRun();
}

/** The "Airth is working" line: a breathing orb and shimmering text. */
function working(label) {
  return h("div", { class: "working" }, h("span", { class: "orb", "aria-hidden": "true" }), h("span", { class: "shimmer" }, label));
}

/** Animate only what is new since the last render: the transcript is rebuilt on every poll. */
function animateNew(nodes) {
  const first = runEls.seen === null;
  const seen = first ? new Set() : runEls.seen;
  for (const top of nodes) {
    for (const n of [top, ...top.querySelectorAll("[data-k]")]) {
      const k = n.dataset && n.dataset.k;
      if (!k) continue;
      if (!first && !seen.has(k)) n.classList.add("enter");
      seen.add(k);
    }
  }
  runEls.seen = seen;
}

const keyed = (node, k) => { node.dataset.k = k; return node; };

async function sendFollowUp(message, model) {
  await api(`${runPath(app.realm, app.route.id)}/messages`, model ? { message, model } : { message });
  app.version = null;
  app.diff = null;
  await Promise.all([pollRun(), refreshRuns()]);
}

function liveStatus(d) {
  if (!d.live) return d.interrupted ? "interrupted" : d.state.status;
  if (d.stopping) return "stopping";
  if (d.pending) return "awaiting";
  if (d.state.change && d.state.change.checks && !d.state.change.checks.finished) return "checking";
  return "running";
}

const STATUS_LABEL = { awaiting: "awaiting approval", stopping: "stopping…", checking: "running checks" };

function renderRun() {
  const d = app.run;
  if (!d || !runEls) return;
  const s = d.state;
  const status = liveStatus(d);

  const canStop = d.live && !d.elsewhere && !d.stopping;
  const open = !runEls.layout.classList.contains("side-closed");
  const hasPlan = s.plan && s.plan.length > 0;
  const closedSteps = hasPlan ? s.plan.filter((p) => p.status === "done" || p.status === "skipped").length : 0;
  const statusClass = status === "stopping" ? "stopped" : status === "checking" ? "running" : status;
  const ctx = [
    h("span", { class: "rh-state " + statusClass }, STATUS_LABEL[status] || status),
    h("span", null, "updated " + ago(s.updated_at)),
    d.repo && ((app.realmDetail && app.realmDetail.repos) || []).length > 1 && h("span", { class: "rh-tag", title: "The repository this chat works in" }, icon("code"), d.repo),
    s.origin && h("span", { class: "rh-tag", title: s.origin.trigger }, icon("clock"), s.origin.objective),
    s.skill && h("a", { class: "rh-tag", href: `#/r/${enc(app.realm)}/skills` }, icon("spark"), "developing " + s.skill),
  ].filter(Boolean);
  if (!app.renaming) runEls.header.replaceChildren(
    h("span", { class: "rh-dot dot " + (d.pending ? "awaiting" : statusClass), title: STATUS_LABEL[status] || status }),
    h("div", { class: "run-title" },
      h("h1", { class: "renamable", title: `${s.objective}\n\nClick to rename`, tabindex: "0",
        onclick: (e) => editInPlace(e.currentTarget, s.id, runName({ title: d.title, objective: s.objective })),
        onkeydown: (e) => { if (e.key === "Enter") editInPlace(e.currentTarget, s.id, runName({ title: d.title, objective: s.objective })); } },
        runName({ title: d.title, objective: s.objective })),
      h("div", { class: "run-meta rh-ctx" }, ctx.flatMap((n, i) => (i ? [h("span", { class: "rh-sep", "aria-hidden": "true" }, "·"), n] : [n])))),
    h("div", { class: "rh-actions" },
      canStop && h("button", { class: "btn rh-stop", type: "button", onclick: stopRun, title: "Stop after the current step" }, icon("stop"), "Stop"),
      // A named toggle (not another panel icon): "Plan 2/5", or "Details" without a plan.
      h("button", { class: "btn ghost side-toggle" + (open ? " on" : ""), type: "button", onclick: toggleSide,
        title: open ? "Hide plan & details" : "Show plan & details", "aria-pressed": String(open) },
        icon(hasPlan ? "listCheck" : "panel"), hasPlan ? "Plan" : "Details",
        hasPlan && h("span", { class: "progress-mini" }, `${closedSteps}/${s.plan.length}`)),
      copyButton(() => s.objective, "Copy the objective")));
  runEls.side.replaceChildren(...sideNodes(d, status));

  const sc = runEls.scroller;
  const nearBottom = sc.scrollHeight - sc.scrollTop - sc.clientHeight < 80;
  const focusedId = document.activeElement && document.activeElement.id;
  const nodes = transcriptNodes(d);
  animateNew(nodes);
  runEls.transcript.replaceChildren(...nodes);
  if (focusedId === "deny-reason") $("#deny-reason")?.focus();
  if (nearBottom || app.version === null) sc.scrollTop = sc.scrollHeight;
  syncToBottom();
  updateTitle();
  renderStatus();

  const c = runEls.composer;
  const closed = s.change && s.change.status !== "open";
  // A missing realm model only blocks when nothing else was picked for the run.
  const picked = storedModel(`run:${app.realm}/${app.route.id}`) || s.model_choice;
  const noModel = !!(app.realmDetail && app.realmDetail.model_error) && (!picked || picked === "auto");
  const disabled = d.live || closed || noModel;
  runEls.wrap.classList.toggle("live", !!d.live && !d.pending);
  // While the run works you can already write your next message; it sends once it stops.
  c.ta.disabled = closed;
  c.block(disabled);
  if (c.picker) c.picker.refresh();
  c.ta.placeholder = d.live
    ? (d.elsewhere ? "This chat is in progress in another Airth process…" : "Airth is working… write your next message, it can be sent when it stops.")
    : closed ? `This chat's change was ${s.change.status}. Start a new chat to keep going.`
    : status === "interrupted" ? "This chat was interrupted. Send a message to resume it…" : "Send a follow-up…";
}

function transcriptNodes(d) {
  const s = d.state;
  const events = new Map();
  for (const e of s.events) if (e.tool_use_id) events.set(e.tool_use_id, e);
  const results = new Map();
  for (const m of s.messages)
    if (m.role === "user") for (const b of m.content) if (b.type === "tool_result") results.set(b.tool_use_id, b);

  // The call waiting for approval is the first unanswered call in the last turn.
  let awaitingId = null;
  if (d.pending) {
    const last = [...s.messages].reverse().find((m) => m.role === "assistant");
    const call = last && last.content.find((b) => b.type === "tool_use" && !events.has(b.id) && !results.has(b.id));
    awaitingId = call && call.id;
  }

  const nodes = [];
  // Consecutive thinking and tool calls fold into one activity line.
  let group = [];
  const flush = (last) => {
    if (group.length === 1) nodes.push(group[0].node);
    else if (group.length > 1) nodes.push(activityGroup(group, last && d.live));
    group = [];
  };
  const lastAt = s.messages.length - 1;
  const formAt = !d.live && lastAt >= 0 && s.messages[lastAt].role === "assistant"
    && numberedQuestions(s.messages[lastAt].content.filter((b) => b.type === "text").map((b) => b.text).join("\n")).length ? lastAt : -1;
  s.messages.forEach((m, mi) => {
    if (s.compaction && mi === s.compaction.upto) {
      flush(false);
      nodes.push(keyed(keyedDetails(`compact:${mi}`, { class: "thinking" },
        h("summary", null, "Earlier conversation summarized to save tokens"),
        h("div", { class: "prose", html: mdc(s.compaction.summary.replace(RUNTIME_PREFIX, "").trim()) })), `compact:${mi}`));
    }
    if (m.role === "user") {
      const texts = m.content.filter((b) => b.type === "text");
      if (texts.length) flush(false);
      texts.forEach((b, ti) => {
        if (b.text.startsWith(RUNTIME_PREFIX)) {
          const body = b.text.slice(RUNTIME_PREFIX.length).trim();
          // Long runtime texts are instructions for the model: folded, behind a plain-language line.
          nodes.push(keyed(body.length > 400
            ? keyedDetails(`rt:${mi}:${ti}`, { class: "runtime-brief" },
                h("summary", null, h("span", { class: "ns-star", "aria-hidden": "true" }, "★"), " ", runtimeLabel(body), h("span", { class: "muted" }, " · how Airth was asked to work")),
                h("div", { class: "prose", html: mdc(body) }))
            : h("div", { class: "msg runtime" }, h("div", { class: "who" }, "Airth"), h("div", { class: "prose", html: mdc(body) })), `rt:${mi}:${ti}`));
        } else {
          nodes.push(keyed(h("div", { class: "msg user" + (mi === 0 ? " objective" : "") },
            h("div", { class: "who" }, mi === 0 ? "Objective" : "You"),
            h("div", { class: "prose", html: mdc(b.text) })), `u:${mi}:${ti}`));
        }
      });
      return;
    }
    m.content.forEach((b, bi) => {
      if (b.type === "reasoning" && b.summary && b.summary.trim()) {
        group.push({ kind: "think", node: keyed(keyedDetails(`think:${mi}:${bi}`, { class: "thinking" },
          h("summary", null, "Thinking"),
          h("div", { class: "prose", html: mdc(b.summary) })), `think:${mi}:${bi}`) });
      } else if (b.type === "text" && b.text.trim()) {
        flush(false);
        // The questions of the latest answer are shown once, in the form below it.
        const text = mi === formAt ? withoutQuestions(b.text) : b.text;
        if (text.trim()) nodes.push(keyed(assistantMessage(text), `t:${mi}:${bi}`));
      } else if (b.type === "tool_use") {
        const ev = events.get(b.id), res = results.get(b.id);
        group.push({ kind: "tool", name: b.name, input: b.input || {},
          failed: (ev && (ev.status === "error" || ev.status === "denied")) || (!ev && res && res.is_error),
          node: keyed(toolCard(b, ev, res, d.live, b.id === awaitingId), "tool:" + b.id) });
      }
    });
  });
  flush(true);
  const answers = nodes.filter((n) => n.classList.contains("assistant"));
  if (answers.length) answers[answers.length - 1].classList.add("latest");

  const lastMsg0 = s.messages[s.messages.length - 1];
  if (!d.live && lastMsg0 && lastMsg0.role === "assistant") {
    const text = lastMsg0.content.filter((b) => b.type === "text").map((b) => b.text).join("\n");
    const questions = numberedQuestions(text);
    const key = `answers:${s.id}:${s.messages.length - 1}`;
    if (questions.length) nodes.push(keyed(answerForm(questions, key, speaksFrench(s)), key));
  }
  const checking = s.change && s.change.checks && !s.change.checks.finished && d.live;
  if (d.pending) nodes.push(keyed(approvalCard(d.pending), "approval:" + d.pending.id));
  else if (d.live && !checking) {
    const lastMsg = s.messages[s.messages.length - 1];
    let label = "Thinking…";
    if (d.stopping) label = "Stopping after the current step…";
    else if (lastMsg && lastMsg.role === "assistant") {
      const running = [...lastMsg.content].reverse().find((b) => b.type === "tool_use" && !events.has(b.id) && !results.has(b.id));
      label = running ? activeLabel(running.name, running.input || {}) : "Running…";
    }
    nodes.push(keyed(working(label), "working"));
  } else if (!d.live) {
    const status = d.interrupted ? "interrupted" : s.status;
    const last = s.messages[s.messages.length - 1];
    const asks = last && last.role === "assistant" && numberedQuestions(last.content.filter((b) => b.type === "text").map((b) => b.text).join("\n")).length > 0;
    const text = {
      completed: asks ? "Your turn: Airth is waiting for your answer." : `Completed after ${s.iterations} iteration${s.iterations === 1 ? "" : "s"}.`,
      failed: "The chat failed.",
      stopped: "The chat stopped.",
      interrupted: "The chat was interrupted: the process exited before it finished.",
    }[status];
    const showReport = status !== "completed" && s.report;
    const closed = s.change && s.change.status !== "open";
    const canResume = status !== "completed" && !closed && !d.elsewhere;
    nodes.push(keyed(h("div", { class: "end " + status },
      h("div", { class: "grow" }, text, showReport && h("pre", null, s.report),
        canResume && h("div", { class: "muted small" }, "Resume to let Airth check where it stopped and carry on with its plan, or send a message to redirect it.")),
      canResume && h("button", { class: "btn primary", type: "button", disabled: !!(app.realmDetail && app.realmDetail.model_error), onclick: resumeRun }, "Resume")), "end:" + status));
  }
  return nodes;
}

const RUNTIME_PREFIX = "[Airth runtime]";

/** What a long runtime instruction is about, for the owner. */
function runtimeLabel(body) {
  if (body.includes("conversation grew long")) return "Earlier conversation summarized";
  return "Instructions Airth received";
}

/** The numbered questions at the end of an answer ("1. Where do you stand? ..."), if there are at least two. */
function numberedQuestions(text) {
  const items = [];
  for (const line of text.split("\n")) {
    const m = line.match(/^\s*(\d+)[.)]\s+(.+)$/);
    if (m && m[2].includes("?")) items.push(m[2].replace(/\*\*|__|`/g, "").trim());
  }
  return items.length >= 2 ? items : [];
}

/** Whether the conversation is in French (the app's own words are English, but what it sends for the owner follows their language). */
function speaksFrench(s) {
  // What the owner and Airth wrote, not Airth's own (English) runtime notes.
  const text = s.messages.slice(-4).flatMap((m) => m.content).filter((b) => b.type === "text" && !b.text.startsWith(RUNTIME_PREFIX)).map((b) => b.text).join(" ").toLowerCase();
  const hits = (text.match(/\b(je|tu|vous|est|les|des|une|pour|avec|que|qui|pas|mon|ton|c'est|qu'est-ce)\b/g) || []).length;
  const en = (text.match(/\b(the|you|is|are|and|for|with|what|your|this|that)\b/g) || []).length;
  return hits > en;
}

/** The text without its numbered questions (they move to the answer form). */
function withoutQuestions(text) {
  return text.split("\n").filter((line) => !(/^\s*\d+[.)]\s+/.test(line) && line.includes("?"))).join("\n").replace(/\n{3,}/g, "\n\n");
}

/** Answers typed in the question form, kept across redraws (the transcript is rebuilt on every poll). */
const answerDrafts = new Map();

/** One field per question, sent as a single follow-up. */
function answerForm(questions, key, fr) {
  const fields = questions.map((q, i) => {
    const ta = h("textarea", { rows: 1, placeholder: "Your answer (or leave empty)", "aria-label": q });
    ta.value = answerDrafts.get(`${key}:${i}`) || "";
    const fit = () => { ta.style.height = "auto"; ta.style.height = ta.scrollHeight + "px"; };
    ta.addEventListener("input", () => { answerDrafts.set(`${key}:${i}`, ta.value); fit(); });
    setTimeout(fit, 0);
    return { q, ta };
  });
  const send = async () => {
    const lines = fields.map((f, i) => `${i + 1}. ${f.q}\n→ ${f.ta.value.trim() || (fr ? "(pas de réponse)" : "(no answer)")}`);
    if (fields.every((f) => !f.ta.value.trim())) return fields[0].ta.focus();
    btn.disabled = true;
    try {
      await sendFollowUp(lines.join("\n\n"));
      fields.forEach((_, i) => answerDrafts.delete(`${key}:${i}`));
    } catch (e) { flash(e.message, true); btn.disabled = false; }
  };
  const btn = h("button", { class: "btn primary", type: "button", onclick: send }, "Send my answers");
  return h("div", { class: "answer-form" },
    h("div", { class: "answer-head" }, "Answer here, or reply in your own words below."),
    fields.map((f, i) => h("label", { class: "answer-q" }, h("span", { class: "answer-n" }, i + 1), h("span", { class: "grow" }, h("span", { class: "answer-text" }, f.q), f.ta))),
    h("div", { class: "row-end" }, btn));
}

/** Past tense for a finished activity line: [verb, one, many]. */
const DONE_VERBS = {
  read_file: ["read", "file", "files"],
  list_directory: ["listed", "folder", "folders"],
  write_file: ["wrote", "file", "files"],
  edit_file: ["edited", "file", "files"],
  execute_command: ["ran", "command", "commands"],
  http_request: ["sent", "request", "requests"],
  remember: ["saved", "memory", "memories"],
  propose_skill: ["proposed", "skill", "skills"],
};

/** "Read 4 files, ran 2 commands, updated the plan" */
function describeTools(tools) {
  const counts = new Map();
  for (const t of tools) counts.set(t.name, (counts.get(t.name) || 0) + 1);
  const parts = [...counts].map(([name, n]) => {
    if (name === "update_plan") return "updated the plan";
    const v = DONE_VERBS[name];
    if (v) return `${v[0]} ${n} ${n === 1 ? v[1] : v[2]}`;
    return `used ${name.replace(/_/g, " ")}` + (n > 1 ? ` ×${n}` : "");
  });
  const text = parts.join(", ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

const ACTIVE_VERBS = {
  read_file: "Reading", list_directory: "Listing", write_file: "Writing", edit_file: "Editing",
  execute_command: "Running", http_request: "Calling", remember: "Remembering", update_plan: "Updating the plan",
};

/** What Airth is doing right now, e.g. "Running $ cargo test". */
function activeLabel(name, input) {
  if (name === "update_plan") return "Updating the plan…";
  const what = toolLabel(name, input);
  return `${ACTIVE_VERBS[name] || "Using " + name.replace(/_/g, " ")} ${what && what.length < 90 ? what : ""}`.trim() + "…";
}

/** Several steps in a row, folded into one line you can open. */
function activityGroup(items, live) {
  const tools = items.filter((i) => i.kind === "tool");
  const failed = tools.filter((t) => t.failed).length;
  const key = "act:" + items[0].node.dataset.k;
  const names = [...new Set(tools.map((t) => t.name))].slice(0, 4);
  return keyed(keyedDetails(key, { class: "activity" + (live ? " live" : "") },
    h("summary", null,
      h("span", { class: "act-icons", "aria-hidden": "true" }, names.map((n) => h("span", { class: "icon" }, toolIcon(n)))),
      h("span", { class: "act-label" }, tools.length ? describeTools(tools) : "Thought it through"),
      failed > 0 && h("span", { class: "chip denied" }, `${failed} not run`),
      h("span", { class: "act-count" }, `${items.length} steps`)),
    h("div", { class: "act-body" }, items.map((i) => i.node))), key);
}

/** An answer, with a copy button under it. */
function assistantMessage(text) {
  return h("div", { class: "msg assistant" },
    h("div", { class: "prose", html: mdc(text) }),
    h("div", { class: "msg-actions" }, copyButton(() => text, "Copy answer")));
}

function copyButton(getText, label) {
  const btn = h("button", { class: "copy-btn", type: "button", title: label, "aria-label": label }, icon("copy"));
  btn.addEventListener("click", () => copyText(getText(), btn));
  return btn;
}

async function copyText(text, btn) {
  try {
    await navigator.clipboard.writeText(text);
    if (btn) { btn.classList.add("done"); setTimeout(() => btn.classList.remove("done"), 1400); }
    flash("Copied");
  } catch {
    flash("Could not copy to the clipboard", true);
  }
}

// Code blocks rendered by md() carry a constant copy button; one listener serves them all.
document.addEventListener("click", (e) => {
  const btn = e.target.closest && e.target.closest(".code-copy");
  if (!btn) return;
  const code = btn.closest(".code").querySelector("code");
  copyText(code ? code.textContent : "", btn);
});

const STEP_MARK = { pending: "", in_progress: "", done: "✓", blocked: "!", skipped: "–" };

/** The right column: the plan, live, then what the run cost. */
function sideNodes(d, status) {
  const s = d.state;
  const plan = s.plan || [];
  const nodes = [];
  if (plan.length) {
    const closed = plan.filter((p) => p.status === "done" || p.status === "skipped").length;
    const pct = Math.round((100 * closed) / plan.length);
    // Set through the CSSOM: the page's CSP blocks inline style attributes.
    const fill = h("span");
    fill.style.width = pct + "%";
    // Flash the steps whose status just changed.
    const first = runEls.steps === null;
    const seen = first ? new Set() : runEls.steps;
    const steps = plan.map((p, i) => {
      const k = `${i}:${p.status}:${p.title}`;
      const changed = !first && !seen.has(k);
      seen.add(k);
      return h("li", { class: "step " + p.status + (changed ? " changed" : "") },
        h("span", { class: "mark", title: p.status.replace("_", " ") }, STEP_MARK[p.status] ?? "•"),
        h("div", { class: "grow" }, h("div", { class: "step-title" }, p.title), p.note && h("div", { class: "step-note" }, p.note)));
    });
    runEls.steps = seen;
    nodes.push(h("section", { class: "side-section" },
      h("div", { class: "side-head" }, h("span", null, "Plan"), h("span", { class: "progress-mini" }, `${closed}/${plan.length}`)),
      h("div", { class: "plan-bar", "aria-hidden": "true" }, fill),
      h("ol", { class: "plan-steps" + (d.live ? " live" : "") }, steps)));
  } else {
    nodes.push(h("section", { class: "side-section" },
      h("div", { class: "side-head" }, h("span", null, "Plan")),
      h("p", { class: "side-empty" }, d.live ? "Airth writes its plan here as soon as the work takes more than a couple of steps." : "No plan for this chat.")));
  }
  // A finished chat's open change is what needs the owner: it goes first.
  if (s.change) {
    const section = h("section", { class: "side-section" }, changePanel(d));
    if (s.change.status === "open" && !d.live) nodes.unshift(section);
    else nodes.push(section);
  }
  const row = (k, v, title) => v != null && v !== false && h("div", { class: "kv", title: title || "" }, h("span", { class: "k" }, k), h("span", { class: "v" }, v));
  nodes.push(h("section", { class: "side-section" },
    h("div", { class: "side-head" }, h("span", null, "Details")),
    row("Status", STATUS_LABEL[status] || status),
    s.origin && row("Started by", s.origin.objective, s.origin.trigger),
    row("Iterations", String(s.iterations)),
    row("Tool calls", String(s.events.length)),
    (s.input_tokens || s.output_tokens) ? row("Tokens", `${kfmt(s.input_tokens)} in · ${kfmt(s.output_tokens)} out`, `${s.input_tokens} in / ${s.output_tokens} out`) : null,
    s.usage && s.usage.length ? row("Model", s.usage[s.usage.length - 1].model) : null,
    s.tier ? row("Routing", s.tier === "fast" ? "fast model (simple ask)" : "main model", "With a fast connection, each ask is triaged: simple ones run on the fast model, the rest on the main one") : null,
    s.usage && s.usage.some((u) => u.cache_read) ? (() => {
      const input = s.usage.reduce((a, u) => a + u.input, 0);
      const cached = s.usage.reduce((a, u) => a + u.cache_read, 0);
      return row("From cache", `${input ? Math.round((100 * cached) / input) : 0}% of input`, `${cached} of ${input} input tokens read from the prompt cache`);
    })() : null,
    s.change && row("Change", s.change.status),
    row("Updated", ago(s.updated_at)),
    row("Chat", s.id)));
  return nodes;
}

async function resumeRun() {
  try {
    await api(`${runPath(app.realm, app.route.id)}/resume`, {});
    app.version = null;
    await Promise.all([pollRun(), refreshRuns()]);
  } catch (e) {
    flash(e.message, true);
  }
}

function keyedDetails(key, attrs, ...children) {
  const el = h("details", attrs, ...children);
  el.open = app.openKeys.has(key);
  el.addEventListener("toggle", () => (el.open ? app.openKeys.add(key) : app.openKeys.delete(key)));
  return el;
}

function toolLabel(name, input) {
  switch (name) {
    case "read_file": return input.path;
    case "list_directory": return (input.path || ".") + (input.recursive ? "  (recursive)" : "");
    case "write_file": return `${input.path}  · ${(input.content || "").split("\n").length} lines`;
    case "execute_command": return "$ " + String(input.command || "").split("\n")[0];
    case "remember": return input.text;
    case "edit_file": return input.path;
    case "http_request": return `${input.method || "GET"} ${input.integration ? input.integration + ": " : ""}${input.url || ""}`;
    case "update_plan": {
      const steps = input.steps || [];
      return `${steps.filter((p) => p.status === "done" || p.status === "skipped").length}/${steps.length} steps done`;
    }
    default: return JSON.stringify(input);
  }
}

function toolCard(b, event, result, live, awaiting) {
  let status, label;
  if (event) { status = event.status; label = event.status; }
  else if (result) { status = result.is_error ? "denied" : "ok"; label = result.is_error ? "not run" : "ok"; }
  else if (awaiting) { status = "awaiting"; label = "awaiting approval"; }
  else if (live) { status = "running"; label = "running"; }
  else { status = "denied"; label = "not run"; }
  const output = event ? event.result : result ? result.content : null;
  const input = b.input || {};

  const body = [];
  if (b.name === "execute_command") body.push(section("Command", h("pre", { class: "out" }, input.command)));
  else if (b.name === "write_file") body.push(section("New content · " + input.path, h("pre", { class: "out" }, input.content)));
  else if (b.name === "edit_file") body.push(section("Edit · " + input.path, diffView((input.old_text || "").split("\n").map((l) => "-" + l).concat((input.new_text || "").split("\n").map((l) => "+" + l)).join("\n"), "diff")));
  else if (b.name === "http_request") body.push(section("Request", h("pre", { class: "out" }, toolLabel(b.name, input) + (input.body ? "\n\n" + input.body : ""))));
  else if (!["read_file", "list_directory", "remember", "update_plan"].includes(b.name)) body.push(section("Input", h("pre", { class: "out" }, JSON.stringify(input, null, 2))));
  if (output != null) body.push(section("Result", h("pre", { class: "out" }, output || "(empty)")));

  return keyedDetails("tool:" + b.id, { class: "tool" },
    h("summary", null,
      h("span", { class: "icon" }, toolIcon(b.name)),
      h("span", { class: "label" }, h("span", { class: "name" }, b.name), toolLabel(b.name, input)),
      h("span", { class: "chip " + status }, label)),
    h("div", { class: "body" }, body));
}

const section = (title, ...children) => h("div", null, h("h4", null, title), ...children);

function diffView(text, kind) {
  const el = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    let cls = "";
    if (kind === "command") cls = line.startsWith("$ ") ? "cmd" : "hunk";
    else if (kind === "plain") cls = "cmd";
    else if (/^(\+\+\+|---)\s/.test(line) || /^(create|modify)\s/.test(line) || line.startsWith("diff --git")) cls = "file";
    else if (line.startsWith("@@")) cls = "hunk";
    else if (/^(index |new file|deleted file|similarity|rename )/.test(line)) cls = "meta";
    else if (line.startsWith("+")) cls = "add";
    else if (line.startsWith("-")) cls = "del";
    el.append(h("div", { class: cls }, line || " "));
  }
  return el;
}

/** A memory change, as text rather than code. */
function readablePreview(text) {
  return h("div", { class: "preview-readable" }, text.split("\n").filter((l) => l.trim()).map((l) =>
    /^\d+\. /.test(l) ? h("div", { class: "pr-item" }, l) : h("div", { class: "pr-line" }, l)));
}

function approvalCard(p) {
  const title = { write_file: "Write this file?", edit_file: "Apply this edit?", execute_command: "Run this command?", remember: "Remember this?", http_request: "Send this request?" }[p.tool] || `Allow ${p.tool}?`;
  const kind = p.tool === "execute_command" ? "command" : ["remember", "http_request"].includes(p.tool) ? "plain" : "diff";
  const reason = h("input", {
    id: "deny-reason", type: "text", placeholder: "Reason for denying (optional; Airth sees it)",
    value: app.reason, autocomplete: "off",
    oninput: (e) => (app.reason = e.target.value),
    onkeydown: (e) => { if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) { e.preventDefault(); answer(false); } },
  });
  return h("div", { class: "approval", role: "region", "aria-label": "Approval required" },
    h("div", { class: "approval-head" },
      h("h3", null, title),
      h("span", { class: "chip rule-ask" }, p.capability)),
    p.warnings.map((w) => h("div", { class: "approval-warn" }, "⚠ " + w)),
    p.tool === "remember" ? readablePreview(p.preview) : diffView(p.preview, kind),
    h("div", { class: "approval-actions" },
      h("button", { class: "btn primary", type: "button", onclick: () => answer(true) }, "Approve ", h("kbd", null, modKey + "↵")),
      p.grantable && h("button", { class: "btn", type: "button", title: "Later requests like this one in this chat are approved without asking", onclick: () => answer(true, true) }, `Allow ${p.grant_label} for this chat`),
      h("button", { class: "btn danger", type: "button", onclick: () => answer(false) }, "Deny"),
      reason));
}

