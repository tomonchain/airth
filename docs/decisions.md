# Architecture decisions

Short records of the choices behind Airth's structure, newest first. Each one says what was decided, why, and what it costs.

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
