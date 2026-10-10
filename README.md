# Airth

A local-first autonomous AI agent runtime, written in Rust. v0.1 is a CLI whose first job is to improve its own codebase. See [SPECS.md](SPECS.md).

```bash
export ANTHROPIC_API_KEY=...
cargo run -- doctor
cargo run -- run "Improve yourself"
```

## The app

```bash
cargo run -- ui
```

This opens Airth in your browser, served locally on `127.0.0.1`. From there you can:
- start runs and watch them live: reasoning, tool calls and results;
- approve or deny each write (shown as a diff) and each command, with an optional reason the agent sees;
- stop a run, send follow-ups, and resume interrupted runs.

The link printed in the terminal contains a private key for this launch; API calls without it are rejected. Use `--port`, `--no-open` and `--max-iterations` to change the defaults.

## Command line

Talk to agents through the same interface (`src/hub.rs`) the CLI uses:

```bash
cargo run -- runs                                # what agents have done: id, status, objective
cargo run -- send last "Now add docs for it"     # continue a run (by id or `last`) with a follow-up
cargo run -- skills                              # what agents can do, and the permission each needs
```

Reads are automatic. Every file write (shown as a diff) and every shell command needs your approval: answer `y`, or type a reason to deny it, and the agent sees that reason.

Ctrl-C stops a run cleanly: the running command is killed and the run is saved as stopped, so `airth send` can continue it. Press Ctrl-C twice to quit at once. If Airth is killed outright, the run shows as `interrupted` and can still be continued.

## Configuration

| Variable | Default |
|---|---|
| `AIRTH_PROVIDER` | `anthropic` |
| `AIRTH_MODEL` | `claude-opus-5-5` |
| `AIRTH_EFFORT` | `high` |
| `AIRTH_MAX_TOKENS` | `16000` |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` | — |
| `ANTHROPIC_WORKSPACE_ID` | — (needed for API keys not scoped to a workspace) |

Flags for `run` and `send`: `--workspace <dir>`, `--max-iterations <n>` (default 50), `-v` (print full tool results).

Each run is saved to `.airth/runs/<id>.json`: the objective, status, the conversation, and a structured event for every tool call (timestamp, action, arguments, result, status).

## Layout

```text
src/
  main.rs         CLI: run, send, runs, skills, ui, doctor, interactive prompt
  hub.rs          the interface to agents: dispatch, send, skills, runs, run locks
  ui/             the local app: HTTP server (mod.rs) and the page (index.html, app.js, app.css)
  term.rs         terminal output for CLI runs
  cancel.rs       stop signal shared by the CLI, the app, the agent loop and tools
  agent.rs        the loop: model call → tool calls → results → repeat
  runtime.rs      the single path from a tool call to execution (permission check + event)
  permissions.rs  capabilities (read/write/execute), policy, approval prompt
  tools/          read_file, list_directory, write_file, execute_command
  models/         provider-neutral Model trait + Anthropic provider
  state.rs        persisted run state
```
