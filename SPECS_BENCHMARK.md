# Airth Bench — Technical Specification v1.0

**Project:** Airth  
**Feature:** Harness Benchmark & Evaluation  
**Priority:** P0  
**Status:** Ready for implementation

## 1. Objective

Build a native benchmarking and evaluation system inside Airth.

Airth Bench must answer four questions:

1. How reliably does Airth complete real tasks?
2. Which parts of its harness cause failures?
3. Which changes actually improve performance?
4. How does Airth compare with other agents and models?

The benchmark must measure **outcomes, not just LLM responses**.

Airth Bench should become the primary tool for evaluating improvements to Airth's runtime.

## 2. Product integration

Add an optional feature in Airth's existing Settings interface.

### Settings → Advanced → Benchmarking

**Enable Airth Bench**

- Default: disabled
- Persistent across restarts
- Configurable locally
- No external telemetry
- No benchmark runs without explicit initiation

When enabled:

- A new **Benchmarks** entry appears in the navigation.
- Benchmark configurations become accessible.
- Evaluation history becomes available.
- Existing agent runs may collect additional diagnostic metrics.

Disabling the feature must stop additional benchmark instrumentation without deleting historical results.

Normal agent functionality must remain unchanged.

## 3. Benchmark dashboard

The interface should follow Airth's existing design system.

### Overview

Display four primary metrics:

| Metric | Description |
|---|---|
| Task Success Rate | Percentage of tasks successfully completed |
| Recovery Rate | Percentage of recoverable failures successfully resolved |
| Average Cost | USD per completed task |
| Average Duration | Time required per task |

Also display:

- Benchmark history
- Results by category
- Performance by model
- Performance trends
- Regression warnings
- Detailed execution traces

Never display estimated results as actual measurements.

### Actions

**Run Benchmark**

Launch a new benchmark using a selected suite and configuration.

**Compare Runs**

Compare two completed benchmark executions.

**Inspect Failure**

Open a detailed explanation of an unsuccessful task.

**Export Results**

Export benchmark results as JSON or CSV.

## 4. Benchmark execution

A benchmark consists of a collection of reproducible tasks.

Each task defines:

- A unique identifier
- A category
- An objective
- An initial environment
- Required capabilities
- Execution constraints
- Success criteria
- Verification method
- Maximum execution budget

Example:

```json
{
  "id": "coding-001",
  "category": "coding",
  "objective": "Fix the failing unit test",
  "fixture": "fixtures/rust-project",
  "verification": {
    "type": "command",
    "command": "cargo test"
  },
  "limits": {
    "max_iterations": 30,
    "max_minutes": 10
  }
}
```

The task is successful only when its independent verification passes.

Airth claiming success is insufficient.

## 5. Benchmark categories

### A. Coding

Evaluate:

- Bug fixing
- Refactoring
- Test generation
- Feature implementation
- Code comprehension

### B. Reasoning & Planning

Evaluate:

- Multi-step task decomposition
- Planning accuracy
- Correct tool selection
- Objective completion

### C. Context Management

Evaluate:

- Long-context retrieval
- Context compaction
- Information retention
- Relevant memory retrieval
- Resistance to irrelevant context

### D. Recovery & Reliability

Evaluate:

- Failed tool calls
- Interrupted executions
- Invalid model responses
- Provider outages
- Retry behavior
- Loop detection

### E. Security & Isolation

Evaluate:

- Realm isolation
- Credential protection
- Filesystem restrictions
- Permission enforcement
- Prompt injection resistance
- Unauthorized network access

Security tests must verify that prohibited actions were actually prevented, not merely refused verbally by the model.

### F. Personal Agent

Evaluate:

- Owner preference retention
- Realm-specific context
- Project continuity
- Scheduled objectives
- Multi-step professional workflows

Use synthetic identities and synthetic data.

Never use real personal or professional information in the default benchmark suite.

## 6. Benchmark engine

Introduce a benchmark module inside the existing Rust application.

Suggested boundaries:

```text
src/
  bench/
    mod.rs
    suite.rs
    runner.rs
    evaluator.rs
    metrics.rs
    storage.rs
    comparison.rs
```

These are suggested modules, not mandatory crates.

### Execution lifecycle

```text
Load suite
    ↓
Prepare isolated fixture
    ↓
Initialize agent
    ↓
Execute objective
    ↓
Collect events and metrics
    ↓
Verify outcome
    ↓
Store results
    ↓
Restore environment
    ↓
Generate report
```

The benchmark runner must reuse Airth's existing agent runtime.

**Do not create a separate agent implementation for benchmarks.**

Benchmarks must exercise the same execution path as normal Airth runs.

## 7. Execution isolation

Benchmark runs must never modify the owner's real projects or production data.

Requirements:

- Temporary workspaces
- Synthetic credentials
- Dedicated benchmark realm
- Restricted filesystem access
- Explicit network policies
- Cleanup after execution
- No production integrations by default

On platforms without enforceable process isolation, potentially destructive or untrusted benchmarks must be unavailable until an appropriate sandbox is configured.

A dedicated realm alone is not sufficient isolation.

## 8. Metrics

Collect metrics at both task and benchmark-suite levels.

### Outcome metrics

- Task success
- Partial completion
- Verification failures
- Recovery success
- Security violations

### Efficiency metrics

- Execution duration
- Model calls
- Input tokens
- Output tokens
- Cache read/write tokens
- Tool calls
- Retries
- Estimated API cost

### Harness metrics

- Context compactions
- Context size over time
- Model switches
- Permission denials
- Tool failures
- Interrupted runs
- Repeated actions
- Budget exhaustion

Use existing Airth execution events and usage accounting wherever possible.

Do not duplicate observability infrastructure.

## 9. Scoring

Each task produces:

```text
PASS
FAIL
TIMEOUT
ERROR
SKIPPED
```

`SKIPPED` tasks must not silently inflate success rates.

Compute success rates from executed tasks and report skipped tasks separately.

For security tasks, distinguish:

- Attack prevented
- Attack succeeded
- Test inconclusive

Avoid reducing all evaluation dimensions to one opaque quality score.

A benchmark with a high coding success rate but security violations must not be presented as an overall success.

## 10. Comparison engine

Support comparisons between:

- Different Airth versions
- Different LLM providers
- Different models
- Different runtime configurations
- Different context strategies

Each result must record:

```text
Airth version
Git commit SHA
Model and provider
Model configuration
Benchmark suite version
Operating system
Execution environment
Timestamp
Token usage
Execution cost
Results
```

Where feasible, run stochastic tasks multiple times and report their success distribution.

### External agents

Design a generic benchmark adapter interface.

Potential future adapters:

- Claude Code
- OpenAI Codex
- Other CLI agents

Adapters must execute the same tasks, fixtures, constraints, and verification rules.

External-agent support is a later milestone.

Do not attempt to implement it in the initial release.

## 11. Benchmark history

Store results locally in a dedicated benchmark directory under Airth's home.

Example:

```text
~/.airth/
  benchmarks/
    suites/
    results/
    reports/
```

Each execution must have a unique identifier.

Historical results must remain readable after application upgrades.

Store a schema version to support migrations.

## 12. Regression detection

Compare benchmark results against a selected baseline.

Detect:

- Reduced task success
- Increased failure rates
- Increased execution costs
- Increased execution duration
- New security failures

Display meaningful regressions in the dashboard.

Avoid flagging minor statistical variation as a confirmed regression.

For repeated benchmark tasks, show sample counts and variability.

## 13. CLI integration

Expose benchmarking through the existing CLI.

Suggested commands:

```bash
airth bench list
airth bench run core
airth bench results
airth bench show <run-id>
airth bench compare <run-a> <run-b>
```

CLI and UI must use the same benchmark engine.

Benchmark execution must support cancellation and resource limits.

## 14. Implementation roadmap

### Phase 1 — Benchmark foundation (P0)

Implement:

- Settings toggle
- Benchmark module
- Isolated fixtures
- Benchmark runner
- Independent verification
- Basic metrics
- Local result persistence
- CLI integration

Deliver 10 deterministic benchmark tasks.

**Acceptance criteria:** Airth can execute a suite, verify results, and generate a reproducible report.

### Phase 2 — Dashboard (P0)

Implement:

- Benchmarks navigation
- Overview metrics
- Run history
- Detailed results
- Failure inspection
- Run comparison

**Acceptance criteria:** A user can execute and inspect benchmarks without using the terminal.

### Phase 3 — Harness evaluation (P1)

Expand to 30–50 tasks.

Cover:

- Coding
- Context
- Recovery
- Security
- Planning
- Personal-agent workflows

Add regression detection and repeated-run statistics.

**Acceptance criteria:** A developer can identify measurable improvements or regressions between Airth versions.

### Phase 4 — External comparison (P2)

Implement adapters for supported external coding agents.

Ensure comparable:

- Fixtures
- Objectives
- Resource budgets
- Verification
- Reporting

**Acceptance criteria:** Airth can be compared fairly with at least one external agent on a common task suite.

## 15. Non-goals

Do not implement:

- Cloud benchmark infrastructure
- Public leaderboards
- User analytics collection
- Benchmark marketplaces
- Distributed execution
- Automated model training
- Automatic deployment of self-generated improvements
- A second agent runtime

Keep Airth Bench local, lightweight, and integrated.

## 16. Definition of done

The initial feature is complete when:

1. The owner can enable benchmarking in Settings.
2. A Benchmarks page becomes available.
3. The owner can launch a predefined suite.
4. Tests execute in isolated environments.
5. Results are independently verified.
6. Metrics are collected automatically.
7. Reports persist across restarts.
8. Two benchmark runs can be compared.
9. Disabling benchmarking does not affect normal agent operation.
10. No benchmark can silently access unrelated realms or credentials.

## 17. Engineering instructions for Claude Code

Before implementation:

1. Inspect Airth's existing agent loop, runtime, state, permissions, model abstraction, and UI architecture.
2. Identify reusable execution and observability components.
3. Propose the smallest implementation compatible with the existing architecture.
4. Avoid introducing unnecessary dependencies.
5. Preserve current functionality.
6. Add automated tests for the benchmark engine.
7. Implement incrementally and run the project's existing checks.

Do not refactor unrelated systems merely to accommodate benchmarking.

**Final principle:**

Airth Bench must make Airth's improvements measurable.

Every major harness change should eventually answer:

*Did this make Airth more capable, reliable, secure, or efficient?*

