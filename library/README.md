# Airth's library

Teams of agents a business installs from the Marketplace (decision 74): the
agents with their jobs, their routines and the prompts those use, plus the
skills and connectors they need. Everything here is reviewed in a pull
request before it ships; Airth embeds it in its binary.

A team is a folder under `teams/`:

- `team.md`: its title, description, the skills (from Anthropic's catalog)
  and connectors it needs in the front matter, and what to do once it is
  installed in the body, one line per step.
- `agents/`, `routines/`, `prompts/`: files in the business folder's format,
  copied as they are. A routine ships with a manual trigger when it needs the
  founder's setup first; it gets its schedule once that is done.

Nothing here names a real business. Where a team needs the business's own
words (its sources, its voice), the file says so in square brackets.
