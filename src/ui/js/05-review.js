// ---------- change review ----------

let confirmDiscard = false;

function changePanel(d) {
  const s = d.state;
  const c = s.change;
  const checks = c.checks;
  const hasChanges = c.base !== c.head;
  const checksCurrent = checks && checks.commit === c.head;
  const checksRunning = checks && !checks.finished && d.live;
  const passed = checksCurrent && checks.finished && checks.results.every((r) => r.ok);
  const failed = checks && checks.results.find((r) => !r.ok);
  const failedNow = checksCurrent && checks.finished && !passed;
  const open = c.status === "open";
  const viaPr = app.realmDetail && app.realmDetail.delivery === "pull_request";
  const pr = c.pull_request;
  const conflicts = c.conflicts || [];
  const resolving = !!c.merging;
  const idle = open && hasChanges && !d.live && !resolving;
  const base = c.base_branch || "main";
  const tool = (ic, label, onclick, cls = "") => h("button", { class: "c-act " + cls, type: "button", title: label, "aria-label": label, onclick }, icon(ic));

  // Head: what this is, where it stands, and the secondary actions as icons.
  const discard = idle && (confirmDiscard
    ? h("button", { class: "c-act sure", type: "button", title: pr ? "Closes the pull request too" : "Throw this change away",
        onclick: () => { confirmDiscard = false; changeAction("discard"); } }, pr ? "Close & discard?" : "Discard?")
    : tool("trash", pr ? "Discard: close the pull request and throw the change away" : "Discard this change", () => {
        confirmDiscard = true; renderRun();
        setTimeout(() => { if (confirmDiscard) { confirmDiscard = false; renderRun(); } }, 3000);
      }, "danger"));
  const head = h("div", { class: "change-head" },
    h("div", { class: "grow" },
      h("div", { class: "change-title" }, h("h3", null, "Change"),
        h("span", { class: "chip " + (c.status === "adopted" ? "completed" : c.status === "discarded" ? "stopped" : "running") }, c.status)),
      h("div", { class: "change-sub", title: `${c.branch}, from ${c.base}` }, `${base} ← `, h("code", null, c.branch.replace(/^airth\//, "")))),
    h("div", { class: "change-tools" },
      hasChanges && tool("code", "View the diff", () => openDiff(s.id, runName({ ...s, title: d.title }))),
      idle && tool("refresh", checks ? "Run the checks again" : "Run the checks", () => changeAction("checks")),
      idle && failed && tool("tool", "Ask Airth to fix the failed check", () => askToFix(failed)),
      discard));

  const files = hasChanges
    ? h("div", { class: "files" }, c.files.map((f) => {
        const [st, ...rest] = f.split("\t");
        const path = rest.join(" → ");
        return h("div", { class: "file-row", title: path }, h("span", { class: "st st-" + st[0] }, st[0]), h("span", { class: "file-path" }, path));
      }))
    : h("div", { class: "change-empty" }, d.live ? "Changes appear here when Airth finishes." : "No files changed.");

  let checkList = null;
  if (checks) {
    checkList = h("div", { class: "checks" },
      h("div", { class: "checks-title" }, "Checks", !checksCurrent && h("span", null, " · on an older commit")),
      checks.results.map((r, i) =>
        keyedDetails(`check:${checks.started_at}:${i}`, { class: "check " + (r.ok ? "ok" : "err") },
          h("summary", null, h("span", { class: "mark" }, r.ok ? "✓" : "✗"), h("code", { title: r.command }, r.command), h("span", { class: "dur" }, (r.duration_ms / 1000).toFixed(1) + "s")),
          h("pre", { class: "out" }, r.output || "(no output)"))),
      checksRunning && h("div", { class: "working small" }, h("span", { class: "spinner" }), "Running checks…"),
      !checksRunning && !checks.finished && h("div", { class: "change-empty" }, "The checks did not finish."));
  } else if (open && hasChanges && d.checks_required && !d.live) {
    checkList = h("div", { class: "checks" }, h("div", { class: "change-empty" }, "Checks have not run on this change."));
  }

  // Where the change stands against its base branch.
  let standing = null;
  if (open && resolving) {
    standing = h("div", { class: "change-note warn" }, icon("gitMerge"),
      h("div", { class: "grow" }, h("div", { class: "strong" }, d.live ? `Resolving the conflict with ${base}…` : `The conflict with ${base} is not resolved yet`),
        h("div", null, conflicts.join(", "))));
  } else if (open && conflicts.length) {
    standing = h("div", { class: "change-note warn" }, icon("gitMerge"),
      h("div", { class: "grow" }, h("div", { class: "strong" }, `Conflicts with ${base}`),
        h("div", null, `${base} changed in the same places: `, conflicts.join(", "))),
      !d.live && h("button", { class: "btn primary small", type: "button", onclick: () => changeAction("resolve") }, "Resolve with Airth"));
  } else if (pr && open) {
    standing = pullRequestBox(d, pr, idle);
  }
  const mine = new Set((c.files || []).map((f) => f.split("\t").pop()));
  const overlap = open && !d.live ? app.runs.filter((r) => r.id !== s.id && r.change === "open" && (r.touches || []).some((f) => mine.has(f))) : [];
  const overlapNote = overlap.length > 0 && h("div", { class: "change-note subtle" }, icon("alert"),
    h("div", { class: "grow" }, "Also changed by ", overlap.map((r, i) => [i > 0 && ", ", h("a", { href: `#/r/${enc(app.realm)}/run/${enc(r.id)}` }, runName(r))]),
      ": ", [...new Set(overlap.flatMap((r) => r.touches.filter((f) => mine.has(f))))].join(", "),
      viaPr ? "." : ". Adopt one first; the other is then updated with it."));

  // The one main action, full width at the bottom.
  let main = null;
  if (idle && !conflicts.length && viaPr && !pr) {
    main = h("button", { class: "btn primary small block", type: "button",
      title: passed || !d.checks_required ? "Push the branch and open its pull request" : "The checks did not pass: open it anyway, as work in progress",
      onclick: () => changeAction("pull-request") }, icon("gitPull"), "Open pull request");
  } else if (idle && !conflicts.length && !viaPr) {
    main = h("button", { class: "btn primary small block", type: "button", disabled: failedNow,
      title: failedNow ? "The checks must pass first" : `Brings in what changed on ${base}, checks again if needed, then merges into your checkout`,
      onclick: () => changeAction("adopt") }, icon("check"), "Adopt into checkout");
  }
  const err = app.changeError ? h("div", { class: "error-banner" }, app.changeError) : null;

  return h("div", { class: "change" }, head, standing, overlapNote, err, files, checkList, main && h("div", { class: "change-actions" }, main));
}

/** A run's pull request: its state on GitHub (refreshed every 30 seconds while shown), merge and open. */
function pullRequestBox(d, pr, idle) {
  const s = d.state;
  const st = app.prStatus && app.prStatus.runId === s.id ? app.prStatus : null;
  if (!st || (!st.loading && Date.now() - st.at > 30000)) refreshPrStatus(s.id);
  const g = st && st.status;
  const pushed = pr.pushed === s.change.head;
  const ci = !g ? null
    : g.checks === "passing" ? h("span", { class: "pr-ci ok" }, icon("check"), "CI passed")
    : g.checks === "failing" ? h("span", { class: "pr-ci err", title: g.failed.join(", ") }, icon("x"), "CI failed")
    : g.checks === "pending" ? h("span", { class: "pr-ci run" }, h("span", { class: "spinner" }), "CI running")
    : h("span", { class: "pr-ci" }, "No CI yet");
  const MERGE = { clean: "Ready to merge", unstable: "Ready to merge", has_hooks: "Ready to merge", behind: `Behind ${s.change.base_branch || "main"}: Airth updates it`,
    dirty: "Conflicts", blocked: "Waiting for its rules", draft: "Draft", unknown: "GitHub is checking it" };
  const ready = g && g.state === "open" && g.checks === "passing" && pushed && g.head === s.change.head && !["dirty", "behind"].includes(g.merge_state);
  return h("div", { class: "pr-box" },
    h("div", { class: "pr-top" }, icon("gitPull"),
      h("a", { class: "pr-title", href: pr.url, target: "_blank", rel: "noopener noreferrer", title: "Open on GitHub" },
        h("span", { class: "pr-num" }, `#${pr.number}`), " ", (g && g.title) || "Pull request"),
      h("a", { class: "c-act", href: pr.url, target: "_blank", rel: "noopener noreferrer", title: "Open on GitHub", "aria-label": "Open on GitHub" }, icon("external"))),
    h("div", { class: "pr-state" },
      !pushed ? h("span", { class: "pr-ci" }, "New commits are pushed once the checks pass")
        : [ci, g && g.state === "open" && h("span", { class: "pr-merge" }, MERGE[g.merge_state] || g.merge_state)],
      !g && st && st.error && h("span", { class: "pr-ci err", title: st.error }, "GitHub unreachable")),
    idle && h("button", { class: "btn primary small block", type: "button", disabled: !ready,
      title: ready ? "Squash-merge it on GitHub, titled like the pull request" : "Once GitHub's checks pass on the latest commit",
      onclick: () => changeAction("merge") }, icon("gitMerge"), "Squash and merge"));
}

async function refreshPrStatus(runId) {
  app.prStatus = { ...(app.prStatus && app.prStatus.runId === runId ? app.prStatus : { runId, at: 0 }), loading: true };
  let next;
  try {
    const { status } = await api(`${runPath(app.realm, runId)}/pull-request`);
    next = { runId, status, at: Date.now(), loading: false };
  } catch (e) {
    next = { runId, status: null, error: e.message, at: Date.now(), loading: false };
  }
  if (app.prStatus && app.prStatus.runId === runId) app.prStatus = next;
  if (app.route.view === "run" && app.route.id === runId) renderRun();
}

/** The change's full diff, in a wide sheet over the page. */
async function openDiff(runId, name) {
  let text;
  try { text = (await api(`${runPath(app.realm, runId)}/diff`)).diff; } catch (e) { return flash(e.message, true); }
  const files = (text.match(/^diff --git /gm) || []).length;
  const close = () => { sheet.remove(); document.removeEventListener("keydown", onKey); };
  const onKey = (e) => { if (e.key === "Escape") close(); };
  const sheet = h("div", { class: "sheet-back", onclick: (e) => { if (e.target === sheet) close(); } },
    h("div", { class: "sheet", role: "dialog", "aria-label": "Diff" },
      h("div", { class: "sheet-head" },
        h("div", { class: "grow" }, h("div", { class: "strong" }, name), h("div", { class: "muted small" }, `${files} file${files === 1 ? "" : "s"} changed`)),
        h("button", { class: "c-act", type: "button", title: "Close (Esc)", "aria-label": "Close", onclick: close }, icon("x"))),
      h("div", { class: "sheet-body" }, diffView(text || "(empty diff)", "diff"))));
  document.addEventListener("keydown", onKey);
  document.body.append(sheet);
}

async function changeAction(action) {
  app.changeError = null;
  try {
    const r = await api(`${runPath(app.realm, app.route.id)}/${action}`, {});
    if (r && r.checking) flash("Checking the change with the latest main, then adopting it");
    if (r && r.adopted) flash("Adopted");
    if (r && r.resolving) flash("Airth is resolving the conflict");
    if (r && r.pull_request) flash(`Pull request #${r.pull_request.number} opened`);
    app.version = null;
    if (action !== "checks") app.diff = null;
    await Promise.all([pollRun(), refreshRuns()]);
  } catch (e) {
    app.changeError = e.message;
    renderRun();
  }
}

function askToFix(failed) {
  const lines = failed.output.split("\n");
  const tail = lines.slice(-60).join("\n");
  sendFollowUp(`The check \`${failed.command}\` failed:\n\n\`\`\`\n${tail}\n\`\`\`\n\nFix the problem, then summarize what you changed.`)
    .catch((e) => { app.changeError = e.message; renderRun(); });
}

async function answer(approve, forRun) {
  const d = app.run;
  if (!d || !d.pending || app.busy) return;
  app.busy = true;
  try {
    await api(`${runPath(app.realm, app.route.id)}/approval`, {
      approval_id: d.pending.id, approve, for_run: !!forRun, reason: approve ? "" : app.reason,
    });
    app.reason = "";
    app.version = null;
    await pollRun();
  } catch (e) {
    app.error = e.message;
  } finally {
    app.busy = false;
  }
}

async function stopRun() {
  try {
    await api(`${runPath(app.realm, app.route.id)}/stop`, {});
    await pollRun();
  } catch (e) {
    app.error = e.message;
  }
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && app.route.view === "run" && app.run && app.run.pending) {
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag !== "TEXTAREA") { e.preventDefault(); answer(true); }
  }
});

