# Backlog

- Same-event grouping in pipelines: a judge that sees items together, so three articles on one event count as one. Why: `distinct` only works per field. Helps: milestone 4 (the watch).
- A steward that decides missions, with verifiers (branch `business-autonomy`, its decision 26). Why: open-ended running of a business beyond fixed routines. Helps: after the beta.
- Targets read from Stripe or Qonto by code (branch `business/reading.rs`). Why: a goal's figure from its source. Helps: after the beta (connected banking).
- A business set up from an idea (branch `business/onboard.rs`). Why: faster creation. Helps: milestone 8 (business templates).
- Acceptance scenarios A-01…A-25 (branch). Why: written for the due diligence framing; to rewrite for businesses. Helps: milestone 6.
- Network for development routines (Product & tech department): updating dependencies or building in a mission needs crates.io, npm and GitHub, which decision 51 cuts for a routine's commands. Lead: a local proxy in Airth that allows only those registries, the sandbox letting commands out only to it. No decision yet. Helps: milestone 8 and the Product & tech department.
- Persist `http_request`'s memory of writes that got no answer (#52) in the action journal. Why: today it lives as long as the run's tool, so a mission resumed after Airth restarts could send the same write again. Helps: when a resumed mission needs it (missions launched by an Act, milestone 4).
- Airth executes an approved MCP tool call itself, once, like an approved HTTP action, instead of leaving it to the founder. Why: today the founder approves a commits call from a task, then does it by hand (decision 56). Helps: milestone 8 at the latest (Numirama's MCP), sooner if Hedgen's MCP comes first.
- Read followed accounts on X through the X API (pay per use, about $90 a month for 30 accounts), so Airth can suggest posts to answer instead of the founder sending each link (decision 63). To revisit at milestone 6, with audience data; needs its own decision on cost.
- Legal deadlines beyond decision 66: VAT returns once a business is VAT-registered, payroll (PAYE) once it employs, Corporation Tax instalments over £1.5 million of profit. Why: decision 66 covers the four filings every UK Ltd has. Helps: milestone 8 (France's rules), or sooner when Hedgen registers for VAT.
