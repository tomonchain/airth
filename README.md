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
- **permissions**: per capability (`read`, `write`, `execute`, `memory`), each `allow`, `ask` or `deny`;
- **memory**: facts Airth keeps between runs, each with where it came from;
- **credentials**: given to that realm's commands as environment variables, never shown to the model;
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
- review, adopt or discard changes;
- edit each realm's permissions, checks, memory and credentials, and your profile.

The link printed in the terminal contains a private key for this launch; API calls without it are rejected. Use `--port`, `--no-open` and `--max-iterations` to change the defaults.

## Command line

```bash
airth run "<objective>"           # start a run in the current realm
airth send last "Now add docs"    # continue a run (by id or `last`)
airth runs                        # the realm's runs: status, checks, objective
airth skills                      # the tools, and this realm's rule for each
```

Approvals happen in the terminal: answer `y`, or type a reason to deny, and the agent sees that reason. Ctrl-C stops a run cleanly; press it twice to quit at once. A run whose process was killed shows as `interrupted` and can still be continued.

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

Any of these model settings can also be set per realm as a credential (`airth secret set ANTHROPIC_API_KEY`), for example to bill a work realm to the employer's key. Model keys are used for the realm's model only, and never passed to commands.

## Where things live

```text
~/.airth/                      readable only by you; no run can reach it
  owner.json                   your profile
  config.json                  the default realm
  realms/<name>/
    realm.json                 folder, permissions, isolation, checks
    memory.json                long-term memory, with provenance
    secrets.env                credentials (mode 600)
    runs/<id>.json             each run: conversation, tool events, change, checks
    worktrees/<id>/            isolated working copies
    files/                     the folder of realms without their own
```

## Layout

```text
src/
  main.rs         CLI: run, send, runs, review, check, adopt, discard, ui, doctor
  cli.rs          CLI: realm, owner, memory, secret
  hub.rs          the interface to agents: runs per realm, workspaces, checks, adoption
  home.rs         the Airth home: owner profile, realms, default realm
  realm.rs        a realm: settings, credentials, folders
  owner.rs        the owner profile
  memory.rs       a realm's long-term memory
  isolation.rs    git worktrees per run: create, commit, diff, adopt, discard
  sandbox.rs      the command sandbox (macOS sandbox-exec)
  ui/             the local app: HTTP server (mod.rs) and the page (index.html, app.js, app.css)
  term.rs         terminal output for CLI runs
  cancel.rs       stop signal shared by the CLI, the app, the agent loop and tools
  agent.rs        the loop and the system prompt
  runtime.rs      the single path from a tool call to execution (permission check, redaction, event)
  permissions.rs  capabilities, rules, policy, terminal approval
  tools/          read_file, list_directory, write_file, execute_command, remember; workspace access rules
  models/         provider-neutral Model trait + Anthropic provider
  state.rs        persisted run state
```
