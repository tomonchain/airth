# Airth

**A personal AI agent that works for you in the background, on your machine, inside the limits you set.**

Give Airth a goal ("triage new issues every morning", "reach 100 sales by December") and it plans, acts, checks its own work and reports back. It runs locally, written in Rust, and keeps its memory between chats. One owner, multiple realms, one intelligence.

Free to use, fair source: see [License](#license).

## Why Airth

- **Realms keep your life apart.** Your job, your company and a side project each get their own folder, permissions, memory and credentials. Nothing crosses between them, and the runtime enforces it (on macOS, commands run in a sandbox).
- **You review before anything lands.** Each chat works in its own git worktree. You read the diff and the test results, then adopt or discard, or merge its pull request on GitHub.
- **It works on its own, within bounds.** Schedules, file watches and webhooks start work for you. What a run may do is decided in advance, and anything else waits for your approval.
- **One goal at a time.** Set a North Star and Airth picks the next step toward it every day.
- **Your keys, your models.** Claude, GPT, Grok, DeepSeek, Mistral, OpenRouter, or a fully local model with Ollama. Credentials are never shown to the model.
- **It extends itself.** It can propose new skills, and you review and install them. It also connects to Notion, Linear, Stripe, GitHub and other MCP services.

## Quick start

You need [Rust](https://rustup.rs) and an [Anthropic API key](https://console.anthropic.com). Other providers work too, see [Configuration](#configuration).

```bash
git clone https://github.com/tomonchain/airth && cd airth
cargo install --path .                            # installs ~/.cargo/bin/airth
export ANTHROPIC_API_KEY=...
airth realm create myproject --root ~/code/myproject --description "My project"
airth doctor                                      # checks the setup
airth ui                                          # opens the app in your browser
airth run "Find and fix one small bug, with a test."
```

Then run `airth review last` to read what it changed, and `airth adopt <run>` to merge it. On a Mac, [Installing on a Mac](#installing-on-a-mac) adds a native app that starts at login.

See [SPECS_V1.1.md](SPECS_V1.1.md) for the vision and roadmap, and [docs/decisions.md](docs/decisions.md) for how it is built.

## Realms

A realm is an isolated area of your life: personal, a job, a company, a side project. Each realm has its own:

- **folder**: a directory you choose (`--root`), or a private one in Airth's home;
- **permissions**: per capability (`read`, `write`, `execute`, `memory`, `network`, `skill`, `apps`), each `allow`, `ask` or `deny`;
- **memory**: facts Airth keeps between runs, each with where it came from;
- **credentials**: given to that realm's commands as environment variables, never shown to the model;
- **integrations**: services its runs may call, with credentials bound to them (see below);
- **runs**.

Nothing crosses between realms. Airth's runtime enforces this; it does not rely on the model behaving:

- File tools refuse paths in other realms' folders and in Airth's home, symlinks included.
- On macOS, every command runs under `sandbox-exec` with the same rules. Elsewhere, commands are not confined yet, and `airth doctor` says so.
- Commands get a minimal environment plus the realm's own credentials, not Airth's environment, so your API keys don't leak into them.
- Credential values are replaced with `[secret:NAME]` in everything the model sees.

The CLI picks the realm whose folder contains the current directory, else the default realm. Use `--realm <name>` to choose one explicitly.

```bash
airth realm create acme --root ~/work/acme --description "My job at Acme"
airth realm list
airth realm show
airth realm policy execute allow   # stop asking before commands in this realm
airth realm checks "cargo check --all-targets" "cargo test"
airth realm use personal           # the default realm
airth memory add "Release notes go in CHANGELOG.md"
airth secret set GITHUB_TOKEN      # read from stdin, not echoed
airth owner set name "Thomas"
airth owner add boundaries "Never push to main"
```

## Tasks: plans, approvals, resuming

Give Airth an objective, not steps. For anything beyond a couple of steps it writes a plan and keeps it current as it works. You see the plan live in the app (and in the terminal).

When Airth needs approval, you can answer yes, no (with a reason it sees), or yes **for the rest of this run**: all file changes, all commands, or requests to one integration or host. Requests with a warning, like edits to Airth's own security code, are always asked one by one. Grants end with the run.

A run that did not finish can be resumed. That covers a closed terminal, a crash, a stop, or a model outage that outlasted the retries. Airth answers the calls the interruption cut off and tells the agent how the run ended and where its plan stood. The agent checks what actually happened before going on.

```bash
airth resume last      # or the Resume button in the app
```

## Working on its own

An **objective** is work Airth starts by itself: on a schedule, when files change in the realm, or when something on your machine calls its webhook. You decide in advance what its runs may do. Anything else waits for you in the app (10 minutes by default), then is denied, and the run reports what it needed.

```bash
airth objective add morning-triage "Triage new issues and write today's summary to triage.md" \
  --every "weekdays 08:30" --allow network:tracker --allow write --max-minutes 20
airth objective add filer "File the notes dropped in inbox/" --watch inbox --allow write
airth objective add deploy-check "Check the deploy that just ran" --webhook
airth objective list                # triggers, next run, last result
airth objective run morning-triage  # now, in this terminal
```

Every run, started by an objective or not, can have a **budget**: `--max-iterations`, `--max-tokens`, `--max-minutes`.

Routines (called objectives in the CLI) run while Airth runs in the background:

```bash
airth secret set ANTHROPIC_API_KEY --all-realms   # the service does not see your shell
airth service install    # macOS: start at login, keep running (or run `airth serve` yourself)
airth ui                 # opens the running app
```

When a run ends, Airth notifies you (on macOS, also on the desktop, showing only the realm and the event). Catch up in the app's **Inbox**, or:

```bash
airth inbox        # new notifications
airth summary      # the last 24 h across realms: what ran, what waits for review, what's next
```

## North Star: one goal, every day

Give a realm a **North Star**, the one goal its work serves ("100 kit sales by December"). From the realm's home in the app:

1. **Define it with Airth.** Write the goal (when you set it, or when you create the realm) and Airth starts a short conversation: it asks you 3 to 5 questions (where things stand, what success looks like and by when, your constraints, what to delegate, the main risk), then proposes a sharper, measurable goal, 3 to 7 milestones each with how you will know it is done, the facts worth remembering, and a rhythm to push it forward. You adjust it in the run; when you agree, it records everything. Changing the goal itself always asks you, whatever the realm's policy. "Refine with Airth" reworks the plan later.
2. **Push it forward now**, or let **Airth push it forward on its own** (default `weekdays 09:00`, or `every 3h` for all day). Each push takes the next milestone, does the most valuable step it can complete and verify, records the progress, and reports to your inbox. Those runs may record North Star progress without asking; everything else follows the realm's rules.
3. **See the progress.** The goal and its progress bar stay at the top of the sidebar. Click a milestone's mark to set its status yourself.

Every run of the realm sees the North Star and its milestones, prefers the work that moves the next one, and records what it advanced. **Focus** (on by default, per realm, in the sidebar) shadows the chats and routines that did not serve the goal, and the New chat page suggests goal-oriented work instead of generic ideas. The goal and its milestones live on their own North Star page (click the goal in the sidebar). Chats that need your approval are never shadowed.

```bash
airth northstar set "100 kit sales by December" --why "Fund the next product"
airth northstar show                 # goal, milestones, progress
airth northstar milestone m2 done    # set a status yourself
airth northstar daily "every 3h"     # or "off"
```

## Skills: Airth extends itself

When a run needs a capability Airth lacks, it can **propose a skill**: a small program with tests that would become a new tool. You decide what happens next:

1. **Develop:** Airth builds it in the skill's own folder, which cannot reach the rest of the realm or its credentials.
2. **Evaluate:** the runtime runs the skill's tests in the sandbox, and you get a notification with the result.
3. **Install:** you review the files and the test results, then install it. Only exactly the files that passed can be installed, and from then on runs in that realm can use it as a tool, under the realm's `skill` rule.

```bash
airth skill list
airth skill develop word_count               # a proposed skill (or --description "..." for a new one)
airth skill show word_count --source         # review it
airth skill install word_count
airth skill develop word_count --feedback "Also count lines"   # rework it, then install again
```

The app's **Skills** page does the same: develop, review files and test results, ask for changes, install, uninstall. `airth tools` lists every tool a run has, installed skills included.

The same page has more tabs:

- **Connectors:** like the connectors of Claude or ChatGPT. Click **Connect** on Notion, Linear, Stripe, Vercel, Sentry, PayPal, Canva, Intercom, Figma, Webflow, Cloudflare, Square or Supabase: the service's sign-in page opens in your browser, and once you accept, Airth keeps the access in the realm (renewed on its own, never readable by commands or the model) and runs get the service's tools. Search the official [MCP Registry](https://registry.modelcontextprotocol.io) for more, or add a custom server URL. Services that only issue API keys (Lemon Squeezy, Qonto, Brevo, GitHub…) are below: paste a token once, and Airth binds it to the service's official API and checks it. Gmail and Google go through macOS (System Settings → Internet Accounts) and the Mac app tools.
- **Catalog:** ready-made Agent Skills from [github.com/anthropics/skills](https://github.com/anthropics/skills) (PDF, Word, slides, design, MCP building…). Install one in a realm and its runs see it in their `use_skill` tool: they read its instructions when a task matches, as guidance that never outranks your boundaries. Binaries and very large files are left out, and a skill's scripts run only once copied into the workspace, with your approval.
- **MCP servers:** add a stdio [Model Context Protocol](https://modelcontextprotocol.io) server by the command that starts it (e.g. `npx -y @modelcontextprotocol/server-memory`). Airth connects it in the realm's sandbox, with the realm's credentials (put its token under Realm settings → Access), and lists its tools. Runs get them as `mcp__<server>__<tool>`, under the realm's `skill` rule. Each call starts the server and stops it afterwards.

In the app, a run is a **chat**. Chats can be renamed: click a chat's title, or double-click it in the sidebar. The name is only for you; the objective the agent works on does not change. What you type in a message box stays there as a draft (per chat, and per realm on the new-chat screen) when you switch chats or reload; you can write your next message while Airth works, and send it when it stops.

Changed the app's page? `node scripts/ui_smoke.js` renders every view in a fake DOM with data full of empty fields, and fails if any visible text says `null`, `false`, `undefined` or similar.

## Installing on a Mac

```bash
cargo install --path .                              # installs ~/.cargo/bin/airth (put it on your PATH)
airth secret set ANTHROPIC_API_KEY --all-realms     # the background service does not see your shell
airth service install                               # runs in the background, starts at login, adds Airth.app
```

Open **Airth.app** from Spotlight, Launchpad or the Dock. It is a native app with its own window: ⌥⇧A shows or hides it from anywhere, and closing the window keeps Airth in the menu bar. Use 🎙 in the message box to dictate. `airth ui` opens the same app in your browser.

The first time, a short guide walks you through it: who you are, your Anthropic API key (checked, then kept in `~/.airth/secrets.env` for every realm), and your first realm with its North Star. Reopen it anytime with ⌘K → Welcome guide.

With your approval, Airth works with your Mac apps: Calendar, Reminders, Notes, Mail (it only prepares drafts; you send them), Safari tabs, the clipboard, and opening apps and links. The realm's `apps` rule decides; answer "Allow Calendar for this run" to stop being asked about one app.

**Updates.** When the Airth checkout you installed from gets new commits (your own, an Airth improvement you adopted, or its upstream's), the app builds and tests the new version in the background. If the tests pass, it shows **New update available · Restart to update**. Nothing changes until you click. From the terminal: `airth update` (`--check` to only look, `--rollback` to go back one version). The app downloaded from GitHub Releases updates from the published releases instead: it checks each new version's signature before offering it (see [docs/RELEASING.md](docs/RELEASING.md)).

## Integrations

An integration is a service a realm's runs may call over HTTP, with the credentials you bind to it:

```bash
airth secret set GITHUB_TOKEN
airth integration add github https://api.github.com \
  -H "Authorization: Bearer [secret:GITHUB_TOKEN]" --description "GitHub REST API"
airth integration list
```

The agent calls it by name (`http_request` with `integration: "github"`). The runtime adds the credential only to requests under the base URL, so a token can never be sent anywhere else, and the model never sees it. Without an integration, requests carry no credentials and can only reach public addresses, not your local network. Requests need approval unless the realm's `network` rule is `allow`.

## Isolated changes

In a realm whose folder is a git repository, each run works in its own git worktree, on a branch `airth/<run id>`. Your checkout is never touched while the run works. When the run finishes:

1. The runtime commits what changed.
2. It runs the realm's checks (detected for Cargo projects).
3. You review and decide:

```bash
airth review last   # files, check results, full diff
airth adopt <run>   # merge into your checkout; the checks must have passed
airth discard <run> # throw it away
airth check <run>   # re-run the checks
```

The app shows the same: a change panel with the files, the checks, the diff, and Adopt or Discard. If the checks fail, an "Ask Airth to fix" button sends their output back to the agent.

**Several chats at once.** Each chat starts from your branch as it was. Before a change is adopted, Airth merges in what your branch got since, runs the checks again on the result, and only then merges: what lands is what was checked. After an adoption, the realm's other open changes are updated the same way, so a conflict shows early, in the chat it concerns, with a "Resolve with Airth" button: the chat resolves it itself, keeping its own intent and what changed meanwhile, then the checks run again. The panel also tells when two open chats touch the same files. Adoption refuses to overwrite edits you have not committed (it names the files).

```bash
airth resolve <run>  # have the chat resolve its conflict with the base branch
```

**Pull requests.** A realm whose repository is on GitHub can deliver changes as pull requests instead (Settings → Changes, or `airth realm delivery pr`):

1. Add a [fine-grained token](https://github.com/settings/personal-access-tokens) for the repository only, with *Contents* and *Pull requests* read and write: `airth secret set GITHUB_TOKEN`. The runtime pushes with it; chats never see it.
2. When a chat's checks pass, Airth pushes its branch and opens the pull request, with the chat's report and the check results.
3. You review and merge on GitHub (CI runs there too: `.github/workflows/ci.yml`). Airth follows each pull request: when the base branch moves it merges it in and pushes, a conflict goes back to the chat to resolve, and once merged the chat is marked adopted and its copy removed. Closing a pull request discards the change.

Protect the base branch on GitHub (require a pull request and passing checks): then nothing reaches it without your review, whatever a chat tries.

## The app

```bash
airth ui
```

This opens Airth in your browser, served locally on `127.0.0.1`. From there you can:
- switch realms;
- start chats and watch them live: reasoning, tool calls and results; archive or delete the ones you are done with;
- approve or deny each step that needs it, with an optional reason the agent sees;
- follow each chat's plan, and resume chats that did not finish;
- set up routines, and read your inbox and the daily summary;
- develop, review and install skills;
- review, adopt or discard changes;
- see token usage across realms: per day, realm, model and run, with the share served from the prompt cache (every model call is recorded in its run);
- switch between light, dark and system appearance (the sun/moon button, or your profile), and set the text size, reading width and whether ↵ or ⌘↵ sends;
- move around from the keyboard: ⌘K searches chats, pages, realms and actions; ⌘⇧O starts a new chat; ⌘B hides the sidebar;
- work on each realm's **Documents** like in a small code editor: the files on the left, open files in tabs, syntax colors, ⌘S to save, ⌘P to go to a file, previews for Markdown, images and PDF, files you drop in, and what Airth wrote or edited. ⌘L asks Airth about the file or the selected lines. A save never replaces a change made meanwhile (by a chat, say): you choose which to keep;
- edit each realm's permissions, checks, memory, credentials and integrations, and your profile.

The link printed in the terminal contains a private key for this launch; API calls without it are rejected. Use `--port`, `--no-open` and `--max-iterations` to change the defaults.

## Command line

```bash
airth run "<objective>"           # start a run in the current realm
airth send last "Now add docs"    # continue a run (by id or `last`)
airth resume last                 # resume a run that was interrupted, stopped or failed
airth runs                        # the realm's runs: status, plan progress, checks, objective
airth serve                       # the app and the scheduler, without opening a browser
airth tools                       # the tools (installed skills included), and this realm's rule for each
```

Approvals happen in the terminal: answer `y`, `a` to allow it for the rest of the run, or type a reason to deny, and the agent sees that reason. Ctrl-C stops a run cleanly; press it twice to quit at once. A run whose process was killed shows as `interrupted` and can still be continued.

## Configuration

**Pick the model per request.** The message box has a model menu (the brain icon). **Auto** uses the realm's model and routing; or pick any model of a provider whose key the realm has: Claude, GPT, Grok, DeepSeek, Mistral, or anything on OpenRouter. The lists come live from each provider, and you can type `provider/model` for one that is not listed. The choice sticks to the run until you change it. Each provider only ever receives its own key. Add keys under Realm settings → Access, or for every realm with `airth secret set DEEPSEEK_API_KEY --all-realms`.

| Variable | Default |
|---|---|
| `AIRTH_HOME` | `~/.airth` |
| `AIRTH_REALM` | the realm whose folder holds the current directory, else the default |
| `AIRTH_PROVIDER` | `anthropic` (also `openai`, `xai`, `ollama`) |
| `AIRTH_MODEL` | `claude-opus-5-5` (required for the other providers) |
| `AIRTH_EFFORT` | `high` (other providers: sent as `reasoning_effort` only if set) |
| `AIRTH_MAX_TOKENS` | `16000` |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` | — |
| `ANTHROPIC_WORKSPACE_ID` | — (needed for API keys not scoped to a workspace) |
| `OPENAI_API_KEY` | — (`openai`; also used by a custom endpoint) |
| `XAI_API_KEY` | — (`xai`, the Grok models) |
| `DEEPSEEK_API_KEY` | — (`deepseek`) |
| `MISTRAL_API_KEY` | — (`mistral`) |
| `OPENROUTER_API_KEY` | — (`openrouter`: most models with one key) |
| `OPENAI_BASE_URL` | the provider's (`https://api.openai.com/v1`, `https://api.x.ai/v1`, `http://localhost:11434/v1`) |

`openai`, `xai` and `ollama` share one OpenAI-compatible Chat Completions provider, so any compatible endpoint (LM Studio, OpenRouter, a vLLM server...) works with `OPENAI_BASE_URL`. For example, a fully local realm:

```bash
airth secret set AIRTH_PROVIDER     # ollama
airth secret set AIRTH_MODEL        # a local model with tool calling, e.g. qwen3
airth doctor                        # model provider  ollama/qwen3
```

Any of these model settings can also be set per realm as a credential (`airth secret set ANTHROPIC_API_KEY`), for example to bill a work realm to the employer's key, or once for every realm (`--all-realms`), which the background service needs. Model keys are used for the model only, and never passed to commands.

### Several models: fast for simple asks, a fallback for outages

A realm can have up to three connections. Each extra one takes the main settings and overrides some, under its prefix:

| Variable | What it does |
|---|---|
| `AIRTH_FAST_MODEL` | a quicker, cheaper model. It triages each new ask in one short call: simple asks (a lookup, a short text, a small edit) run on it, everything else on the main model. A run stays on its tier; a follow-up that needs more moves it to the main model, never back |
| `AIRTH_FAST_PROVIDER`, `AIRTH_FAST_EFFORT`, `AIRTH_FAST_MAX_TOKENS` | when they differ from the main ones |
| `AIRTH_FALLBACK_MODEL` (and `AIRTH_FALLBACK_PROVIDER`, ...) | used when the others fail: an outage, exhausted credit |

Whatever the tier, a connection that fails hands the run over to the next one (main → fallback → fast, or fast → main → fallback), so one provider running out of credit no longer stops the work. For example, Opus for real work, a local model for quick asks, Grok when Anthropic is down:

```bash
airth secret set AIRTH_FAST_PROVIDER --all-realms       # ollama
airth secret set AIRTH_FAST_MODEL --all-realms          # qwen3
airth secret set AIRTH_FALLBACK_PROVIDER --all-realms   # xai
airth secret set AIRTH_FALLBACK_MODEL --all-realms      # grok-4
airth doctor   # model provider  anthropic/claude-opus-5-5 · fast: ollama/qwen3 · fallback: xai/grok-4
```

The run's details show which tier it used; the Usage page counts the triage calls with the run.

Desktop notifications are on by default on macOS; `airth inbox --desktop off` turns them off.

## Where things live

```text
~/.airth/                      readable only by you; no run can reach it
  owner.json                   your profile
  config.json                  the default realm, notification settings
  secrets.env                  model settings for all realms (mode 600)
  notifications.json           your inbox
  server.json                  how to reach the running app (while it runs)
  logs/serve.log               the background service's log
  realms/<name>/
    realm.json                 folder, permissions, isolation, checks, integrations
    memory.json                long-term memory, with provenance
    objectives.json            objectives: triggers, approvals, budgets, last run
    northstar.json             the North Star, its milestones and the runs that moved them
    skills/                    skills.json, drafts/<skill>/, installed/<skill>/
    secrets.env                credentials (mode 600)
    runs/<id>.json             each run: conversation, plan, grants, tool events, change, checks
    worktrees/<id>/            isolated working copies
    files/                     the folder of realms without their own
```

## Layout

```text
src/
  main.rs         CLI: run, send, resume, resolve, runs, review, check, adopt, discard, ui, serve, doctor
  cli.rs          CLI: realm, owner, northstar, memory, secret, integration, objective, inbox, summary, skill
  hub.rs          the interface to agents: runs per realm, workspaces, checks, adoption
  home.rs         the Airth home: owner profile, realms, default realm
  realm.rs        a realm: settings, credentials, folders
  owner.rs        the owner profile
  memory.rs       a realm's long-term memory
  objective.rs    persistent objectives, schedules, file watches
  northstar.rs    a realm's North Star: goal, milestones, progress
  scheduler.rs    which objectives are due; cross-realm summaries
  usage.rs        token usage across realms, from each run's recorded model calls
  notify.rs       the owner's inbox and desktop notifications
  service.rs      the login service (launchd)
  skill.rs        skills: manifests, evaluation, installation
  integration.rs  integrations: base URLs with credentials bound to them
  isolation.rs    git worktrees per run: create, commit, diff, update with the base, conflicts, adopt, discard
  github.rs       pull requests on GitHub: push, open, follow, close
  sandbox.rs      the command sandbox (macOS sandbox-exec)
  ui/             the local app and background runtime: HTTP server, scheduler thread, webhooks
                  (mod.rs) and the page (index.html, theme.js, app.js, app.css)
                  editor/: the Documents editor (CodeMirror), bundled into
                  vendor/editor.js by editor/build.sh
  term.rs         terminal output for CLI runs
  cancel.rs       stop signal shared by the CLI, the app, the agent loop and tools
  agent.rs        the loop and the system prompt
  runtime.rs      the single path from a tool call to execution (permission check, redaction, event)
  permissions.rs  capabilities, rules, policy, run grants, terminal approval
  tools/          read_file, list_directory, write_file, edit_file, execute_command, http_request,
                  update_plan, remember, north_star, propose_skill, test_skill, installed skills; workspace access rules
  models/         provider-neutral Model trait; Anthropic and OpenAI-compatible (OpenAI, xAI, Ollama) providers;
                  router.rs: fast/main/fallback connections, triage, failover
  state.rs        persisted run state, plans
```

## License

Airth is **fair source**, under the [Functional Source License 1.1, Apache 2.0 future license](LICENSE.md) (FSL-1.1-ALv2):

- **Free to use**, for yourself or inside your company: install it, run it, read the code, change it, share your changes.
- **Not for competing products:** you may not offer Airth, or something built from it with the same purpose, as a commercial product or service (for example, hosted Airth). The hosted Airth offer is Hedgen Ltd's.
- **Becomes open source:** each version is also licensed under Apache 2.0 two years after its release.

Airth is made by Hedgen Ltd (United Kingdom), which holds the copyright and the name "Airth": forks must use another one.
