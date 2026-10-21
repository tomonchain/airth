//! Comparing two benchmark runs (a baseline and a candidate): what got
//! better, what got worse, and whether a difference is more than noise.
//!
//! A drop in success is *confirmed* only when a two-proportion test says it
//! is unlikely to be chance (z ≥ 1.96, about 95%); with few samples it stays
//! a *possible* regression, shown as such. Costs and durations must rise by
//! more than a quarter, on most of the tasks both runs passed, to count.
//! A new security failure is always a confirmed regression.

use super::metrics::Summary;
use super::storage::{BenchRun, Outcome, SecurityOutcome};
use serde::Serialize;

/// How much a cost or duration must grow to be flagged.
const GROWTH: f64 = 0.25;
const Z_95: f64 = 1.96;

#[derive(Debug, Serialize)]
pub struct RunRef {
    pub id: String,
    pub suite: String,
    pub suite_version: String,
    pub airth_version: String,
    pub commit: Option<String>,
    pub model: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub summary: Summary,
}

#[derive(Debug, Serialize)]
pub struct TaskDelta {
    pub task: String,
    /// Passes and executed attempts, in each run.
    pub a: Option<(usize, usize)>,
    pub b: Option<(usize, usize)>,
    /// improved, regressed, unchanged, added, removed or not_comparable.
    pub change: &'static str,
    pub mean_duration_ms: (Option<f64>, Option<f64>),
    pub mean_cost_usd: (Option<f64>, Option<f64>),
}

#[derive(Debug, Serialize)]
pub struct Finding {
    /// success, task, cost, duration or security.
    pub kind: &'static str,
    pub message: String,
    /// False: worth a look, but within what chance could explain.
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub a: RunRef,
    pub b: RunRef,
    pub tasks: Vec<TaskDelta>,
    pub regressions: Vec<Finding>,
    pub improvements: Vec<Finding>,
    /// What makes the runs less comparable (another suite version, model…).
    pub caveats: Vec<String>,
}

fn run_ref(run: &BenchRun) -> RunRef {
    RunRef {
        id: run.id.clone(),
        suite: run.suite.clone(),
        suite_version: run.suite_version.clone(),
        airth_version: run.environment.airth_version.clone(),
        commit: run.environment.commit.clone(),
        model: run.environment.model.clone(),
        started_at: run.started_at,
        summary: Summary::of(run),
    }
}

/// z statistic of the drop from `a` to `b` (passes over trials); positive
/// when `b` is worse.
pub fn drop_z(a: (usize, usize), b: (usize, usize)) -> f64 {
    let (pa, na) = (a.0 as f64, a.1 as f64);
    let (pb, nb) = (b.0 as f64, b.1 as f64);
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    let pooled = (pa + pb) / (na + nb);
    let se = (pooled * (1.0 - pooled) * (1.0 / na + 1.0 / nb)).sqrt();
    if se == 0.0 {
        return 0.0;
    }
    (pa / na - pb / nb) / se
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

pub fn compare(a: &BenchRun, b: &BenchRun) -> Comparison {
    let (ra, rb) = (run_ref(a), run_ref(b));
    let mut c = Comparison {
        tasks: Vec::new(),
        regressions: Vec::new(),
        improvements: Vec::new(),
        caveats: Vec::new(),
        a: ra,
        b: rb,
    };
    if a.suite != b.suite {
        c.caveats
            .push(format!("different suites ({} and {})", a.suite, b.suite));
    } else if a.suite_version != b.suite_version {
        c.caveats.push(format!(
            "different versions of the suite ({} and {})",
            a.suite_version, b.suite_version
        ));
    }
    if a.environment.model != b.environment.model {
        c.caveats.push(format!(
            "different models ({} and {})",
            a.environment.model, b.environment.model
        ));
    }
    if a.environment.sandbox != b.environment.sandbox {
        c.caveats
            .push("only one of the runs had the OS sandbox".into());
    }

    // Overall success, on tasks both runs executed.
    let (sa, sb) = (&c.a.summary, &c.b.summary);
    let common: Vec<&str> = sa
        .tasks
        .iter()
        .filter(|t| t.executed > 0)
        .filter(|t| sb.tasks.iter().any(|u| u.task == t.task && u.executed > 0))
        .map(|t| t.task.as_str())
        .collect();
    let tally = |s: &Summary| {
        s.tasks
            .iter()
            .filter(|t| common.contains(&t.task.as_str()))
            .fold((0, 0), |(p, n), t| (p + t.passed, n + t.executed))
    };
    let (ta, tb) = (tally(sa), tally(sb));
    if ta.1 > 0 && tb.1 > 0 {
        let (ra, rb) = (ta.0 as f64 / ta.1 as f64, tb.0 as f64 / tb.1 as f64);
        let z = drop_z(ta, tb);
        let message = format!(
            "success on the tasks both ran: {} → {} ({}/{} → {}/{})",
            pct(ra),
            pct(rb),
            ta.0,
            ta.1,
            tb.0,
            tb.1
        );
        if rb < ra {
            c.regressions.push(Finding {
                kind: "success",
                message,
                confirmed: z >= Z_95,
            });
        } else if rb > ra {
            c.improvements.push(Finding {
                kind: "success",
                message,
                confirmed: -z >= Z_95,
            });
        }
    }

    // Per task.
    let mut names: Vec<&str> = sa.tasks.iter().map(|t| t.task.as_str()).collect();
    for t in &sb.tasks {
        if !names.contains(&t.task.as_str()) {
            names.push(&t.task);
        }
    }
    let (mut costlier, mut slower, mut both_passed) = (0, 0, 0);
    for name in names {
        let ta = sa.tasks.iter().find(|t| t.task == name);
        let tb = sb.tasks.iter().find(|t| t.task == name);
        let counts = |t: Option<&super::metrics::TaskStats>| {
            t.filter(|t| t.executed > 0).map(|t| (t.passed, t.executed))
        };
        let (pa, pb) = (counts(ta), counts(tb));
        let change = match (ta, tb, pa, pb) {
            (None, Some(_), _, _) => "added",
            (Some(_), None, _, _) => "removed",
            (_, _, Some(x), Some(y)) => {
                let (ra, rb) = (x.0 as f64 / x.1 as f64, y.0 as f64 / y.1 as f64);
                if rb < ra {
                    "regressed"
                } else if rb > ra {
                    "improved"
                } else {
                    "unchanged"
                }
            }
            _ => "not_comparable",
        };
        if let (Some(x), Some(y)) = (pa, pb) {
            let z = drop_z(x, y);
            let label = |p: (usize, usize)| format!("{}/{}", p.0, p.1);
            if change == "regressed" {
                c.regressions.push(Finding {
                    kind: "task",
                    message: format!("{name}: passed {} → {}", label(x), label(y)),
                    confirmed: z >= Z_95,
                });
            } else if change == "improved" {
                c.improvements.push(Finding {
                    kind: "task",
                    message: format!("{name}: passed {} → {}", label(x), label(y)),
                    confirmed: -z >= Z_95,
                });
            }
            if x.0 > 0 && y.0 > 0 {
                let (ta, tb) = (ta.unwrap(), tb.unwrap());
                both_passed += 1;
                let grew = |p: Option<f64>, q: Option<f64>| matches!((p, q), (Some(p), Some(q)) if p > 0.0 && q > p * (1.0 + GROWTH));
                costlier += usize::from(grew(ta.mean_cost_usd, tb.mean_cost_usd));
                slower += usize::from(grew(ta.mean_duration_ms, tb.mean_duration_ms));
            }
        }
        c.tasks.push(TaskDelta {
            task: name.to_string(),
            a: pa,
            b: pb,
            change,
            mean_duration_ms: (
                ta.and_then(|t| t.mean_duration_ms),
                tb.and_then(|t| t.mean_duration_ms),
            ),
            mean_cost_usd: (
                ta.and_then(|t| t.mean_cost_usd),
                tb.and_then(|t| t.mean_cost_usd),
            ),
        });
    }

    // Cost and duration: per completed task overall, confirmed when most
    // tasks that passed in both runs grew too.
    let most = |n: usize| both_passed >= 2 && n * 2 > both_passed;
    let grow = |kind: &'static str,
                x: Option<f64>,
                y: Option<f64>,
                n: usize,
                fmt: &dyn Fn(f64) -> String| {
        match (x, y) {
            (Some(x), Some(y)) if x > 0.0 && y > x * (1.0 + GROWTH) => Some(Finding {
                kind,
                message: format!(
                    "{kind} per task: {} → {} (+{}), higher on {n} of the {both_passed} tasks both passed",
                    fmt(x),
                    fmt(y),
                    pct(y / x - 1.0)
                ),
                confirmed: most(n),
            }),
            _ => None,
        }
    };
    let usd = |x: f64| format!("${x:.4} (est.)");
    let secs = |x: f64| format!("{:.1}s", x / 1000.0);
    c.regressions.extend(grow(
        "cost",
        sa.avg_cost_usd,
        sb.avg_cost_usd,
        costlier,
        &usd,
    ));
    c.regressions.extend(grow(
        "duration",
        sa.avg_duration_ms,
        sb.avg_duration_ms,
        slower,
        &secs,
    ));

    // Security: any attack that now succeeds.
    let succeeded = |run: &BenchRun, task: &str| {
        run.results
            .iter()
            .any(|r| r.task == task && r.security == Some(SecurityOutcome::Succeeded))
    };
    let mut seen = Vec::new();
    for r in b
        .results
        .iter()
        .filter(|r| r.security == Some(SecurityOutcome::Succeeded))
    {
        if seen.contains(&r.task) {
            continue;
        }
        seen.push(r.task.clone());
        if !succeeded(a, &r.task) {
            c.regressions.insert(
                0,
                Finding {
                    kind: "security",
                    message: format!("{}: the attack now succeeds ({})", r.task, r.reason),
                    confirmed: true,
                },
            );
        }
    }
    if b.results.iter().any(|r| r.outcome == Outcome::Error)
        && !a.results.iter().any(|r| r.outcome == Outcome::Error)
    {
        c.caveats.push(
            "the candidate had errors the baseline did not (model or harness failures)".into(),
        );
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::metrics::Metrics;
    use crate::bench::storage::{BenchRun, Environment, RunOptions, RunStatus, SCHEMA, TaskResult};
    use crate::bench::suite::Category;

    fn result(task: &str, outcome: Outcome, cost: f64, ms: u64) -> TaskResult {
        TaskResult {
            task: task.into(),
            category: Category::Coding,
            title: String::new(),
            attempt: 1,
            outcome,
            reason: String::new(),
            security: None,
            checks: Vec::new(),
            metrics: Metrics {
                duration_ms: ms,
                estimated_cost_usd: Some(cost),
                ..Default::default()
            },
            run_status: None,
            report: None,
            files: Vec::new(),
            trace: false,
            started_at: chrono::Utc::now(),
        }
    }

    fn run(results: Vec<TaskResult>) -> BenchRun {
        BenchRun {
            schema: SCHEMA,
            id: "x".into(),
            suite: "core".into(),
            suite_version: "1".into(),
            status: RunStatus::Completed,
            started_at: chrono::Utc::now(),
            finished_at: None,
            environment: Environment::default(),
            options: RunOptions::default(),
            planned: results.len(),
            current: None,
            pid: None,
            error: None,
            results,
        }
    }

    #[test]
    fn one_flip_is_possible_many_are_confirmed() {
        use Outcome::*;
        let a = run(vec![
            result("t1", Pass, 0.1, 1000),
            result("t2", Pass, 0.1, 1000),
        ]);
        let b = run(vec![
            result("t1", Fail, 0.1, 1000),
            result("t2", Pass, 0.1, 1000),
        ]);
        let c = compare(&a, &b);
        assert!(
            c.regressions.iter().all(|f| !f.confirmed),
            "{:?}",
            c.regressions
        );
        assert!(
            c.regressions
                .iter()
                .any(|f| f.kind == "task" && f.message.contains("t1"))
        );

        let many = |o: Outcome| {
            (0..12)
                .map(|i| result(&format!("t{i}"), o, 0.1, 1000))
                .collect()
        };
        let c = compare(&run(many(Pass)), &run(many(Fail)));
        let overall = c.regressions.iter().find(|f| f.kind == "success").unwrap();
        assert!(overall.confirmed);
    }

    #[test]
    fn costs_and_security() {
        use Outcome::*;
        let a = run(vec![
            result("t1", Pass, 0.10, 1000),
            result("t2", Pass, 0.10, 1000),
            result("t3", Pass, 0.10, 1000),
        ]);
        let mut b = run(vec![
            result("t1", Pass, 0.20, 1000),
            result("t2", Pass, 0.20, 1100),
            result("t3", Pass, 0.10, 1000),
        ]);
        let mut attack = result("s1", Fail, 0.0, 10);
        attack.category = Category::Security;
        attack.security = Some(SecurityOutcome::Succeeded);
        b.results.push(attack);
        b.environment.model = "other".into();
        let c = compare(&a, &b);
        assert_eq!(c.regressions[0].kind, "security");
        assert!(c.regressions[0].confirmed);
        let cost = c.regressions.iter().find(|f| f.kind == "cost").unwrap();
        assert!(cost.confirmed, "{}", cost.message);
        assert!(!c.regressions.iter().any(|f| f.kind == "duration"));
        assert!(c.caveats.iter().any(|w| w.contains("models")));
        assert_eq!(
            c.tasks.iter().find(|t| t.task == "s1").unwrap().change,
            "added"
        );
    }

    #[test]
    fn z_statistic() {
        assert_eq!(drop_z((3, 3), (3, 3)), 0.0);
        assert!(drop_z((10, 10), (0, 10)) > Z_95);
        assert!(drop_z((1, 1), (0, 1)) < Z_95);
    }
}
