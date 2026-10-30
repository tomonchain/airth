# Reconciling `main` and `business-autonomy`

Status: accepted by the owner on 30 October 2026. Each port below is its own change, on its own branch.

`main` is at `dbaee12` (0.1.5, plus the 15a/15b rename), with the owner's uncommitted work: decisions 25–30, `CLAUDE.md`, and a cost-by-purpose change to usage (`usage.rs`, `state.rs`, `agent.rs`, palette). `business-autonomy` forked from `b7d70f0` and holds 22 commits (`31649be` to `3751ecb`): 104 files, +15,446 / −13,561 lines.

Default rules applied, as given by the owner:
- Hedgen is a finance and on-chain research media on X. The due diligence framing is obsolete.
- Decisions 25–29 of `main` keep their numbers. Branch decisions that are kept are renumbered after 30.
- SQLite is kept for business state if it is clean and tested.
- Branch code is sorted into what serves the seven pilot steps (keep) and the rest (backlog).

**Proposed overall direction: `main` is the base. The branch is not merged.** What is kept is ported to `main` in small, separate changes (under 400 lines each, per `CLAUDE.md`). The branch stays as a reference until every port is done, then it is deleted.

## 1. Decisions

| Branch | Title | Proposal |
|---|---|---|
| 25 | Business records live in SQLite; what is delegated is read again before every action | **Keep the storage half as 31** (see §2). Drop the mandate half (see §4). |
| 26 | A business is run by a steward, missions and verifiers that are other runs | **Drop.** Contradicts `main` 26: the model sequences the work. Backlog. |
| 27 | The coding and personal-goal features are removed | **Drop.** Contradicts `main` 25: existing features stay as they are during the pilot. |
| 28 | A target's figure is read from its source by Airth's code | Backlog (no pilot step needs Stripe or Qonto figures). |
| 29 | Daily limits per service, enforced by the permission gate | Backlog. It changes security code for a need the pilot does not have (Airth never posts; Typefully only receives drafts). |
| 30 | The owner is reached on Telegram, through a bot of their own | **Superseded by `main` 28.** The code is reused (§4), but adapted to 28: the token becomes an integration credential bound to `api.telegram.org`; buttons become Approve / Edit / Reject on drafts. |
| 31 | Earlier work can be read and exported | **Drop.** It only exists because branch 27 removed those features. |
| 32 | Airth opens on the portfolio; the owner's time is measured | **Superseded by `main` 27.** The pilot needs one record per interaction (what was shown, the answer, the delay, the edit), not minutes counted while the app is open. |
| 33 | The weekly report is written by code and sent through Resend | **Keep as 32**, rewritten for the pilot's report (step 7). |
| 34 | The bench comes back as the spec's acceptance scenarios | **Keep the harness as 33**: throwaway home, scripted model, local stand-ins for outside services. It becomes how pipelines are tested. The A-01…A-25 catalogue (written for the due diligence spec) goes to the backlog. |

Resulting order on `main`: 25–29 (unchanged), 30 (pipelines, Proposed), 31 (SQLite for business state), 32 (weekly report by email), 33 (pipeline test harness). Each is written fresh against the pilot, not copied.

## 2. Storage

**`main`:** "State is JSON in the Airth home, written atomically. No database for the pilot." (`CLAUDE.md`)

**Branch:** one `business.db` per realm, through `rusqlite` with the `bundled` feature (no system SQLite needed). Run state, settings and realm config stay JSON.

**Assessment: clean and tested.**
- **Transactions:** every write is one transaction taken with `BEGIN IMMEDIATE`, with WAL and a 10 s busy timeout.
- **Migrations:** schema migrations are versioned. A database newer than the binary is refused, not overwritten.
- **Records:** each one carries a revision, and an update with a stale revision fails instead of overwriting.
- **Audit:** an audit table is append-only (enforced by trigger).
- **Tests:** migrating once, append-only audit, refusing a newer schema, and concurrent writers not losing updates (`business/mod.rs`, 4 tests). The record layer is exercised by `model.rs` (8 tests) and the hub's business tests (12).

**Proposal: keep it for business state only.**
- **What goes in `business.db`:** the draft queue and its state machine, the owner-time ledger, heartbeats, and the record of model calls (prompt hash, model, tokens, cost estimate, latency, outcome).
- **What stays JSON:** run state, objectives, settings and memory, as today.
- **Why:**
  - the draft queue must never publish twice, even when a retried job and a phone answer arrive at once, and a transaction makes that a property of the store rather than of each caller;
  - the model-call record is the evaluation dataset; it grows with every call and must be queried (by prompt version, by outcome), which append-only JSON does poorly.

**Proposed `CLAUDE.md` change** (Architecture rules), replacing "State is JSON in the Airth home, written atomically (decision 1). No database for the pilot.":

> - Run state, objectives, settings and memory are JSON in the Airth home, written atomically (decision 1).
> - Business state (draft queue, owner-time ledger, heartbeats, model-call records) lives in the realm's `business.db`, SQLite (decision 31). Every write is one transaction; schema changes are numbered migrations with a test. No other database.

`rusqlite` is the one new crate, justified by decision 31 (`CLAUDE.md`: no new crate without a decision).

## 3. What Hedgen is

**`main`:** a finance and on-chain research media on X. The pilot is posts, a weekly thread, replies and a report (`CLAUDE.md`, decisions 25–29).

**Branch:** due diligence reports on crypto protocols, sold to funds (spec v0.4, the handoff, the mockups, the P0 questionnaire). The live Hedgen realm was set up that way (§6).

**Proposal: the media framing holds.** These documents become obsolete:
- `docs/SPECS_BUSINESS_AUTONOMY.md`
- `docs/HANDOFF_CONCEPTION.md`
- `docs/SPECS_AMENDEMENT_APPRENTISSAGE.md`
- `docs/design/`
- `docs/P0_HEDGEN_QUESTIONNAIRE.md`
- `docs/P0_AUDIT.md`

They stay on the branch only and are not ported. Ideas from them that may matter after the pilot, such as graduated autonomy (amendment), owner time as the success measure (v0.4 §2.2), and proposals with a deadline and a fallback (v0.4 §10.2), get one backlog line each.

## 4. Code

### Serves the pilot: port to `main`

| Pilot step | From the branch | What changes in the port |
|---|---|---|
| Foundation | `business/mod.rs`, `records.rs`: SQLite store, migrations, revisions, audit (~700 lines) | First port, alone, with its tests. Schema restarted at version 1 for the pilot's tables. |
| 1. Business memory | `Profile` and its `links` (`business/model.rs`); `brief::overview`, which puts them in a run's context | Only the profile and links. Add the archive of past posts. Make the overview available to every run of the realm, not only to business runs. |
| 2. Owner-time ledger | `owner_time.rs` (week totals, time added by hand) | Becomes the per-interaction ledger of `main` 27, in `business.db`. Week totals and hand-added time are kept. |
| 3. Phone approvals | `telegram.rs`: pairing by one-time code, only the paired chat is honoured, outbound polling, a tap applied like a click, stale buttons refused | Adapted to `main` 28: token as an integration credential; Approve / Edit / Reject on drafts; every answer through the queue's state machine and the ledger. |
| 5. Production | `business/actions.rs`: journal of calls to other systems; a call with an unknown outcome is never repeated blindly | Used for the Typefully send, so a retried job never sends a draft twice. |
| 7. Weekly report | `report.rs`: written by code, sent once per week with the week as idempotency key, bounded retries, a log of attempts | New content (output, engagement, owner time, share of drafts sent unedited). Sending and its guarantees kept. |
| Testing | `scenarios.rs`: throwaway home, scripted model, local HTTP stand-ins | Kept as a test harness for pipelines (`#[cfg(test)]` or a test-support module), without the A-xx catalogue. |
| Runtime | `Hub::with_model` and `NoObserver` made non-test | Port only if the harness needs them outside `#[cfg(test)]`. |

Steps 4 (daily watch) and 6 (replies radar) have nothing on the branch.

### Not for the pilot: backlog

One line each in `docs/backlog.md`, with the step it could help:
- **Steward, missions, verifiers, proposals:** `business/brief.rs` (steward and mission briefs), `business/tool.rs`, `hub/business.rs`, and most of `model.rs`.
- **Mandate versions and daily limits**, with their changes to security code: `permissions.rs` +286 lines, `runtime.rs` +43, `tools/http.rs` +38. **These are not ported.** Security code is changed only on the owner's specific request (`CLAUDE.md`).
- **Targets read from Stripe or Qonto:** `reading.rs`.
- **Onboarding from an idea:** `onboard.rs`.
- **The redesigned app:** portfolio, business space, decision, case and setup pages, links page, rail, colours (`ui/portfolio.rs`, `ui/business.rs`, `10-portfolio.js`, `12-business.js`, `13-detail.js`). The pilot needs an approval queue and a report page, not these screens. The colour tokens could be reused later.
- **Acceptance scenarios A-01…A-25** and the setup checklist.
- **Earlier work, read and export:** `legacy.rs`.
- **The `airth business` command line** (`business/cli.rs`).

### Removed on the branch: not carried over

The branch deleted these modules; `main` keeps them, unchanged:
- `isolation.rs`, `github.rs`
- `northstar.rs`, `projects.rs`, `tools/northstar.rs`
- `hub/changes.rs`, `hub/delivery.rs`, `hub/repos.rs`
- `src/bench/`, `ui/bench.rs`

The branch also made the scheduler fire only the business steward, so objectives no longer started (`scheduler.rs`, `objective.rs`). That contradicts `main` 26, where pipelines are started by objectives, so it is not carried over either.

### Conflicts with the owner's uncommitted work on `main`

The branch deleted `bench/metrics.rs` and `hub/delivery.rs`, which `main`'s uncommitted work touches. Since `main` is the base, this disappears. The cost-by-purpose usage work (`Purpose` on each usage record, cost estimates by model) is what the model-call record of step 2 can build on.

## 5. A pipeline or business execution on the branch, and decision 30

**The branch has a business execution mechanism, but not a pipeline.** It works like this:
- the scheduler starts a *steward* (an agent run) on a schedule or when events arrive;
- the steward decides *missions* with acceptance criteria;
- each mission is carried out by another agent run and accepted or refused by a third (a verifier);
- all of it runs under a token reservation per day and per run, and resumes after a restart.

This is the model sequencing the work: what `main` 26 rules out for routines. It confirms option B rather than changing it.

It also adds three deterministic, scheduled jobs that are pipelines in all but name:
- reading figures hourly (`hub::take_readings`);
- the weekly report (`report::tick`);
- the phone push (`telegram::push`).

Each runs from the background loop, in code, with no model. Their patterns give decision 30 concrete pieces:
- **idempotency key and bounded retries:** from the report;
- **journal of external calls:** before the Typefully send;
- **heartbeat:** the background loop's last verified tick, adapted per pipeline;
- **phone answers applied like clicks, stale ones refused:** from the Telegram push.

**What this changes in decision 30:**
- **Storage:** with §2 accepted, the pipeline's queue, heartbeats and call records go to `business.db` instead of JSON files.
- **The way it starts:** the steward's path (`business_due`) is not the way to start pipelines. Objectives are, as 30 proposes, which means restoring `main`'s scheduler behaviour, which `main` already has.

The owner's amendment to 30 (business-specific prompts and settings in the realm's folder, versioned in git; prompt version = content hash) is not affected by this reconciliation and is written into 30 once this document is accepted.

## 6. What the service runs

**Now:**
- `~/Applications/Airth.app` runs the branch build (`3751ecb`), installed from the `airth-autonomy` worktree. Its update check follows that worktree, not `main`.
- The Hedgen realm is set up as a due diligence business: a profile, rules v1, four targets, a steward every 2 hours, a 3M tokens a day budget.
- Since 29 October it has used about 4.5M tokens on 28 runs. Of 13 missions, 1 succeeded, 6 failed, 1 was cancelled and 5 are queued.
- **It is still running, on the obsolete framing.**

The home also holds files only the branch writes (`main` ignores them):
- `telegram.json`
- `owner-time.json`
- `report.json`
- `scenarios.json`
- `realms/hedgen/business.db`

**Proposal, in this order:**
1. **Now, owner's go:** pause Hedgen's operations (one click, Pause, on its page), so it stops spending tokens on due diligence work.
2. **Reinstall `main`'s build** (0.1.5 plus the uncommitted work once committed) with `airth service install` from the main checkout, so updates follow `main`. The backup `~/Applications/Airth-0.1.5-backup.app.bak` exists if needed.
3. **Keep the Hedgen data, marked as obsolete:** move `realms/hedgen/business.db` to `business-dd-2026-10.db.bak`. The pilot's foundation port then creates a fresh `business.db` with its own schema. Nothing is deleted.
4. Leave `telegram.json` in place until the Telegram port. The bot and its pairing can be reused if the port reads that file, or redone in two minutes.
