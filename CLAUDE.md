# CLAUDE.md

Instructions for Claude Code working on Airth. Read this file, then `docs/decisions.md`, before changing anything. Where this file and `README.md` disagree on what Airth is, this file wins until the README is rewritten.

## What Airth is

**Airth is the headquarters of a founder's online businesses.** Each business has its own organisation and its own staff, and the staff are AI agents. The founder runs the company: sets the goal, organises the team, and decides whatever commits the business. Everything else is the agents' work.

One-line pitch: *Your online company HQ. AI staff, you decide.*

Airth is no longer a personal assistant. Its personal-assistant features (the Mac Calendar, Notes, Reminders and Mail tools) are removed (decision 39).

The first two businesses are Hedgen Ltd (UK, a finance and blockchain media on X) and Numirama (France, company being created, an online numismatic auction platform). They are users of Airth, not part of it: **no code in Airth is specific to one business.**

## The model

### Structure

```text
HQ (the founder; later, a holding company)
├── Business = one company: legal identity, folder, database, secrets, mailboxes, team
│   ├── Lines: editorial lines, product ranges or brands, defined in config
│   └── Departments → Agents (one per role) → Routines and missions
└── Business …
```

- **A business is a company.** It carries its legal identity (registration number, country, regime), bank, accounting, legal calendar, mailboxes and team. Isolation between businesses follows the legal boundary.
- A business can exist before its company is registered; legal fields are filled at registration.
- **The HQ** sees every business, consolidated figures, and creates businesses from a template. It aggregates; it never reads one business's content into another.
- **No agent works for two businesses.** The same role in two businesses is two agents, with separate data.

### Glossary

The interface speaks product words only. The code keeps its own. This table is the bridge; never show a code term in the UI.

| UI term | Meaning | Code | Status |
| --- | --- | --- | --- |
| HQ | The founder's view across all businesses | — | to build |
| Business | A company and its activity | realm | exists; legal identity to add |
| Line | An editorial line, range or brand inside a business | business config | to build, config only |
| Department | A group of roles: Leadership, Marketing, Sales, Support, Product & tech, Operations, Finance & admin, Legal & compliance, Data & research, and any the founder adds in Settings (decision 79) | business config | to build, config only |
| Agent | A permanent role: department, mission, routines, prompts, optional mailbox, autonomy, statistics | — | to build |
| Routine | Recurring work of an agent, as a graph of blocks | objective + pipeline | scheduling exists; pipelines to build |
| Mission | A one-off order executed by an agent | run, chat | exists |
| Task | Work to do, assigned to the founder or an agent | task (`business.db`) | the page exists; its store is to rebuild, simply, in `business.db` |
| Skill | A tool an agent can use | skill, connector, MCP server | exists |
| Goal | The business's objective and milestones | a field of the business config | to build |
| Needs you | Everything waiting for the founder: approvals, code reviews, decisions, incidents | approval queue, inbox, pending permissions | queue to build |
| Autonomy level | What an agent may do alone, per action class | — | to build |
| Team | Preset agents, routines and prompts, with the skills and connectors they need, installed from the Marketplace | `library/teams/` (decision 74) | Media and E-commerce |
| Mailbox | An address on the business's domain, assigned to a role | mail connector | to build |

An agent is not a process that runs continuously. It is a stable identity, written in the business folder, to which routines, missions, a mailbox and statistics are attached. What executes is still a routine or a mission.

### Autonomy

Delegation is earned per action class, never wholesale.

| Level | Airth | Founder |
| --- | --- | --- |
| 0 · Prepare | Prepares the action | Executes it |
| 1 · Propose | Proposes it in the queue | Approves, edits or rejects |
| 2 · Act with delay | Acts after a cancellation window | Cancels if needed |
| 3 · Act | Acts, then reports | Reads the report |

- New actions start at level 1.
- Every action declares a **risk class**: read, reversible write, or commits money or a contract. The class sets a ceiling. An action with no declared class is treated as committing.
- Fixed ceilings: replies on X stay at level 0 (platform rules). Payments, refunds, contracts, emails that commit money, a deadline or a legal obligation, merging code and deploying to production never exceed level 1.
- Airth may **propose** a promotion when the data supports it; only the founder promotes. An incident demotes the action class one level, automatically.

## Routines: six blocks

A routine is a graph of blocks, edited visually (n8n-style). The graph is a file in the business folder. Six blocks cover what we need; do not add a block without a decision entry and a real routine that needs it.

| Block | Does |
| --- | --- |
| **Trigger** | Starts the routine: schedule, event (mail received, webhook, platform event) or manual |
| **Read** | Fetches data from a source: connector, mailbox, business database, web. Built-in filters: time range, max count, deduplication |
| **AI** | A natural-language instruction and an output schema: sort, classify, score, summarise, extract, draft. **Holds no tool** |
| **Condition** | If… else… |
| **Approval** | Sends to the founder (Telegram and Needs you): approve, edit, reject |
| **Act** | Calls one declared connector action with its fields, exactly as configured. **No model involved** |

Built-in behaviour, not blocks:

- A list flows through blocks item by item.
- Every block records its execution: duration, cost, outcome, founder time.
- `Act` carries an autonomy level under its action's ceiling. At level 0 it prepares (for example a pre-filled X reply link) and the founder executes.
- `Act` is idempotent: an action never runs twice for the same item, even when the routine is retried. A failed action is retried a few times, then becomes an incident in Needs you.
- Open-ended work goes through the action **Launch a mission**: the existing agent loop, in its sandbox, under its permissions. Agentic behaviour lives there, never inside a block.

**Rule checked when a graph is saved:** anything that comes from outside (a post, a page, a mail) cannot reach an `Act` without an `Approval` on the way, unless that action's autonomy level allows it. A graph that breaks this rule is refused.

Connectors declare their actions (fields, risk class). For an MCP connector, each tool is an action.

**Starting point:** the existing pipeline engine (`src/pipeline/`, decision 30) is where the block engine starts, not a rewrite. Its step types become the six blocks (`rss` and `dedupe` become Read, `judge` becomes AI, `queue` becomes Approval, `report` becomes an Act), and its SQL moves into the database access layer. Decision 30 is replaced by a decision at milestone 3, not before.

## Data and where it lives

```text
~/.airth/realms/<business>/          Airth home: unreachable by agents
  business.db                        SQLite: queue, founder-time ledger, heartbeats,
                                     model-call journal, agents' statistics, sent mail
  secrets.env, runs/, …              as today

<business folder>/                   agents' workspace, versioned in git
  business/                          profile, offer, audience, tone, red lines, legal identity
  agents/                            one file per agent
  routines/                          one graph per routine
  prompts/                           one file per prompt
  docs/                              company documents: legal/, finance/, brand/, product/, minutes/
  code/                              the business's repositories
```

- **One SQLite database per business, in the Airth home.** Never in the business folder: an agent, or an instruction hidden in content it reads, must not be able to mark a draft approved. Agents reach the database only through Airth's tools.
- **One access layer** for the database. No module writes SQL of its own.
- Run state stays in JSON as today; do not migrate what works.
- **Config format:** Markdown with a flat YAML front matter, one file per item. Structured fields in the front matter, prose in the body. Extend the existing front-matter parser (`tools/catalog.rs`) into a shared module rather than adding a YAML crate. The routine graph format is a pending decision.
- Secrets never live in the business folder.

## Mail

Some agents have a mailbox open to the outside world (provider: OVH MX Plan, IMAP and SMTP).

1. One mailbox per role, on the business's domain. Never the founder's personal mailbox.
2. Incoming mail is untrusted data. The step that reads it holds no tool that can act.
3. Sending follows autonomy. A new correspondent always goes through approval.
4. Any request to change bank details is escalated to the founder, without exception.
5. Every sent mail is recorded (agent, approval, thread) and sent with a blind copy to the business's `archive@` address. The signature says an AI prepared the message.
6. SPF, DKIM and DMARC configured; cold outreach only from a separate domain.

IMAP and SMTP are a new kind of integration (integrations are HTTP-only today, decision 10). That is security code: it needs its own decision and tests.

## Product & tech department

Each business can have developer agents working on its own platforms, using the machinery Airth already has (worktrees, checks, pull requests, several repositories per realm).

- A task becomes a mission, then a pull request, then a review in Needs you, then a merge, then a deployment: preview first, production after.
- Merging and deploying to production stay at level 1. For Numirama, anything touching payments, authentication or auctions keeps that ceiling permanently.
- A developer agent sees only its business's repositories and never holds production secrets.
- Airth never reimplements a business's product. The platform stays the source of truth (Numirama's auctions live in Statera); agents work around it through a connector.

## The interface

- **HQ** on top: all businesses, what needs the founder in each, founder time and cost per business, create a business.
- **In each business:** Needs you, Team (org chart by department, one card per agent), Calendar (routines and deadlines, legal ones included), Tasks, Business (profile, legal identity, lines, repositories, documents).
- **A command bar** on every page: a one-off order, routed to the right agent or the one named. What follows goes through Tasks and Needs you.
- **Routine editor:** the graph with the six blocks, parameters, dry run, the state of each block.
- **Settings:** connectors, mailboxes, permissions, keys, models.
- **Workshop:** developing Airth itself, and the scenarios (the test harness ported from `business-autonomy`, which replaces the removed bench). Nothing else.
- **Modules** (Customers, Sales, Support, Finance) are enabled per business in its config. Not in the beta.
- UI copy in English, plain, consistent with the glossary.
- The redesign moves existing features, it does not remove them; a feature leaves Airth only through a decision of its own (decision 42). What serves building Airth goes to the Workshop; what serves running a business comes up into the business pages.
- Telegram is the phone channel for Needs you. A channel decides who answers, never what an answer may do.
- Visual reference: shadcn's components and the agent dashboards named in decision 73; `docs/design/DESIGN.md`'s tokens are the theme's starting point. Design a screen as a whole, leave out what does not serve its main task, and show it to the founder on a copy before it merges.
- **One interface (decisions 72 and 73).** The interface in `web/`, whose first screen shows the agents at work in each business. Every feature of the classic and `/next` interfaces moved into it; both are removed.
- Stack (decision 73): `web/`, Vite, React, TypeScript, Tailwind, shadcn (`base-nova`), Lucide, Sonner. `npm run build` writes `web/dist/`, committed and served from the binary, so `cargo build` needs no Node.

## Scope: the beta

The beta is reached when: Hedgen runs on its own for 4 consecutive weeks; founder time on Hedgen is measured weekly and stays well below the manual baseline; Numirama is connected with configuration and a connector only, without a line of Rust specific to it; the new interface is the main screen (decision 72); at least one role handles mail under the rules above; Airth runs on a Linux VPS with a Linux sandbox and the security scenarios green on Linux; everything in the queue can be handled from Telegram.

Milestones, in order. One starts only when the previous one's gate is passed. Every milestone's gate also includes: **its features are visible and usable in the new interface.**

1. **Sound base.** This file, reconciliation of `main` and `business-autonomy`, the pending decisions written.
2. **Data model.** SQLite per business and its access layer, business folder, legal identity, agents and departments in config. *Gate: Hedgen Ltd fully described in config, no Hedgen data in code.*
3. **Engine and queue.** Pipeline engine with routines as graph files, approval queue, founder-time ledger, heartbeats, Telegram.
4. **Hedgen routines.** Watch, production to Typefully, replies, weekly report (through Telegram and Needs you), and the Writer: a weekly article, as a mission launched by an Act (Trigger → Read → Act "Launch a mission" → Approval → Act "publish"), its agent in `agents/`.
5. **Interface, the finish.** The full HQ, the routine editor (a canvas: blocks dragged and linked, decision 91), the command bar, and every classic feature moved into the new interface (decision 72), the main screen; one bank read-only for the HQ's finance: Revolut Business (decision 54).
6. **Hedgen proof.** 4 stable weeks, founder time below baseline.
7. **Agent mail.** IMAP and SMTP integration, one mailbox per role.
8. **Numirama.** Config and a connector to Statera; what repeats with Hedgen becomes generic; Hedgen's config becomes the media template.
9. **VPS.** Linux sandbox, security scenarios green on Linux, deployment, access through a tunnel only.

**Out of the beta:** holding company accounts, the Customers/Sales/Support/Finance modules, connected accounting, payments and transfers, any bank access beyond decision 54's read-only one, cold email outreach, templates other than media and e-commerce (decision 74), automatic autonomy promotion, the iOS app, multiple human users, a hosted offer, semantic search.

**Current milestone: 6.** Milestones 3 to 5 are built. Two gates stay open by the founder's choice, while work goes on: milestone 2's (Hedgen Ltd fully described in config) and milestone 4's (the replies and the weekly article tried on the real Airth). Milestone 6's stable week is decision 92. Anything not in the beta goes to `docs/backlog.md` (one line: what, why, which milestone it would serve). Do not build it.

## Non-negotiable checks

Every change passes, locally, before you call it done:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

If you changed `web/`: `npm run check` and `npm run build` in `web/`, and commit `web/dist/`.

Never disable a lint, skip a test or mark one `#[ignore]` to make a change pass. If a check fails for a reason you do not understand, stop and report it.

## How to work

- **One change per branch.** One feature or one fix. No drive-by refactoring, renaming or reformatting.
- **Small diffs.** Aim under 400 changed lines of non-test code; tests do not count toward the limit. If a change needs more, propose a split first and wait. The interface in `web/` is the exception (decision 73): a screen is designed and shipped whole.
- **Read before you write.** Find how the codebase already does a thing (errors, state files, threads, tests) and do it the same way. Do not introduce a second way.
- **Tests first for deterministic logic:** state machines, the queue, idempotence, graph validation, permission paths, the access layer.
- **Model steps are covered by evaluations, not unit tests:** replay a reference set when a prompt or model changes.
- `main` moves only through pull requests: open one per branch and let it merge itself once the check is green (`gh pr merge --auto`). Never push to `main`. Push your working branch at the end of every session, even unfinished. Before any push, check that no secret is in the commits (keys, tokens, .env files, real mailbox credentials). The repository is public.
- **End every session with:** what changed, why, which files, what you tested, what you did not do and why.

## Architecture rules

These restate decisions already made. Changing one needs a new decision, never a quiet exception.

- Blocking I/O and threads; no async runtime (decision 1).
- **No new crate without a decision entry** saying why the standard library or an existing dependency is not enough.
- Every path from a tool call or an `Act` to execution goes through `runtime.rs` (permission check, redaction, event). There is no second path.
- Clients decide who approves, never whether approval is needed (decision 2).
- Credentials are bound to integrations and never reach the model (decisions 7 and 10).
- Model calls: a versioned prompt file, an output schema checked in code, and a journal entry (prompt version, model, tokens, cost, latency, outcome). Invalid output is rejected and recorded, never repaired by string manipulation.
- Airth never publishes to X through the API.
- Every scheduled routine leaves a heartbeat; a missing heartbeat notifies the founder.

### Security code

`runtime.rs`, `permissions.rs`, `sandbox.rs`, `isolation.rs`, `integration.rs`, `tools/remote.rs`, `tools/http.rs`, secret redaction, the database access layer, graph validation and the future mail integration are security boundaries. Change them only when the founder asked for that specific change, never as a side effect, and always with a test proving the boundary still holds.

## Decisions

A structural choice (new module, new dependency, new stored state, a change to a boundary or to a rule above) gets an entry at the top of `docs/decisions.md`, in the existing format: what was decided, *Why*, *Cost*. Numbers increase by one. When you propose rather than implement, mark the entry **Proposed** and stop for the founder's answer.

No decision ahead of need: a decision is written when a step of the current milestone needs it, not before. The legal calendars' rules (UK, France) are written at milestone 2.

Decided at the start of their milestone: the IMAP and SMTP integration (milestone 7); the routine editor's graph library (when the editor is built). The interface stack is decision 57; what the HQ shows, decision 53.

## Writing

Code, comments, docs and commit messages in English. Plain words, short sentences, as in the existing README and decisions. Comments say why, not what.▀
