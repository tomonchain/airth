# Airth — Product & Technical Specification v0.2

**Status:** Foundational specification  
**Language:** Rust  
**Philosophy:** Local-first, open-source, owner-centric, model-agnostic

## 1. Vision

**Airth is a persistent, autonomous personal AI agent dedicated to one owner across every aspect of their life.**

Its mission is to understand its owner, execute meaningful work on their behalf, and continuously improve its capabilities.

**Product statement:**

*One owner. Multiple worlds. One intelligence.*

Airth is not a chatbot, coding assistant, or enterprise productivity application.

It is an independent digital operator that accompanies an individual throughout their personal and professional life.

## 2. Product thesis

We believe individuals will increasingly manage multiple simultaneous activities:

- Traditional employment
- Entrepreneurship
- Consulting or freelancing
- Side businesses
- Investments
- Personal projects
- Family and household responsibilities

Today's software is organized around applications, accounts, and organizations.

**Airth is organized around the individual.**

It should reduce the cognitive and operational burden of managing multiple activities without compromising their separation.

## 3. Foundational principles

### 3.1 Owner-centric

Every Airth instance belongs to an individual.

The owner defines its objectives, permissions, priorities, and boundaries.

Airth must remain portable and independent of any particular employer, model provider, or cloud platform.

### 3.2 Persistent identity

Airth maintains a persistent understanding of its owner:

- Preferences and working habits
- Short- and long-term goals
- Ongoing commitments
- Projects and responsibilities
- Relevant relationships
- Historical decisions
- Explicitly established boundaries

This understanding must be editable, inspectable, and deletable.

Memory is not merely accumulated conversation history.

### 3.3 Multiple realms

A **Realm** is an isolated operational environment representing an activity or area of life.

Examples:

- Personal
- Employment at Company A
- Ownership of Company B
- Freelance business
- Family
- Investments

Each realm maintains its own context, tools, credentials, memory, objectives, and policies.

The owner can create and archive realms as their life evolves.

**Cross-realm access is denied by default.**

Airth may coordinate activities using explicitly permitted information, such as calendar availability, without transferring confidential documents or knowledge between realms.

### 3.4 Autonomous execution

Airth accepts objectives rather than requiring detailed step-by-step instructions.

It should be capable of:

1. Understanding an objective
2. Planning the work
3. Selecting appropriate tools and models
4. Executing authorized actions
5. Evaluating results
6. Recovering from failures
7. Reporting outcomes

Autonomy must remain bounded by permissions, budgets, and owner-defined constraints.

### 3.5 Continuous self-improvement

Airth should progressively improve its ability to serve its owner.

It may identify missing capabilities, propose new skills, develop implementations, and test improvements.

Self-improvement is **a mechanism, not an independent objective**.

Airth must never modify its security boundaries, permissions, or update mechanism without explicit authorization.

### 3.6 Local-first

The Rust runtime, credentials, execution state, and core owner data remain local by default.

Remote models and services are optional integrations.

Data sent to external providers must respect realm-specific policies.

### 3.7 Model independence

Airth must support multiple LLM providers through a common interface.

Future versions may route tasks according to capability, latency, privacy requirements, and cost.

No single provider should be architecturally mandatory.

---

## 4. Conceptual architecture

```text
OWNER
  |
  v
AIRTH
  |
  +-- Owner Model
  +-- Agent Runtime
  +-- Reasoning & Planning
  +-- Memory
  +-- Model Providers
  +-- Skills & Tools
  +-- Task Scheduler
  +-- Permissions & Audit
  |
  +-- Realm Manager
        |
        +-- Personal
        +-- Employment
        +-- Business A
        +-- Business B
        +-- Family
```

These are conceptual boundaries, not mandatory Rust crates.

The runtime must enforce realm isolation independently of LLM instructions.

## 5. Agent lifecycle

Airth should eventually support:

**Reactive execution:** An owner submits an objective.

**Scheduled execution:** Airth executes approved recurring tasks.

**Event-driven execution:** An authorized event triggers an evaluation or action.

**Persistent objectives:** Airth maintains longer-running goals across sessions and restarts.

Every execution must have a bounded scope, observable state, and explicit termination conditions.

## 6. Memory

Memory should eventually distinguish:

- Owner profile
- Realm-specific knowledge
- Episodic history
- Project and task state
- Preferences and learned patterns

Memory must support correction, deletion, retention policies, and source provenance.

**No external memory system is required.**

Do not introduce Morrow or any other external memory dependency into the initial architecture.

Start with minimal local persistence and evolve based on demonstrated requirements.

## 7. Security model

Security is a foundational product requirement.

Airth must eventually provide:

- Realm-scoped credentials
- Least-privilege tool access
- Explicit approval for sensitive actions
- Isolated execution environments
- Audit logs
- Secret redaction
- Network and filesystem restrictions
- Revocable permissions
- Controlled software updates

Airth must treat retrieved documents, tool outputs, and external content as untrusted input.

**LLM-generated intentions must never substitute for authorization.**

Self-modification must occur in an isolated workspace and require verification before adoption.

## 8. Self-improvement model

Airth's initial development experiment remains:

**Build Airth using Airth.**

However, improvements must be tied to concrete objectives and evaluated against measurable criteria.

```text
Identify limitation
       ↓
Propose improvement
       ↓
Obtain approval
       ↓
Implement in isolation
       ↓
Compile and test
       ↓
Evaluate results
       ↓
Approve integration
```

Successful compilation alone does not establish that a change is safe or beneficial.

## 9. Development roadmap

### v0.1 — Bootstrap

**Goal:** Airth can safely contribute to its own development.

Implement:

- Rust CLI
- One LLM provider behind an abstraction
- Repository inspection
- File reading and editing
- Restricted command execution
- Simple agent loop
- Human approval
- Build and test verification
- Structured execution logs

Acceptance test:

`airth run "Identify and implement one small improvement to Airth."`

The agent must propose a change, receive approval, modify an isolated working copy, run checks, and report the result.

### v0.2 — Owner & Realms

**Goal:** Introduce the foundations of a personal agent.

Implement:

- Persistent owner profile
- Realm creation and selection
- Realm-specific state
- Basic local memory
- Credential separation
- Realm-scoped tool policies

Acceptance test:

Airth can operate in two realms without exposing one realm's private data to the other.

### v0.3 — Useful Operator

**Goal:** Complete meaningful personal and professional tasks.

Implement:

- Persistent tasks
- Tool integrations
- Task planning and recovery
- Approval workflows
- Basic owner preferences

Acceptance test:

Airth completes a multi-step task across authorized tools and resumes after interruption.

### v0.4 — Persistent Autonomy

**Goal:** Work without continuous prompting.

Implement:

- Background runtime
- Scheduler
- Event subscriptions
- Persistent objectives
- Execution budgets
- Notifications and summaries

Acceptance test:

Airth completes an approved scheduled objective while the owner is absent.

### v0.5 — Adaptive Capabilities

**Goal:** Extend itself in response to owner needs.

Implement:

- Skills interface
- Capability discovery
- Isolated skill development
- Automated evaluation
- Controlled installation

Acceptance test:

Airth identifies a missing capability, implements a solution, tests it, and requests approval before enabling it.

### v1.0 — Personal Autonomous Agent

**Goal:** Deliver a reliable assistant across multiple realms.

The owner can delegate work across personal and professional activities while maintaining confidentiality, control, and continuity.

---

## 10. Explicit non-goals for v0.1

Do not implement:

- Desktop or web UI
- Multi-agent orchestration
- Full memory architecture
- External memory dependencies
- Cloud infrastructure
- Model routing
- Complex skills ecosystem
- Unsupervised self-updates
- Background autonomy
- Full realm management

The long-term architecture must accommodate these capabilities without implementing them prematurely.

## 11. Engineering requirements

- Rust stable
- Small dependency footprint
- Idiomatic asynchronous execution where justified
- Modular but not excessively fragmented
- Automated tests
- Structured errors and logs
- Configuration separate from secrets
- Documented architecture decisions
- Explicit limits on execution time, iterations, and resource consumption

Prefer simple, testable abstractions over speculative frameworks.

## 12. Success metrics

Airth's success should eventually be measured through:

- Useful tasks completed
- Owner time saved
- Reliability of autonomous execution
- Recovery from failures
- Privacy and security incidents
- Owner intervention required
- Cost per completed objective
- Measurable improvement in capabilities

The number of autonomous actions or self-generated code changes is not itself a success metric.

## 13. Guiding statement

**Airth exists to increase its owner's ability to live, work, create, and pursue multiple ambitions.**

It should understand its owner, respect boundaries between their different worlds, perform meaningful work, and improve when doing so creates real value.

Build the smallest version capable of progressing toward that vision.

Do not build autonomy for its own sake.

**Build autonomy in service of the owner.**

