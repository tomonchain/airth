//! Airth Bench in the app: the overview, runs, traces and comparisons.

use super::*;

impl App {
    /// The benchmark routes; None when the request is not one of them.
    pub(super) fn bench_routes(
        &self,
        request: &mut Request,
        method: &Method,
        segments: &[&str],
        query: &str,
    ) -> Result<Option<HttpResponse>> {
        let ok = || json_response(200, json!({"ok": true}));
        Ok(Some(match (method, segments) {
            (Method::Get, ["bench"]) => json_response(200, self.bench_overview()?),
            (Method::Put, ["bench"]) => {
                let body = read_json(request)?;
                let on = body["enabled"]
                    .as_bool()
                    .context("enabled must be true or false")?;
                Bench::new(self.hub.home()).set_enabled(on)?;
                json_response(200, self.bench_overview()?)
            }
            (Method::Put, ["bench", "baseline"]) => {
                let body = read_json(request)?;
                Bench::new(self.hub.home())
                    .store()
                    .set_baseline(body["run"].as_str())?;
                ok()
            }
            (Method::Post, ["bench", "runs"]) => {
                let body = read_json(request)?;
                let id = self.start_bench(&body)?;
                json_response(202, json!({ "id": id }))
            }
            (Method::Get, ["bench", "runs", id]) => {
                let bench = Bench::new(self.hub.home());
                let run = bench.store().load(id)?;
                json_response(
                    200,
                    json!({
                        "run": self.bench_run_view(&run),
                        "results": &run.results,
                        "summary": Summary::of(&run),
                        "baseline": bench.baseline_for(&run).ok().map(|b| b.id),
                    }),
                )
            }
            (Method::Post, ["bench", "runs", id, "stop"]) => {
                match self.benches.lock().unwrap().get(*id) {
                    Some(cancel) => cancel.cancel(),
                    None => bail!("this benchmark is not running in this app"),
                }
                ok()
            }
            (Method::Get, ["bench", "runs", id, "export"]) => {
                let run = Bench::new(self.hub.home()).store().load(id)?;
                let (body, kind, ext) = if param(query, "format") == "csv" {
                    (crate::bench::csv(&run), "text/csv; charset=utf-8", "csv")
                } else {
                    (
                        serde_json::to_string_pretty(&run)?,
                        "application/json",
                        "json",
                    )
                };
                Response::from_string(body)
                    .with_header(content_type(kind))
                    .with_header(
                        Header::from_bytes(
                            "Content-Disposition",
                            format!("attachment; filename=\"{}.{ext}\"", run.id),
                        )
                        .unwrap(),
                    )
            }
            (Method::Get, ["bench", "runs", id, "tasks", task, attempt]) => {
                let attempt: u32 = attempt.parse().context("attempt must be a number")?;
                json_response(
                    200,
                    Bench::new(self.hub.home())
                        .store()
                        .trace(id, task, attempt)?,
                )
            }
            (Method::Get, ["bench", "compare"]) => {
                let (a, b) = (param(query, "a"), param(query, "b"));
                let c = Bench::new(self.hub.home())
                    .compare((!a.is_empty()).then_some(a.as_str()), &b)?;
                json_response(200, json!(c))
            }
            _ => return Ok(None),
        }))
    }
}

impl App {
    /// A benchmark run as listed: everything but its results.
    fn bench_run_view(&self, run: &BenchRun) -> Value {
        let s = Summary::of(run);
        json!({
            "id": run.id,
            "suite": run.suite,
            "suite_version": run.suite_version,
            "status": run.display_status(),
            "started_at": run.started_at,
            "finished_at": run.finished_at,
            "environment": run.environment,
            "options": run.options,
            "planned": run.planned,
            "done": run.results.len(),
            "current": run.current,
            "error": run.error,
            "here": self.benches.lock().unwrap().contains_key(&run.id),
            "summary": {
                "executed": s.executed,
                "passed": s.passed,
                "skipped": s.skipped,
                "success_rate": s.success_rate,
                "recovery_rate": s.recovery_rate,
                "avg_cost_usd": s.avg_cost_usd,
                "avg_duration_ms": s.avg_duration_ms,
                "security": s.security,
                "security_violation": s.security_violation,
                "by_category": s.by_category,
            },
        })
    }

    /// The Benchmarks page: suites, past runs, and how the latest finished
    /// run compares with its baseline.
    fn bench_overview(&self) -> Result<Value> {
        let bench = Bench::new(self.hub.home());
        let runs = bench.store().list()?;
        let latest = runs
            .iter()
            .find(|r| r.status == crate::bench::storage::RunStatus::Completed);
        let comparison = latest.and_then(|r| {
            let base = bench.baseline_for(r).ok()?;
            Some(crate::bench::comparison::compare(&base, r))
        });
        let suites: Vec<Value> = bench
            .suites()
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "version": s.version,
                    "description": s.description,
                    "builtin": s.builtin,
                    "tasks": s.tasks.iter().map(|t| json!({
                        "id": t.id,
                        "category": t.category,
                        "title": t.title,
                        "unavailable": crate::bench::runner::unavailable(t),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({
            "enabled": bench.enabled(),
            "sandbox": sandbox::available(),
            "suites": suites,
            "baseline": bench.store().baseline(),
            "runs": runs.iter().map(|r| self.bench_run_view(r)).collect::<Vec<_>>(),
            "comparison": comparison,
            "max_repeat": crate::bench::MAX_REPEAT,
        }))
    }

    /// Record a benchmark run and execute it on its own thread.
    fn start_bench(&self, body: &Value) -> Result<String> {
        let options = RunOptions {
            tasks: body["tasks"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            repeat: body["repeat"]
                .as_u64()
                .unwrap_or(1)
                .min(u64::from(u32::MAX)) as u32,
            model: body["model"]
                .as_str()
                .filter(|m| !m.trim().is_empty())
                .map(String::from),
            max_tokens: body["max_tokens"].as_u64(),
            keep: false,
        };
        let suite = body["suite"].as_str().unwrap_or("core");
        let bench = Bench::new(self.hub.home());
        let (mut run, suite) = bench.prepare(&self.hub, suite, options)?;
        let id = run.id.clone();
        let cancel = CancelToken::default();
        self.benches
            .lock()
            .unwrap()
            .insert(id.clone(), cancel.clone());
        let (hub, benches) = (self.hub.clone(), self.benches.clone());
        std::thread::spawn(move || {
            struct Quiet;
            impl Observer for Quiet {}
            if let Err(e) = bench.execute(&hub, &mut run, &suite, cancel, &mut Quiet, &mut |_| {}) {
                eprintln!("benchmark {} failed: {e:#}", run.id);
            }
            benches.lock().unwrap().remove(&run.id);
        });
        Ok(id)
    }
}
