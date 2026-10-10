# Airth

A persistent, local-first personal agent, written in Rust. One owner, multiple realms, one intelligence. See [SPECS_V1.1.md](SPECS_V1.1.md) for the vision and roadmap, and [docs/decisions.md](docs/decisions.md) for how it is built.

```bash
export ANTHROPIC_API_KEY=...
cargo run -- realm create airth --root . --description "Airth's own source code"
cargo run -- doctor
cargo run -- ui                                   # the app
cargo run -- run "Identify and implement one small improvement to Airth."
```

## Realms

A realm is an isolated area of your life: personal, a job, a company, a side project. Each realm has its own:

- **folder**: a directory you choose (`--root`), or a private one in Airth's home;
- **permissions**: per capability (`read`, `write`, `execute`, `memory`, `network`, `skill`), each `allow`, `ask` or `deny`;
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

Objectives run while Airth runs in the background:

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

## The app

```bash
airth ui
```

This opens Airth in your browser, served locally on `127.0.0.1`. From there you can:
- switch realms;
- start runs and watch them live: reasoning, tool calls and results;
- approve or deny each step that needs it, with an optional reason the agent sees;
- follow each run's plan, and resume runs that did not finish;
- set up objectives, and read your inbox and the daily summary;
- develop, review and install skills;
- review, adopt or discard changes;
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

| Variable | Default |
|---|---|
| `AIRTH_HOME` | `~/.airth` |
| `AIRTH_REALM` | the realm whose folder holds the current directory, else the default |
| `AIRTH_PROVIDER` | `anthropic` |
| `AIRTH_MODEL` | `claude-opus-5-5` |
| `AIRTH_EFFORT` | `high` |
| `AIRTH_MAX_TOKENS` | `16000` |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` | — |
| `ANTHROPIC_WORKSPACE_ID` | — (needed for API keys not scoped to a workspace) |

Any of these model settings can also be set per realm as a credential (`airth secret set ANTHROPIC_API_KEY`), for example to bill a work realm to the employer's key, or once for every realm (`--all-realms`), which the background service needs. Model keys are used for the model only, and never passed to commands.

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
    skills/                    skills.json, drafts/<skill>/, installed/<skill>/
    secrets.env                credentials (mode 600)
    runs/<id>.json             each run: conversation, plan, grants, tool events, change, checks
    worktrees/<id>/            isolated working copies
    files/                     the folder of realms without their own
```

## Layout

```text
src/
  main.rs         CLI: run, send, resume, runs, review, check, adopt, discard, ui, serve, doctor
  cli.rs          CLI: realm, owner, memory, secret, integration, objective, inbox, summary, skill
  hub.rs          the interface to agents: runs per realm, workspaces, checks, adoption
  home.rs         the Airth home: owner profile, realms, default realm
  realm.rs        a realm: settings, credentials, folders
  owner.rs        the owner profile
  memory.rs       a realm's long-term memory
  objective.rs    persistent objectives, schedules, file watches
  scheduler.rs    which objectives are due; cross-realm summaries
  notify.rs       the owner's inbox and desktop notifications
  service.rs      the login service (launchd)
  skill.rs        skills: manifests, evaluation, installation
  integration.rs  integrations: base URLs with credentials bound to them
  isolation.rs    git worktrees per run: create, commit, diff, adopt, discard
  sandbox.rs      the command sandbox (macOS sandbox-exec)
  ui/             the local app and background runtime: HTTP server, scheduler thread, webhooks
                  (mod.rs) and the page (index.html, app.js, app.css)
  term.rs         terminal output for CLI runs
  cancel.rs       stop signal shared by the CLI, the app, the agent loop and tools
  agent.rs        the loop and the system prompt
  runtime.rs      the single path from a tool call to execution (permission check, redaction, event)
  permissions.rs  capabilities, rules, policy, run grants, terminal approval
  tools/          read_file, list_directory, write_file, edit_file, execute_command, http_request,
                  update_plan, remember, propose_skill, test_skill, installed skills; workspace access rules
  models/         provider-neutral Model trait + Anthropic provider
  state.rs        persisted run state, plans
```
