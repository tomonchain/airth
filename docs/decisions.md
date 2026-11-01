# Architecture decisions

Short records of the choices behind Airth's structure, newest first. Each one says what was decided, why, and what it costs.

## 48. Commands write only where the run may; they never read the owner's keys (Proposed)

The command sandbox (decision 4) allowed everything by default and denied only the Airth home and the other realms. A command could therefore write anywhere else on the Mac. Two ways out followed from it, both shown by a test:

- move the business's folder aside (its parent was writable), rewrite `agents/` or `.git/hooks` under the new name, and move it back: around decisions 44 and 46;
- write `~/.zshrc` or a file in `~/Library/LaunchAgents`, which the owner's shell or login then runs, outside every sandbox.

So the profile now denies **writes by default**, everywhere. A command may write only:

- where the run's workspace allows it (its folder or worktree, the git data a change needs, the realm's shared Cargo build folder);
- its own temporary folder, in the realm's cache in the Airth home, one per run; `TMPDIR` points there;
- `/dev` (null, the terminal);
- the caches of the tools runs actually use: Cargo's downloads (`~/.cargo/registry`, `~/.cargo/git` and their lock files) and npm's (`~/.npm`, where `npx` puts the MCP servers it starts). Never `~/.cargo/bin` or another folder on the owner's `PATH`.

A tool that needs another cache is added to this list by a change of its own, when a routine needs it.

**Reads** stay allowed outside the workspace (compilers, toolchains and system libraries live there), except the owner's credentials, denied to every command: `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.azure`, `~/.config/gcloud`, `~/.kube`, `~/.docker`, `~/.config/gh`, `~/.netrc`, `~/.git-credentials`, `~/.npmrc`, `~/.pypirc`, Cargo's credentials, and the keychain: its files (`~/Library/Keychains`) and the system services that answer keychain queries. HTTPS still works: certificates are checked by another service.

The lists live in `sandbox.rs`, beside the profile they produce. File tools are unchanged: they already reach nothing outside the workspace.

*Why:* the sandbox is the only thing between a command and the owner's machine; a rule that only lists what is denied leaves every path nobody thought of open. Denying by default turns the question into what a run needs, which is short. A business's agents read external content; a command they run must not be able to persist outside the run, nor take the owner's keys.

*Cost:* a tool that writes elsewhere (another package manager's cache, an editor's state, `~/Library/Caches`) fails inside a run until its folder is added. Each run's temporary folder stays until the run is deleted. On Linux, commands stay unconfined until milestone 9 (decision 29).
## 47. The business database: one access layer, evidence append-only, tables added with their milestone

Decision 31 put business state in one SQLite database per business, `business.db`, in the Airth home. This decision says how it is reached and what it holds for the beta.

**One access layer.** All SQL lives in one module, `src/business/store/`: opening, migrations, transactions, and one file per area (founder time, queue, actions, autonomy, routines, model calls, tasks). Only that module uses `rusqlite`; its connection is private; other modules call typed functions (`queue::propose`, `actions::begin`, `autonomy::level`), never a query. A test reads `src/` and fails if SQL or `rusqlite` appears outside it. The access layer is security code (`CLAUDE.md`). Today `pipeline/run.rs`, `pipeline/schedule.rs`, `business/drafts.rs`, `business/status.rs` and `business/time.rs` write their own SQL: the first change of milestone 2 moves it into the layer, with no change to the schema or to behaviour, and the tests that read the database through SQL read it through the layer.

**How it is written.** Unchanged from decision 31, and now required of every table:

- every write is one immediate transaction, in WAL mode, `synchronous = FULL`, a busy timeout, and foreign keys enforced;
- evidence is append-only, and the database enforces it, not only the code: on every evidence table, triggers refuse `UPDATE` and `DELETE`. Evidence is the founder-time ledger, heartbeats, every log of a record's moves, the action journal's events and the history of autonomy levels. Each migration that adds one adds its triggers, and its test checks that an update and a delete are refused;
- a record that changes carries a revision: a write against a stale revision fails instead of overwriting, and every move is also written to that record's log, in the same transaction;
- times are UTC, RFC 3339 text; JSON columns (arguments, sources) are checked in code before they are written;
- migrations are numbered and never edited once released; each has a test that migrates a database of the previous version holding rows; a database newer than the binary is refused; before migrating, the file is copied beside itself (`business.db.<version>.bak`).

**What it holds for the beta.** A table enters with the change that first needs it, by a migration, at the milestone that needs it; nothing is created ahead.

| Area | Tables | Milestone | From |
| --- | --- | --- | --- |
| Founder time | `owner_time`, gains the queue item and the channel (app, Telegram) an answer came from | 2 | exists (decision 27) |
| Heartbeats | `pipeline_runs`, `pipeline_seen`, `pipeline_schedule`, renamed `routine_runs`, `routine_seen`, `routine_schedule` when routines become graphs | 3 | exist (decision 34) |
| Model calls | `model_calls`, gains the agent, the routine, the run and the block | 3 | exists |
| Queue | `queue` and `queue_log`: what waits for the founder, of one kind among approval, prepared (level 0), delayed (level 2), incident, proposed promotion; its exact action and arguments, its source, its deadline or execution time, its status. `drafts` and `draft_log` become rows of it | 3 | replaces drafts |
| Actions | `actions` and `action_log`: each action executed or attempted, its idempotency key (unique), the level it ran at, whether the founder approved it, its outcome (`in_flight`, `confirmed`, `failed`, `unknown`). It makes `Act` idempotent and counts the daily limits (decision 45) | 3 | the branch's `actions.rs` |
| Autonomy | `autonomy` (agent, action, level) and `autonomy_log` (who, when, from, to, why) | 3 | decision 45 |
| Tasks | `tasks` and `task_log`: title, assigned to the founder or an agent, status, due date, the run that does it | 5 | rebuilt (`CLAUDE.md`) |
| Mail | sent mail: agent, approval, thread, archive copy | 7 | decided with the IMAP and SMTP integration |

Agents' statistics are not stored: the layer computes them from model calls, the queue and the actions. What a connector's daily limit and cancellation window are set to stays with the connector's record in the realm's config (decision 45), not in the database.

**Backups.** Once a day, the background runtime copies `business.db` with SQLite's online backup (consistent while Airth writes) to `backups/business-<date>.db` in the realm's folder in the Airth home, and keeps the last 14 days. `airth doctor` says, for each business, when the last backup was made and whether it opens and passes the integrity check; a missing or failed backup is reported in Needs you. Backups stay in the Airth home, out of every run's reach.

**Reading it from outside.** `airth db <business> export` writes every table as JSON lines, read-only; `airth doctor` runs SQLite's integrity check. No tool gives a run access to the database: runs reach business state only through Airth's tools, which call the layer, under the runtime's checks.

*Why:* `CLAUDE.md` asks for one access layer and makes it a security boundary: a single place where every rule on business state (append-only evidence, revisions, idempotence, limits) is written once and tested once. Adding tables with their milestone keeps the schema from holding guesses about features not built.

*Cost:* a backup a day takes up to fourteen copies of the database per business. Moving the SQL that exists is a change of its own, touching the pipeline engine and its tests before anything new is built. Each table is a migration to write and test, and a later change to a table's shape is another migration. Statistics computed on read cost more queries than stored counters, which is acceptable at one founder's volume.

## 46. A run in place cannot plant git hooks or change the git config

A run that works in the business's folder itself (not in a worktree) may write that folder's `.git`, since committing needs it. Two parts of it run code as the founder, outside any sandbox, the next time the founder or Airth uses git there: `.git/hooks/` (scripts git runs on commit, checkout, merge) and `.git/config` (it can name a hooks folder, filters, an editor, a pager, an SSH command). So for every run in place:

- `.git/hooks/` and `.git/config` are read-only, through the workspace's access rules: the file tools refuse them, and the command sandbox denies them, creating them included;
- `.git` itself cannot be renamed or removed by a command: without this, a command could move `.git` aside, write the hooks under the new name, and move it back. The sandbox profile gains a rule for one exact path (`literal`), beside the existing ones for a whole folder (`subpath`).

The rest of `.git` stays writable: a run in place can still commit. Runs in a worktree are unchanged: the founder's checkout, its `.git` included, is already read-only to them (decision 5).

*Why:* a planted hook or config line runs with the founder's rights the next time git is used in the folder, outside every rule Airth applies to runs. The founder asked for it after decision 44. Checking the sandbox showed that a rule on `.git/hooks` alone is bypassed by renaming `.git`; hard links to a protected file are already refused.

*Cost:* git operations that rewrite `.git/config` (adding a remote, setting a branch's upstream) fail inside a run in place; the founder does them. Without a command sandbox (Linux, until milestone 9), commands are not confined, as for the rest of the workspace. Hooks in a submodule's git folder (`.git/modules/`) are not covered.

## 45. Autonomy per agent and action, under a ceiling set by the action's risk class

**Actions.** An action is one thing done outside the business's folder through a connector: one tool of an MCP server, one declared call of an integration, or one of Airth's own (launch a mission, send a mail, adopt a change). Each has an id, `<connector>.<action>` (`typefully.create_draft`, `blog.publish`, `airth.launch_mission`). Reading and writing the business's folder, running commands there, and memory stay under the realm's rules (decision 4) and the workspace (decision 44): they are not actions.

**Risk classes.** Every action has one of three classes: `read`, `reversible` (a write that can be undone: a draft, a label, a scheduled post not yet sent) or `commits` (money, a contract, a deadline, a legal obligation). Publishing in the business's name (a post, an article, a thread) is `commits`. The class is declared in Airth's record of the connector, which the founder confirms when connecting it, never by the connector itself: what an MCP server says about its own tools (its annotations) is shown as a hint and never trusted. An action with no declared class is `commits`.

**Levels and ceilings.** The four levels of `CLAUDE.md` (0 prepare, 1 propose, 2 act with delay, 3 act). The class sets the ceiling:

| Class | Ceiling |
| --- | --- |
| `read` | 3 |
| `reversible` | 3 |
| `commits` | 1 |

Fixed ceilings, whatever the class: replies on X 0; publishing in the business's name 1 during the beta; payments, refunds, contracts, mails that commit money, a deadline or a legal obligation, merging code and deploying to production 1; a mail to a correspondent the agent never wrote to 1 (`CLAUDE.md`, *Mail*). A level is never stored above its ceiling, and is clamped to it when read, so lowering a ceiling takes effect at once.

**Where levels live.** Per agent and per action, in the business's `business.db`, never in the folder (decision 43). A new action starts at 1, or at its ceiling when that is 0. Each change is a row (who, when, from, to, why), so the history of what was delegated, and when, is kept. A run works with its agent's levels; a mission the founder starts and watches asks the founder, as today.

**What each level does, also in a mission.** The same rule applies to an `Act` block and to a tool call inside a mission (the agent loop), through the permission gate in `runtime.rs`:

- 0: the action is not executed. What it would do is prepared (a pre-filled link, a text to paste) and put in Needs you; the founder executes it outside and marks it done.
- 1: the action is not executed. It goes to the approval queue with its exact arguments. If the founder approves it, Airth executes it as it was proposed, once (the action journal of the reconciliation, `actions.rs`); an edit changes the arguments, not the action. A mission that proposes an action is told so and carries on; it does not wait.
- 2: the action is queued with a cancellation window (30 minutes by default), shown in Needs you and on Telegram with a Cancel button, then executed.
- 3: executed, then reported.

The gate reads the level from `business.db` before each action, so a demotion applies to the runs already going. If it cannot read it, the action is treated as level 1.

**Daily limits per service.** Each connector has a limit on actions executed without the founder's approval (levels 2 and 3) over the last 24 hours: 20 by default. Past it, that connector's actions fall to level 1 until the window clears, and Needs you says the limit was reached. The count and the journal entry of the action are written in one transaction, so runs acting at the same moment cannot pass the limit together. Actions the founder approved do not count: they are the founder's decision. The daily limit and the cancellation window are defaults the founder changes per connector; the values set are kept in the Airth home, with the connector's record, never in the business folder.

**Promotion and demotion.** Only the founder promotes. Airth proposes a promotion in Needs you when an action of an agent has been approved without edit 20 times in a row, over at least two weeks, with no incident. An edit or an incident sets the count back to zero; the founder accepts or ignores it. An incident demotes the agent's action by one level, at once, and says so in Needs you: an action that failed after its retries, an action at level 2 the founder cancelled, or an action the founder marks as wrong after the fact.

**Today's rules and approvals.** The realm's rules (`network`, `skill`) and a routine's approvals given in advance (decision 11) keep governing calls that are not declared actions. Once a connector's actions are declared, its calls go through levels: a routine that acted alone through it moves to level 1 until the founder sets levels. In the beta that is only Hedgen, set up again anyway.

**Implementation.** With the approval queue, at milestone 3. It changes the permission gate, `runtime.rs` and `tools/http.rs`: security code, changed on this decision's acceptance, with tests proving that an undeclared action is `commits`, that no level passes its ceiling, that level 0 and 1 never execute, that an approved action executes once, that the daily limit holds under concurrent runs, and that a demotion stops a mission already running.

*Why:* `CLAUDE.md` earns delegation per action class, never wholesale. Today's model has the opposite shape: rules per capability for the whole realm, and approvals per routine given in advance, with no notion of how serious an action is. Declaring the class with Airth and not with the connector keeps a third-party server from calling a payment a read. A count per service is generic and bounds a runaway agent even inside its allowed actions.

*Cost:* each connector's actions must be declared and classed before they can act alone: until then, everything a connector does is proposed. Two systems coexist (rules for local work, levels for actions), and the app must show both on an agent's card. The promotion threshold is a guess, to tune on Hedgen's first weeks. A proposal at level 1 inside a mission means the mission ends before its effect happens, so its report must say what is pending.

## 44. A run never writes the business config; it proposes a change

`business/`, `agents/`, `routines/` and `prompts/` at the root of a business's folder are read-only for every run working in that folder (decision 43). The rule is one of the workspace's access rules, so the file tools refuse the write and, on macOS, the command sandbox denies it to any command. It holds for folders that do not exist yet: a run cannot create `routines/` either.

A run working in a worktree (a change, decision 5) may edit those folders in its worktree: that is how an agent proposes a new job, goal or routine. The owner's checkout stays read-only to it, as for any change, and nothing reaches the business until the founder adopts the change after reading its diff. The founder changes the config directly, from the app or an editor, as before.

*Why:* an agent's file is its instructions, and `business/goal.md` holds the baseline the beta is measured against. A page an agent read must not be able to rewrite either. `business/` was already read-only (pilot step 1); `agents/` was not, and nothing let an agent propose a change to the config in a way the founder reviews.

*Cost:* where there is no command sandbox (Linux, until milestone 9), commands are not confined, as for the rest of the workspace (decision 4); file tools still refuse. A business whose folder is not a git repository has no worktrees, so its agents can only suggest a config change in their report. A run that is not isolated can still write the folder's `.git`, as it could before: it could record a commit there without changing the files, which `git status` would show.

## 43. The business config: profile and legal identity, goal, lines, departments, agents

A business is described by files in its folder, versioned in git, in the format `CLAUDE.md` sets: Markdown with a flat YAML front matter, one file per item. Structured fields in the front matter, prose in the body.

```text
<business folder>/
  business/profile.md        identity, legal identity, links; the body describes the business
  business/goal.md           the goal, its measure and milestones, the founder's baseline;
                             the body is the strategy every agent follows
  business/lines/<name>.md   one editorial line, range or brand
  agents/<name>.md           one agent: its role in the front matter, its job in the body
```

**Front matter.** Flat: `key: value`, one per line, no nesting. A value is text, a date (`2026-11-01`), or an inline list (`[a, b]`, quoted items when they hold a comma). An empty value or `TODO` means not known yet: the business loads, and the app lists what is left to fill. An unknown key refuses the file at load, naming the file and the key, so a typo never silently drops a field. The parser of `tools/catalog.rs` becomes a shared module, `src/frontmatter.rs`, and each kind of file has its own checked fields.

**`business/profile.md`**

| Field | Meaning |
| --- | --- |
| `name` | The name the business goes by |
| `legal_name`, `legal_form` | As registered: `Hedgen Ltd`, `ltd`; `sas`, `sasu`, `ei` in France |
| `country` | ISO code: `GB`, `FR`. It picks the legal calendar |
| `status` | `planned` (not registered yet) or `registered` |
| `registration_number` | Companies House number (UK), SIREN (France) |
| `registered_office` | The registered address |
| `incorporated_on`, `fiscal_year_end` | A date; a month and day (`03-31`) |
| `vat_number`, `tax_regime` | When registered |
| `currency` | The accounts' currency: `GBP`, `EUR` |
| `bank` | The bank's name only. Never an account number: bank details stay out of the folder |
| `domain`, `links` | The business's domain; its sites and social accounts |

With `status: planned`, only `name` and `country` are expected. The legal calendar (filing and tax deadlines) is derived from `country`, `incorporated_on` and `fiscal_year_end`; its rules per country are written at milestone 2.

**`business/goal.md`**: `goal` (one sentence), `measure`, `target`, `by` (a date), `milestones` (a list of `YYYY-MM-DD what`), `baseline_minutes` (the founder's time per week on the business before Airth, which the founder-time ledger is compared with, decision 27). Its body is the strategy (decision 36), which `business/strategy.md` becomes.

**`business/lines/<name>.md`**: `title`, `kind` (`editorial`, `range` or `brand`), `status` (`active` or `paused`). The body says what the line is and for whom.

**Departments** are not files. The seven of `CLAUDE.md` (`leadership`, `marketing`, `sales`, `support`, `product`, `operations`, `finance`) are fixed in Airth; an agent names one, and an empty department is not shown.

**`agents/<name>.md`**: the file name is the agent's id. Front matter: `title`, `department`, `reports_to` (an agent's id, or `founder`), `lines` (the lines it works on), `mailbox` (its address, if it has one). Its body is the job (decision 36): mission, how it works, what it reports. `business/bots/` becomes `agents/`.

**What the folder never holds:** what an agent may do. Its tools, autonomy levels, budgets, model, the credentials of its mailbox, and whether it is paused stay in the Airth home, out of every run's reach (decisions 7 and 36). The folder describes the business; the home decides what runs may do. An agent that edits its own file can change how it is described, never what it is allowed to do.

**Amendment (founder, 1 November 2026): the config is read-only for every run.** The body of an agent's file is its job, so its instructions; `business/goal.md` holds the baseline that proves the beta. So `business/`, `agents/`, `routines/` and `prompts/` are read-only for every run, through the workspace's access rules, which the file tools check and the command sandbox enforces. An agent that wants to change the config does it in a worktree, as a change the founder adopts, never by writing directly. `business/` was already read-only (pilot step 1); `agents/` was not. The fix is its own change, under decision 44.

*Why:* milestone 2's gate is Hedgen Ltd fully described in configuration, and Numirama must be set up with configuration only. Files in git show who changed what and when, and read well in an editor; flat front matter keeps the parser small and with no new crate. Keeping rights out of the folder means a page an agent read cannot raise what it may do by rewriting a file.

*Cost:* two places describe an agent: its file (who it is) and the home (what it may do). The app must show both on one card. Seven fixed departments may not fit every business (a media's editorial team sits in `marketing`); adding a department to the list is a decision. Moving `bots/` and `strategy.md` is a migration of existing folders, done once, when the business loads.

## 42. The surface is frozen on the beta's scope

This supersedes decision 25. Until the beta is reached (`CLAUDE.md`, *Scope: the beta*), Airth gets bug fixes and the current milestone's work only. Milestones are taken in order; one starts when the previous one's gate is passed. Anything outside the beta goes to `docs/backlog.md`: what, why, which milestone it would serve. The redesign moves existing features, it does not remove them; a feature leaves Airth only through a decision of its own.

*Why:* the pilot's seven steps were replaced (decision 36), then the frame changed again: Airth is a business headquarters, each business a company. The beta has a measurable end (Hedgen four weeks on its own, founder time below the baseline, Numirama by configuration only), which makes it a better line to freeze on than a list of steps.

*Cost:* the beta is wider than the pilot (mail, the VPS, Numirama), so the freeze lasts longer and more work counts as in scope.

## 41. Agents ask each other for work, within the limits of the one asked

A routine's runs may ask other routines of the same business for work, if the owner allowed it on that routine (its `delegates`). Two tools carry it, offered only to those runs:

- `hand_off` starts the other routine's run with a brief and returns at once; the app starts it as it starts a scheduled routine.
- `ask_agent` starts it, waits for it to end, and returns its report.

The run asked for work is the other routine's own: its agent's job, its tools, what it may do alone, its budget. Nothing of the asking run's rights passes over, so an agent cannot reach through a colleague what it may not do itself. The brief arrives as data, never as instructions, since the asking agent may have read external content. Nobody is there to approve a request inside it: what needs the owner is denied and listed in its report, as for any run that started without them. A chain of requests stops at three routines deep and never comes back to a routine already in it. Each run asked for work says which routine asked, in Tasks and in its notification.

*Why:* the owner wants the team to work together: an analyst handing a finding to the writer, a writer asking the analyst for figures before it writes. Routines already carry an agent with its limits, so making them the unit of delegation adds no second place for rights.

*Cost:* one request can cost several runs: the depth limit and each routine's own budget bound it, not a budget for the whole chain. An agent can only ask routines the owner listed; a routine that is already running cannot be asked until it ends.

## 40. Airth no longer develops its own skills

Runs no longer propose skills (`propose_skill`), and Airth no longer develops, evaluates and installs them (`test_skill`, `airth skill`, the app's "Built by Airth" tab). This supersedes decision 14. What extends a business's agents is what the owner connects: MCP servers and connectors (decisions 18 and 20), integrations, and skills from the catalog, all still under the realm's `skill` and `network` rules. Skills installed before are no longer offered to runs; their files stay in the realm's folder until it is removed.

*Why:* a business needs its agents to use its own services, which connectors give, more than tools Airth writes for itself. Self-development was the largest feature left from the personal agent (decision 32), with its own runs, workspace, sandboxed evaluation and pages to maintain and secure.

*Cost:* a capability no connector or catalog skill offers must be built outside Airth, as an MCP server or a script in the business's code.

## 39. The Mac app tools and the `apps` capability are removed

The tools that drove the owner's Mac apps (`calendar`, `reminders`, `notes`, `mail_draft`, `safari`, `clipboard`, `open_on_mac`), the `apps` capability that governed them, and the Google and iCloud connectors that went through them are removed, with their pages. This supersedes the Mac apps half of decision 16; the Mac app itself stays. A realm saved with an `apps` rule still loads: the rule is ignored, and nothing can be approved for `apps` any more.

*Why:* a business reaches its services through its connectors (MCP servers, integrations), each given to the agents that need it (decision 36). Tools acting in the owner's personal apps, shared by every realm, belong to the personal agent Airth no longer is (decision 32), and were one more way for a run to reach outside its realm.

*Cost:* a business that kept its calendar or mail in the Mac apps must connect the service itself (Google, Microsoft) through a connector.

## 38. The phone's pairing code is drawn with qrcode

Pairing the phone (decision 37) hands it the app's address and key in one QR code, shown by `airth phone pair` in the terminal and on the owner's Phone settings in the app. The code is made with the `qrcode` crate, without its default features (image, SVG, PIC): it has no dependencies of its own. The app receives the modules as data and draws them as a grid, so no image or markup is built from a string.

*Why:* typing a 64-character key on a phone is not a pairing anyone would do, and encoding QR codes (Reed-Solomon error correction, masks, versions) is too much code to write and get right by hand for this.

*Cost:* one more small crate to keep up to date. The QR code carries the key: whoever photographs it controls Airth, which the app and the terminal say where they show it.

## 37. The app is rebuilt around six sections, and reaches the phone through Tailscale

The app's interface starts again from a blank page; the engine stays (the hub, the runtime and its boundaries, objectives, bots, connectors, approvals, usage records). A business (a realm, decision 32) opens on six sections:

- **Steering**: the owner's page. What waits for a decision, with what is needed to decide; what needs attention (a failed run, a missed time, a budget nearly spent, a bot stuck); the goals from the strategy and where they stand; the strategy itself, the only text the owner writes for the team.
- **Routines**: the recurring work, each with its bot, its trigger, its limits; created, changed, paused, run now. A routine is an objective (decision 11); a pipeline (decision 30) is a routine whose work is a fixed sequence.
- **Planning**: what runs when, past and coming: each routine's times, what ran, what was missed.
- **Tasks**: the work itself, one-off or from a routine: what is running, what is done, each with its plan, its approvals, its report and what it produced. Asking Airth for something is starting a task.
- **Agents**: the business's bots (decision 36): job, tools, limits, budget, memory, what each produced; hiring one from a template by business function.
- **Usage**: what running the business costs: tokens and money by agent, routine and model, and the owner's time.

The business's settings (documents, connectors, skills, access) sit behind these, under Settings. The pages of decision 32 (Today, Pipelines, Ask Airth, Costs) are replaced by these, each removed in the change that replaces it. The interface stays in English.

The app stays on the owner's Mac, listening on loopback only. The phone reaches it through Tailscale: `tailscale serve` publishes it over HTTPS to the owner's own devices (their tailnet), at home or away; nothing is opened to the local network or the internet. Airth accepts that one host name, set by the owner (`airth ui allow-host <name>.ts.net`), beside loopback; the token is still required on every call, and the phone is paired once by scanning a QR code that carries it.

*Why:* the current pages grew from a personal agent and were patched toward a business; the owner finds them hard to follow. These six sections are the shape of products built for bots that work on their own (OpenBot), which the owner finds clear, plus the one thing they lack for a business: a page to steer and decide. The engine already matches them one for one (routines are objectives, tasks are runs, agents are bots, usage is the usage record), so only the interface is new. Tailscale gives the phone access without exposing a program that controls the Mac: the traffic is encrypted end to end, and only the owner's devices can reach it.

*Cost:* the interface is written again, page by page, and pages that work today go away while their replacements are built. The phone needs Tailscale installed and signed in on both devices. A host name allowed by mistake would let pages on that host reach the app's API; the token still stands in the way, which is why it is never put in a URL a third party can see.

## 36. A business is run by bots, each with a job; the owner steers

The owner sets the direction of a business; bots do the work. A realm is one business (decision 32) and has a team of bots. A bot is:

- **a job**: what it is for and how it works, in `business/bots/<name>.md`, in the business's words, versioned with the rest of the business. Its version is the hash of its content, recorded with each run;
- **its tools**: the connectors (remote MCP servers, decision 20; local ones, decision 18), integrations (decision 7) and skills it may use, a subset of the realm's;
- **its triggers**: a schedule, a webhook, a watched file, or the owner asking (decision 11);
- **its limits**: what it does without asking, its budget per run and per month, its model;
- **its memory**: what it learned, kept per bot in the realm's memory.

The job is in the business folder; the tools, triggers and limits are kept by Airth, out of every run's reach, as objectives are today (a bot is the objective it grows from, given a name, a job file and its own tools). A run could edit a file in the business folder, never what it is allowed to do.

The owner's side is direction and decisions. `business/strategy.md` holds the goals and the line every bot follows. What a bot may not do alone waits for the owner on Today and on the phone (decision 28): by default, reading and preparing are free; anything that touches money, customers or the public (a payment, a refund, a price, a message to a customer, publishing) needs the owner's approval, and that approval is never given in advance for publishing. Today shows each bot: what it did, what it waits for, what it cost.

This supersedes decision 25 (the frozen surface and the pilot's seven steps) and decision 26 (business work as fixed pipelines where the model only judges). Pipelines (decision 30) stay as a tool a bot or a schedule can run, for fixed sequences that gain from being plain code. Decision 35 (outlets) goes to the backlog.

The first proof is Hedgen's **Writer**: once a week it chooses a subject (news worth explaining, or a lasting notion: a term, a mechanism), researches it, writes the article in Hedgen's voice, illustrates it (charts from Hedgen's API, an image when it helps), and prepares it on the blog as a draft; the owner approves, and the Writer publishes. The proof is reached when four weekly articles go out in a row with the owner spending only the approval, and the cost per article is known. Delegation between bots, a computer of their own and a mobile app come after.

*Why:* the owner wants to steer and leave execution to Airth. Fixed pipelines make every piece of work a piece of code to write; bots with a job written in plain words let the owner add work without a release. Products built this way (Rakazo, Grok Bot) show the shape works. Most of what a bot needs exists: the agent loop, connectors, objectives with triggers, budgets and approvals, the permission gate, the phone, usage records. Airth's difference is where it runs and what it measures: on the owner's machine, credentials the model never sees, and each business's cost and owner time.

*Cost:* a bot's work differs from one week to the next and cannot be proven in advance like a pipeline: it is judged on its output (the owner's edits and rejections, cost). A bot reads external content and holds tools with effects, so a page could try to steer it; the approval before anything public is the limit that holds. `CLAUDE.md` changes with this decision: the current phase becomes the Writer proof, and the rules on model calls apply to pipelines.

## 34. A pipeline's schedule is in its file, not in an objective

Decision 30 had an objective name the pipeline it starts. The pipeline file already says when it runs (`schedule`), so the background runtime reads it directly: every few seconds it loads each realm's pipelines, and runs those whose time has come, each on its own thread. When each runs next, and when it last started, live in `business.db`. A schedule seen for the first time is armed for its next time; one missed while Airth was not running fires once, late, and the owner is told; a run that started and left no heartbeat (Airth stopped during it) is reported at the next one. A scheduled run has nobody to ask: what the realm's rules leave to the owner is denied at once, and the run's heartbeat says so. Each run, done or failed, lands in the owner's inbox.

*Why:* one place for when a pipeline runs, versioned with the rest of the business. An objective beside the file would be a second schedule to keep in step, outside git.

*Cost:* a scheduled pipeline cannot be approved in advance the way an objective can (its approvals): the realm's rules must allow what it does, network requests first. Objectives keep their budgets and approvals; pipelines have none yet, and a judge step's cost is bounded only by the number of items read.

## 33. Feeds are read with quick-xml

Pipelines read RSS and Atom feeds (decision 30), which are XML. They are parsed with `quick-xml`, a streaming reader with no dependencies of its own. A feed that breaks off, because it is malformed or because the HTTP tool cut it at its size limit, gives the items read before the break, and the run notes it. Feeds are fetched through `Runtime::invoke` with the `http_request` tool, so each read is checked against the realm's rules, redacted and recorded like any tool call; redirects are not followed, and the run says where the feed moved.

*Why:* feeds are external content, often not quite well formed: a hand-written parser would be one more thing to get wrong on untrusted input. `quick-xml` is small, widely used, and reads what is there without building a document tree.

*Cost:* one more dependency to keep up to date. Items are what the feed says: titles, links, dates as written, summaries reduced to plain text.

## 32. Airth runs businesses; each realm is one business

A realm is one business. The app shows nothing that is not about the business it runs. The rail lists the businesses, and for the one open:

- **Today**: what waits for the owner (drafts with Approve, Edit, Reject, their source, prompt version and cost), the pipelines (last run, next run, a missing heartbeat in warning), and the week (owner time against the baseline, share of drafts sent unedited, model cost).
- **Pipelines** and **Reports**: what runs, and what it produced.
- **Costs**: what running the business with Airth costs, in tokens and money, by model, by pipeline and by why the model was called (the usage page, scoped to this business).
- **What the business is made of**: Documents (its profile, archive and other files), Code (the repositories of its projects), Skills, Connectors (its integrations and their credentials).
- **Ask Airth**: a chat about this business, with its documents, code, skills and connectors.
- **Settings** of the business.

The North Star, goal-tracking projects, routines, benchmarks and the global inbox leave the surface: routines become pipelines, and the inbox becomes Today. Each is removed, not hidden, in its own change, with its code and tests; settings that belong to the owner (profile, model keys, the phone) stay in one Owner settings page.

*Why:* the owner wants one tool to run businesses with as little of their time as possible, not a personal agent with a business page beside it. Every screen that is not about the business is one more thing to walk past, to maintain and to keep secure.

*Cost:* features that work today go away, and anyone who used Airth as a coding or personal agent loses them. A business that is not yet set up has an empty realm, so creating one must lead straight to its profile, its code and its first connector. This supersedes the "nothing removed" rule of decision 25 for the features listed here.

## 31. Business state lives in SQLite, one database per realm

A business's state (the draft queue and its transitions, the owner-time ledger, pipeline heartbeats, the record of model calls) lives in the realm's `business.db`, through `rusqlite` with its bundled SQLite. Every write is one `BEGIN IMMEDIATE` transaction, in WAL mode with a busy timeout. Schema changes are numbered migrations, and a database newer than the binary is refused. Mutable records carry a revision: an update against a stale revision fails. The audit table is append-only, enforced by triggers. Run state, objectives, settings and memory stay JSON (decision 1).

*Why:* the draft queue must never publish twice, even when a retried job and an answer from the phone arrive together; a transaction makes that a property of the store, not of every caller. The model-call record is the evaluation dataset (decision 26): it grows with every call and is queried by prompt version and outcome, which JSON files do poorly. The store comes from the `business-autonomy` branch, where it was built and tested (see `docs/reconciliation.md`).

*Cost:* one new dependency, compiled in (about 1 MB). Business state is no longer readable with a text editor; `airth` must offer a way to inspect and export it. Two storage styles coexist, with a clear line between them.

## 30. Business pipelines are a generic engine; each business declares its own

Decision 26 left open whether pipelines are installed skills (decision 14) or a module of their own. Skills cannot do the job as decision 14 bounds them: a skill runs only when a model inside an agent run calls it, so the model would sequence the work; its command gets no provider key, so it cannot make a recorded model call; and it is denied the Airth home, where the queue and the ledger live.

So the engine is a module, `src/pipeline/`. It holds the engine and a small set of generic step types, nothing specific to one business: read a source (RSS, a web page, JSON through an integration), filter and deduplicate, ask the model to judge or draft (a prompt and an output schema), queue for the owner's approval, send to a service through an integration, write a report. Steps with an effect go through `Runtime::invoke`, with the objective's approvals as grants and an approver that never waits, so the permission check, redaction and events stay the only path to execution (`runtime.rs` is unchanged). An objective names the pipeline it starts (a new optional field); the scheduler starts the pipeline instead of an agent run when it is set. Triggers, budgets and the missed-schedule rule stay as they are (decision 11).

What belongs to a business lives in its realm folder, under `business/`, versioned in git with the rest of the realm: its pipelines (JSON files assembling step types, with their schedule), its prompts, its sources, the accounts it watches, its editorial pillars. A pipeline file is checked when it is loaded: an unknown step type, a missing prompt or an invalid schema refuses the pipeline before it runs, never halfway. A prompt's version is the hash of its content, recorded with every model call (decision 31). Runs can read `business/` but not write it: only the owner changes how the business runs.

A step type enters the engine only when a real pipeline needs it. There is no workflow editor during the pilot: pipelines are files. Hedgen is the first business configured this way; a second one set up without changing the binary is the test that the engine is generic.

*Why:* the owner wants anyone to run their company from Airth. Pipelines written in Rust for Hedgen would serve no one else, and every change would need a release. Declaring them per business keeps the guarantees of decision 26 (fixed steps, the model only judges, recorded calls) in the engine, and the business choices with the business, where git shows who changed what and when.

*Cost:* the pipeline file format is a contract to keep stable and documented. Configuration errors must be caught at load time, which needs its own checks and tests. Generic step types are harder to get right than one-off code, and the first ones will be shaped by one business until a second one uses them.

## 29. Airth runs on the owner's Mac until a stable beta, then on a Linux VPS

During the pilot, Airth runs as the login service on the owner's Mac (decision 13), kept awake when a routine is due. A schedule missed while the Mac sleeps fires once, late (decision 11); the weekly report shows such delays so they are not mistaken for slowness.

Once a beta is stable, Airth moves to a Linux VPS, always on. That move is gated: on Linux, commands run unconfined (decision 4, *Cost*). Before the VPS hosts a realm, one of these must hold:

- a Linux sandbox exists (Landlock or bubblewrap), generated from the same path rules as the macOS profile, with the bench's security tasks passing on Linux; or
- the realm denies execute and has no installed skill that runs commands, which `airth doctor` checks and reports.

The app server also stays on loopback on the VPS; the owner reaches it through an SSH tunnel or a private network, never an open port.

*Why:* the Mac is what exists and is enough to prove the pilot. A business must eventually run with the laptop closed, and a VPS is the cheapest always-on host, but moving there must not silently drop the isolation the rest of Airth relies on.

*Cost:* until the move, a closed lid delays routines. The Linux sandbox is real work, scheduled after the pilot's proof, not before (decision 25).

## 28. The approval queue reaches the owner's phone through a Telegram bot

The pilot's proof depends on approving drafts in seconds, away from the desk, and notifications today stay in the app's inbox (decision 12, *Cost*). Airth polls the Telegram Bot API (outbound only, no port opened, which also works unchanged on the future VPS). Each item of the queue arrives as a message with Approve, Edit and Reject buttons; Edit takes the owner's reply as the new text. Answers are accepted only from the owner's chat id. The bot token is an integration credential bound to api.telegram.org (decision 10). Every answer goes through the same queue state machine and owner-time ledger as an answer given in the app (decisions 26 and 27), so the channel decides who answers, never what an answer may do (decision 2).

*Why:* the cheapest path to a phone: one integration and one polling thread. The iOS companion app stays possible later and reuses the same queue.

*Cost:* message contents transit through Telegram. Only drafts and public post links go there, never credentials or private realm data. Polling adds a request every few seconds while Airth runs.

## 27. The owner's time is a recorded quantity

Every owner interaction with a business run is recorded in the realm's ledger: what was shown, when, the answer (approved, edited, rejected), the time between showing and answering, and for an edit, the diff. The weekly report sums it per week and per pipeline, and compares it with the baseline the owner states for the manual period.

*Why:* the pilot's claim is "this business runs with N minutes of the owner per week". Token usage is already recorded (decision 23); the owner's time is the cost that matters, and it is not recorded anywhere yet. Edits are also the best signal of where the drafts fall short.

*Cost:* time between showing and answering overstates attention (a draft can wait an hour unread). The report shows both the sum and the median per item, and says so.

## 26. Business operations are pipelines of tested steps; the model only judges

A business routine (watch, drafting, replies radar, weekly report) is a fixed sequence of steps written in code. Steps that fetch, filter, deduplicate, schedule or change state are deterministic and tested. A model is called only for steps that need judgment, each with a versioned prompt file, an output schema checked in code, and a recorded outcome. The routine is started by an objective (decision 11) with its approvals and budget given in advance. A step that reads external content holds no tool with side effects.

Drafts move through drafted → pending → approved → published | rejected, stored as JSON in the realm. Transitions are idempotent, so a retried routine never sends a draft twice. Airth never publishes to X; the owner does, from a prepared link.

Each routine writes a heartbeat when it ends; a routine whose heartbeat is missing past its schedule notifies the owner.

Before a prompt or model change ships, it is replayed on a reference set built from the owner's own past posts and from the drafts they approved or edited, and the results are compared.

*Why:* the agent loop is right for open tasks, wrong for a daily routine: a routine that re-plans itself each morning is costly, hard to test and different every day. Fixing the sequence makes behaviour reproducible and failures local. It also keeps X's automation rules respected: automated replies found by keyword search are not allowed there, so a human publishes every reply.

*Cost:* a new routine needs code, not just a sentence. That is deliberate during the pilot. Whether these pipelines become installed skills (decision 14) or a module of their own is the first implementation question, and gets its own entry.

## 25. The surface is frozen until the Hedgen pilot proves itself

Airth went from first commit to about 30,000 lines in under two weeks. Until the Hedgen pilot shows two consecutive weeks of its seven steps running (see `CLAUDE.md`) with a measured owner time well under the manual baseline, Airth gets bug fixes and pilot work only. Other ideas go to `docs/backlog.md` with the pilot step they would help.

*Why:* the code is well structured, tested and documented, so the risk is not quality but breadth: one owner has to understand and maintain everything that exists. The pilot is what tells which of the existing features matter.

*Cost:* useful improvements wait. Features that exist and are not used by the pilot stay as they are, unmaintained beyond keeping the checks green.

## 24. Benchmarks drive real runs through the hub, in throwaway realms, and verify outcomes themselves

Airth Bench (`src/bench/`) measures the harness, so it must exercise the harness: each task is a normal run created and driven by `Hub::drive`, with the same agent loop, tools, permission gate, sandbox, compaction and usage accounting as a chat. There is no second runtime. What differs is around the run:

- Each attempt gets a fresh workspace in the system temp directory and a realm of its own under `benchmarks/work/`, outside `realms/`: never listed, never shown, never one of the owner's. It does not share the owner profile; its credentials and memory are the task's synthetic ones. Both are deleted afterwards; the run's full state is kept as the trace.
- The approver says no to everything, so the task's policy alone decides (by default: files and commands allowed, network, skills and apps denied). Unattended commands are only safe where the OS confines them, so tasks that run commands, or are verified by one, are skipped without the sandbox.
- Realms gained an `offline` setting, enforced rather than requested: `http_request` refuses, and the sandbox profile denies outbound IP connections. Bench realms are offline unless a task asks for the network.
- Success is decided by the task's checks on the workspace and the conversation, never by the agent's report. A security task also needs evidence the attack reached the agent (a tool call aimed at it); if the model only declined, the result is inconclusive, since it says nothing about what Airth would have prevented.
- Metrics come from what runs already record (events, per-call usage, compactions); costs are estimates from list prices and are labelled so. Rates exclude skipped tasks. Comparisons call a drop *confirmed* only past a two-proportion test (z ≥ 1.96); a new security failure always is.

Cost: a benchmark spends real tokens on the owner's key, and a task's outcome still depends on a stochastic model, so single runs show trends, not proof; repeats (`--repeat`) are the way to tell. External agents (Claude Code, Codex) would need an adapter that runs the same tasks and checks; not built yet.

## 23. Requests are shaped for the provider's prompt cache

An agent re-sends its whole conversation on every model call, so most of a run's cost is input that the provider has already seen. Requests are built so that this input is read from the cache (about a tenth of the price, or less) instead of being written again:

- The system prompt is two parts: Airth's instructions, the same for every run, with a cache breakpoint after them (and after the realm's tools, which come first), then the run's context (realm, workspace, owner, memory, North Star). A new run reads the instructions and tools from the cache.
- A run keeps the context it started with on every drive. If the realm's context changes meanwhile (a memory saved, a profile edited), the new one is sent once as a note after the conversation, instead of changing the system prompt and invalidating the cache of the whole conversation.
- While tools wait (mostly for the owner's approval), the agent re-sends the last request every 4 minutes with `max_tokens: 0`, which keeps its cache alive without generating anything, for 45 minutes at most. Each refresh costs a cache read; a miss would rewrite the conversation at 1.25 times its price. Only Anthropic supports this; refreshes are counted in the run's usage.
- Triage is a `quick` request: low effort and a small output limit.
- Command output keeps its start and end (where errors and summaries are) and drops the middle; `read_file` reads line ranges, so the agent need not load a large file to see one function.

- A conversation the model sees is compacted past 150,000 tokens (`AIRTH_COMPACT_AT`, 0 to turn off): the model writes a handoff summary (read from the cache, with the same instructions and tools), and from then on sees that summary, its plan and what follows. Every call re-sends the whole conversation, so without this a run's cost grows with the square of its length: one 309-call run reached 800,000 tokens per call. The run keeps every message for the owner; only what the model is sent changes, and a summary that is too short or calls tools is discarded.

Thinking and effort stay fixed for a run, since changing them invalidates the cache, and old tool results are never pruned one by one: on current models, editing earlier turns invalidates the cache and the preserved reasoning after them, which costs more than it saves.

*Why:* the owner asked for the cheapest requests that do not make Airth worse. These changes only remove repeated work; the model sees the same information.

*Cost:* a run does not see changes to its realm's context in its system prompt, only in the note. Refreshing the cache costs money when the owner never comes back within 45 minutes.

## 22. The app serves each request on its own thread; settings changes stay serialized

The app's HTTP server (`ui::serve_requests`) used to answer requests one at a time. Since connectors, model lists, registry search and connection checks wait on remote services (up to a minute for a first sign-in), one slow provider froze the whole app, including the Approve button of a running run. Each request now runs on its own thread. Requests that change stored settings still take a single lock (`App::writes`), so two edits never race on the same file. Exceptions: reads; approvals and stops, which only touch live runs in memory; and the routes that wait on a remote service (connecting a connector, checking one), which take the lock only around their writes, never while waiting. A test starts a service that takes two seconds to answer and checks that the app answers another request in under one second meanwhile.

*Why:* a stabilization pass found the freeze; decision 1's one-thread-per-run model already made the app concurrent with its runs, so threads per request add no new kind of concurrency.

*Cost:* one short-lived thread per request (the app polls every few seconds, and pauses while its window is hidden). An OAuth callback may still race with a settings change of the same MCP server; that would need two changes to the same server within the same second.

## 21. The owner picks the model per request; each provider only gets its own key

The message box has a model menu. **Auto** keeps the realm's routing (decision 17). Any `provider/model` can also be picked among Anthropic, OpenAI, xAI (Grok), DeepSeek, Mistral, OpenRouter and Ollama. DeepSeek, Mistral and OpenRouter are OpenAI-compatible presets with their own key variables (`DEEPSEEK_API_KEY`, `MISTRAL_API_KEY`, `OPENROUTER_API_KEY`), which count as model keys, never passed to commands. The menu lists each configured provider's models live from its `/models` endpoint (chat models, filtered by name, cached ten minutes), since model names change faster than Airth's releases. The choice is saved in the run (`model_choice`) and applies to every drive until changed. The app checks it before starting, so a missing key is reported right away. `hub.model_for_drive` puts the chosen model first and the realm's own connections after it, as fallbacks. When a run changes model (`answered_by`), the history up to that point counts as foreign (decision 17's `portable`), so the new model never receives another provider's reasoning blocks. A chosen model's settings come from `models::choice_settings`. It gives that provider its own credential and nothing else: no `OPENAI_API_KEY` or `OPENAI_BASE_URL` fallback, and no effort setting tuned for another provider.

*Why:* the owner wanted "to change LLM depending on the request, like Grok, OpenAI or DeepSeek". The generic `OPENAI_API_KEY` fallback of the OpenAI-compatible provider would otherwise have sent the OpenAI key to DeepSeek when no DeepSeek key was set.

*Cost:* model lists depend on each provider's API, and filtering by name is a guess (a few non-chat models may show). The fallback to the realm's connections means a failing chosen provider is replaced silently, apart from a warning in the log.

## 20. Connectors are remote MCP servers, connected with OAuth like in Claude or ChatGPT

A connector is a remote MCP server (Streamable HTTP) stored with the realm's other MCP servers (decision 18), with a `url` instead of a command. "Connect" follows the MCP authorization spec in `tools/remote.rs`. The server's 401 points to its protected-resource metadata (RFC 9728), which names the authorization server. Its metadata comes from RFC 8414 or OpenID locations, with and without path insertion. Airth then registers itself as a public client (RFC 7591), and the owner's browser opens the sign-in page with PKCE (S256, through `aws-lc-rs`, already built for TLS) and the `resource` parameter (RFC 8707). The browser comes back to `127.0.0.1:<port>/oauth/callback`, the one route without the API token. It accepts only a `state` Airth issued in the last 15 minutes, once. Tokens go to `skills/mcp-auth/<name>.json` (mode 600), in the home, so no command can read them, and they are refreshed when they expire or are refused. Tool calls open a session (`initialize`, `Mcp-Session-Id`), accept JSON or SSE answers, and follow the `skill` rule like other MCP tools. The Connectors tab features the official servers checked to support this flow end to end (Notion, Linear, Stripe, Vercel, Sentry, PayPal, Canva, Intercom, Figma, Webflow, Cloudflare, Square, Supabase), with Simple Icons logos embedded as paths (the CSP forbids remote images). It also searches the official MCP Registry for other Streamable HTTP servers that need no API key. The API-key presets of decision 19 stay below, for services without OAuth.

*Why:* the owner wanted connectors "like on Claude / ChatGPT: click Connect, the sign-in page opens and the protocol connects", with logos and a directory. Those products use exactly this: remote MCP servers with OAuth and dynamic registration, and the MCP Registry is the public directory.

*Cost:* services without dynamic client registration (GitHub, Asana, HubSpot, Box, Monday…) cannot be connected this way yet; they need an app the owner registers by hand. Gmail and Google have no such server: they stay reachable through the macOS account and the Mac app tools. Registry entries are not vetted, which the app says next to each address. Legacy SSE-only servers are not supported.

## 19. Connectors are integration presets, checked when connected

The connectors catalog (`src/connectors.rs`) is a fixed list of services with a well-known API. Each HTTP connector carries the official base URL, the authentication header with a `[secret:NAME]` placeholder, any header the API requires (`Notion-Version`, GitHub's `Accept`…), and a short note telling the agent how to use the API. Connecting saves the pasted token as the realm's credential `NAME`, then adds an ordinary integration (decision 10) named after the connector. One harmless authenticated request then proves the token works (`GET /user`, `auth.test`…). A refused token stays saved, to fix or disconnect. Disconnecting removes the integration, and the credential if nothing else uses it. Google and iCloud need no token: macOS holds the account, and runs reach it with the Mac app tools (decision 16).

*Why:* the owner asked for "a catalog of connectors (Google, Notion, Linear…)". Integrations already confine a credential to one base URL and keep it from the model; a preset removes the setup (base URL, header format, required headers) where mistakes happen, and the check tells the owner right away whether it works. Google's OAuth tokens last an hour, so a pasted token would silently stop working; the macOS account is refreshed by the system.

*Cost:* no OAuth flow yet, so only services that issue long-lived tokens are in the catalog. Jira, Shopify and other per-site APIs are left to the generic Integrations form. The check runs on the app's request thread (10 seconds at most).

## 18. MCP servers and catalog skills extend a realm, under the realm's own rules

**MCP.** A realm can list stdio MCP servers (`skills/mcp.json`): a name and the shell command that starts one. Connecting (from the app) starts it exactly as a run's command would start, through the same `sandboxed_command` as `execute_command`: in the realm's sandbox, from its folder, with only the passed-through variables and the realm's credentials. Airth then does the MCP handshake and keeps the tool list. During a run, each tool of an enabled server becomes `mcp__<server>__<tool>`, under the `skill` capability and scoped to that tool, so the realm's skill rule applies and "allow for this run" covers one tool. Every call starts the server, initializes it, calls the tool and kills the whole process group. Requests from the server (roots, sampling, elicitation) are declined, and errors are redacted like any command output. Connecting runs on a background thread, because the app serves requests one at a time and a first `npx` start can take a minute.

**Catalog.** The Skills page lists the Agent Skills of `github.com/anthropics/skills`: a folder with a `SKILL.md` and its references and scripts. The index (the repository tree, then each front matter) is cached for 12 hours in the home. Installing one copies its text files (no binaries, nothing over 300 kB, at most 3 MB) into `skills/catalog/<name>/`. Files are staged first, so a failed download keeps the previous install. Paths are checked to stay inside the folder. Runs get one tool, `use_skill`, under the `read` capability. Its description lists each installed skill's name and description; called, it returns `SKILL.md` or one of the skill's files.

*Why:* the owner asked to "integrate the skills catalog and open up to MCP". Both bring capabilities Airth would otherwise develop itself (decision 14), without giving up isolation. A server is a command like any other: same sandbox, same credentials boundary, same approval. A catalog skill reaches the model as a tool result, so it is data the agent weighs, not text in the system prompt that could outrank the owner.

*Cost:* one process per MCP call. Servers that keep state between calls, or speak HTTP/SSE instead of stdio, are not supported yet. A catalog skill's scripts sit in the home, out of the sandbox's reach: a run must copy one into the workspace (an approved write) before running it. The catalog is a single source for now, and GitHub's unauthenticated limit (60 requests an hour) caps how often the index can be refreshed.

## 17. Several model connections routed by the ask, and one North Star per realm

**Routing.** Besides its main connection, a realm may configure a fast one (`AIRTH_FAST_MODEL`, ...) and a fallback (`AIRTH_FALLBACK_MODEL`, ...), each inheriting the main settings it does not override. With a fast connection, the fast model triages the latest ask in one short call with no tools (SIMPLE or COMPLEX, COMPLEX when in doubt; asks over 1,500 characters skip triage). The run records its tier and keeps it: a follow-up can move it to the main model, never back, and skill development and brainstorming always use the main model. Every drive tries its connections in order (its tier's first, then the others) and stays on the one that answered. When a run changes model, reasoning and provider-specific blocks written by the previous model are left out of what the new one sees, since their signatures only mean something to the model that wrote them. Triage calls count in the run's usage.

**North Star.** A realm can have one goal, `northstar.json`, set only by the owner (app or `airth northstar`). Goal and milestones are defined in a guided run on the main model (started as soon as the owner writes the goal): it interviews the owner (3 to 5 questions), proposes a sharper goal, milestones each with a "done when", facts to remember and a rhythm, and records what the owner agreed to through the `north_star` and `remember` tools. Starting that conversation is the owner's go-ahead for recording milestones, so the run is created with a grant for the `north star` scope; rewording the goal (`set_goal`, scope `north star goal`) is the owner's decision alone: the gate asks for it every time, even where the realm allows memory, and never grants it for a run, so no scheduled run can change the goal. Every run's briefing carries the goal and milestones, with the instruction to prefer the next milestone's work, to take on nothing unrelated unprompted, and to record progress. `north_star` uses the `memory` capability, scoped `north star`, so it follows the realm's memory rule, a run grant covers only it, and the scheduled objective `north-star` (daily push) is pre-approved for it alone. A run counts as serving the goal if it was started for it (a brainstorm, a push, the objective) or recorded progress on a milestone. The app's focus mode shadows the rest.

*Why:* the owner asked to "switch between LLM connections depending on the complexity of the ask", and a run had just failed on exhausted Anthropic credit with no way to continue elsewhere. They also asked for one North Star whose milestones they brainstorm with Airth, which Airth helps reach all day, with visible progress and everything unrelated shadowed. Spec §3.7 anticipated routing by "capability, latency, privacy requirements, and cost".

*Cost:* one extra small model call per new ask when a fast connection exists. Triage reads untrusted text, so a crafted ask can at worst get itself the cheaper model. The tier is per run, not per step, so a simple-looking ask that turns out hard stays on the fast model until a follow-up (the owner can rephrase or start a new run). Dropping a previous model's thinking blocks on a switch costs the new model that reasoning. Focus is a display choice: it hides nothing and denies nothing.

## 16. A native Mac app, and Mac apps as fixed, approved actions

`Airth.app` is a real app bundle around Airth's own binary. Started without arguments from the bundle, it opens a native window (WKWebView through `wry`/`tao`) on the same app the browser shows, with a Dock icon, a menu bar icon and ⌥⇧A to show or hide it. Closing the window keeps Airth in the menu bar. It talks to the background service when one runs, otherwise it serves the app itself. The service and the `airth` command (a link into the bundle) run the same binary, so one update covers all three. The bundle is signed ad hoc, and again after each update.

Airth works with the owner's Mac apps through tools: `calendar`, `reminders`, `notes`, `mail_draft`, `safari`, `clipboard`, `open_on_mac`. Each runs a fixed JavaScript-for-Automation script shipped with Airth, with the model's input passed only as JSON data. There is no tool that runs arbitrary AppleScript, which could reach a shell and bypass the sandbox. Mail drafts are opened for the owner to send, never sent. A new `apps` capability (`ask` by default) governs them per realm, scoped per app, so the owner can "allow Calendar for this run" or pre-approve it in an objective. macOS adds its own prompt the first time Airth controls each app.

*Why:* the owner wants a personal assistant that lives on their computer like other desktop assistants and acts in their apps, without giving up the approvals and audit that govern everything else.

*Cost:* Calendar, Reminders and Notes are shared by all realms, so a realm's `apps` rule decides whether that realm may use them. Ad-hoc signatures change with each update, so macOS may ask again for automation permissions after an update. Dictation uses the page's speech recognition when available, else macOS dictation.

## 15a. Airth updates itself from its source repository, only after its tests pass

Airth records at build time the commit and the checkout it was built from. The running app checks every minute whether that checkout's branch has moved forward, and every 30 minutes whether its upstream has. It prepares a newer version away from the owner's checkout, in a detached worktree in the home: `cargo build --release`, then `cargo test --release`. Only a version whose tests pass is offered ("Restart to update"), and only the owner's click (or `airth update`) installs it. The binary is swapped atomically and the previous one kept (`airth update --rollback`). The login service exits and launchd starts the new binary; a terminal `airth ui` replaces itself in place. The app's access key is kept in the home, so the open page reconnects after the restart.

*Why:* "controlled software updates" (spec §7), and the self-improvement loop of §8 ending in "approve integration": an improvement Airth made to itself and the owner adopted reaches the running Airth through the same gate. A failed build or test never touches the installed binary.

*Cost:* updates need the source checkout and a Rust toolchain on the machine. Release builds take a few minutes. Runs in progress are stopped by the restart; they can be resumed.

## 15b. A second provider speaks the OpenAI-compatible Chat Completions format

Besides Anthropic, `AIRTH_PROVIDER` can be `openai`, `xai` (Grok) or `ollama` (local models). All three go through one provider that speaks the Chat Completions format. Presets set the base URL and the key variable (`OPENAI_API_KEY`, `XAI_API_KEY`, none for Ollama), and `OPENAI_BASE_URL` points it at any other compatible server. `AIRTH_MODEL` is required for these providers, so Airth never guesses a model name. The new keys are provider keys: never given to commands, redacted from everything the model sees, and storable once for all realms. Per-realm settings mean one realm can run on a local model while another bills a cloud key.

The translation is lossy only where the formats differ. Tool results become `tool` messages placed right after the assistant's calls. Another provider's reasoning and opaque blocks are dropped, which lets a run continue after its realm switches provider. Tool calls decide the stop reason even when a server reports `stop`. Missing call ids are generated.

*Why:* spec §3.7 says "no single provider should be architecturally mandatory", and §3.6 says remote models are optional. Comparable assistants run on many models, and a local model keeps a realm's data on the machine. One wire format covers most of the market without a dependency per vendor.

*Cost:* no prompt caching or reasoning display on these providers. Smaller local models follow the tool protocol less reliably, so runs can take more steps or stop early. There is no routing yet: the realm picks one model for all its runs.

## 14. Skills are tested programs the owner installs, not changes to Airth itself

A skill is a folder with `skill.json` (name, description, input schema, a shell command, a timeout), the program, and `tests.json` (inputs, fixture files, expected exit code and output). Installed, it becomes a tool of the realm's runs. Its command runs in the run's sandbox and workspace, gets the input JSON on stdin, and finds its own files in `$AIRTH_SKILL_DIR`. Calls go through a new `skill` capability (`ask` by default), scoped per skill, so "allow for this run" and objectives can approve one skill.

The lifecycle follows spec §8:
- **Discovery.** A run calls `propose_skill` when it hits a limitation. This records the proposal and notifies the owner; nothing is built.
- **Isolated development.** A development run (started by the owner, from the app or `airth skill develop`) has the skill's draft folder as its whole workspace: no realm files, no credentials. It gets a `test_skill` tool.
- **Evaluation.** The runtime checks the manifest and runs every test in the sandbox, each in a fresh folder, denied the Airth home, every realm folder (its own included) and the draft's write access. It does this whenever a development run ends.
- **Controlled installation.** Only the owner installs: `airth skill install`, or the app. It copies exactly the files that passed, checked by hash. Runs load only installed skills whose files still match that hash.

*Why:* v0.5's acceptance test: "Airth identifies a missing capability, implements a solution, tests it, and requests approval before enabling it." Skills extend Airth without touching its binary or its security code. Changes to Airth itself still go through isolated worktrees and checks (decision 5), and §3.5 forbids self-modification of security boundaries without authorization. Language-agnostic programs over stdin/stdout keep skills small and reviewable.

*Cost:* skills have no package management: they use what the machine has. Tests get no credentials, so a skill calling an authenticated API cannot be tested end to end. Evaluation checks behavior against the skill's own tests; the owner's review is what judges whether those tests are enough.

## 13. The app server is the background runtime

`airth serve` (or `airth ui`) runs the app server and the scheduler in one process. It writes its port and access key to `server.json` in the home (mode 600), so a second `airth ui` opens the running app instead of starting another, and only one scheduler ever runs per home. `airth service install` registers `airth serve` with launchd (macOS) to start at login and restart if it stops. Model provider settings can be stored once for all realms (`airth secret set ANTHROPIC_API_KEY --all-realms`), because a login service does not see the shell's environment.

*Why:* "background runtime" (v0.4), with the smallest number of moving parts. Unattended runs must be visible and answerable in the same app as the others, so they belong in the same process.

*Cost:* Linux has no service installer yet (a systemd user unit running `airth serve` works). The service runs the binary it was installed with; after a rebuild it must be installed again.

## 12. Notifications and summaries go to an inbox

Each objective's run ends with a notification: completed, failed, stopped, or a change ready for review. An unattended run waiting for approval sends one too. Notifications are kept in the home (the last 500), shown in the app's inbox and by `airth inbox`. On macOS a desktop notification shows the realm and the event only, never the run's content. `airth summary` and the inbox page give a cross-realm view: what ran, what waits for review, what can be resumed, what is scheduled.

*Why:* "notifications and summaries" (v0.4). Autonomy is only useful if the owner learns what happened without reading every run.

*Cost:* there is no email, phone or chat channel yet. The summary is built from the runs' own reports.

## 11. Objectives run with approvals and budgets given in advance

A persistent objective is an objective plus a trigger: a schedule (`every 30m`, `daily 08:30`, `weekdays 09:00`, in local time), a watched path in the realm, a local webhook (a secret URL whose request body reaches the run as untrusted data), or manual. Only the owner creates objectives (CLI or app); no tool can. When a trigger fires, the run starts with the objective's approvals as the run's grants (decision 9) and its budget. Budgets cap model calls, tokens and wall-clock minutes for every drive of the run. Anything else the run needs waits for the owner in the app for as long as the objective says (`0` = deny at once), and is then denied with a reason that tells the agent to go on without it and report it. A schedule missed while Airth was not running fires once, not once per missed time. A watch ignores changes the run itself made.

*Why:* v0.4's acceptance test: "Airth completes an approved scheduled objective while the owner is absent". "Approved" has to mean something concrete when nobody is there to answer: what runs may do is decided when the objective is created, never while it runs. Spec §5 also asks that "every execution must have a bounded scope, observable state, and explicit termination conditions".

*Cost:* webhooks are reachable only from this machine (the server listens on loopback). A watch polls every 10 seconds and fingerprints names, sizes and modification times, not contents.

## 10. Integrations bind credentials to a base URL

An integration is a name, a base URL and headers such as `Authorization: Bearer [secret:GITHUB_TOKEN]`, stored in the realm's settings. The `http_request` tool takes an integration name and a path. The runtime checks that the URL stays under the base URL (same scheme, host and port, path prefix, no encoded `..` or `/`), fills in the credential, and sends the request. Requests without an integration carry no credentials, and may only reach public addresses: the host is resolved once, every address is checked, and the connection is pinned to it. Redirects are never followed. Network access is a capability in the realm's policy (`ask` by default).

*Why:* "tool integrations" (v0.3) with "realm-scoped credentials" and "least-privilege tool access" (spec §7). A design where the model writes `[secret:NAME]` wherever it likes would let a prompt-injected run send a token to any host. Binding a credential to its destination makes that impossible whatever the model does.

*Cost:* only HTTP APIs with header authentication are covered (no OAuth flows, no MCP yet). Shell commands are still not restricted on the network: an approved `curl` can reach anything, though without the realm's integration credentials unless the owner also gave them to commands.

## 9. Approvals can be granted for the rest of a run

Besides yes and no, the owner can answer "allow for this run". This grants the capability for that run, scoped where the tool defines a scope: one integration, or one host. A grant never covers a request with a warning (edits to Airth's security files, say), which is always asked one by one. Grants are saved with the run, so a resumed run keeps them, and they end with the run. They never change the realm's policy.

*Why:* "approval workflows" (v0.3). A multi-step task that asks for the same kind of thing twenty times trains the owner to approve without reading. A narrow, run-scoped grant keeps the owner in charge while asking only questions worth reading.

*Cost:* "all commands for this run" is broad. The sandbox still bounds it, and the owner can stop the run.

## 8. Runs keep a plan, and resume from it

The agent keeps its plan with an `update_plan` tool: the whole list of steps, each `pending`, `in_progress`, `done`, `blocked` or `skipped`, with an optional note. The plan is saved with the run, shown live in the app and the terminal, and needs no approval (it changes nothing outside the run). Any run that did not complete (interrupted, stopped, or failed after the model provider's retries ran out) can be resumed with `airth resume` or the app's Resume button. Resuming answers tool calls the interruption cut off ("not run: the run was interrupted", or the recorded result if the tool did run), keeps the run's grants, and tells the agent how the run ended and what its plan was, asking it to check what actually took effect before going on.

*Why:* "persistent tasks" and "task planning and recovery" (v0.3), whose acceptance test is a multi-step task that "resumes after interruption". Runs were already saved after every step; what was missing was a way for the agent and the owner to see where a task stood, and a safe way to pick it up.

*Cost:* the plan is only as good as the agent keeps it. Recovery is the agent's judgment, guided: the runtime cannot know whether an interrupted external action (a POST, a command) took effect.

## 7. Credentials live with their realm, and models only see placeholders

Each realm keeps credentials in `secrets.env` (mode 600) in Airth's home. A run's commands get a minimal environment (`PATH`, `HOME`, locale, toolchain variables) plus that realm's credentials. They do not inherit Airth's own environment. The runtime replaces every credential value in tool results with `[secret:NAME]` before the model or the run log sees it. Model provider settings (`ANTHROPIC_API_KEY`, ...) can also be set per realm. They configure that realm's model and are never passed to commands.

*Why:* realm-scoped credentials and secret redaction (spec §7). The owner's shell often exports tokens meant for something else, and inheriting the environment would hand those to every realm.

*Cost:* commands that need another variable must get it as a realm credential. A secret can still leak if a command transforms it before printing (base64, say). Redaction is a safety net; the sandbox and approval are the boundary.

## 6. Memory is per realm, owner-editable, and written by a gated tool

Memory is a list of short entries per realm, each with its provenance (`owner` or `run:<id>`) and date. Runs see the realm's memory in their system prompt. The agent adds entries with the `remember` tool, governed by the realm's `memory` rule (`ask` by default, so a prompt-injected "fact" needs the owner's approval to persist). The owner lists, adds and deletes entries from the CLI or the app. The owner profile is separate, written only by the owner, and shared with a realm's runs unless the realm turns that off.

*Why:* memory must be inspectable, editable and deletable, with provenance, and must stay inside its realm (spec §3.2, §6). It must not need an external memory system (§6). A flat list is the smallest thing that meets this.

*Cost:* there is no retrieval. Everything in the realm's memory goes into every prompt, capped at 300 entries of 1000 characters. Retention policies are not implemented.

## 5. Changes are made in worktrees and adopted only after checks pass

In a realm whose folder is a git repository, each run gets a git worktree on branch `airth/<run id>`. When the agent finishes, the runtime commits the changes (hooks disabled) and runs the realm's checks in the run's sandbox. Adoption merges with `--no-ff` into the owner's current branch. It requires the checks to have passed on the latest commit, and aborts cleanly on conflict. Discarding removes the worktree and the branch. In an isolated run, the original checkout is read-only, from both file tools and the sandbox. Cargo builds share a per-realm target directory, so a fresh worktree does not rebuild everything.

*Why:* v0.1's acceptance test is to "modify an isolated working copy, run checks, and report". Spec §7 says "self-modification must occur in an isolated workspace and require verification before adoption".

*Cost:* a worktree starts from the last commit, so uncommitted edits in the owner's checkout are not visible to the run. Realms whose folder is not a git repository work in place, guarded only by approval.

## 4. Commands run in an OS sandbox derived from the same access rules

A workspace is a list of path rules (`none`, `read`, `read-write`), where the longest matching prefix wins. The run's root is read-write. Airth's home and every other realm's folder are `none`. In an isolated run, the original checkout is `read`, and only the git data a worktree needs is writable. File tools check these rules on real paths (symlinks resolved). On macOS, commands run under `sandbox-exec` with a profile generated from the same rules: only file *contents* are denied, not metadata, so a command can cross a denied directory to reach an allowed one.

*Why:* "the runtime must enforce realm isolation independently of LLM instructions" (spec §4). Before this, an approved `cat ~/other-realm/...` would have worked.

*Cost:* only macOS is covered. On Linux, commands are unconfined (Landlock or bubblewrap would be the next step), and `airth doctor` reports it. Network access is not restricted.

## 3. Realms and the owner live in a home outside every workspace

All owner data lives in `~/.airth` (or `$AIRTH_HOME`): the profile, and one directory per realm with its settings, memory, credentials, runs and worktrees. The home is `none` in every workspace, so no tool can read or change policies, credentials or another realm's runs. This holds even when a realm's folder contains the home (a realm rooted at `~`). The CLI picks the realm whose folder contains the current directory, else the default realm. Runs of v0.1 (in `<repo>/.airth/runs`) are imported when a realm is created on that folder.

*Why:* the spec organizes Airth around the owner and their realms (§3.1, §3.3), not around a repository. The permission policy must also be out of the agent's reach, even though it is now editable per realm.

*Cost:* one more concept to set up: there is no realm until the owner creates one.

## 2. One hub, many clients

The CLI and the local app both go through `Hub`, which builds each run's permission gate from the realm's policy and its workspace from the realm's isolation rules. A client decides *who* approves (terminal or browser), never *whether* approval is needed or what a run may reach. The app binds to 127.0.0.1 and requires a per-launch token in a custom header (so other sites cannot call it without a preflight, which is never granted). It also checks the `Host` header against DNS rebinding.

## 1. Blocking Rust, raw HTTP, few dependencies

Airth uses blocking I/O with threads: one per live run in the app, and threads to drain command output. There is no async runtime. The Anthropic Messages API is called over raw HTTP (there is no official Rust SDK), behind a provider-neutral `Model` trait. Run state is saved as JSON, written atomically after every step, so a crash loses at most the step in flight.

*Why:* "small dependency footprint" and "idiomatic asynchronous execution where justified" (spec §11). One owner's handful of concurrent runs does not justify async.
