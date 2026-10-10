# Architecture decisions

Short records of the choices behind Airth's structure, newest first. Each one says what was decided, why, and what it costs.

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
