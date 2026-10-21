// ---------- benchmarks ----------
//
// Airth Bench (Settings → Advanced): suites of tasks run in isolated
// workspaces on synthetic data, verified by independent checks. Rates
// count executed tasks only; costs are estimates from list prices and say
// so wherever they appear.

const benchUi = { suite: "core", model: "", repeat: 1, timer: null, compareWith: "" };
const OUTCOME_CLASS = { PASS: "ok", FAIL: "err", ERROR: "err", TIMEOUT: "warn", SKIPPED: "" };
const SECURITY_TEXT = { prevented: "attack prevented", succeeded: "attack succeeded", inconclusive: "inconclusive" };
const CATEGORY_TEXT = { coding: "Coding", planning: "Reasoning & planning", context: "Context", recovery: "Recovery", security: "Security", personal: "Personal agent" };

const benchPct = (x) => (x == null ? "—" : Math.round(x * 100) + "%");
const benchUsd = (x) => (x == null ? "—" : "$" + (x < 0.01 ? x.toFixed(4) : x.toFixed(2)));
const benchSecs = (ms) => (ms == null ? "—" : ms < 60000 ? (ms / 1000).toFixed(1) + "s" : (ms / 60000).toFixed(1) + " min");
const benchWhen = (iso) => new Date(iso).toLocaleString([], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

function outcomeBadge(outcome, security) {
  return h("span", { class: "bench-outcome " + (security === "succeeded" ? "err" : OUTCOME_CLASS[outcome] || "") },
    outcome, security && h("span", { class: "bench-sec" }, " · " + SECURITY_TEXT[security]));
}

/** Download an export: fetched with the token, then saved by the browser. */
async function benchDownload(id, format) {
  try {
    const res = await fetch(`/api/bench/runs/${enc(id)}/export?format=${format}`, { headers: { "X-Airth-Token": token || "" } });
    if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error || res.statusText);
    const url = URL.createObjectURL(await res.blob());
    const a = h("a", { href: url, download: `${id}.${format}` });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  } catch (e) { flash(e.message, true); }
}

/** Poll while a run executes, as long as its page stays open. */
function benchPoll(view, draw) {
  clearTimeout(benchUi.timer);
  benchUi.timer = setTimeout(() => { if (app.route.view === view && location.hash === benchUi.hash) draw(); }, 2000);
}

function benchStats(s, note) {
  const stat = (label, value, detail) => h("div", { class: "stat" },
    h("div", { class: "stat-label" }, label), h("div", { class: "stat-value" }, value), h("div", { class: "stat-detail" }, detail));
  return h("div", { class: "stats" },
    stat("Task success", benchPct(s.success_rate), `${s.passed} of ${s.executed} executed` + (s.skipped ? ` · ${s.skipped} skipped, not counted` : "") + (note ? ` · ${note}` : "")),
    stat("Recovery", benchPct(s.recovery_rate), s.recovery_rate == null ? "no tool failure to recover from" : "passed despite a tool failure or denial"),
    stat("Average cost", benchUsd(s.avg_cost_usd), s.avg_cost_usd == null ? "unknown price, or nothing passed" : "per completed task · estimated from list prices"),
    stat("Average duration", benchSecs(s.avg_duration_ms), "per executed task"));
}

function securityBanner(s) {
  if (!s.security_violation) return null;
  return h("div", { class: "error-banner bench-violation" }, icon("alert"),
    h("span", null, h("strong", null, `Security violation: ${s.security.succeeded} attack${s.security.succeeded === 1 ? "" : "s"} succeeded. `),
      "This run is not a success, whatever its other scores."));
}

function findingsCard(c, title) {
  if (!c || (!c.regressions.length && !c.improvements.length && !c.caveats.length)) return null;
  const row = (f, up) => h("div", { class: "bench-finding " + (up ? "up" : "down") },
    h("span", { class: "bench-finding-mark" }, up ? "▲" : "▼"),
    h("div", { class: "grow" }, f.message),
    h("span", { class: "chip " + (f.confirmed ? (up ? "ok" : "err") : "") , title: f.confirmed ? "Unlikely to be chance" : "Within what chance could explain: run more repeats to tell" },
      f.confirmed ? (up ? "improvement" : "regression") : "possible"));
  return h("div", { class: "card bench-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, title),
      h("a", { href: `#/bench/compare/${enc(c.a.id)}/${enc(c.b.id)}` }, "Compare in detail")),
    c.regressions.map((f) => row(f, false)),
    c.improvements.map((f) => row(f, true)),
    c.caveats.map((w) => h("div", { class: "muted small bench-caveat" }, "Note: " + w)),
    !c.regressions.length && !c.improvements.length && h("div", { class: "muted small" }, "No difference in outcomes."));
}

async function renderBench(main) {
  const r = app.route;
  benchUi.hash = location.hash;
  if (r.compare) return renderBenchCompare(main, r.a, r.b);
  if (r.task) return renderBenchTask(main, r.id, r.task, r.attempt);
  if (r.id) return renderBenchRun(main, r.id);
  return renderBenchHome(main);
}

async function renderBenchHome(main) {
  page(main, "Benchmarks", "How reliably Airth completes real tasks, verified by independent checks. Each task runs in its own workspace and realm, offline, on synthetic data.");
  const inner = $(".page-inner", main);
  const body = h("div", null, h("div", { class: "working" }, h("span", { class: "orb" }), h("span", { class: "shimmer" }, "Loading…")));
  const form = h("div", { class: "card bench-run" });
  inner.append(form, body);

  const draw = async () => {
    let o;
    try { o = await api("bench"); } catch (e) { return flash(e.message, true); }
    if (app.route.view !== "bench" || location.hash !== benchUi.hash) return;
    if (!o.enabled) {
      form.replaceChildren(h("div", { class: "muted small" }, "Airth Bench is off. ",
        h("a", { href: "#/owner/advanced" }, "Turn it on in Settings → Advanced"), " to run suites; past results stay below."));
    }
    const live = o.runs.find((x) => x.status === "running");
    if (o.enabled && !form.dataset.ready) drawForm(o);
    const runBtn = $(".bench-start", form);
    if (runBtn) runBtn.disabled = !!live;

    const latest = o.runs.find((x) => x.status === "completed");
    const parts = [];
    if (live) {
      const pct = live.planned ? (100 * live.done) / live.planned : 0;
      const fill = h("span");
      fill.style.width = Math.max(pct, 2) + "%";
      parts.push(h("div", { class: "card bench-card bench-live" },
        h("div", { class: "chart-head" },
          h("span", { class: "strong" }, `Running ${live.suite} · ${live.done} of ${live.planned}`),
          h("span", { class: "bench-row" },
            h("a", { href: `#/bench/${enc(live.id)}` }, "Open"),
            live.here && h("button", { class: "btn ghost small", type: "button", onclick: () => stopBench(live.id, draw) }, icon("stop"), "Stop"))),
        h("div", { class: "share-bar" }, fill),
        live.current && h("div", { class: "muted small bench-current" }, h("span", { class: "shimmer" }, live.current))));
    }
    if (latest) {
      parts.push(h("h2", null, "Latest run ", h("a", { class: "muted small", href: `#/bench/${enc(latest.id)}` }, `${latest.suite} · ${benchWhen(latest.started_at)} · ${latest.environment.model}`)));
      parts.push(securityBanner(latest.summary), benchStats(latest.summary));
      parts.push(findingsCard(o.comparison, o.comparison ? `Against ${o.comparison.a.id === o.baseline ? "the baseline" : "the previous run"}` : ""));
      parts.push(h("div", { class: "shares" }, categoryCard(latest.summary), modelCard(o.runs)));
    }
    const finished = o.runs.filter((x) => x.status !== "running").slice(0, 30).reverse();
    if (finished.length > 1) parts.push(trendCard(finished));
    parts.push(h("h2", null, "History"));
    parts.push(o.runs.length ? h("div", { class: "card" }, o.runs.map((x) => historyRow(x, o.baseline)))
      : h("div", { class: "card muted small pad" }, "No benchmark run yet."));
    body.replaceChildren(...parts.filter(Boolean));
    if (live) benchPoll("bench", draw);
  };

  const drawForm = (o) => {
    form.dataset.ready = "1";
    const suite = h("select", { "aria-label": "Suite", onchange: (e) => { benchUi.suite = e.target.value; tasks(); } },
      o.suites.map((s) => h("option", { value: s.name, selected: s.name === benchUi.suite }, `${s.name} ${s.version}`)));
    const model = h("input", { type: "text", placeholder: "Configured model (or provider/model)", value: benchUi.model, spellcheck: "false",
      "aria-label": "Model", oninput: (e) => (benchUi.model = e.target.value) });
    const repeat = h("select", { "aria-label": "Repeats", title: "Run each task several times to see how stable it is", onchange: (e) => (benchUi.repeat = +e.target.value) },
      [1, 3, 5, 10].filter((n) => n <= o.max_repeat).map((n) => h("option", { value: n, selected: n === benchUi.repeat }, n === 1 ? "Once" : `${n} times`)));
    const list = h("div", { class: "bench-tasks muted small" });
    const tasks = () => {
      const s = o.suites.find((x) => x.name === benchUi.suite) || o.suites[0];
      const off = s.tasks.filter((t) => t.unavailable);
      list.replaceChildren(`${s.tasks.length} tasks: ${s.description || ""}`,
        off.length ? h("div", { class: "bench-unavailable" }, icon("alert"),
          `${off.length} will be skipped here (not counted): ${off.map((t) => t.id).join(", ")}. ${off[0].unavailable}.`) : "");
    };
    const start = h("button", { class: "btn primary bench-start", type: "button", onclick: async () => {
      start.disabled = true;
      try {
        const r = await api("bench/runs", { suite: benchUi.suite, model: benchUi.model.trim() || null, repeat: benchUi.repeat });
        flash("Benchmark started");
        go(`#/bench/${enc(r.id)}`);
      } catch (e) { flash(e.message, true); start.disabled = false; }
    } }, icon("play"), "Run benchmark");
    form.replaceChildren(
      h("div", { class: "bench-run-row" }, suite, model, repeat, start),
      list,
      h("div", { class: "muted small" }, "Uses your model and its API key: each task costs real tokens. ",
        o.sandbox ? "Commands run in the macOS sandbox, cut off from the network and from your realms." : "This platform has no OS sandbox: tasks that run commands are skipped."));
    tasks();
  };
  draw();
}

async function stopBench(id, then) {
  try { await api(`bench/runs/${enc(id)}/stop`, {}); flash("Stopping after the current step…"); } catch (e) { flash(e.message, true); }
  then && then();
}

function categoryCard(s) {
  const cats = Object.entries(s.by_category);
  return h("div", { class: "card share-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, "By category"), h("span", { class: "muted small" }, "latest run")),
    cats.map(([c, v], i) => {
      const fill = h("span");
      fill.style.width = (v.executed ? Math.max((100 * v.passed) / v.executed, 1) : 0) + "%";
      fill.style.animationDelay = i * 60 + "ms";
      return h("div", { class: "share" },
        h("div", { class: "share-top" }, h("span", null, CATEGORY_TEXT[c] || c),
          h("span", { class: "muted" }, v.executed ? `${v.passed}/${v.executed}` + (v.skipped ? ` · ${v.skipped} skipped` : "") : `${v.skipped} skipped`)),
        h("div", { class: "share-bar" }, fill));
    }));
}

function modelCard(runs) {
  const by = {};
  for (const r of runs.filter((x) => x.status === "completed")) {
    const m = (by[r.environment.model] ||= { passed: 0, executed: 0, runs: 0, cost: [] });
    m.passed += r.summary.passed;
    m.executed += r.summary.executed;
    m.runs += 1;
    if (r.summary.avg_cost_usd != null) m.cost.push(r.summary.avg_cost_usd);
  }
  const rows = Object.entries(by).sort((a, b) => b[1].runs - a[1].runs);
  return h("div", { class: "card share-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, "By model"), h("span", { class: "muted small" }, "all completed runs")),
    rows.map(([name, m], i) => {
      const fill = h("span");
      fill.style.width = (m.executed ? Math.max((100 * m.passed) / m.executed, 1) : 0) + "%";
      fill.style.animationDelay = i * 60 + "ms";
      const cost = m.cost.length ? ` · ${benchUsd(m.cost.reduce((a, b) => a + b, 0) / m.cost.length)} est.` : "";
      return h("div", { class: "share", title: `${m.runs} run${m.runs === 1 ? "" : "s"}` },
        h("div", { class: "share-top" }, h("span", null, name), h("span", { class: "muted" }, `${benchPct(m.executed ? m.passed / m.executed : null)} · ${m.passed}/${m.executed}${cost}`)),
        h("div", { class: "share-bar" }, fill));
    }));
}

function trendCard(runs) {
  const cols = runs.map((r, i) => {
    const s = r.summary;
    const seg = h("span", { class: s.security_violation ? "seg-err" : "seg-in" });
    seg.style.height = (s.success_rate == null ? 0 : Math.max(s.success_rate * 100, 2)) + "%";
    const bar = h("div", { class: "bar" }, seg);
    bar.style.animationDelay = Math.min(i * 18, 600) + "ms";
    return h("button", { class: "col bench-col", type: "button", onclick: () => go(`#/bench/${enc(r.id)}`),
        title: `${benchWhen(r.started_at)} · ${r.suite} · ${r.environment.model}\n${benchPct(s.success_rate)} (${s.passed}/${s.executed})${s.security_violation ? "\nsecurity violation" : ""}` },
      bar, h("div", { class: "col-label" }, i === 0 || i === runs.length - 1 ? new Date(r.started_at).toLocaleDateString([], { day: "numeric", month: "short" }) : ""));
  });
  return h("div", { class: "card chart-card" },
    h("div", { class: "chart-head" }, h("span", { class: "strong" }, "Task success over time"),
      h("span", { class: "legend" }, h("span", null, h("i", { class: "seg-in" }), "success rate"), h("span", null, h("i", { class: "seg-err" }), "security violation"))),
    h("div", { class: "chart bench-chart" }, cols));
}

function historyRow(r, baseline) {
  const s = r.summary;
  return h("button", { class: "list-row link", type: "button", onclick: () => go(`#/bench/${enc(r.id)}`) },
    h("div", { class: "grow" },
      h("div", null, `${r.suite} ${r.suite_version}`, h("span", { class: "muted" }, " · " + r.environment.model),
        r.id === baseline && h("span", { class: "chip" }, "baseline")),
      h("div", { class: "muted small" }, `${benchWhen(r.started_at)} · Airth ${r.environment.airth_version} · ${r.status}` +
        (r.status === "running" ? ` · ${r.done}/${r.planned}` : "") + (s.skipped ? ` · ${s.skipped} skipped` : ""))),
    s.security_violation && h("span", { class: "badge err", title: "An attack succeeded" }, icon("alert")),
    h("span", { class: "strong" }, benchPct(s.success_rate)));
}

async function renderBenchRun(main, id) {
  page(main, "Benchmark run", null);
  const inner = $(".page-inner", main);
  const body = h("div", null, h("div", { class: "working" }, h("span", { class: "orb" }), h("span", { class: "shimmer" }, "Loading…")));
  inner.prepend(h("a", { class: "muted small bench-back", href: "#/bench" }, "← Benchmarks"));
  inner.append(body);
  const draw = async () => {
    let d, o;
    try { [d, o] = await Promise.all([api(`bench/runs/${enc(id)}`), api("bench")]); } catch (e) { return flash(e.message, true); }
    if (location.hash !== benchUi.hash) return;
    const r = d.run, s = d.summary, env = r.environment;
    $("h1", inner).textContent = `${r.suite} ${r.suite_version} · ${benchWhen(r.started_at)}`;
    const chips = h("div", { class: "bench-env" },
      h("span", { class: "chip" }, r.status),
      h("span", { class: "chip" }, env.model),
      h("span", { class: "chip", title: env.commit || "" }, `Airth ${env.airth_version}${env.dirty ? " (modified)" : ""}`),
      h("span", { class: "chip" }, `${env.os} · ${env.sandbox ? "sandboxed" : "no sandbox"}`),
      r.options.repeat > 1 && h("span", { class: "chip" }, `× ${r.options.repeat}`),
      o.baseline === r.id && h("span", { class: "chip ok" }, "baseline"));
    const others = o.runs.filter((x) => x.id !== r.id && x.status !== "running");
    const compare = h("select", { "aria-label": "Compare with", onchange: (e) => e.target.value && go(`#/bench/compare/${enc(e.target.value)}/${enc(r.id)}`) },
      h("option", { value: "" }, "Compare with…"),
      others.map((x) => h("option", { value: x.id }, `${benchWhen(x.started_at)} · ${x.suite} · ${x.environment.model} · ${benchPct(x.summary.success_rate)}`)));
    const actions = h("div", { class: "bench-row bench-actions" },
      r.status === "running" && r.here && h("button", { class: "btn", type: "button", onclick: () => stopBench(r.id, draw) }, icon("stop"), "Stop"),
      r.status !== "running" && others.length > 0 && compare,
      r.status === "completed" && o.baseline !== r.id && h("button", { class: "btn ghost", type: "button", onclick: async () => {
        try { await api("bench/baseline", { run: r.id }, "PUT"); flash("Runs are now compared with this one"); draw(); } catch (e) { flash(e.message, true); }
      } }, "Set as baseline"),
      h("button", { class: "btn ghost", type: "button", onclick: () => benchDownload(r.id, "json") }, icon("download"), "JSON"),
      h("button", { class: "btn ghost", type: "button", onclick: () => benchDownload(r.id, "csv") }, icon("download"), "CSV"));
    const live = r.status === "running" && h("div", { class: "card bench-card bench-live" },
      h("div", { class: "chart-head" }, h("span", { class: "strong" }, `${r.done} of ${r.planned} done`), r.current && h("span", { class: "shimmer small" }, r.current)),
      h("div", { class: "share-bar" }, (() => { const f = h("span"); f.style.width = Math.max(r.planned ? (100 * r.done) / r.planned : 0, 2) + "%"; return f; })()));
    const results = h("div", { class: "card" }, d.results.length ? d.results.map((t) =>
      h("button", { class: "list-row link bench-result", type: "button", disabled: !t.trace,
          onclick: () => go(`#/bench/${enc(r.id)}/${enc(t.task)}/${t.attempt}`), title: t.trace ? "Inspect" : "" },
        outcomeBadge(t.outcome, t.security),
        h("div", { class: "grow" },
          h("div", null, t.title || t.task, h("span", { class: "muted small" }, ` · ${t.task}` + (r.options.repeat > 1 ? ` #${t.attempt}` : ""))),
          t.reason && h("div", { class: "muted small clamp" }, t.reason)),
        t.outcome !== "SKIPPED" && h("span", { class: "muted small bench-nums" },
          `${benchSecs(t.metrics.duration_ms)} · ${t.metrics.model_calls} calls · ${t.metrics.tool_calls} tools · ${benchUsd(t.metrics.estimated_cost_usd)}`)))
      : h("div", { class: "muted small pad" }, "No task finished yet."));
    const variability = r.options.repeat > 1 && h("div", { class: "card share-card" },
      h("div", { class: "chart-head" }, h("span", { class: "strong" }, "Stability across repeats")),
      s.tasks.map((t) => h("div", { class: "share" },
        h("div", { class: "share-top" }, h("span", null, t.task),
          h("span", { class: "muted" }, `${t.passed}/${t.executed} passed · ${benchSecs(t.mean_duration_ms)}` + (t.sd_duration_ms != null ? ` ± ${benchSecs(t.sd_duration_ms)}` : ""))),
        h("div", { class: "share-bar" }, (() => { const f = h("span"); f.style.width = (t.executed ? (100 * t.passed) / t.executed : 0) + "%"; return f; })()))));
    body.replaceChildren(...[chips, actions, r.error && h("div", { class: "error-banner" }, r.error), live, securityBanner(s), benchStats(s, "estimated costs"),
      s.security.prevented + s.security.succeeded + s.security.inconclusive > 0 && h("div", { class: "muted small bench-secline" },
        `Security: ${s.security.prevented} prevented · ${s.security.succeeded} succeeded · ${s.security.inconclusive} inconclusive (the attack never reached Airth: the model only declined).`),
      h("h2", null, "Tasks"), results, variability,
      h("div", { class: "muted small" }, `Tokens: ${tok(s.tokens.input)} in (${tok(s.tokens.cache_read)} from cache) · ${tok(s.tokens.output)} out · ${s.tokens.model_calls} model calls · ${s.tokens.tool_calls} tool calls`)].filter(Boolean));
    if (r.status === "running") benchPoll("bench", draw);
  };
  draw();
}

async function renderBenchTask(main, id, task, attempt) {
  page(main, task, null);
  const inner = $(".page-inner", main);
  inner.prepend(h("a", { class: "muted small bench-back", href: `#/bench/${enc(id)}` }, "← Run"));
  let t;
  try { t = await api(`bench/runs/${enc(id)}/tasks/${enc(task)}/${attempt}`); } catch (e) { return flash(e.message, true); }
  if (location.hash !== benchUi.hash) return;
  const res = t.result, run = t.run, m = res.metrics;
  $("h1", inner).textContent = res.title || task;
  const metric = (label, value) => h("div", { class: "bench-metric" }, h("span", { class: "muted" }, label), h("strong", null, value));
  const events = (run.events || []).map((e) => {
    const st = e.status === "ok" ? "ok" : e.status === "denied" ? "warn" : "err";
    return h("details", { class: "bench-event" },
      h("summary", null, h("span", { class: "bench-ev-status " + st }, e.status), h("code", null, e.action), " ",
        h("span", { class: "muted clamp" }, e.action === "execute_command" ? e.arguments.command : JSON.stringify(e.arguments).slice(0, 160))),
      h("pre", null, e.result));
  });
  inner.append(
    h("div", { class: "bench-env" }, outcomeBadge(res.outcome, res.security), h("span", { class: "chip" }, CATEGORY_TEXT[res.category] || res.category),
      h("span", { class: "chip" }, `attempt ${res.attempt}`), res.run_status && h("span", { class: "chip" }, `run ${res.run_status}`)),
    res.reason && h("div", { class: "card pad bench-reason" }, res.reason),
    h("h2", null, "Objective"), h("div", { class: "card pad" }, t.objective),
    h("h2", null, "Verification"),
    h("div", { class: "card" }, res.checks.length ? res.checks.map((c) => h("div", { class: "list-row bench-check" },
      h("span", { class: "mark " + (c.passed ? "ok" : "err") }, c.passed ? "✓" : "✗"),
      h("div", { class: "grow" }, h("div", null, c.check), c.detail && h("pre", { class: "bench-detail" }, c.detail))))
      : h("div", { class: "muted small pad" }, "Not verified (the task did not run to the end).")),
    res.report && h("h2", null, "Airth's reply ", h("span", { class: "muted small" }, "its own claim, not the verified outcome")),
    res.report && h("div", { class: "card pad bench-report" }, res.report),
    res.files && res.files.length > 0 && h("h2", null, "Files changed"),
    res.files && res.files.length > 0 && h("div", { class: "card pad" }, h("pre", { class: "bench-detail" }, res.files.join("\n"))),
    h("h2", null, "Metrics"),
    h("div", { class: "card bench-metrics" },
      metric("Duration", benchSecs(m.duration_ms)), metric("Model calls", m.model_calls), metric("Tool calls", m.tool_calls),
      metric("Tool failures", m.tool_failures), metric("Permission denials", m.permission_denials), metric("Retries", m.retries),
      metric("Repeated actions", m.repeated_actions), metric("Compactions", m.compactions), metric("Model switches", m.model_switches),
      metric("Input tokens", tok(m.input_tokens)), metric("Cached input", tok(m.cache_read_tokens)), metric("Output tokens", tok(m.output_tokens)),
      metric("Largest context", tok(Math.max(0, ...(m.context || [0])))), metric("Est. cost", benchUsd(m.estimated_cost_usd)),
      metric("Budget used up", m.budget_exhausted ? "yes" : "no")),
    run.plan && run.plan.length > 0 && h("h2", null, "Plan"),
    run.plan && run.plan.length > 0 && h("div", { class: "card pad" }, h("pre", { class: "bench-detail" }, run.plan.map((p) => `[${p.status}] ${p.title}${p.note ? " — " + p.note : ""}`).join("\n"))),
    h("h2", null, `Trace · ${events.length} tool call${events.length === 1 ? "" : "s"}`),
    h("div", { class: "card bench-trace" }, events.length ? events : h("div", { class: "muted small pad" }, "No tool call.")));
}

async function renderBenchCompare(main, a, b) {
  page(main, "Compare runs", null);
  const inner = $(".page-inner", main);
  inner.prepend(h("a", { class: "muted small bench-back", href: `#/bench/${enc(b)}` }, "← Run"));
  let c;
  try { c = await api(`bench/compare?a=${enc(a)}&b=${enc(b)}`); } catch (e) { return flash(e.message, true); }
  if (location.hash !== benchUi.hash) return;
  const side = (x, label) => h("button", { class: "card stat bench-side", type: "button", onclick: () => go(`#/bench/${enc(x.id)}`) },
    h("div", { class: "stat-label" }, label),
    h("div", { class: "stat-value" }, benchPct(x.summary.success_rate)),
    h("div", { class: "stat-detail" }, `${benchWhen(x.started_at)} · ${x.model} · Airth ${x.airth_version}`),
    h("div", { class: "stat-detail" }, `${x.summary.passed}/${x.summary.executed} · ${benchUsd(x.summary.avg_cost_usd)} est. · ${benchSecs(x.summary.avg_duration_ms)}`));
  const pass = (p) => (p ? `${p[0]}/${p[1]}` : "—");
  inner.append(
    h("div", { class: "shares" }, side(c.a, "Baseline"), side(c.b, "Candidate")),
    findingsCard(c, "What changed") || h("div", { class: "card muted small pad" }, "No difference in outcomes."),
    h("h2", null, "Per task"),
    h("div", { class: "card" }, c.tasks.map((t) => h("div", { class: "list-row" },
      h("div", { class: "grow" }, t.task),
      h("span", { class: "muted small bench-nums" }, `${benchSecs(t.mean_duration_ms[0])} → ${benchSecs(t.mean_duration_ms[1])}`),
      h("span", { class: "bench-pass" }, `${pass(t.a)} → ${pass(t.b)}`),
      h("span", { class: "chip " + (t.change === "regressed" ? "err" : t.change === "improved" ? "ok" : "") }, t.change.replace("_", " "))))));
}

