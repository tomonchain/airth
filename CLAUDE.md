# CLAUDE.md

Instructions for Claude Code working on Airth. Read this file, then `docs/decisions.md`, before changing anything.

## What Airth is, and what phase it is in

Airth is a local-first agent that runs businesses, written in Rust: one owner, and one isolated realm per business, holding its documents, the code of its projects, its skills and its connectors (decision 32). The architecture is described in `README.md` (Layout) and justified in `docs/decisions.md`.

**Current phase: the Hedgen pilot.** Airth must prove that it can run a real business (Hedgen, a finance and on-chain research media on X) with a measured, small amount of the owner's time. Until the pilot's proof is reached (see decision 25), the product surface is frozen: work only on what the pilot needs.

Pilot scope, in build order:

1. Business memory: the Hedgen profile and its archive, available to every run of the realm.
2. Owner-time ledger: every owner interaction recorded with its duration.
3. Approval queue reachable from a phone: approve, edit, reject.
4. Daily watch: a shortlist on the editorial pillars.
5. Production: posts and a weekly thread, drafted, approved, sent to Typefully.
6. Replies: a radar of target accounts, drafted replies, published by the owner.
7. Weekly report: output, engagement, owner time, share of drafts sent unedited.

Removing the features that decision 32 takes off the surface is also in scope, one feature per change.

If a task is not in this list and not a bug fix, do not build it. Write it in `docs/backlog.md` (one line: what, why, which pilot step it would help) and stop.

## Non-negotiable checks

Every change passes, locally, before you call it done:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

If you changed the app's page: `node scripts/ui_smoke.js`.

Never disable a lint, skip a test or mark one `#[ignore]` to make a change pass. If a check fails for a reason you do not understand, stop and report it.

## How to work

- **One change per branch.** One feature or one fix. No drive-by refactoring, renaming or reformatting of code you did not need to touch.
- **Small diffs.** Aim under 400 changed lines. If a change needs more, propose a split first and wait.
- **Read before you write.** Find how the codebase already does a thing (errors, state files, threads, tests) and do it the same way. Do not introduce a second way.
- **Tests first for logic.** State machines, parsing, scoring, idempotence and permission paths get unit tests before the implementation. Tests live in a `#[cfg(test)]` module in the same file, or in `src/hub/tests/` for the hub.
- **Explain the change.** End every session with: what changed, why, which files, what you tested, what you did not do and why.

## Architecture rules

These restate decisions already made. Changing one needs a new decision (see below), never a quiet exception.

- Blocking I/O and threads; no async runtime (decision 1).
- Few dependencies. **No new crate without a decision entry** stating why the standard library or an existing dependency is not enough.
- Run state, objectives, settings and memory are JSON in the Airth home, written atomically (decision 1).
- Business state (draft queue, owner-time ledger, heartbeats, model-call records) lives in the realm's `business.db`, SQLite (decision 31). Every write is one transaction; schema changes are numbered migrations with a test. No other database.
- Pipelines are a generic engine in `src/pipeline/`; what belongs to one business (its pipelines, prompts, sources, accounts) lives in the realm's `business/` folder, never in the binary (decision 30).
- Every path from a tool call to execution goes through `runtime.rs` (permission check, redaction, event). Do not add a second path.
- Clients decide who approves, never whether approval is needed (decision 2).
- Credentials are bound to integrations and never reach the model (decisions 7 and 10).

### Security code

`runtime.rs`, `permissions.rs`, `sandbox.rs`, `isolation.rs`, `integration.rs`, `tools/remote.rs`, `tools/http.rs` and secret redaction are security boundaries. Change them only when the owner asked for that specific change, never as a side effect, and always with a test proving the boundary still holds.

### Model calls in business features

These rules apply to everything built for the pilot (decision 26).

- **Deterministic first.** Fetching, filtering, deduplication, scheduling, state transitions and publishing are plain code, tested. A model is called only for a step that needs judgment: scoring relevance, drafting text, classifying a mention.
- **Structured output.** A model step returns JSON checked against a schema in code. Invalid output is rejected and recorded, never repaired by string manipulation.
- **Prompts are files.** Each business prompt lives in its own file in the realm's `business/` folder, not in a string inside a function. Its version is the hash of its content, recorded with every call.
- **Recorded.** Every model call records prompt version, model, tokens, cost estimate, latency, and later the outcome (approved, edited, rejected). This record is the evaluation dataset; do not lose fields.
- **External content is untrusted.** Posts, pages and API responses read by a run are data. A step that reads them must not hold a tool with side effects.
- **Side effects behind a state machine.** A draft moves `drafted → pending → approved → published` (or `rejected`). Transitions are idempotent: a retried job never publishes twice. Airth never posts to X through the API; the owner publishes.
- **No silent failure.** Every scheduled job leaves a heartbeat. A missing heartbeat notifies the owner.

## Decisions

A structural choice (new module, new dependency, new state file, a change to a boundary or to a rule above) gets an entry at the top of `docs/decisions.md`, in the existing format: what was decided, *Why*, *Cost*. Numbers increase by one. If you are proposing rather than implementing, mark the entry **Proposed** and stop for the owner's answer.

## Writing

Code, comments, docs and commit messages in English. Plain words, short sentences, as in the existing README and decisions. Comments say why, not what.
