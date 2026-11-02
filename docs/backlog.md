# Backlog

- Same-event grouping in pipelines: a judge that sees items together, so three articles on one event count as one. Why: `distinct` only works per field. Helps: milestone 4 (the watch).
- A steward that decides missions, with verifiers (branch `business-autonomy`, its decision 26). Why: open-ended running of a business beyond fixed routines. Helps: after the beta.
- Targets read from Stripe or Qonto by code (branch `business/reading.rs`). Why: a goal's figure from its source. Helps: after the beta (connected banking).
- A business set up from an idea (branch `business/onboard.rs`). Why: faster creation. Helps: milestone 8 (business templates).
- Acceptance scenarios A-01…A-25 (branch). Why: written for the due diligence framing; to rewrite for businesses. Helps: milestone 6.
- Network for development routines (Product & tech department): updating dependencies or building in a mission needs crates.io, npm and GitHub, which decision 51 cuts for a routine's commands. Lead: a local proxy in Airth that allows only those registries, the sandbox letting commands out only to it. No decision yet. Helps: milestone 8 and the Product & tech department.
- Persist `http_request`'s memory of writes that got no answer (#52) in the action journal. Why: today it lives as long as the run's tool, so a mission resumed after Airth restarts could send the same write again. Helps: when a resumed mission needs it (missions launched by an Act, milestone 4).
