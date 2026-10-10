# Airth — v0.1 Specification

## 1. Vision

Airth is an **open-source, local-first autonomous AI agent runtime**, written in Rust.

Airth is not a chatbot or an AI-powered IDE.

Its purpose is to provide a runtime in which an AI agent can:

- perceive its environment;
- reason about objectives;
- execute actions;
- use tools;
- inspect and modify its own environment;
- progressively become more capable.

The long-term goal is to make Airth a general-purpose runtime for autonomous AI agents.

---

## 2. First principle

Airth's first mission is:

> **Build Airth.**

The first version should therefore be capable of inspecting its own codebase, proposing improvements, modifying itself, compiling and testing the result.

This is a bootstrap experiment, not yet the final autonomous architecture.

---

## 3. Scope of v0.1

v0.1 is a **CLI application**.

It must be able to:

- communicate with an LLM;
- inspect the current repository;
- read files;
- create and modify files;
- execute approved shell commands;
- compile the project;
- run tests;
- maintain a basic task loop;
- request human approval before consequential actions.

Example:

```bash
airth run "Improve yourself"
```

---

## 4. Core agent loop

The runtime should implement a simple loop:

```text
Observe
   ↓
Reason
   ↓
Plan
   ↓
Request permission if required
   ↓
Execute
   ↓
Observe result
   ↓
Evaluate
   ↓
Continue / finish
```

The loop must support multiple iterations.

Airth should not assume that one LLM response equals one task.

---

## 5. Tools

Start with the smallest useful toolset:

### Filesystem

```text
read_file
write_file
list_directory
```

### Shell

```text
execute_command
```

### Project

The agent should be able to:

```text
cargo check
cargo test
cargo build
```

These should be invoked through the shell capability rather than hard-coded as special cases wherever practical.

---

## 6. Permissions

Tools are capabilities.

Every tool invocation must pass through a permission layer.

For v0.1, permissions can be simple:

```text
read       → automatic
write      → approval
execute    → approval
```

The architecture must allow more granular permissions later.

Airth must never be able to silently remove or bypass its own permission mechanism.

---

## 7. LLM abstraction

Airth must not be coupled to one LLM provider.

Create a minimal provider abstraction allowing different models to be plugged in.

The abstraction should eventually support:

```text
OpenAI
Anthropic
Google
xAI
Local models
Other providers
```

For v0.1, implement only the provider required to get the system working.

Do not build a router yet.

---

## 8. State

Airth needs minimal persistent execution state.

For v0.1, state may contain:

```text
task
current objective
conversation/context
tool history
execution status
```

Do not build a sophisticated memory system.

Airth must work perfectly well without Morrow.

If a memory abstraction is introduced, keep it generic and optional.

---

## 9. Self-improvement

Airth must be able to operate on its own repository.

A typical run should look like:

```text
User:
"Improve yourself."

Airth:
→ inspect repository
→ understand architecture
→ identify improvement
→ explain proposed change
→ request approval
→ modify files
→ cargo check
→ cargo test
→ inspect failures
→ iterate
→ report result
```

Airth should prefer **small, reversible changes**.

Git integration is desirable but not required for the first implementation.

---

## 10. Observability

Every tool invocation should produce a structured internal event.

At minimum:

```text
timestamp
action
arguments
result
status
```

The CLI should expose enough information for a developer to understand what Airth is doing.

Do not implement a full telemetry platform.

---

## 11. CLI

The CLI should remain extremely simple.

Minimum interface:

```bash
airth
airth run "<objective>"
```

Useful additional commands may include:

```bash
airth --help
airth --version
airth doctor
```

Do not build a TUI or graphical interface yet.

---

## 12. Architecture

Use idiomatic Rust and keep the architecture modular.

Suggested conceptual boundaries:

```text
agent
runtime
models
tools
permissions
state
cli
```

These are conceptual boundaries, not mandatory crate boundaries.

Avoid premature micro-crates.

---

## 13. Non-goals

Explicitly out of scope for v0.1:

- GUI
- web interface
- multi-agent systems
- sophisticated memory
- Morrow integration
- model routing
- marketplace
- plugin ecosystem
- cloud infrastructure
- distributed execution
- autonomous network access
- unrestricted self-modification
- production-grade sandboxing

These may become important later.

---

## 14. Success criteria

v0.1 is successful if a developer can clone the repository and run:

```bash
cargo run -- run "Improve yourself"
```

and Airth can autonomously complete at least one meaningful self-improvement cycle:

```text
inspect
→ reason
→ propose
→ approve
→ modify
→ compile
→ test
→ report
```

The system should be small enough that a developer can understand the entire runtime.

---

## 15. Guiding principle

Do not build the final Airth.

Build the **smallest useful Airth that can help us discover what the final Airth should be**.

The architecture should be designed for evolution, but implementation should remain ruthlessly minimal.
