# Reconciling `main` and `business-autonomy`

Status: rewritten on 1 November 2026 against the new `CLAUDE.md` (a business is a company, one SQLite database per business in the Airth home, routines as graphs of six blocks, agents in `agents/`). It replaces the version of 30 October, written for the Hedgen pilot's seven steps.

`business-autonomy` forked from `b7d70f0` (0.1.5) and holds 21 commits. It is not merged: `main` is the base, and what is kept is ported in small, separate changes, each at the milestone that needs it. The branch stays as a reference until every port is done, then it is deleted.

Verdicts: **keep** (port as it is, or already ported), **adapt** (the idea or the code serves, in another shape), **backlog** (outside the beta, one line in `docs/backlog.md`), **drop**.

## 1. The branch's decisions

| Branch | Title | Verdict | Why |
|---|---|---|---|
| 25 | Business records live in SQLite | **Keep** (done: `main` 31) | Exactly the rule of `CLAUDE.md`: one database per business, in the Airth home. |
| 25 | What is delegated is read again before every action | **Adapt** | Becomes "read the autonomy level again before every Act". An input to the pending decision on autonomy classes and ceilings. |
| 26 | A business is run by a steward, missions and verifiers | **Backlog** | The model sequences the work. In `CLAUDE.md` the graph sequences it, and a mission is one action ("Launch a mission"). |
| 27 | The coding and personal-goal features are removed | **Drop** | The Product & tech department needs worktrees, changes and repositories. |
| 28 | A target's figure is read from its source by Airth's code | **Backlog** | Connected accounting and banking are outside the beta. |
| 29 | Daily limits per service, enforced by the permission gate | **Adapt** | To be proposed in the decision on risk classes and ceilings (milestone 1). It changes security code, so it is ported only when the founder asks for that change. |
| 30 | The owner is reached on Telegram, through a bot of their own | **Keep**, adapted to `main` 28 | Milestone 3. The beta requires that everything in the queue can be handled from Telegram. |
| 31 | Earlier work can be read and exported | **Drop** | It existed for features the branch removed. Exporting `business.db` stays due (`main` 31, *Cost*), see §2. |
| 32 | Airth opens on the portfolio; the owner's time is measured | **Adapt** | The portfolio becomes the HQ (milestone 5). Time is measured by the ledger of `main` 27, already on `main`. |
| 33 | The weekly report is written by code and sent through Resend | **Adapt** | Milestone 4. Written by code, as on the branch; delivered through Telegram and Needs you. **No Resend.** |
| 34 | The bench comes back as acceptance scenarios | **Keep the harness** | Throwaway home, scripted model, local stand-ins for outside services. It replaces the removed bench: routine dry runs, the Workshop's scenarios, and the security scenarios that gate the VPS (milestone 9). The A-01…A-25 catalogue goes to the backlog. |

## 2. The branch's code

| From the branch | Verdict | Milestone | What changes |
|---|---|---|---|
| `business/mod.rs`, `records.rs`: SQLite store, migrations, revisions, audit | **Keep** (done: `business/store.rs`) | 2 | Adapt: the SQL written in `pipeline/run.rs` and `pipeline/schedule.rs` moves into it, so there is one access layer. |
| `business/model.rs`: `Profile` and its links | **Adapt** | 2 | Becomes the profile and legal identity in the business folder's `business/`. |
| `business/brief.rs`: overview in a run's context | **Keep** (done: `business::briefing`) | — | |
| `owner_time.rs` | **Keep** (done: `business/time.rs`) | 3 | The baseline is missing; the founder states it. |
| `telegram.rs`: pairing code, only the paired chat, outbound polling, stale buttons refused | **Keep** | 3 | Token as an integration credential (`main` 28); Approve / Edit / Reject on the queue; every answer through the queue's state machine and the ledger. |
| `business/actions.rs`: journal of external calls; an unknown outcome is never repeated blindly | **Keep** | 3 | It is the Act block's idempotence. |
| `report.rs`: idempotency key per week, bounded retries | **Keep** | 4 | New content (output, engagement, founder time, cost). Delivered through Telegram and Needs you. |
| `scenarios.rs`, `Hub::with_model`, `NoObserver` | **Keep** | 3, then 9 | The test harness for routines, then the security scenarios on Linux. |
| `business/mandate.rs`, and its changes to `permissions.rs` (+286), `runtime.rs` (+43), `tools/http.rs` (+38) | **Adapt** | 1, then 3 | Input to the decision on autonomy and risk classes. Ported only on the founder's specific request, with tests. |
| `business/cli.rs` | **Adapt** | 2 | Inspecting and exporting `business.db`. |
| `ui/portfolio.rs`, `10-portfolio.js` | **Adapt** | 5 | Reference for the HQ, once the interface stack is decided. |
| `ui/business.rs`, `12-business.js`, `13-detail.js` | **Adapt** | 5 | Reference for Needs you, Team and Business. |
| `business/onboard.rs`: a business from an idea | **Backlog** | 8 | Could seed business templates. |
| `business/tool.rs`, `hub/business.rs`: the steward | **Backlog** | — | |
| `business/reading.rs`: Stripe and Qonto | **Backlog** | — | |
| `legacy.rs` | **Drop** | — | |
| The scheduler firing only the steward | **Drop** | — | Routines are started by their trigger. |

**Removed on the branch, kept on `main`:** `isolation.rs`, `github.rs`, `hub/changes.rs`, `hub/delivery.rs`, `hub/repos.rs`. The Product & tech department uses them.

**Removed on both:** the bench (`main` 32, commit `7cae00f`). The branch's harness replaces it (§1, branch 34).

## 3. Documents

The due diligence documents (`docs/SPECS_BUSINESS_AUTONOMY.md`, `docs/HANDOFF_CONCEPTION.md`, `docs/SPECS_AMENDEMENT_APPRENTISSAGE.md`, `docs/design/`, `docs/P0_HEDGEN_QUESTIONNAIRE.md`, `docs/P0_AUDIT.md`) are **dropped**: Hedgen is a finance and blockchain media on X. Their ideas that last (graduated autonomy, the founder's time as the measure, proposals with a deadline) are now in `CLAUDE.md`.

## 4. What the service runs

Done on 30 October: the branch's Hedgen realm was archived (`~/airth-archive-2026-10-30`), the realms were cleared, and the service runs `main`'s build. Nothing of the branch runs.
