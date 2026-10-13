# Architecture decisions

Short records of the choices behind Airth's structure, newest first. Each one says what was decided, why, and what it costs.

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

## 15. Airth updates itself from its source repository, only after its tests pass

Airth records at build time the commit and the checkout it was built from. The running app checks every minute whether that checkout's branch has moved forward, and every 30 minutes whether its upstream has. It prepares a newer version away from the owner's checkout, in a detached worktree in the home: `cargo build --release`, then `cargo test --release`. Only a version whose tests pass is offered ("Restart to update"), and only the owner's click (or `airth update`) installs it. The binary is swapped atomically and the previous one kept (`airth update --rollback`). The login service exits and launchd starts the new binary; a terminal `airth ui` replaces itself in place. The app's access key is kept in the home, so the open page reconnects after the restart.

*Why:* "controlled software updates" (spec §7), and the self-improvement loop of §8 ending in "approve integration": an improvement Airth made to itself and the owner adopted reaches the running Airth through the same gate. A failed build or test never touches the installed binary.

*Cost:* updates need the source checkout and a Rust toolchain on the machine. Release builds take a few minutes. Runs in progress are stopped by the restart; they can be resumed.

## 15. A second provider speaks the OpenAI-compatible Chat Completions format

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
