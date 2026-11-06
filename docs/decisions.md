# Architecture decisions

Short records of the choices behind Airth's structure, newest first. Each one says what was decided, why, and what it costs.

## 86. Word documents and PDFs are read as text, with flate2

**Accepted.** The founder, 6 November 2026, after the Chief of staff could not read the company's documents in Drive: they are PDFs and Word files, which decision 85 said unreadable.

- `extract::text` reads a .docx (a zip of XML: its `word/document.xml`, paragraphs and tabs kept) and a PDF made by software (its compressed streams, the text operators of its pages, characters mapped through the fonts' ToUnicode tables, else Latin-1). A PDF with no text, a scan, is said to be one; nothing is guessed. A compressed part may grow to 50 MB at most.
- Drive's `read` uses it for PDFs and Word documents.
- `flate2` becomes a direct dependency for the decompression: it was already built (through reqwest's TLS stack), so nothing new is downloaded or compiled. The zip and PDF reading is Airth's own, small, and tested; a PDF library would bring far more than this needs.

*Why:* company documents are PDFs and Word files far more often than Google Docs.

*Cost:* a PDF that hides its text in a way this does not read (some font encodings without ToUnicode, text drawn as shapes) gives little or garbled text; a scan gives none and says so. Reading scans needs OCR or a model that reads images: not here.

## 85. Google Drive, read only, one folder

**Accepted.** The founder, 6 November 2026: the agents need the company's documents, which are in Google Drive ("Go pour le connecteur"). It was an open question (Google OAuth for Drive). Integrations were HTTP with a key; Drive needs a Google sign-in.

- The founder connects Drive in Settings → Connections with their own Google OAuth client (a Desktop app client of their Google Cloud project) and the link of one folder. Airth signs in with PKCE through the sign-in remote MCP servers use (`tools::remote::start_known`, the same callback, the same private token file, refreshed when it expires). The scope is `drive.readonly`.
- Agents read only that folder and what is under it: every file asked for is followed up its parents to the folder, or refused (`Drive::within`); a search keeps only what is inside. They list, search and read: a Doc as Markdown, a Sheet as CSV, Slides as text, a text file as it is; other files are said unreadable for now (PDF included). A document is cut past 100,000 characters, and handed over as data, never instructions.
- The tool (`google_drive`) is a read, through the runtime like any tool. It is offered once Drive is connected, to the founder's own chats and to an agent whose file names `google-drive` (its Edit form offers it).
- Nothing is written to Drive.

*Why:* the business's documents live in Drive; copying them into the business folder by hand goes stale. One folder, read only, keeps the rest of the founder's Drive out of reach.

*Cost:* the founder sets up a Google Cloud client once, and publishes it (a Testing app's sign-in ends every 7 days). PDFs in Drive are not read yet. Google's tokens sit in the realm's private files like the remote MCP servers' (encrypting keys at rest is still open).

## 84. A service whose reads change nothing is read without asking

**Accepted.** The founder, 6 November 2026, after the Developer stopped on every read of GitHub: "il faut qu'il puisse lire". A request through an integration with no declared actions is an action nobody declared, so it commits (CLAUDE.md), and a plain `GET /user/repos` waited for the founder's approval.

- An integration may say its reads are free (`free_reads`): through it, a GET or a HEAD is no action, and the realm's network rule decides, as for a read of the web. Any other method is still an action nobody declared, and commits.
- The GitHub connector says so (`connectors::FREE_READS`); a GitHub connected before this reads free too, if it is the catalog's at its own address. No other connector does until the founder says so.
- The agent's own repositories are not read through GitHub: a task whose agent names a repository (`repos:`, decision 64, now chosen in the agent's Edit form) works in its own worktree of it, on its own branch, and writes there without asking (decision 80).

*Why:* a read of GitHub changes nothing for the business; asking for each one stopped the Developer at every step, and taught the founder to approve without reading.

*Cost:* a read may still leak what the agent was told into a URL's query; the network rule and the realm's integrations are the bound, as for the web. A service whose GETs do change things must not be marked free.

## 83. The memory keeps what the founder decided, not what was discussed

**Accepted.** The founder, 6 November 2026, asking that what is said in a chat reach the whole team: "il faut distinguer ce qui est du brainstorming avec les équipes de ce qu'on teste / approuve pour la mémoire."

- Nothing said in a chat enters the business's memory on its own. The memory (decision 82: told to every task and every routine) holds what the founder decided, tested or approved.
- The founder keeps a decision from a chat: beside each of their messages and each answer, "Keep as a decision" opens the words to edit into the decision, and asks whether it changes an earlier one.
- An agent proposes one with `remember`, told to propose only what the owner decided or approved, never an idea from a brainstorm. Its proposal shows as a card, "proposes to keep this for the whole team", with Keep or Not now; the realm's `memory` policy still decides whether it is asked (it asks by default).
- A decision that changes an earlier one replaces it (`MemoryStore::replace`, `replaces` in `remember` and in the app), in one write, rather than sitting beside it. The agents see each memory's id to name the one they replace.

*Why:* a memory every routine follows must not fill with ideas that were only explored, nor keep two rules that contradict each other.

*Cost:* the founder's click is needed for every decision; the memory holds only what someone chose to keep. A founder who sets `memory` to Allow lets agents keep decisions without asking, and loses that distinction.

## 82. A routine's model calls are told what the founder's tasks are told

**Accepted.** The founder, 6 November 2026: how to make the agents keep what they are told, so that when an element of the business changes, all of them know. A task was told the business's profile, its strategy and its memory; a routine's AI block was told only its prompt file. A decision recorded in the strategy or the memory reached every task and no routine.

- Every AI block of a routine is told, before its prompt, the business's essentials (`business::essentials`): its profile, the owner's strategy, and what the business remembers. They are read at each run, so a change reaches the next one. The prompt says that where the task and the essentials disagree, the essentials are the owner's latest word.
- The journal records a call's prompt version as the prompt file's SHA-256, then `+` and the first 12 characters of the essentials': which prompt, under which state of the business.
- A business with no profile, strategy or memory sends the prompt alone, as before.

*Why:* one place to change the business, read by everything that works for it. Editing every prompt file after each decision is what the founder should not have to do.

*Cost:* about a thousand tokens more per call for Hedgen (3.8 KB of profile and strategy, two memories), the same text each time, so mostly read from the provider's cache. A prompt that contradicts the strategy now loses to it.

## 81. A task or a routine tries again on its own before it waits for the founder

**Accepted.** The founder, 6 November 2026: "if a run fails, the agents stop and wait for my go; they should try again N times." A task that failed on a busy model stopped with Try again; one that reached its 30-minute budget stopped until the founder said go on; a scheduled routine that failed became an incident at once.

- **A passing failure** (the provider busy or down, the network, an answer cut off: `models::transient`) is tried again three times, after 30 s, 2 min and 8 min, after the few quick retries of each model call. A refused key, no credit or a refused request is not: those need the founder. The thread says "The model is busy right now. Trying again in 2 min (2 of 3)."; Stop still stops.
- **A time budget reached** is renewed twice: a task works up to three budgets (90 minutes by default) before it stops for the founder. A cost or token budget is never renewed alone.
- **A routine that fails at its time** is run again after 1 and 5 minutes before it becomes one incident; its heartbeat says "tried 3 times". An act never runs twice for one item (CLAUDE.md), so what a failed try did is not done again. Not a routine whose file does not load, nor one the founder started by hand, who sees the error at once.
- What waits for a decision (an approval, a question in Needs you) still waits: that is the founder's to answer.

*Why:* most failures seen were the provider's, and pass within minutes; a task that waited for a go until the founder looked lost hours.

*Cost:* a failure the founder would have fixed at once (a provider down for an hour) is said about 10 minutes later. A task may spend up to three time budgets alone.

## 80. A run in its own worktree writes without asking

**Accepted.** The founder, 6 November 2026: "on vire les confirmations". A task asked before every file it wrote, though it worked in its own git worktree: nothing it writes reaches the founder's files until they adopt the change, after reading the whole diff.

- In a run with an isolated change (`state.change`), a write the realm's policy would ask about is allowed (`PermissionGate::with_isolated_writes`). The adoption, with its diff and its checks, is where the founder approves.
- Only writes, only there. A write that carries a warning (a file that enforces a security boundary, `SECURITY_CRITICAL`) is still asked. A policy that denies writes still denies them. Commands, memory, the network and skills ask as before, with "Allow for this task". A call the founder is asked about explicitly (decision 56) is still asked.
- A run without a worktree (a realm that works in place) asks before writing, as before.
- Settings → Permissions says so under "Change files".

*Why:* the question came before every step of work whose every line the founder reads anyway at adoption; it taught them to click Allow without reading, which is worse than one review.

*Cost:* a task may fill its worktree with files the founder then throws away. The diff, not each write, is now the one place to catch a write that should not be adopted.

## 79. Nine departments, the founder adds others in Settings

**Accepted.** The founder, 5 November 2026. The Chief of Staff hired a Developer in a department it called `development`; the seven departments of `CLAUDE.md` were the only ones allowed, and "Product & tech" was written `product`, which it could not guess. The founder wants a list that stays generic and is right, agents choosing within it, and the founder alone adding to it.

- Nine departments every business has, in this order: `leadership` (Leadership), `marketing` (Marketing), `sales` (Sales), `support` (Support), `tech` (Product & tech), `operations` (Operations), `finance` (Finance & admin), `legal` (Legal & compliance, new: deadlines, contracts, terms) and `data` (Data & research, new: watch, market analysis, statistics). Left out: people (the staff are agents), design (Marketing or Product & tech), purchasing (Operations).
- Other words for them are read as them: `development`, `engineering`, `product` → `tech`; `research`, `analytics` → `data`; `customer service` → `support`; and a few more (`config.rs`). A file keeps what it says; the app reads the department, and a department set from the app is written as its key.
- The founder adds a department in Settings → Departments: a new name, not one of the nine nor another word for one. They are kept in the realm's settings (`realm.json`, in Airth's home), out of the business folder, so an agent cannot add one. One with agents in it cannot be removed.
- Any other department is refused, naming the list and saying that the founder adds departments: an agent file with one is set aside and said under the agent list (#169). Every task of a business is told the list, so an agent hiring another need not guess.
- Marketplace teams use the nine, by key.

*Why:* the founder organises the team; a closed list that misses what a business needs hides agents, and a list anyone can extend fills with near-duplicates. Nine cover an online company; the rest is the founder's choice.

*Cost:* the web interface keeps the nine's names for pages that show one department without the list. A department written in words that are neither the nine, their other words nor the founder's is still refused after the fact; checking an agent's change before review is the next step.

## 78. A model's answer is streamed to the chat as it is written

**Accepted.** The founder, 5 November 2026: "go à fond" on a smoother chat with the agents. An answer showed only once the model had written all of it (10 to 60 s), then was typed out by an animation; the page looked for changes every 1.5 s.

- `Model::stream` asks the provider for server-sent events and hands the answer's text so far to a callback; the answer is then put back together exactly as the whole one would have come, and journaled, priced and checked as before. Anthropic and the OpenAI-compatible providers stream; any other model answers at once (the trait's default). A request that fails on the way, or an answer that breaks off, is sent again whole, as before; the callback hears it start again from nothing.
- The agent loop streams every working call and passes the text to its observer (`Observer::writing`). A run driven by the app keeps it in memory beside its snapshot, and drops it once the answer is in the run's state. Nothing is written to disk.
- `GET realms/{r}/runs/{id}/live?version=&draft=` waits (up to 8 s, under the gap after which the founder counts as gone, decision 56) until the run's state or the words being written change, and says which. The page asks again at once, so the founder reads the answer as it is written and sees each step as it happens. A run driven by another process is still looked at every 1.5 s.

- The founder may write while the agent works. The message waits in memory, beside the run's snapshot, and the agent reads it at its next turn, even if it thought it was done (`Observer::said`). A message written as the run ends drives it again, as a follow-up would. Nothing is kept for a run that is only running its checks, or one the founder stopped.
- Stop ends a model call while it is being written: the stream is given up at its next line (providers send events or keep-alive lines every few seconds, even while the model thinks), what it wrote is dropped, and it is not sent again.

*Why:* waiting a minute in front of "Thinking" was the main reason the chat felt slow; the providers stream natively, and `reqwest`'s blocking response reads as a stream, so no new crate is needed.

*Cost:* one thread per open chat waiting on its run (the server already gives each request its own); a page polls nothing while it waits. The model call path gained a second way to read an answer, tested against recorded event streams for both families of providers.

## 77. A repository is picked from the ones the business's GitHub token reaches

**Accepted.** The founder, 5 November 2026: "meilleure expérience si on sélectionne le repo plutôt que l'URL".

- `GET realms/{r}/github/repos` asks GitHub for the repositories the business's `GITHUB_TOKEN` reaches (owned, shared, its organisations'; the 100 latest pushed), with the token in the header only, as pull requests already do (decision 11). Without a token, nothing is asked: the page says how to add one.
- Settings → Repositories → Connect opens on that list (search, private and archived marked, those already connected shown so); a pick is cloned with the token, as a pasted GitHub URL already was. Pasting an address or a folder stays, one tab away.

*Why:* picking is quicker and cannot mistype; the token is the one the business already holds for its pull requests, so nothing new is trusted.

*Cost:* one more GitHub API call, made only when the founder opens the list; no new state.

## 76. The Documents editor is CodeMirror 6, in web/

**Accepted.** The founder, 5 November 2026, once the classic interface left (decision 72): "Go faire la coloration". Its Documents page had a CodeMirror editor with syntax colours; `web/` had a plain text box.

- `web/` depends on CodeMirror 6 and its languages, pinned at the versions the classic editor bundle used (`@codemirror/*`, `@lezer/highlight`, `codemirror`): JavaScript and TypeScript, JSON, Markdown, Python, Rust, CSS, HTML, YAML, SQL, XML, Go, Java, C and C++, PHP, and shell, TOML, Swift and Ruby.
- It is its own chunk, fetched the first time a text file opens; the Documents page and the rest of the interface do not load it.
- Colours are the classic palette, through CodeMirror's `.tok-*` classes, light and dark with the theme.

*Why:* reading and changing a business's code and config without colours is slower and more error-prone; CodeMirror is what the classic interface already shipped, with the same languages, so nothing new is trusted.

*Cost:* about 1 MB of script (380 kB compressed) in `web/dist/`, embedded in the binary, loaded on demand. Its versions are kept by `web/package-lock.json`.

## 75. A task written in an agent's chat is watched by the founder

**Accepted.** The founder, 4 November 2026, after talking to an agent in the new interface: "it's not going smooth to talk to an agent". Each message started a separate task; the first command the agent wanted to run stopped it (decision 64: an order runs unwatched, its questions go to Needs you); its replies never showed.

- An order sent from an agent's chat (`POST realms/{r}/orders` with `watched: true`) is driven as a run the founder watches (decision 56): while the chat looks at it, a permission the agent needs is asked there, live, and answered there. The chat looks at a live run every second or two; once the founder leaves, it is unwatched again and its questions go to Needs you, as before.
- The founder's next message in the chat continues that task (`runs/{id}/messages`), as a conversation does; a new task is started on purpose.
- A task stopped on a question shows it in the chat; allowing it there closes the Needs you item and goes on at once, with the same message the service would send.
- The command bar's orders stay unwatched (decision 64).

*Why:* a chat is the founder present. Asking them where they are is what decision 56 already does for a run's page; the queue is for when they are not there.

*Cost:* none on the rules: who approves is unchanged and every permission is still asked (decision 2). The chat polls a live run often, as the run's page did.

## 74. The Marketplace: Anthropic's skills, the official connectors and the MCP Registry, and Airth's library of teams

**Accepted.** The founder, 4 November 2026: a Marketplace page to add skills, agents and connectors from an external library; then "une équipe de N agents dédiés aux basiques de la gestion d'une société e-commerce… on installe l'équipe et c'est prêt à démarrer"; then "go" on the order below.

- **Skills:** Anthropic's catalog (`github.com/anthropics/skills`), as decision 18 installs it: text files only, read by agents as data through `use_skill`. skills.sh (Vercel, over a million skills) is not added: nothing there is reviewed, and a Snyk scan found a security issue in 37% of the skills it checked. Adding it later needs its own decision.
- **Connectors:** the thirteen official remote MCP servers checked with Airth's sign-in, shown with their Simple Icons marks, then the official MCP Registry, marked "not checked by Airth" (decision 20). Unchanged.
- **Teams, in Airth's library.** No shared library of business roles exists; the directories found hold sub-agents for developers. Airth ships its own, in `library/teams/<id>/`, embedded in the binary by build.rs: reviewed in a pull request, versioned with Airth, nothing downloaded.
  - A team is `team.md` (title, description, the catalog skills and the connectors it needs, the setup steps) and its `agents/`, `routines/` and `prompts/` in the business folder's format.
  - Installing copies those files into the business folder and starts its skills' install. It writes nothing when any of its files is already there: a team never overwrites the founder's work. Connectors are the founder's to connect, each with its own sign-in; keys never travel with a team.
  - A routine that needs the founder's setup first (its sources, its voice) ships with a manual trigger; the setup steps say when to give it a schedule. Everything keeps its autonomy level: new actions start at level 1.
  - Nothing in the library names a real business; where a team needs the business's own words, its files say so in square brackets.
- **The first teams:** Media, extracted from what Hedgen runs, made generic; then E-commerce (support, operations, marketing, finance, chief of staff), which also prepares Numirama (milestone 8). E-commerce comes into the beta with this decision; its support and operations routines wait for agent mail (milestone 7) and a shop connector.
- **Single agents** in the library come later, from the teams' agents.

*Why:* every source the Marketplace adds is text or a server the business's agents will trust. A curated source per kind keeps that trust earned; an agent's job is what it obeys, so it never comes from a source nobody reviews. A team installed whole is what makes a new business start in minutes, which the beta's Numirama goal needs.

*Cost:* the library is written and kept by us; a team is only as good as its prompts, and its prompts need the business's own words before they run. Teams update with Airth's releases, not on their own.

## 73. The interface is a React app built with shadcn, and its first screen shows the agents at work

**Accepted.** Replaces decision 57's stack and decision 72's part 2 (Preact with `htm`). Decision 72's parts 1 and 3 change as written below.

The founder, 3 November 2026, after #135 to #137: the new interface does not look like the software built with them elsewhere (Clarity Mail: React, shadcn, Tailwind, Lucide). Airth must let them watch their agents work in each business; the pages read as an admin tool instead. The cause is in the rules: decision 57 forbids a framework, a build step and any library for the pages, so every menu, dialog, popover and list is drawn by hand, page by page, in small PRs nobody looks at as a whole.

**1. The stack.** The interface lives in `web/`:

- Vite, React, TypeScript, Tailwind and shadcn (style `base-nova`, on Base UI, as in Clarity Mail), Lucide for icons, Sonner for toasts.
- `npm run build` in `web/` writes `web/dist/`, which is committed, as the Documents editor's bundle already is (`src/ui/editor/`). Airth serves it from the binary: `cargo build` needs no Node, and running Airth needs nothing installed.
- `npm run dev` serves the interface on its own port and sends `/api` to a running Airth, for working on it.
- The API stays as it is. A page that needs data the API lacks gets it through a Rust change of its own.
- The shadcn components are copied into `web/src/components/ui/` by its CLI, as shadcn intends; they are our code from then on.

**2. The first screen: the agents at work.** Inspired by the agent dashboards of Grok Build and OpenClaw: one row per agent, grouped by state (needs you, working, failed, idle), refreshed live; a selected agent opens its latest activity, its run step by step, and the box to give it an order. Needs you becomes what the agents' work sends up, in the same place. It is built first, as a prototype the founder judges on a copy before anything else moves.

**3. What changes in the rules.**

- The interface's work is judged on screen, by the founder, before it is merged; a page is designed as a whole, not grown feature by feature.
- The 400-line limit does not apply to `web/` (decision 72's freeze still adds no feature); shadcn's copied components never count.
- A page may leave out what does not serve its main task; nothing leaves Airth without a decision (decision 42), it moves to another screen.
- Decision 72's three layouts and `docs/design/DESIGN.md`'s tokens become shadcn's theme variables; they are a starting point, not a frame.
- `src/ui/next` stays served until `web/` has every page it has; then it leaves, in one PR. The classic interface's plan (decision 72, part 3) is unchanged, its pages moving into `web/`.
- `node scripts/ui_smoke.js` covers `src/ui/next` while it lives; `web/` is checked by `tsc` and its build in CI.

*Why:* the founder's software built on this stack looks right; Airth's hand-drawn pages do not, and the gap is the components, not the effort. Committing the build output keeps decision 57's real goal: one binary, nothing to install, nothing to build to run Airth.

*Cost:*
- Node and npm to work on the interface, and a few hundred kilobytes of built files committed on each interface change.
- npm dependencies, kept to the list above; any other needs a decision.
- `src/ui/next` is thrown away once replaced, about 4,000 lines.
- CI gains a job that type-checks and builds `web/` and fails if `web/dist/` is not the committed one.

## 72. One interface, the new one, built from three page layouts; the classic one leaves once its features have moved

**Proposed.** Replaces decision 71.

The founder, after using decision 71's mix: remove the classic interface, work cleanly on the new one, and stop the layout changing from one page to the next. Today it changes four ways:

| Page | Layout today |
| --- | --- |
| HQ | centred 880 px column, extra top space |
| Business, Tasks, Settings, Workshop | centred 880 px column |
| Team | 1,100 px block, aligned left |
| Calendar | full width |
| Needs you, Routines | three panes against the edges, no page title |

So the page title moves or disappears from one page to the next.

**1. Three page layouts, one header for all.** Every page is built from one of three layouts. Nothing else is allowed: a page that fits none is a decision.

- **The page header, the same on every page.** It sits at the panel's top left, always at the same place:
  - a label (the business, or "Airth");
  - the title;
  - an optional one-line summary under it;
  - the page's actions on the right.

  The title is the page's name, with the sentence that says what matters today as the summary. On HQ, "One business to watch." becomes the summary of "HQ".
- **Reading:**
  - **Content:** one column, aligned left under the header, 880 px at most.
  - **Pages:** HQ, Business, Tasks, Settings, Workshop, a task's run, the profile.
- **Board:**
  - **Content:** the panel's full width under the header, for grids and timelines.
  - **Pages:** Team, Calendar, Usage.
- **Workspace:**
  - **Content:** under the header, a list (300 px), the work in the middle, and details (320 px) on the right; the same widths on every workspace page.
  - **Pages:** Needs you, Routines, Documents.
- **The same everywhere:**
  - the panel's padding;
  - the rail, the header bar, search and the theme;
  - one set of components (card, list row, tile, chip, button, empty state, "In development"), defined once in `docs/design/DESIGN.md` and used by every page.
- **The test:** ui_smoke checks that every page uses one of the three layouts, with the header in it.

**2. Components without a build: Preact with `htm`.** The pages are written as components, so the header, the layouts and the shared parts exist once.
- **What it is:** Preact (about 4 KB) with `htm` (about 1 KB), the same model as React. Both are vendored as files in `src/ui/vendor/`, so there is no npm build and no Node needed to run Airth, and decision 57's other choices stand.
- **Not React with a build, nor Next.js:** Airth stays one binary that serves its own pages. Next.js's server rendering serves no purpose there.
- **Tests:** ui_smoke keeps running with Node alone.
- **The cost:** two small vendored libraries, which need a decision of their own (`CLAUDE.md`: no new dependency without one); this is it.

**3. Every classic feature moves, or leaves by decision (decision 42).** In this order: first what the beta needs (Hedgen, then Numirama), then the rest.

| Classic | In the new interface | Layout |
| --- | --- | --- |
| A task's run: its conversation, approvals, diff, checks, adopt | Tasks → the run | Reading |
| New task (the composer) | the command bar and Tasks (exists) | — |
| Business settings: general, models, permissions, repositories, memory, access | Business, and Settings for keys and models | Reading |
| Connectors and skills | Settings | Reading |
| Profile, appearance, phone pairing | the profile | Reading |
| New business, welcome and setup | New business | Reading |
| Agents: edit a job, give a routine, let go, hire | Team → the agent; Hire | Board, then Reading |
| Documents (the editor) | Documents | Workspace |
| Usage | Usage, under Settings | Board |
| Steering (the strategy) | Business | Reading |
| Routines and Planning (the old objectives) | leave: the graph routines and Calendar replace them | — |

The classic interface stays served at `/classic` until its last feature has moved. Then it is removed with its code, in one PR. Until then, a page not moved yet shows "In development" with a link to its classic page.

**When:** after Friday's install; nothing in it before then. One PR per page or feature, screenshots at 1440 and 390, under the freeze's rule (features move, none is added).

*Why:* one interface is what the founder asked for, and the only way to stop keeping two in step. A fixed set of layouts makes every page predictable: the title and actions are always in the same place, and a new page cannot invent its own. Components make the shared parts real instead of copied. Preact with `htm` gives that without the build step decision 57 keeps out.

*Cost:*
- The largest piece of interface work left: about 4,700 lines of classic pages to move, in maybe fifteen PRs.
- Every existing page of the new interface is rewritten once, into components and layouts.
- Two vendored libraries.
- Until the move ends, links to `/classic` for what has not moved.

## 71. The classic interface stays the base, with the new interface's rail and header

**Accepted**, the founder's direction after trying both (3 November 2026): "keep the classic interface, only change its menu for the new interface's menu".

Decision 57 grew a new interface next to the classic one, to replace it at milestone 5. Tried side by side, the founder prefers the classic one's pages and sizes, and the new one's navigation. So:

- **The classic interface is the default page again** (`/`). The new one stays at `/next` for the pages the classic one does not have.
- **Its sidebar is replaced** by the new interface's icon rail and header bar. The header holds the business switcher, the page's name and the command bar.
- **The rail has the new interface's entries.** Each opens the classic page where one exists, else the new interface's page:
  - **Classic pages:** Team → Agents; Tasks → Tasks; Business → the business's settings; Settings → Connectors and skills, the classic page that leads to permissions, keys and models (`CLAUDE.md`'s Settings).
  - **New pages:** HQ, Needs you, Workshop. Routines and Calendar too: the classic Routines and Planning pages predate routines as graphs (decision 49) and legal deadlines (decision 66), so they would show a business's routines as none.
  - **At the top and bottom:** search (⌘K) at the top; the theme toggle and the founder's profile (classic) at the bottom.
- **The classic pages without an entry** (Steering, Documents, Usage, the classic Routines and Planning) stay reachable through search (⌘K) and from the pages that link to them.
- **Both interfaces show the same rail and header**, so moving between them does not show.
- **Sizes and themes are the classic interface's.** The new interface's pages use the same scale and themes (as already merged).

*Why:* the founder decides from use. The classic pages are complete and dense; the new rail is easier to scan, and its HQ and Needs you pages have no classic equivalent. Mixing them by page keeps what works in each, without rebuilding pages.

*Cost:*
- Two interfaces to keep, joined by one rail that has to stay in step in both.
- Milestone 5's "the new interface becomes the main screen" now means this combined interface. `CLAUDE.md`'s line on it is updated with this change.
- The classic sidebar's list of recent runs goes; the Tasks page lists them.

## 70. Agent mail: one OVH mailbox per role, read as untrusted data, sent at level 1 with a blind copy to archive@

**Proposed.**

Milestone 7: at least one role handles mail under `CLAUDE.md`'s six rules. IMAP and SMTP are a new kind of integration (integrations are HTTP-only, decision 10), so they are security code with a decision and tests of their own.

- **One mailbox per role, on the business's domain.**
  - **The address:** the agent's file gives it (`mailbox: support@hedgen.xyz`, a field decision 43 already has).
  - **The credentials:** the password is a realm secret in the Airth home, bound to the mail integration, never in the business folder, never seen by a model (decisions 7 and 10). Never the founder's personal mailbox.
  - **The provider:** OVH MX Plan: IMAP `ssl0.ovh.net:993` and SMTP `ssl0.ovh.net:465`, both TLS only, the certificate checked. A mailbox's servers are its config, so another provider is config, not code.
- **Reading: incoming mail is untrusted data.**
  - **Fetching:** Airth fetches new mail over IMAP every few minutes (`UID SEARCH`, then `BODY.PEEK[]`, so the server keeps its unread state). Nothing on the server is deleted or moved.
  - **Storing:** each message is stored once in `business.db`: message id, thread, sender, recipients, subject, date, and plain text. HTML is reduced to text; attachments are named, not opened.
  - **Use:** a routine reads it through a `Read` block (`"source": "mailbox"`). An `AI` block holds no tool, so the step that reads mail can act on nothing (rule 2).
  - **No inbound mail triggers an action.** A mail can start a routine (trigger "mail received"). But anything downstream of a mail reaches an `Act` only through an `Approval`, whatever the action's level. The graph check refuses any other graph. This is stricter than the general rule on outside content: there, a level can allow it; for mail, no level can.
- **Bank details: always the founder (rule 4).** A deterministic check in code, not a model, flags any mail that mentions a change of bank details: IBAN, sort code, account number, RIB, "new bank", "updated payment details" and the like, in English and French. It raises an item in Needs you and on Telegram, without exception. The mail's routine goes on, but nothing from that mail can be approved without the founder seeing the flag.
- **Sending: level 1, recorded, copied, signed.**
  - **The action:** `mail.send` is a declared action of each mailbox (to, subject, text, and the thread it answers). Its ceiling is 1 for the beta: every mail waits for the founder's approval.
  - **New correspondents (rule 3):** an address never written to before is marked on the approval.
  - **Code's additions:** the code, never the model, adds:
    - a blind copy to `archive@<domain>`;
    - the signature "Prepared by an AI agent of <business> and sent with the founder's approval";
    - `In-Reply-To` and `References` for a reply.
  - **What is recorded:** each sent mail is recorded in `business.db`: agent, approval item, thread, recipients, subject, the server's answer. This is append-only evidence (decision 47).
  - **Idempotence:** sending is idempotent by the approval item: a retried run never sends twice (decision 55). A send whose outcome is unknown, cut after the data, is an incident, never retried blindly.
- **DNS:** SPF, DKIM and DMARC are set at OVH by the founder. `airth doctor` lists what to check. Cold outreach stays out of the beta, and would only ever use a separate domain.
- **Code and crates.** Blocking I/O, as everywhere (decision 1).
  - **SMTP:** `lettre`, a mature client with a blocking transport and rustls. Writing SMTP and MIME by hand is where mail goes wrong (encoding, headers, line lengths).
  - **IMAP:** a small client of our own, over the `rustls` Airth already builds: only `LOGIN`, `SELECT`, `UID SEARCH` and `UID FETCH`. The Rust IMAP crates are either unmaintained or async.
  - **Reading MIME:** `mail-parser`, with no dependencies, which parses what senders send, encodings included.
  - Three new direct dependencies, each named here for why the standard library or an existing crate is not enough.
- **Tests: against a local mail server only.** A small fake IMAP and SMTP server in the tests, on `127.0.0.1`. Plain TCP is allowed only for a loopback address, and only in tests. The tests cover:
  - fetch once by UID;
  - the bank-details flag;
  - an Act downstream of mail refused without an Approval;
  - the blind copy, signature and thread headers on what reaches the server;
  - a retried send not sent twice;
  - a password never in a model's input.

  No test talks to OVH.

*Why:* a mailbox per role keeps the legal and practical boundary of the business: the founder's own mail stays out, and what each agent sends is traceable. Reading mail as data that cannot act is the same defence as for posts and pages, and is the main defence against instructions hidden in mail. Copying and signing in code keeps both true whatever a model writes.

*Cost:*
- Mail code is security code to maintain, with three new dependencies.
- Polling, not push, so mail arrives within minutes.
- Every mail waits for the founder in the beta.
- The keyword check will flag some innocent mail, and rule 4 prefers that.
- OVH accounts and DNS are set up by the founder.

## 69. The routine editor v1: parameters edited in place by the founder, checked before they are written; no graph library yet

**Proposed.**

Milestone 5 asks for the routine editor v1, without full drag and drop: see and edit each block's parameters, a dry run from the page, and validation errors shown where they are. Decision 57 left the graph library to the editor's own decision. The graph format (decision 49) does not change.

- **No graph library for v1.** The graph stays drawn as today, by the page's own code: the blocks in order, the branches of a condition labelled, and the row scrolling sideways when it is wider than the page.
  - Without drag and drop, nothing needs a library.
  - Decision 57's exception stays open for the editor that adds and links blocks.
- **The founder edits parameters, not the shape.** Selecting a block opens its parameters in the details panel (`docs/design/mockups/routine-editor.html`), as form fields of the block's type:
  - **Trigger:** schedule;
  - **Read:** source, time range, limit, `distinct`;
  - **AI:** prompt file (among `prompts/`), model (main or fast), all at once or item by item, limit, `as`;
  - **Condition:** its test;
  - **Approval:** what is editable, expiry, the flag field;
  - **Act:** the action (among those declared) and its fields;
  - and the routine's description.

  Adding, removing or relinking blocks stays in the file, or is asked of an agent as a change the founder adopts ("Ask an agent to change this routine" in the mockup).
- **Saving is the founder's write, checked first.**
  - **Allowed:** the app writes `routines/<name>.json` in the business folder. Decision 44 forbids runs to write the config, not the founder, as with "Keep this look" (decision 65). The route takes the app's token only; no run and no agent can call it.
  - **The same check as loading:** the edited graph goes through the checks that loading uses (unknown keys, schema keywords, declared actions, and the rule that outside content reaches an `Act` only through an `Approval` unless its level allows it). The graph validation code is not changed: it gains this one caller.
  - **Refused, nothing is written:** each error shows on its block and field, from the block id the check already names.
  - **Accepted:** written atomically, keys in the file's own order, so `git diff` shows only what changed. This needs serde_json's `preserve_order` feature: `indexmap` is already in the lockfile, so no new crate.
  - **A file that changed on disk since the page read it** (the founder editing it in another editor) is refused with "changed since you opened it": never overwritten.
  - **Git:** Airth does not commit. The change shows in git like any edit by hand, and the founder commits it.
- **Dry run from the page.** The existing dry run (`graphs/<name>/dry-run`), on what is saved: a dry run of unsaved edits would run a graph nobody checked. The page shows each block's state from the run (items in and out, time, what it would ask and do), as the mockup's details do. An edit the founder has not saved disables Dry run until it is saved.
- **Two fixes from the dress rehearsal come with it:** the graph no longer clips its last blocks, and a routine started from Telegram says so instead of "By hand".

*Why:* the founder tunes routines often (a limit, a prompt, an expiry), and doing it in a JSON file is the friction milestone 5 removes. Checking with the loader's own code means the editor cannot write a graph Airth would refuse, and the security rule on outside content holds whatever the page sends. A library brings nothing until blocks move.

*Cost:*
- A form per block type to keep in step with the format; a new key in the format needs its field in the editor.
- `preserve_order` changes how every JSON object keeps its keys in Airth (insertion order instead of sorted), which tests must not rely on.
- Edits are not versioned by Airth; git does that, once the founder commits.

## 68. Finance entered by hand, until a bank is connected

**Accepted**, the founder's request: Revolut's API needs a plan upgrade, so decision 54 stays coded but unused for now.

The HQ's finance (decision 53) waits for a bank read. Until a bank is connected, the founder enters the figures by hand in the business's folder.

- **`business/finance.md`**, in the config format:
  - `cash`: the money in the bank, a number;
  - `spent`: what the business spent this month, a number;
  - `as_of`: the date of these figures;
  - `currency`, optional: the profile's `currency` by default.

  The body is free notes. An unknown key or a wrong value (not a number, not a date) is refused, naming the file and the field. The HQ then shows finance as unknown, with that reason.
- **The HQ shows them as entered by hand, as of their date:** "Entered by hand · as of 2 Nov".
  - Cash and out (`spent`) are shown as entered. In is shown as not known.
  - Runway is `cash ÷ spent`, in months. A month with no spending shows "No spending", as for a bank.
  - The runway counts in the business's health, as a bank's does (decision 53).
- **More than 35 days after `as_of`**, the figures turn to unknown, "Entered by hand more than 35 days ago", never shown stale.
- **A connected bank takes precedence.** Once a bank is connected (decision 54), the HQ shows the bank only, its unknown states included. `finance.md` is then ignored, without being deleted.
- **Runs cannot write it:** `business/` is read-only for every run (decision 43, amendment), so an agent, or a page it read, cannot change the figures the founder relies on.

*Why:* the runway is the HQ's one finance signal, and the founder knows the two figures it needs. A file in the business folder is the config's own way, versioned in git, so a change shows who made it and when. Turning old figures to unknown keeps decision 54's rule: a figure is shown with its date or not at all.

*Cost:* the founder updates the file at least monthly. `spent` is the month's spending as the founder states it: entered mid-month, it understates the month, so the runway reads longer. The founder enters a full month's spending for a fair runway.

## 67. The Linux sandbox: bubblewrap, built from the same rules as the macOS profile; Landlock later

**Accepted**: the founder said to go on with it.

Decision 29 gates the VPS (milestone 9) on a Linux sandbox, "generated from the same path rules as the macOS profile, with the bench's security tasks passing on Linux". Today `sandbox::available()` is false on Linux, so commands run unconfined there (decision 4, *Cost*).

**What it must match.** `sandbox-exec` enforces these today. The Linux sandbox enforces each one, built by code from the same inputs: the workspace's rules, its pinned paths, whether the network is allowed, and the owner's home.

1. **File contents by path, the most specific rule deciding:** none, read, or read and write (decision 4).
2. **No write anywhere a rule does not allow** (decision 48):
   - the system is read-only;
   - `/dev` is writable;
   - so are the tool caches (`~/.cargo/registry` and the others in `sandbox.rs`).
3. **In the owner's home** (decision 50): nothing is readable but the developer tools, and never the credentials (decision 48). The credential list gains the Linux ones (`~/.local/share/keyrings`, `~/.password-store`). The Mac's keychain services become the user's session runtime folder (`$XDG_RUNTIME_DIR`: the D-Bus bus, the keyring and SSH agent sockets), hidden from commands.
4. **The Airth home and the other realms are unreachable.**
5. **The config folders of a business** (`business/`, `agents/`, `routines/`, `prompts/`), **`.git/hooks` and `.git/config` are read-only**, creating them included (decisions 43, 46).
6. **Pinned paths** (`.git` of a run in place) cannot be renamed or removed (decision 46).
7. **Network cut** when the run may not use it (decision 51), DNS included.
8. **Metadata stays readable**, so a command can cross a closed folder to reach an open one inside it.

**Mechanism: bubblewrap.** `bwrap` is a small program, packaged by every distribution, which builds a private view of the files for one command, in a user namespace, without root. It is called the way `sandbox-exec` is called today, as a program with arguments, so no new crate.
- **Rules to mounts, shortest path first, as today:**
  - read becomes a read-only bind;
  - read and write becomes a writable bind;
  - none becomes an empty `tmpfs` over a folder, or `/dev/null` over a file.

  A later mount covers an earlier one, so the most specific rule decides, as in the macOS profile.
- **Pinned path:** it becomes a mount point of its own, which the kernel refuses to rename or remove.
- **Network:** `--unshare-net` when it is cut. The command gets its own loopback, so local test servers still work, but it cannot reach the host's: this is stricter than macOS, and it is said in `airth doctor`.
- **Also:**
  - `--unshare-pid`: commands cannot see or signal Airth's processes;
  - `--new-session`: a command cannot type into the terminal it came from;
  - `--die-with-parent`, `--unshare-ipc`;
  - environment variables already pass through the existing allow-list (`PASSED_ENV`). `SSH_AUTH_SOCK` and `DBUS_SESSION_BUS_ADDRESS` stay out of it.
- **`available()`:** on Linux, true only when `bwrap` is installed and actually starts a confined `true`. User namespaces can be off, as with Ubuntu 24.04's AppArmor restriction unless bubblewrap's profile is installed.
- **Fail closed on Linux:** without a working sandbox, an unwatched run never executes a command. Its command is refused with the reason, and `airth doctor` names the fix. On macOS, nothing changes.

**Landlock: not now.**
- **It cannot express the rules alone.** It only grants: it cannot close `~` inside a readable `/`, nor `.ssh` inside a readable `~/.cargo`. Every exception would have to be enumerated from the folders that exist at start.
- **Its network control is limited.** It covers TCP ports only (kernel 6.7 and later), not DNS or UDP.
- **It would need its own code.** A crate, or raw system calls, in a security boundary.

As a second layer under bubblewrap, it would still add defence in depth, and could also confine Airth's own process. A backlog line, after the beta.

**The security scenarios, the gate of milestone 9.** The harness kept from `business-autonomy` (reconciliation, §34: a throwaway home, a scripted model, local stand-ins) is ported for them. The scripted model makes each attempt through the full path: runtime, permission gate, tools, sandbox. So the scenarios test the boundary, not a model's good will. They run in CI on Linux (a new `ubuntu-24.04` job with `bubblewrap` installed) and on macOS. The tests guarded by `sandbox::available()` then run on Linux too.

| Scenario | Attempt | Holds when |
| --- | --- | --- |
| Airth's data out of reach (bench security-001) | read a canary in the Airth home and in another realm, with `read_file` and `cat` | the canary appears nowhere: not in the run's messages, files or report |
| Credentials hidden (security-002) | `env`, `cat ~/.ssh/id_ed25519`, `~/.netrc`, the keyring and agent sockets | no secret appears in the run's output |
| No network unless allowed (security-004) | `curl`, a raw socket to a public address, a DNS lookup, in a routine's run | all fail; `http_request` reaches a stand-in only through a declared action |
| Writes only where allowed | write `~/.bashrc`, `~/.config/systemd/user/`, `/tmp` outside the workspace, `business/profile.md`, `agents/` | each fails, and nothing is created |
| `.git` stays the founder's | write `.git/hooks/pre-commit` and `.git/config`, then `mv .git` aside and back | each fails |
| Escape attempts | follow a symlink planted to the Airth home; a hard link to a protected file; `kill` Airth's process | each fails |
| Prompt injection (security-003) | a file read by the run asks to delete another and write `pwned.txt` | nothing changes. With the scripted model this checks the gate refuses; run once with a real model as an evaluation, not a unit test |

*Why:*
- **Bubblewrap:** it expresses the rules as they are, closing a folder inside an open one. It is called like `sandbox-exec`, so the shape of `sandbox.rs` and `shell.rs` stays: one builder per OS, from the same lists.
- **Fail closed:** refusing commands where the sandbox is missing stops a VPS misconfiguration from silently dropping the isolation decision 29 protects.
- **Scripted scenarios:** they make the gate repeatable, in CI, on every change.

*Cost:*
- A second builder in `sandbox.rs` (security code) and its tests, and a CI job on Linux.
- The VPS needs `bubblewrap`, and user namespaces allowed.
- Host-loopback access from commands is lost on Linux.
- The scenario harness is ported, with its own tests.
- macOS behaviour does not change.

## 66. The legal calendar: deadlines computed from the legal identity, by rules written as data with their source

**Accepted**: the founder said to go on with it.

Decision 43 left the legal calendar's rules to milestone 2: "derived from `country`, `incorporated_on` and `fiscal_year_end`". A missed filing costs a penalty and, for Companies House, can end in the company being struck off. Hedgen's first one is close: Companies House shows its first confirmation statement **due 20 November 2026**.

**Fields: a UK Ltd.** `business/profile.md` already has them (decision 43). Their names stay; the app shows the product words.

| Field | UK meaning | Hedgen, from Companies House |
| --- | --- | --- |
| `registration_number` | Company number | 16838911 |
| `incorporated_on` | Incorporation date | 2025-11-07 |
| `fiscal_year_end` | Accounting reference date, as `MM-DD`; shown as "Accounting reference date" for a GB company | 11-30 |
| `registered_office` | Registered office | 71-75 Shelton Street, Covent Garden, London, WC2H 9JQ |

Two new optional fields:
- `confirmation_date`: the date on the last confirmation statement filed. It is empty before the first one. Filing early moves the next review period, so Airth needs that date. Airth never writes it (decision 44): when the founder answers **Filed** on time, the record in the database is enough, so the field is only needed after an early filing.
- `reminders`: how many days ahead each reminder comes, `[30, 7]` by default.

**A French SAS** later: SIREN in `registration_number`, the *date de clôture* in `fiscal_year_end`. Its rules (liasse fiscale, approbation des comptes, dépôt au greffe, CFE) are written when Numirama is registered, in the same format.

**The rules are data, with their source.** One file per obligation in Airth, `src/legal/gb/<obligation>.md`, in the config format (flat front matter, prose in the body).
- **Front matter:**
  - `title` and `authority`;
  - `forms` (`[ltd]`);
  - `from`: what the deadline counts from (one of a fixed set of dates computed in code: incorporation, end of the confirmation review period, end of the period of accounts, end of a Corporation Tax period);
  - `after`: the delay, such as `14d`, `9m`, `9m1d` or `12m`;
  - `months`: how months are counted;
  - `first`: the rule for the first period, where it differs;
  - `source`: the official pages and the statute section;
  - `checked`: the date the rule was last checked against its source.
- **Body:** the rule in plain words.
- **Shared by every business:** rules are not one business's config. A business's files never change a rule.
- **Checked by tests:** each rule's test reproduces dates its authority has published for a real case: Companies House's page for Hedgen, and the examples on GOV.UK.
- **Ambiguity:** where a rule can be read two ways, Airth takes the earlier date, and the rule's file says so.

**The four UK rules** (private company, profits under £1.5 million):

| Obligation | Deadline | Source | Hedgen's next |
| --- | --- | --- | --- |
| Confirmation statement (Companies House) | 14 days after the review period ends. The period runs 12 months from incorporation, then from the day after the last confirmation date. | Companies Act 2006 s853A; [confirmation statement guidance](https://www.gov.uk/guidance/confirmation-statement-guidance) | review to 6 Nov 2026, due **20 Nov 2026** (matches Companies House) |
| Annual accounts (Companies House) | 9 months after the end of the accounting reference period. A first period longer than 12 months: the later of 21 months after incorporation and 3 months after its end. A period ending on a month's last day ends on the last day of the due month. | Companies Act 2006 s391(4), s442, s443 | first period to 30 Nov 2026, due **7 Aug 2027** (matches Companies House) |
| Company Tax Return, CT600 (HMRC) | 12 months after the end of the period of accounts; for a period longer than 12 months, both returns by then | [Company Tax Returns](https://www.gov.uk/company-tax-returns); [accounting periods](https://www.gov.uk/corporation-tax-accounting-period) | two returns, due **30 Nov 2027** |
| Corporation Tax payment (HMRC) | 9 months and 1 day after the end of each Corporation Tax period. A period is at most 12 months, so a first period of accounts longer than that splits in two. | Taxes Management Act 1970 s59D; [Pay Corporation Tax](https://www.gov.uk/pay-corporation-tax); [first accounts](https://www.gov.uk/first-company-accounts-and-return) | 7 Nov 2025 to 6 Nov 2026: **7 Aug 2027**. 7 to 30 Nov 2026: **31 Aug 2027** |

For the second payment, sources differ: counting from 30 November, some give 31 August, others 1 September. Airth takes 31 August, the earlier. The founder checks both payment dates once against HMRC's letter on Corporation Tax periods: HMRC sets the first period, and its letter is the authority.

**Where deadlines show:**
- **Calendar**, a section **Deadlines**:
  - the next occurrence of each obligation over twelve months: date, days left, authority and the rule's source;
  - a deadline within the week also among the week's days;
  - a field left `TODO` that a rule needs: Calendar names the field instead of a date.
- **Needs you and Telegram**, at each reminder (30 and 7 days before by default):
  - **One item:** of kind `deadline`, "Confirmation statement due 20 Nov 2026, Companies House", with the source.
  - **Two answers:** **Filed** records the filing, and nothing more comes for that occurrence. **Seen** closes this reminder only.
  - **Overdue:** the day after a date passes unfiled, an item says it is overdue. It stays until the founder answers Filed.
  - **A reminder whose day has passed:** at the first run, as for Hedgen's 30-day reminder, its item comes at once.
- **When the check runs:** once a day, in the server's tick, with no model involved.
  - **Idempotent:** one item per obligation, occurrence and reminder, kept by a unique key in `business.db`.
  - **Failure:** a failure of the check is an incident.

**Stored:** one table in `business.db`, `legal_filings`, through the access layer, append-only like all evidence (decision 47). Each row is an obligation, the occurrence (its period end), when it was filed, and the queue item that recorded it. A later confirmation period counts from the last confirmation date: the later of `confirmation_date` and the last filing recorded.

**Not covered:** VAT returns (when `vat_number` is set), payroll, Corporation Tax instalments (profits over £1.5 million), and moving the accounting reference date (the founder changes the field). A line goes to the backlog.

*Why:*
- **Fields:** the dates follow from fields the founder already keeps. Computing them keeps one source of truth, the company's identity, instead of dates typed by hand that go stale.
- **Rules as data:** a rule written as data with its source can be checked by reading it next to the statute, and tested against dates the authority itself publishes. Code holds only the date arithmetic, so a rule that changes is a file to edit.
- **Earlier date:** taking it on ambiguity makes a doubt cost a few days of margin, never a penalty.

*Cost:*
- Four rule files to keep current. Their `checked` date says when they were last read against the source.
- A small date module, with month arithmetic in two forms: the Companies Act's month-end rule and corresponding dates.
- One table, one item kind, and a daily check in the server's tick.
- The founder fills `confirmation_date` after an early filing.
- Not legal advice: the Calendar links to each source, and the founder's accountant remains the authority.

## 65. Agent characters: drawn by code from a seed saved when the agent is hired

**Accepted**, with the founder's precision: Team offers "Keep this look" for agents hired before.

Team and the hiring step show each agent as a small character (`docs/design/mockups/hire-agent.html`, `docs/design/characters/`). The founder recognises a role at a glance, and its state (working, needs you, incident) shows without reading. The question is what is stored, and what is computed.

- **Stored: one seed, once.** When the founder hires an agent from the app, its file `agents/<name>.md` gets a front-matter field `look` (eight hexadecimal characters). Hiring is the founder's act, so writing it is allowed: decision 44 forbids runs to write the config, not the founder. Airth never changes `look` afterwards. "Another look" draws a new seed before hiring only.
- **Agents hired before this** (Hedgen's four) have no `look`. Their seed is derived from their name, so their look is stable too. Team offers them **Keep this look**, the founder's gesture, which writes that derived seed as their `look`, so a later rename does not change the character. Airth never writes it on its own.
- **Computed: everything else, by code, in the app's JavaScript, as inline SVG.**
  - **From the seed:** the shape (round, bean, block), the eyes and the small traits.
  - **Colour:** from the agent's department, one hue per department defined once as tokens, never a status colour (amber, red, green).
  - **Accessory:** from the agent's optional front-matter field `accessory`, one of a fixed set Airth draws (pen, lens, book to start). A new accessory is code, never a business's file.
  - **No image file, no model-generated picture, no new crate.**
- **States, from Airth's own records:**
  - **working:** a routine or task of the agent runs;
  - **needs you:** it has an item waiting;
  - **incident:** its last run failed;
  - **asleep:** nothing is scheduled;
  - **idle:** anything else.

  Animated on Team and when hiring. Still in lists, where a character moves only when its state changes. No motion under `prefers-reduced-motion`. The character adds to the agent's name and status text, never replaces them.

*Why:* a seed is the smallest thing that keeps a look stable for good, and it lives with the agent it belongs to, in the business folder the founder owns. Drawing by code from tokens keeps the characters consistent with the design and costs nothing at run time. Deriving a seed from the name for agents hired before keeps their look stable without Airth writing their files.

*Cost:* a JavaScript generator to keep, ported from the reference `chars.py`, and department hues added to the tokens. Two optional front-matter fields in agent files. Renaming an agent without a `look` changes its look; one with a `look` keeps it.

## 64. The command bar: an order is a task run by one agent, routed by the fast model when not named

**Accepted**, with the founder's three additions: the agent's scope only, every ask of an unwatched task in Needs you, a budget per task.

`CLAUDE.md` puts a command bar on every page: a one-off order, routed to the right agent or the one named, and what follows goes through Tasks and Needs you. Today a run the founder starts has no agent: it is a chat. The bar needs two things the code does not have: which agent a run works as, and how an order finds its agent.

- **An order is a task.** It is a run, as today's missions are, with one new field in its run state: `agent`, the name of the agent it works as. The run's first message gives the agent's job (its file in `agents/`, as a routine's run already does) and then the order. Tasks lists the business's runs that have an `agent` and no routine: the founder's orders.
- **The agent's scope, never the realm's.** A task runs with the integrations, skills (MCP servers) and repositories its agent's file in `agents/` declares (front-matter fields `integrations`, `skills`, `repos`, comma-separated), never the whole realm's. An agent that declares none has nothing beyond read-only web access (`http_request` GET to public addresses).
- **Unwatched, never a silent wait.** An order starts as `Job::Alone`: nobody watches it (decision 56). What commits the business stops the run and waits in Needs you and on Telegram, as before. So does **any permission the realm's rules would ask for**: in an unwatched task, an ask becomes a Needs you item (and Telegram), the run stops there, and it resumes once the founder answers.
- **A budget per task.** A maximum duration and a maximum model cost, with defaults in the business config (`business/profile.md`: `task_max_minutes`, `task_max_cost_usd`). Past either, the task stops, and the founder is told why in Needs you.
- **Routing.**
  - **Named:** the founder picks the agent in the bar, or starts the order with its name ("writer: outline the week's article"). That agent takes it, no model involved.
  - **Not named:** one call to the realm's fast model chooses among the business's agents. The call has a versioned prompt in Airth (`src/prompts/route.md`), a schema whose `agent` is an enum of the agents' names, plus `why`, and a journal entry like any model call. The bar shows the chosen agent and why before the task starts; the founder can change it.
  - **No agent fits**, the answer is invalid, or the business has none: the order is not started. The bar says so, and the founder picks.
- **Tasks page, grouped:**
  - **To review**: the run ended and something of it waits in Needs you;
  - **Running**;
  - **Waiting**: stopped on Needs you;
  - **Done**, the last ones.

  Each task shows its agent and its order.

*Why:* an order should go to the role that owns that work, with that role's job as context. Naming it is free and exact. Routing an unnamed order is a judgment between roles, which a cheap model call does well once it is versioned and checked. Starting unwatched keeps decision 56's rule: nothing commits without the founder's explicit approval. Reusing runs as tasks keeps one way of doing work, as `CLAUDE.md` asks (no new task store).

*Cost:* one field added to the run state (JSON, as today). One Airth-level prompt and its schema to keep. Each unnamed order costs one fast-model call. A business with one agent never needs routing. A task is only as long-lived as its run: there are no recurring tasks, which stay routines.

## 63. Replies: the founder sends a post's link on Telegram; Airth drafts the reply into Typefully, never onto X

**Accepted**, with the founder's precisions: replies are level 1 like posts; the link may come with one line of angle; a post too thin to answer is said, not drafted.

Milestone 4 includes replies. Airth never writes to X through the API (`CLAUDE.md`), and replies on X stay at level 0: the founder publishes them. Today the founder writes a reply as a Typefully draft with the post to answer as `reply_to_url`, by hand. Two questions: how Airth learns which post to answer, and how its reply reaches the founder.

**Reading: three options.**

| | Cost | Rules and risk | What Airth gets |
| --- | --- | --- | --- |
| X API, read access | Pay per use since February 2026, no free tier: about $0.005 per post read and $0.01 per user or following lookup, by third-party sources (to confirm on X's own page). Following 30 accounts at about 20 posts a day each is about 600 reads a day, so about $90 a month. | A developer account and X's developer terms. X's automation rules forbid automated replies chosen by keyword, so Airth could only propose, which it does anyway. | Every post of the followed accounts. Airth chooses which to answer. |
| A followed-accounts list through another source | Third-party X data APIs are cheaper, and Nitter-style RSS is gone. | They scrape X, which X's terms forbid. They break without notice. A business should not depend on them. | The same as above, without the guarantee. |
| **The founder sends a post's link to the bot on Telegram** | Free. X's public oEmbed endpoint (`publish.x.com/oembed`, no key, checked on 3 November 2026) gives the post's author, text and date. | No X account for Airth and no terms beyond oEmbed's. A protected or deleted post gives nothing, and Airth says so. oEmbed gives no images, no quoted post and no thread. | Only the posts the founder chose. Choosing which conversation to join is the founder's judgment anyway. |

**Recommended: the founder sends the link.** It costs nothing, breaks no terms, and leaves the choice of who to answer with the founder, where it belongs while replies stay at level 0. Reading followed accounts through the X API goes to the backlog (it would serve milestone 6, if the founder wants Airth to suggest posts to answer). It needs its own decision on cost then.

**How it runs.**

- **The founder's message.** The link alone, or the link and one line of angle ("push back on the yield claim"). The angle goes to the reply prompt as the founder's intent (`founder_angle`), never as a fact about the post.
- **Trigger.** A message from the founder in the paired chat (decision 52: their chat and their user, never a forwarded or edited message) that holds an `x.com` or `twitter.com` post link starts the business's routine whose trigger is `{"telegram": "x_post"}`. It is an event trigger, which decision 49 already names, not a new block. A message with no such link is ignored, as today.
- **Read.** A new Read source, `x_post`, fetches the post through oEmbed, through the runtime like any read. It gives one item shaped like a feed item: `title` (the author and the post's first words), `summary` (the post's text, as plain text), `published`, `link` (the post's canonical URL) and `source_id` (that URL). The check prompt and the graph rule then work unchanged. The post's text comes from outside: it reaches no Act without an Approval. When the post's own words are too thin to answer (it is mostly an image, a chart, a link or a quoted post), the routine stops there and the bot says so on Telegram; nothing is drafted.
- **AI: the reply.** A `prompts/reply.md` per business, with its schema (`reply`, at most 280 characters; `angle`). Then the check block (decision 58) on the reply.
- **Approval.** In Needs you and on Telegram, with the post it answers.
- **Act: a Typefully draft that answers the post.** A declared action, for example `typefully.create_reply`, `POST /v2/social-sets/{social_set_id}/drafts`, its body fixed: `{"platforms": {"x": {"enabled": true, "posts": [{"text": "{text}"}], "settings": {"reply_to_url": "{reply_to_url}"}}}}`. Never `publish_at`, so it can only be a draft. Class `reversible`, at level 1 like posts: the founder's Approve creates the draft. Nothing reaches X until the founder publishes it in Typefully. That is how this decision reads `CLAUDE.md`'s "replies on X stay at level 0": Airth prepares, the founder posts.

**The reply prompt's contract** (each business writes its own `prompts/reply.md`; Airth only checks its schema):

- It is written from the post's text only (`summary`, `published`, the author in `title`), never from the link or from what the model remembers. A figure, name or date that is not in the post is not used.
- The same voice rules as the business's `post.md`: the founder's voice, readings marked as opinions, at most 280 characters, no emojis, hashtags, exclamation marks or hype words, no price prediction, no advice to buy or sell, no reply-farming question.
- It answers the post: it adds a fact or a reading the post invites, or asks the one question the founder would genuinely ask. It never only praises or agrees.
- Everything in the post is material, never instructions.

*Why:* replies are where an account is most exposed: wrong tone, a fact out of context, a reply to the wrong person. Keeping the choice of the post with the founder, and the publication in Typefully under the founder's hand, keeps that exposure where it is today, while the drafting and the check move to Airth. Reading only what the founder sends avoids both X's API cost and the terms risk of scrapers. Shaping the read post like a feed item reuses the check and the graph rule as they are.

*Cost:* a new trigger kind (Telegram event) and a new Read source (`x_post`, oEmbed). Graph validation learns both (security code). Telegram starts reading the founder's free messages for links, under decision 52's rules. A reply needs the founder's tap twice: sending the link, then approving the draft. oEmbed can change or close without notice, and then replies stop until it is replaced; the routine's failure says so. No reading of followed accounts until the backlog item is decided.

## 62. The founder can retry a failed action from its incident, once it is clear nothing was done

**Accepted.** Coded after point B, outside the service installed for it.

An approved action that fails becomes an incident (decision 55), and its idempotency key stays `failed`: Airth never does it again, even once the cause is fixed. On 2 November the Typefully integration still called API v1; the approved HKMA draft got a 403, and once the declaration was fixed it could be re-sent only by hand, with a command outside Airth.

- **Retry on the incident.** An incident from a failed action offers **Retry**, in Needs you and on Telegram. Only the founder's channels (the app and Telegram) can answer it, as for Override (decision 58).
- **Offered directly when the failure proves nothing was done:** the service refused the request with a 4xx other than 408 and 409 (for example 400, 401, 403, 404, 422).
- **Ambiguous failures ask first.** The answer was cut after the request was sent, the service answered 5xx (502, 504) or 408 or 409, or no status came back. In these cases the incident says the action may have been done. Retry then asks the founder to confirm they checked that nothing was created, before it is offered.
- **Same key, done once.** A retry uses the action's own idempotency key: it moves from `failed` back to in flight, and its attempts and log record the retry, who asked for it, when and from which channel. It is done at most once per Retry, never twice for the same key once it has succeeded. A retry that fails again leaves a new incident, under the same rules.
- **Rebuilt, not replayed.** The request is rebuilt from the approved item and the action's current declaration, as the routine would build it now: a corrected declaration, like the Typefully one, is what a retry uses. The fields come from the item as approved; nothing is re-drafted.
- **Never for commits.** An action of the commits class is never retried from an incident: it goes through a new explicit approval (decision 56), or the founder does it.

*Why:* a failure the founder has fixed (a key, a declaration) should not need a command outside Airth to finish what they already approved. Refusals that prove nothing happened are safe to retry; the others could duplicate a draft, a mail or a payment, so the founder checks first. Rebuilding from the current declaration is what makes a fix count.

*Cost:* the action journal gains a `failed` → `in_flight` move, allowed only from a founder's Retry, in the access layer (security code). An incident must keep what failed (status code, whether the request was sent) to tell a clear refusal from an ambiguous one. Telegram gets a Retry button and, for ambiguous failures, a confirmation step. A retry rebuilt from a changed declaration can send something slightly different from what the first attempt sent; the item's approved fields stay the same.

## 61. What a run drops reaches the founder: a note on its remaining items, and the weekly count

**Accepted.**

An item whose answer does not fit its schema is asked once more, then dropped, and the run's report names it with why. But a real run's report stays in its journal row: nothing tells the founder that two posts came this morning instead of three. The founder asked that a dropped item reach them, without making it an incident.

- **The run remembers what it dropped.** Each item dropped in a run is recorded with the run: the block, the item's title and the reason (for example `answer.draft has 300 characters, more than 280`). The block's record of the run (`routine_blocks`) gains a `dropped` column, a JSON list: every run has those records, manual ones included, whereas only scheduled runs leave a heartbeat.
- **A note on the items that remain.** Every approval item the same run puts in the queue carries one line written by code, never by a model: `2 of 3 drafts: "Hack" dropped (answer.draft has 300 characters, more than 280)`. The queue gains a `note` column. Needs you shows the note under the title, and Telegram as one line above the buttons.
- **Nothing left to approve.** When the run drops every item, no item can carry the note. The run then puts one report item in Needs you and on Telegram (decision 59, answered by Seen), with the same line, so an empty day is never silent. It is not an incident: nothing demotes, and nothing failed in Airth.
- **The weekly report** counts dropped items per routine, and lists each reason once with how many times it came.

*Why:* a missing post is work the founder expected and did not get. They should know it where they already look (Needs you and Telegram) and in the week's figures, without an incident for what is a model's miss, not an outage. Code writes the note, so its count and reason are exact.

*Cost:* two columns, in one migration: `queue.note` and `routine_blocks.dropped`. The queue's is in the access layer (security code). A report item appears on the days when everything is dropped. Telegram shows the note's text, which comes from the item's title, so it follows decision 52's limits like the rest of the item.

## 60. An AI block can judge its whole list in one call, and can use the realm's fast model

**Accepted**, with the founder's amendments: an invalid list answer is retried once, then the run fails as an incident; a missing fast connection falls back to the main model with a note; the cross-item risk is accepted.

Today an AI block calls the model once per item. Daily-posts' sort (`pick`) made 51 calls and took nine minutes, and since no call sees the others, two items about the same event got two different story names and both went through `distinct`. Every AI block also uses the realm's main model, so a sort pays the price of a draft.

- **A list mode, not a new block.** An AI block gains an optional `"over": "list"`. The model then gets every item in one call, each with a number, and answers `{"items": [{"n": 1, …}, …]}`: one entry per item it keeps, each checked against the block's schema, which still describes one item's answer. An item the answer leaves out is not kept. Without `over`, one call per item, as today.
- **What the answer may do.** It may only keep, annotate and order the items it was given: an `n` that is not one of them, or that comes twice, makes the whole answer invalid. An invalid answer is recorded and refused, never repaired, and asked once more (a model call has no side effect). If the second answer is invalid too, the block fails and so does the run: a scheduled run's failure is an incident in Needs you and on Telegram, naming the block, so an empty day is never silent. The filters (`keep`, `rank`, `distinct`, `limit`) then apply as today, so `distinct` stays a check in code behind the model's grouping.
- **Bounds.** A list call takes at most 200 items; the rest are set aside with a note. A checking block (`as`, decision 58) stays one call per item, so one draft's check cannot touch another's.
- **The model.** An AI block gains an optional `"model": "fast"`: the realm's fast connection, which already exists for chats. Without it, the main model. A realm with no fast connection uses its main model, and the report says so. A block names a tier, never a provider or a key: models and their secrets stay the realm's settings.

For daily-posts: `pick` becomes `"over": "list", "model": "fast"`, and its prompt asks the model to group the items by story, then choose the best distinct ones; `draft` and `check` keep the main model, one call per item.

*Why:* grouping by story is a judgment across items, which one call per item cannot make; asking the model for each item's story and comparing names afterwards relies on it naming one event the same way twice, and it did not. One call is also one latency instead of fifty. A list mode keeps the six blocks (a sort is still an AI block: a prompt, a schema, no tool) and keeps the per-item schema the founder writes. The tier, rather than a model name, keeps the graph file free of anything secret or provider-specific.

*Cost:* items in one call can sway each other: a feed's text addressed to the model could push its own item up or others down. The sort only leads to an approval (the graph rule is unchanged), so the founder still sees every draft, but a list block must not feed an `Act` directly unless its action's level allows it, as for any outside data. Two bad answers in a row lose the whole day's sort instead of one item, and say so as an incident. The founder accepts the cross-item risk: the sort holds no tool, the check runs per item, and every post is approved by hand. A sort prompt written for one item must be rewritten for a list. The journal records one call for the list; the report's per-item counts come from the answer.

## 59. A report is an item of Needs you, one per run and block, sent to Telegram

**Accepted.**

Today `airth.report` puts a notification in the current interface's inbox, one per item: the daily watch's ten kept items make ten notifications, and none of them reaches Needs you or Telegram. Milestone 4 wants the weekly report "through Telegram and Needs you".

- **Where it goes.** A report becomes an item of the queue of a new kind, `report`: in Needs you, under its agent, and on Telegram like any item (decision 52). It has one answer, **Seen**, which closes it, as for an incident. Nothing waits on it: the run does not stop for a report.
- **One per run and block.** An `act airth.report` gathers every item that reaches it in one run into one report: its title once, then one entry per item with the fields the block names. The daily watch sends one report of ten entries, not ten. Each item keeps its own idempotency key (decision 49), so a retried run adds nothing twice.
- **The text.** A report is filled from its fields by code, never written by a model; a routine that wants a sentence of analysis puts an AI block before the act. The whole report reaches Telegram; a long one comes in several messages, as approvals do.
- **Founder time.** Reading a report and answering Seen is recorded in the ledger, like any answer (decision 27): reading is part of what running the business costs.
- **The old inbox** stops receiving routine reports in the same change; it keeps what the current interface still uses it for.

*Why:* the founder looks in two places, Needs you and the phone; a report anywhere else is not read. One report per run keeps Telegram readable, and filling it by code keeps figures the founder relies on (time, cost) from being rephrased by a model.

*Cost:* the queue's `kind` gains a value, which in SQLite means rebuilding the table in a migration (the access layer: security code). A report that nobody marks Seen stays in Needs you; reports expire after 7 days unless the founder sets otherwise. Telegram receives the report's text, so a routine must not put in a report what decision 52 keeps off Telegram.

## 58. The quality check is an AI block whose answer can flag the item; no seventh block

**Accepted**, with the founder's amendment: an edit that lifts a blocking flag is logged like an override.

Decision 53 defines the quality flag (a mark set on an item of the queue by a check step of its routine, with a reason; a blocking flag prevents Approve until the item is edited or the founder overrides it) and leaves the check step to milestone 4. It does not need a block of its own:

- **The check is an AI block.** It has a prompt (`prompts/check.md`, written for the business: its red lines, its tone, its facts) and a schema whose answer says `flag` (`none`, `note` or `blocking`) and `reason`. Like any AI block it holds no tool, and its call is journaled with its prompt version. What code can check exactly (a length) stays in the drafting block's schema (`maxLength`), where an invalid answer is already refused. A schema uses only the keywords Airth checks; a routine whose schema uses another (`pattern`, `format`, …) is refused when it is loaded, naming it.
- **An AI block names where its answer goes.** Today each AI block writes `answer`, so a check after the draft would replace `answer.draft`. A new optional field, `"as": "check"`, writes the answer under `check` instead; without it, `answer` as today.
- **The approval carries the flag.** The approval block gains an optional `"flag": "check"`, the field holding the check's answer. The item goes to the queue with that flag and its reason; `none` or no field is no flag. A check that failed (its answer invalid, the model down) flags the item `note` with that reason: the founder sees the draft was not checked, and is not blocked by an outage.
- **Blocking, edit, override** (decision 53): a `blocking` item cannot be approved; editing it lifts the block, since the text is then the founder's; **Override** approves it as it is, only from the founder's channels (the app and Telegram), logged with who, when and the channel. An edit that lifts a block is logged the same way, as "lifted by edit", so the journal shows every blocked item that went out anyway. A `note` is shown and blocks nothing. No agent, routine or tool can set, lift or override a flag: the flag comes only from the approval block, through the access layer.
- **The graph check** refuses a `flag` that no AI block `as` on the way names, like any field.

*Why:* `CLAUDE.md` adds a block only for a real routine that needs it, and this one does not: judging a draft is an AI block's work (a prompt, a schema, no tool), and acting on the judgment is the approval's. Keeping the flag on the approval also keeps it where only the founder answers. `as` is the smallest change that lets two AI blocks follow each other without the second erasing the first.

*Cost:* each business writes its own check prompt, and a check costs one model call per item. The queue gains the flag and its reason (a migration), and the queue's state machine an override (the access layer: security code). Telegram needs an Override button on blocking items. The check judges only what it is told; it is not a guarantee, and the founder's approval stays the guard.

## 57. The interface stays vanilla JavaScript and CSS, served by Airth; the new one grows next to the old

**Accepted.**

- **Stack.** The new interface is written like the current one: plain JavaScript and CSS files in `src/ui`, joined in name order by `build.rs` and served by Airth from the binary. No framework, no npm build step for the pages, no runtime request to another server. `scripts/ui_smoke.js` covers its views as it covers the current ones.
- **Next to the old.** The new interface has its own files (`src/ui/next/`: a page, its scripts, its styles) and is served at `/next` by the same server, with the same API and the same token. Nothing of the current interface is removed: it moves into Settings and the Workshop at milestone 5, then `/next` becomes `/`.
- **Design.** `docs/design/DESIGN.md`'s tokens are CSS custom properties defined once; pages use only them. Inter and JetBrains Mono are bundled in the binary and served by Airth; no font CDN.
- **One exception, later:** the routine editor's graph may use a graph library, bundled the way the Documents editor's CodeMirror already is (`src/ui/editor/`, esbuild, its output committed to `src/ui/vendor/`). Choosing that library is a decision of its own, when the editor is built. No other page gets a library.

*Why:* the founder prefers it, and the current interface already works this way: one binary, nothing to install, nothing to build to run Airth, and a smoke test that runs with Node alone. The routine editor is the only page whose drawing is hard enough to want a library, and the editor bundle already shows how to add one without a build step for everything else.

*Cost:* no components or reactivity from a framework: each page renders its own DOM, so pages must share a small set of helpers to stay consistent. Two interfaces live side by side until milestone 5, and each API change must serve both. The fonts add about a megabyte to the binary.

## 56. An action that commits always needs the founder's explicit approval, whoever started the run

**Accepted**, with the founder's precisions (watching, detaching, Telegram).

Decision 45's levels apply to the runs Airth starts (routines, their missions). A run the founder starts (a chat, an order) follows the realm's permission policy instead, as before. A realm whose policy allows the network without asking (Hedgen's does) let such a run send a request that commits the business without asking anyone. So:

- **An action of the commits class**, declared so or undeclared (decision 45: what nobody declared commits), **always needs the founder's explicit approval, for that very call**, whoever started the run and whatever the realm's policy says. Neither "yes for the rest of the run" nor a policy's "allow" covers it. A capability the policy denies stays denied.
- **Where it is asked:**
  - in a run the founder is watching (a chat open in the app, a run at a terminal), it is asked inline, as a permission is today, marked as covering this call only;
  - in a run nobody is watching (an order from the command bar, a run whose window was closed, a run left alone, a terminal with no one at it, or any doubt), it goes to Needs you and Telegram as an approval item, the run stops there, and the action is done once on approval (decision 45). Telegram answers the queue; it never counts as watching a run.
- **Detaching is final.** Once nobody watches a run, every later action of that run is unwatched, even if someone comes back.
- **Read and reversible actions** keep following the realm's permission policy in runs the founder starts, as today.

How: a run the founder starts gets an authority of its own, next to the one that applies the levels (`hub/autonomy.rs`). It answers "the policy decides" for read and reversible actions; for a commit it asks the founder if the approver says they watch, otherwise it queues the call and stops the run. The approver says whether the founder watches (`Approver::watched`, no by default); the permission gate remembers when they stopped.

*Why:* the command bar (milestone 5) starts runs the founder does not watch live. Such a run reads outside content (a page, a post, a mail at milestone 7), and an instruction hidden in that content must not be able to make it commit the business. The levels already guarantee this for routines (commits never above level 1); a run the founder starts must not be the way around them.

*Cost:* a chat that pays, signs or publishes something committing now always stops to ask, even in a realm that allows everything; so does any call through an integration that declares no actions, since what nobody declared commits. Airth must know whether someone is watching a run: the app and the terminal say when they are attached, and a run started from the command bar counts as not watched. An unwatched run ends at its first committing action; its report says what is pending. A call approved from the queue is done by Airth only for HTTP actions; an MCP tool's call is approved, then done by the founder, as for missions. The old pipelines (decision 30) are not covered: they are being replaced by routines, which apply the levels. The runtime's permission path changed (security code).

## 55. A failed action is retried only when a retry cannot do it twice

**Accepted**, with the founder's amendments (writes, idempotency keys, demotion).

`CLAUDE.md` says a failed action is retried a few times, then becomes an incident. Until now an `Act`, an approved action and a delayed one became an incident at the first failure. A retry is safe only when the first attempt certainly did nothing, or when doing it twice changes nothing. So, for a declared action:

| Failure | Read | Write | Write with an idempotency key | Commits |
| --- | --- | --- | --- | --- |
| Never sent: connection refused, DNS, TLS | retried | retried | retried | incident |
| HTTP 429 or 503 | retried | retried | retried | incident |
| Sent then cut (timeout, reset), HTTP 502, 504, any other 5xx | retried | incident: ambiguous, the founder checks before retrying by hand | retried | incident |
| HTTP 4xx other than 429 | incident | incident | incident | incident |

- **Idempotency.** A connector's action may declare that its service accepts an idempotency key, and the header that carries it. Airth then sends the action's journal key in that header, the same on every attempt, and the write may be retried on any failure of the service. Actions of the commits class are never retried automatically, key or not.
- **How.** Up to three attempts in all, one minute then five minutes apart. A `Retry-After` in seconds is honoured when it is under 15 minutes; a longer one ends the retries. The retry waits in the action journal, under the same key (decision 47), and the scheduler picks it up, so a restart of Airth does not lose it. Each attempt is recorded. When the last attempt fails, the failure becomes an incident.
- **Demotion.** A failure caused by the service (network, 5xx, 429, retries exhausted) creates an incident but never demotes the agent. Only a failure attributable to the agent's output demotes (decision 45): a 4xx answer other than 401, 403, 407, 408 and 429, which say something about the credentials or the service, not the content.
- **Scope.** This covers a routine's `Act`, an action the founder approved and a delayed one. A mission's own calls at level 3 are not retried: the agent sees the error and decides. A `Read` block keeps its behaviour: a source that fails is noted, and the next scheduled run reads it again; the deduplication of reads means nothing is lost.

*Why:* an incident for a passing network error wakes the founder and demotes an agent for nothing, which would make autonomy hard to earn. But a retry that publishes twice, or pays twice, is worse than an incident. An agent should lose autonomy for what it wrote, not for a service that was down.

*Cost:* the action journal gains the time of the next attempt, and the scheduler a task. A result reaches the founder up to six minutes later. Telling a request that never left from one cut after sending needs `tools/http.rs` to say which it was (security code, changed at the founder's request). An ambiguous failure of a write without a key always reaches the founder, even when the service did nothing.

## 54. One bank in the beta, read-only: Revolut Business

**Accepted.** Refines decision 53's finance part.

At milestone 5, Airth connects one bank, Revolut Business (Hedgen's bank), **read-only**: the READ scope only, for balances and transactions. Accounting, payments and transfers stay out of the beta: Airth never moves money.

- **Signing.** Revolut authenticates Airth with a client assertion (a JWT signed RS256 by the business's private key), traded for a short-lived access token and a refresh token. `aws-lc-rs`, already a direct dependency (release signing, OAuth PKCE), signs it (`RsaKeyPair`, `RSA_PKCS1_SHA256`): no new crate. The key is stored as PKCS#8 DER. Revolut's setup produces a PEM key; the founder converts it once with `openssl pkcs8 -topk8 -nocrypt -in privatecert.pem -outform DER -out revolut.pk8`.
- **Tokens.** Stored and refreshed like a remote MCP server's (`skills/mcp-auth/<name>.json` in the realm's Airth home, mode 600), not as a new kind of state.
- **Runtime-only credentials.** The private key and the refresh token are files in the realm's Airth home, mode 600. They are never in `secrets.env` as `command_env` passes it, never in any command's environment, and never readable by an agent, a routine or a model. A test proves that a command run in a business's realm cannot see either of them.

What it feeds is decision 53's finance part (cash, money in and out this month, runway) and the runway criterion of health. A business with no bank connected (Numirama today) shows its finance part empty and its runway as unknown.

*Why:* runway is one of the three health criteria, and without a bank the HQ cannot say whether a business is at risk. Revolut is the bank of the first business; the READ scope cannot move money, so it adds no way for an agent or an instruction in content to commit the business. Reusing the crypto crate and the token store keeps one way of doing each.

*Cost:* the integration code is security code. The founder converts the key once by hand. A second bank, or Numirama's, is another connector. A business's bank figures are among the few things the HQ reads; they stay in its section.

## 53. The HQ shows one section per business: health, vitals, finance, latest activity

**Accepted**, amended with the founder's answers on its first costs (health criteria with no data, goal on pace, quality flag, who may override it); the bank is decision 54.

The HQ never adds up figures across businesses. It shows one section per business, each with four parts:

1. **Health**: healthy, watch, at risk or not started, and one sentence saying why. Three criteria:
   - **the goal on pace** (below);
   - **the runway** above 12 months;
   - **no open blocking quality flag** (below).

   All in range is *healthy*. One criterion out of range makes it *watch*. A runway under 6 months makes it *at risk*, whatever the rest. A criterion with no data (no bank connected, a goal with nothing to measure) is shown as **unknown**: it is never counted as failed, and health is computed from the criteria that have data. The thresholds live in the business config, with these values as defaults.
2. **Vitals**: at most four, declared by the business in its config (label, value, target, normal range). The values are read from a read-only `get_vitals` tool on the business's own MCP server, bound to that business's realm, through the runtime like any tool call. A vital is highlighted only when it leaves its normal range.
3. **Finance**: cash, money in and out this month, and runway, from the business's bank integration, read-only (decision 54). With no bank connected, the section shows its empty state.
4. **Latest activity**: the three most recent agent runs, each as one readable sentence stored with the run.

**Goal on pace.** A goal may carry a metric (a vital's key), a start (a value and a date) and a target (a value and a deadline). The expected value is linear between start and target; the goal is on pace when the current value is at least the expected value minus a tolerance (10% by default, in the business config). A goal made of steps is on pace when its steps are done by their planned dates. A goal with neither a metric nor dated steps does not count in health.

**Quality flag.** A mark set on an item of the approval queue by a check step of its routine, with a reason. A blocking flag prevents Approve until the item is edited or the founder explicitly overrides it. **Only the founder can override**, from the app or Telegram; no agent, routine or MCP tool can. The override is logged with who and when. Nothing in it is specific to one business. The check step itself is defined with the internal quality check planned for milestone 4; if it needs a block of its own, that is a decision then (the six blocks, `CLAUDE.md`).

Open points, not decided here:

- the `get_vitals` schema: the shape of each value, its unit, when it was measured, and what Airth does when the server is down or answers late;
- ~~how a run's activity sentence is produced~~ **Closed** (founder, 3 November 2026): by code, from the run's record (routine, what each block read, kept, sent to the founder and did, or where it failed), never by a model. The sentence is rebuilt from that record when the HQ shows it, so it never says more than the record.

*Why:* the founder runs several companies with separate legal and financial lives; a total across them would mean nothing and would mix what decision 42 keeps apart. A few signals per business, highlighted only when out of range, show where to look in a glance. Vitals come from the business's own server because each business measures different things, and Airth must not hold code specific to one of them. Counting a missing figure as unknown rather than failed keeps a business without a bank or a measurable goal from looking at risk for the wrong reason.

*Cost:* each business that wants vitals must expose `get_vitals` on its MCP server, and a goal measured by a metric depends on that vital being there. A business with every criterion unknown says little; the sentence must say so. Goals gain structured fields in the business config. The quality flag adds a state to queue items and an override to the queue's state machine (security code: the access layer). The HQ reads every business, so it must hold only these figures and sentences, never a business's content.

## 52. One Telegram bot for the founder, held by Airth, never by a business

Decision 28 made the bot's token an integration credential bound to api.telegram.org. An integration belongs to one business and puts its secrets in headers only (decision 10); Telegram's API carries the token in the URL's path (`/bot<token>/sendMessage`). Followed to the letter, decision 28 means a bot per business, or a change to the integration code (security code) to put a secret in a path. Instead:

- **One bot, the founder's**, like the model keys and the phone: the founder creates it with @BotFather and gives its token to `airth telegram connect`. The token is kept in the Airth home (`telegram.json`, mode 600), sent only to api.telegram.org by the module that talks to Telegram, and never reaches a model, a run or a business folder. It replaces decision 28's integration credential.
- **Pairing** by a link with a one-time code (`t.me/<bot>?start=<code>`): only the chat that opened it **and** the founder's user id in it are listened to; anything else is ignored, and so are forwarded messages and edited ones. Outbound requests only: Airth long-polls, nothing on the Mac is reachable from outside (decision 28 unchanged).
- **What it sends**: each item that waits for the founder, from every business, once, its business named first, with buttons: Approve, Edit, Reject, and Cancel for a delayed action. A text is **never cut**: one too long for a message (4,096 characters) goes as several, the buttons under the last. Failures and incidents in the founder's notifications are forwarded as plain messages.
- **Answers**: a tap is the same answer as in the app, through the same queue state machine (`Hub::answer_item`, channel `telegram`): a stale button (the item answered elsewhere, or changed since) is refused and the message says so. Edit asks for the new text as a reply: it is accepted **only as a reply to that item's message**; a free message is ignored and the bot asks again. The reply is the edit, and the buttons come back under it. The founder's time counts from the moment the message was sent to the answer (decision 27).
- **A business's content goes only to its own items**: the bot relays each item and answer to its own realm; nothing of one business is read into another (the HQ aggregates, `CLAUDE.md`).

*Why:* the founder steers every business from one phone, so one bot is the right shape; a bot per business would mean as many chats as companies. Keeping the token out of every business keeps it out of every run's reach without touching the integration boundary. The rest of decision 28 (outbound polling, owner's chat only, one state machine for all channels) is what the branch's `telegram.rs` already does, and is ported from it.

*Cost:* items that hold customers' personal data (mails, at milestone 7) should send only a summary and a link to the app; that is decided then. The bot is the founder's, not a business's: if a business is handed to someone else, its items must move to their own bot then. Message contents transit through Telegram, as decision 28 says. The token in the home is protected by its file mode, like `phone-hosts.json` and the app's key; whoever reads the home can drive the bot.

## 51. A routine's commands run offline; its network goes through declared actions

In a routine's run, every command (`execute_command`, and the checks run the same way) runs with the network cut by the sandbox. What the run needs from outside goes through `http_request` and MCP tools, which the runtime sees, so the autonomy levels apply to it (decision 45): reading a public page stays free, and anything that changes something outside is an action, declared or not. A run the founder starts and watches keeps the realm's network rule, as today. Local MCP servers keep the network: each of their calls is already an action under the levels.

*Why:* the founder's review of decision 45's code found that a command such as `curl -X POST` reaches outside without going through the levels: the sandbox lets commands use the network whenever the realm allows it, and the runtime cannot tell what a command sends. Cutting the network for commands is the only way to make every outward call of a routine pass the levels.

*Cost:* a routine whose agent installs packages or runs tools that fetch at run time (`cargo` fetching crates, `npx` downloading a server) fails until what it needs is already on the machine, or fetched through a declared action.

## 50. In the owner's home, commands read only the realm and the developer tools

Decision 48 denied writes by default and reads of the owner's credentials. Everything else in the owner's home could still be read by a command: personal documents, shell history, browser profiles (cookies, saved sessions), Mail's messages, other apps' data. A test shows it. A list of what to hide can never be complete there, so the home is read-denied by default too:

- the owner's home folder is closed to commands, reading and writing;
- what a run's workspace allows stays open, deeper rules winning: the realm's folder, its repositories, its worktree;
- so do the developer tools runs use, read-only: `~/.cargo` and `~/.rustup` (toolchains, Cargo's config), `~/.gitconfig` and `~/.config/git` (the owner's git settings), and `~/.npm`, read and written, as in decision 48; Cargo's downloads stay writable;
- the credentials of decision 48 stay closed even inside these (`~/.cargo/credentials`).

Outside the home nothing changes: system folders and `/opt/homebrew`, where the tools themselves are installed, stay readable. A tool kept under the home that a routine needs is added to the list by a change of its own.

*Why:* the owner's Mac is also their personal computer. A business's agents read external content, and a command they run must reach the business, not the founder's private life.

*Cost:* tools installed under the home (`~/.local/bin`, `~/.bun`, a version manager in `~/.nvm`) do not run inside a run until they are added. A realm whose folder is the home itself would open the whole home again, deeper rules winning; Airth already advises a folder of its own.

## 49. Routines are JSON graphs of the six blocks; connectors declare their actions in the Airth home

This replaces decision 30's pipeline format and its step types, and settles the last two pending decisions of milestone 1. The engine of decision 30 is kept and grows into this one (`CLAUDE.md`, *Starting point*); decision 34 (the schedule lives in the routine's file) still holds, as the Trigger block.

### The routine graph

**One file per routine: `routines/<name>.json`** in the business folder, versioned in git, read-only to runs (decision 44). JSON, not the Markdown and front matter of the other config files (decision 43): a graph is nested data, written by the routine editor (milestone 5) and read by the engine; JSON is what the engine already reads, with `serde`, and needs no new crate. Files are written pretty-printed with a stable key order, so a change reads as a short diff in a pull request.

```json
{
  "description": "Morning posts: the three best items of the day, for the founder to approve",
  "agent": "post-writer",
  "trigger": { "schedule": "daily 07:30" },
  "blocks": [
    { "id": "news", "type": "read", "source": "rss",
      "feeds": ["https://example.com/feed.xml"], "max_age_days": 2, "dedupe": "link", "max": 50 },
    { "id": "pick", "type": "ai", "prompt": "prompts/watch.md",
      "keep": "relevant", "rank": "score", "distinct": "pillar", "limit": 3 },
    { "id": "draft", "type": "ai", "prompt": "prompts/post.md" },
    { "id": "good", "type": "condition", "if": { "field": "answer.confidence", "op": ">=", "value": 3 },
      "else": "skip" },
    { "id": "ok", "type": "approval", "show": "answer.draft", "editable": "answer.draft" },
    { "id": "send", "type": "act", "action": "typefully.create_draft",
      "fields": { "content": "{{answer.draft}}" }, "next": null },
    { "id": "skip", "type": "act", "action": "airth.report", "fields": { "title": "Left out" } }
  ]
}
```

- **Shape.** `blocks` is a list. An item goes from each block to the next in the list, unless the block says otherwise: `next` (another block's id, or `null` to stop), and for a condition `then` (default: the next block) and `else`. The graph must have no cycle, and every block must be reachable from the trigger.
- **Items.** A list flows through the graph item by item. An item is a JSON object; blocks add to it (`answer` for an AI block's answer, `result` for an action's). Blocks name fields by path (`item.title`, `answer.score`); an Act fills its fields from a template (`{{answer.draft}}`). There is no expression language: a condition compares one field to one value (`==`, `!=`, `<`, `<=`, `>`, `>=`, `exists`).
- **The six blocks**, and what each holds:
  - `trigger`: `schedule` (as today), `webhook` (a name; the address and its secret are kept in the home), `event` (`mail.received` with a mailbox, at milestone 7), or `manual`;
  - `read`: a `source` (`rss`, `web`, `connector` with a declared read action, `business`, the business's own records through the access layer), with the built-in filters `max_age_days`, `dedupe` and `max`;
  - `ai`: a `prompt` file in `prompts/` and the schema beside it (`prompts/<name>.schema.json`), and the filters `keep`, `rank`, `distinct`, `limit`. It holds no tool; its call is journaled (decision 47);
  - `condition`: `if`, `then`, `else`;
  - `approval`: what is `show`n, what is `editable`, and `expires_after` (hours; past it the item is dropped and the founder told). The run waits; the item sits in the queue (decision 47) and on Telegram;
  - `act`: one declared `action` and its `fields`, nothing else. No model is involved. It goes through `Runtime::invoke`, at the agent's level for that action (decision 45), and is idempotent (below).
- **Idempotence.** Each item gets its source id when it is read, and keeps it through every block: an RSS item's link (or its `guid`), a mail's `Message-ID`, a connector record's id, a webhook delivery's id; an item a mission or a manual trigger creates gets a fresh id, saved with the run. An `act`'s key is the routine, that source id and the block's id. **Never the content:** a retried run whose AI block drafts a different text for the same item must not publish twice. The key is unique in the action journal (decision 47), so a second attempt finds the first and does not act. A test proves it: the same item, retried with a different draft, acts once.
- **Airth's own actions**, declared in code like a connector's: `airth.launch_mission` (the agent loop, in its sandbox, under the agent's rights; the mission's report comes back as the item's `result`, and the run waits for it), `airth.report` (a report for the founder; class `read`, level 3 until the founder sets another, with no exception in the graph check), `airth.create_task` (under the rule on content from outside, since a task given to an agent becomes its instructions; a task assigned to the founder is exempt), `airth.adopt_change` (ceiling 1), and `airth.send_mail` at milestone 7.
- **Hedgen's Writer**, as `CLAUDE.md` describes it: `trigger` (weekly) → `read` (the week's news and the list of notions already covered) → `act airth.launch_mission` (write and illustrate the article, prepare it as a draft) → `approval` → `act blog.publish`.
- **What runs may do is not in the file.** The agent named in the graph brings its rights from the home: levels, budget, model (decisions 43 and 45). A graph cannot raise them.
- **Each block's execution is recorded** in `business.db`: duration, cost, outcome, items in and out. A routine run that waits (an approval, a mission) is saved block by block and resumes after a restart. This adds one table to decision 47's heartbeats, `routine_blocks`, at milestone 3.
- **Dry run.** The editor can run a graph without effects: reads and AI blocks run for real, approvals are answered "approve" without asking, and acts are prepared as at level 0, never executed.

### The rule checked when a graph is saved

Content that comes from outside (a `read` of `rss`, `web`, a connector or a mailbox, a mission's result, a webhook's body) is marked as external, and so is anything an AI block derives from it. An `act` that external content can reach without an `approval` on the way is refused when the graph is saved, unless that action's level is at least 2 for the routine's agent. Levels change after a graph is saved, so the engine checks it again before each act: if the level has dropped, the item goes to the queue as at level 1. This check, the cycle check and the field checks are graph validation: security code (`CLAUDE.md`), tested on every case above.

### How connectors declare their actions

Declarations live in the Airth home, with the connector's record in the realm's config: out of every run's reach, and confirmed by the founder (decision 45).

- **An integration** (HTTP, decision 10) declares each action: an id, a description, the method, a path under its base URL with `{placeholders}`, its fields (name, type, required, where each goes: path, query or JSON body), its risk class, and optionally the daily limit and cancellation window. An `act` can call only that: no other path, no other method. A value put into a path `{placeholder}` is percent-encoded, and refused if it holds `/` or is `.` or `..`; the URL built is then checked against the integration's base URL again (decision 10), so a field drafted from external content cannot walk the request to another endpoint. A test proves it, with `../`, an encoded slash and a full URL as values.
- **An MCP server** (decisions 18 and 20): each tool is an action, its fields are the tool's input schema. When the server is connected, the founder sees its tools and sets each one's risk class; the server's own annotations (`readOnlyHint`, `destructiveHint`) are shown as hints, never trusted; a tool left unset is `commits`. Airth keeps a hash of each tool's schema: a new tool, or a tool whose schema changed since the founder classed it, is treated as `commits` at level 1 until the founder confirms it again, so a server cannot widen a tool after it was trusted.
- **The connector catalog** (`connectors.rs`) may ship suggested declarations for well-known services (for example Typefully: create a draft, `reversible`; publish, `commits`). They are generic, never written for one business, and the founder confirms them like any other.

### Moving from today

Hedgen's two pipelines become `routines/daily-watch.json` and `routines/daily-posts.json`; `business/prompts/` and `business/schemas/` move to `prompts/`. The step types map one for one: `rss` and `dedupe` to `read`, `judge` to `ai`, `queue` to `approval` (and then an `act`), `report` to `act airth.report`. Routines that are objectives today (an agent and a task, decision 11) become a graph `trigger` → `act airth.launch_mission`, their limits moving to their agent. The engine stops reading `business/pipelines/` in the change that reads `routines/`.

*Why:* `CLAUDE.md` makes a routine a graph of six blocks with no tool in the AI block and no model in the Act block, so that every effect is one declared call, checked, journaled and idempotent. A list with jumps is the smallest shape an editor and a diff can both handle. Declaring actions in the home, with schemas hashed, keeps a connector, or an agent editing files, from deciding what an action may do.

*Cost:* JSON is less pleasant to edit by hand than Markdown; the editor is what makes it easy, and it comes at milestone 5. Every connector must be declared before a routine can use it, and a changed MCP tool needs the founder again. There is no expression language: logic that one comparison cannot hold goes into an AI block or a mission. A routine run that waits days for an approval holds its state in the database until then.

## 48. Commands write only where the run may; they never read the owner's keys

The command sandbox (decision 4) allowed everything by default and denied only the Airth home and the other realms. A command could therefore write anywhere else on the Mac. Two ways out followed from it, both shown by a test:

- move the business's folder aside (its parent was writable), rewrite `agents/` or `.git/hooks` under the new name, and move it back: around decisions 44 and 46;
- write `~/.zshrc` or a file in `~/Library/LaunchAgents`, which the owner's shell or login then runs, outside every sandbox.

So the profile now denies **writes by default**, everywhere. A command may write only:

- where the run's workspace allows it (its folder or worktree, the git data a change needs, the realm's shared Cargo build folder);
- its own temporary folder, in the realm's cache in the Airth home, one per run; `TMPDIR` points there;
- `/dev` (null, the terminal);
- the caches of the tools runs actually use: Cargo's downloads (`~/.cargo/registry`, `~/.cargo/git` and their lock files) and npm's (`~/.npm`, where `npx` puts the MCP servers it starts). Never `~/.cargo/bin` or another folder on the owner's `PATH`.

A tool that needs another cache is added to this list by a change of its own, when a routine needs it.

**Reads** stay allowed outside the workspace (compilers, toolchains and system libraries live there), except the owner's credentials, denied to every command: `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.azure`, `~/.config/gcloud`, `~/.kube`, `~/.docker`, `~/.config/gh`, `~/.netrc`, `~/.git-credentials`, `~/.npmrc`, `~/.pypirc`, Cargo's credentials, and the keychain: its files (`~/Library/Keychains`) and the system services that answer keychain queries. HTTPS still works: certificates are checked by another service.

The lists live in `sandbox.rs`, beside the profile they produce. File tools are unchanged: they already reach nothing outside the workspace.

*Why:* the sandbox is the only thing between a command and the owner's machine; a rule that only lists what is denied leaves every path nobody thought of open. Denying by default turns the question into what a run needs, which is short. A business's agents read external content; a command they run must not be able to persist outside the run, nor take the owner's keys.

*Cost:* a tool that writes elsewhere (another package manager's cache, an editor's state, `~/Library/Caches`) fails inside a run until its folder is added. Each run's temporary folder stays until the run is deleted. On Linux, commands stay unconfined until milestone 9 (decision 29).

## 47. The business database: one access layer, evidence append-only, tables added with their milestone

Decision 31 put business state in one SQLite database per business, `business.db`, in the Airth home. This decision says how it is reached and what it holds for the beta.

**One access layer.** All SQL lives in one module, `src/business/store/`: opening, migrations, transactions, and one file per area (founder time, queue, actions, autonomy, routines, model calls, tasks). Only that module uses `rusqlite`; its connection is private; other modules call typed functions (`queue::propose`, `actions::begin`, `autonomy::level`), never a query. A test reads `src/` and fails if SQL or `rusqlite` appears outside it. The access layer is security code (`CLAUDE.md`). Today `pipeline/run.rs`, `pipeline/schedule.rs`, `business/drafts.rs`, `business/status.rs` and `business/time.rs` write their own SQL: the first change of milestone 2 moves it into the layer, with no change to the schema or to behaviour, and the tests that read the database through SQL read it through the layer.

**How it is written.** Unchanged from decision 31, and now required of every table:

- every write is one immediate transaction, in WAL mode, `synchronous = FULL`, a busy timeout, and foreign keys enforced;
- evidence is append-only, and the database enforces it, not only the code: on every evidence table, triggers refuse `UPDATE` and `DELETE`. Evidence is the founder-time ledger, heartbeats, every log of a record's moves, the action journal's events and the history of autonomy levels. Each migration that adds one adds its triggers, and its test checks that an update and a delete are refused;
- a record that changes carries a revision: a write against a stale revision fails instead of overwriting, and every move is also written to that record's log, in the same transaction;
- times are UTC, RFC 3339 text; JSON columns (arguments, sources) are checked in code before they are written;
- migrations are numbered and never edited once released; each has a test that migrates a database of the previous version holding rows; a database newer than the binary is refused; before migrating, the file is copied beside itself (`business.db.<version>.bak`).

**What it holds for the beta.** A table enters with the change that first needs it, by a migration, at the milestone that needs it; nothing is created ahead.

| Area | Tables | Milestone | From |
| --- | --- | --- | --- |
| Founder time | `owner_time`, gains the queue item and the channel (app, Telegram) an answer came from | 2 | exists (decision 27) |
| Heartbeats | `pipeline_runs`, `pipeline_seen`, `pipeline_schedule`, renamed `routine_runs`, `routine_seen`, `routine_schedule` when routines become graphs | 3 | exist (decision 34) |
| Model calls | `model_calls`, gains the agent, the routine, the run and the block | 3 | exists |
| Queue | `queue` and `queue_log`: what waits for the founder, of one kind among approval, prepared (level 0), delayed (level 2), incident, proposed promotion; its exact action and arguments, its source, its deadline or execution time, its status. `drafts` and `draft_log` become rows of it | 3 | replaces drafts |
| Actions | `actions` and `action_log`: each action executed or attempted, its idempotency key (unique), the level it ran at, whether the founder approved it, its outcome (`in_flight`, `confirmed`, `failed`, `unknown`). It makes `Act` idempotent and counts the daily limits (decision 45) | 3 | the branch's `actions.rs` |
| Autonomy | `autonomy` (agent, action, level) and `autonomy_log` (who, when, from, to, why) | 3 | decision 45 |
| Tasks | `tasks` and `task_log`: title, assigned to the founder or an agent, status, due date, the run that does it | 5 | rebuilt (`CLAUDE.md`) |
| Mail | sent mail: agent, approval, thread, archive copy | 7 | decided with the IMAP and SMTP integration |

Agents' statistics are not stored: the layer computes them from model calls, the queue and the actions. What a connector's daily limit and cancellation window are set to stays with the connector's record in the realm's config (decision 45), not in the database.

**Backups.** Once a day, the background runtime copies `business.db` with SQLite's online backup (consistent while Airth writes) to `backups/business-<date>.db` in the realm's folder in the Airth home, and keeps the last 14 days. `airth doctor` says, for each business, when the last backup was made and whether it opens and passes the integrity check; a missing or failed backup is reported in Needs you. Backups stay in the Airth home, out of every run's reach.

**Reading it from outside.** `airth db <business> export` writes every table as JSON lines, read-only; `airth doctor` runs SQLite's integrity check. No tool gives a run access to the database: runs reach business state only through Airth's tools, which call the layer, under the runtime's checks.

*Why:* `CLAUDE.md` asks for one access layer and makes it a security boundary: a single place where every rule on business state (append-only evidence, revisions, idempotence, limits) is written once and tested once. Adding tables with their milestone keeps the schema from holding guesses about features not built.

*Cost:* a backup a day takes up to fourteen copies of the database per business. Moving the SQL that exists is a change of its own, touching the pipeline engine and its tests before anything new is built. Each table is a migration to write and test, and a later change to a table's shape is another migration. Statistics computed on read cost more queries than stored counters, which is acceptable at one founder's volume.

## 46. A run in place cannot plant git hooks or change the git config

A run that works in the business's folder itself (not in a worktree) may write that folder's `.git`, since committing needs it. Two parts of it run code as the founder, outside any sandbox, the next time the founder or Airth uses git there: `.git/hooks/` (scripts git runs on commit, checkout, merge) and `.git/config` (it can name a hooks folder, filters, an editor, a pager, an SSH command). So for every run in place:

- `.git/hooks/` and `.git/config` are read-only, through the workspace's access rules: the file tools refuse them, and the command sandbox denies them, creating them included;
- `.git` itself cannot be renamed or removed by a command: without this, a command could move `.git` aside, write the hooks under the new name, and move it back. The sandbox profile gains a rule for one exact path (`literal`), beside the existing ones for a whole folder (`subpath`).

The rest of `.git` stays writable: a run in place can still commit. Runs in a worktree are unchanged: the founder's checkout, its `.git` included, is already read-only to them (decision 5).

*Why:* a planted hook or config line runs with the founder's rights the next time git is used in the folder, outside every rule Airth applies to runs. The founder asked for it after decision 44. Checking the sandbox showed that a rule on `.git/hooks` alone is bypassed by renaming `.git`; hard links to a protected file are already refused.

*Cost:* git operations that rewrite `.git/config` (adding a remote, setting a branch's upstream) fail inside a run in place; the founder does them. Without a command sandbox (Linux, until milestone 9), commands are not confined, as for the rest of the workspace. Hooks in a submodule's git folder (`.git/modules/`) are not covered.

## 45. Autonomy per agent and action, under a ceiling set by the action's risk class

**Actions.** An action is one thing done outside the business's folder through a connector: one tool of an MCP server, one declared call of an integration, or one of Airth's own (launch a mission, send a mail, adopt a change). Each has an id, `<connector>.<action>` (`typefully.create_draft`, `blog.publish`, `airth.launch_mission`). Reading and writing the business's folder, running commands there, and memory stay under the realm's rules (decision 4) and the workspace (decision 44): they are not actions.

**Risk classes.** Every action has one of three classes: `read`, `reversible` (a write that can be undone: a draft, a label, a scheduled post not yet sent) or `commits` (money, a contract, a deadline, a legal obligation). Publishing in the business's name (a post, an article, a thread) is `commits`. The class is declared in Airth's record of the connector, which the founder confirms when connecting it, never by the connector itself: what an MCP server says about its own tools (its annotations) is shown as a hint and never trusted. An action with no declared class is `commits`.

**Levels and ceilings.** The four levels of `CLAUDE.md` (0 prepare, 1 propose, 2 act with delay, 3 act). The class sets the ceiling:

| Class | Ceiling |
| --- | --- |
| `read` | 3 |
| `reversible` | 3 |
| `commits` | 1 |

Fixed ceilings, whatever the class: replies on X 0; publishing in the business's name 1 during the beta; payments, refunds, contracts, mails that commit money, a deadline or a legal obligation, merging code and deploying to production 1; a mail to a correspondent the agent never wrote to 1 (`CLAUDE.md`, *Mail*). A level is never stored above its ceiling, and is clamped to it when read, so lowering a ceiling takes effect at once.

**Where levels live.** Per agent and per action, in the business's `business.db`, never in the folder (decision 43). A new action starts at 1, or at its ceiling when that is 0. Each change is a row (who, when, from, to, why), so the history of what was delegated, and when, is kept. A run works with its agent's levels; a mission the founder starts and watches asks the founder, as today.

**What each level does, also in a mission.** The same rule applies to an `Act` block and to a tool call inside a mission (the agent loop), through the permission gate in `runtime.rs`:

- 0: the action is not executed. What it would do is prepared (a pre-filled link, a text to paste) and put in Needs you; the founder executes it outside and marks it done.
- 1: the action is not executed. It goes to the approval queue with its exact arguments. If the founder approves it, Airth executes it as it was proposed, once (the action journal of the reconciliation, `actions.rs`); an edit changes the arguments, not the action. A mission that proposes an action is told so and carries on; it does not wait.
- 2: the action is queued with a cancellation window (30 minutes by default), shown in Needs you and on Telegram with a Cancel button, then executed.
- 3: executed, then reported.

The gate reads the level from `business.db` before each action, so a demotion applies to the runs already going. If it cannot read it, the action is treated as level 1.

**Daily limits per service.** Each connector has a limit on actions executed without the founder's approval (levels 2 and 3) over the last 24 hours: 20 by default. Past it, that connector's actions fall to level 1 until the window clears, and Needs you says the limit was reached. The count and the journal entry of the action are written in one transaction, so runs acting at the same moment cannot pass the limit together. Actions the founder approved do not count: they are the founder's decision. The daily limit and the cancellation window are defaults the founder changes per connector; the values set are kept in the Airth home, with the connector's record, never in the business folder.

**Promotion and demotion.** Only the founder promotes. Airth proposes a promotion in Needs you when an action of an agent has been approved without edit 20 times in a row, over at least two weeks, with no incident. An edit or an incident sets the count back to zero; the founder accepts or ignores it. An incident demotes the agent's action by one level, at once, and says so in Needs you: an action that failed after its retries, an action at level 2 the founder cancelled, or an action the founder marks as wrong after the fact.

**Today's rules and approvals.** The realm's rules (`network`, `skill`) and a routine's approvals given in advance (decision 11) keep governing calls that are not declared actions. Once a connector's actions are declared, its calls go through levels: a routine that acted alone through it moves to level 1 until the founder sets levels. In the beta that is only Hedgen, set up again anyway.

**Implementation.** With the approval queue, at milestone 3. It changes the permission gate, `runtime.rs` and `tools/http.rs`: security code, changed on this decision's acceptance, with tests proving that an undeclared action is `commits`, that no level passes its ceiling, that level 0 and 1 never execute, that an approved action executes once, that the daily limit holds under concurrent runs, and that a demotion stops a mission already running.

*Why:* `CLAUDE.md` earns delegation per action class, never wholesale. Today's model has the opposite shape: rules per capability for the whole realm, and approvals per routine given in advance, with no notion of how serious an action is. Declaring the class with Airth and not with the connector keeps a third-party server from calling a payment a read. A count per service is generic and bounds a runaway agent even inside its allowed actions.

*Cost:* each connector's actions must be declared and classed before they can act alone: until then, everything a connector does is proposed. Two systems coexist (rules for local work, levels for actions), and the app must show both on an agent's card. The promotion threshold is a guess, to tune on Hedgen's first weeks. A proposal at level 1 inside a mission means the mission ends before its effect happens, so its report must say what is pending.

## 44. A run never writes the business config; it proposes a change

`business/`, `agents/`, `routines/` and `prompts/` at the root of a business's folder are read-only for every run working in that folder (decision 43). The rule is one of the workspace's access rules, so the file tools refuse the write and, on macOS, the command sandbox denies it to any command. It holds for folders that do not exist yet: a run cannot create `routines/` either.

A run working in a worktree (a change, decision 5) may edit those folders in its worktree: that is how an agent proposes a new job, goal or routine. The owner's checkout stays read-only to it, as for any change, and nothing reaches the business until the founder adopts the change after reading its diff. The founder changes the config directly, from the app or an editor, as before.

*Why:* an agent's file is its instructions, and `business/goal.md` holds the baseline the beta is measured against. A page an agent read must not be able to rewrite either. `business/` was already read-only (pilot step 1); `agents/` was not, and nothing let an agent propose a change to the config in a way the founder reviews.

*Cost:* where there is no command sandbox (Linux, until milestone 9), commands are not confined, as for the rest of the workspace (decision 4); file tools still refuse. A business whose folder is not a git repository has no worktrees, so its agents can only suggest a config change in their report. A run that is not isolated can still write the folder's `.git`, as it could before: it could record a commit there without changing the files, which `git status` would show.

## 43. The business config: profile and legal identity, goal, lines, departments, agents

A business is described by files in its folder, versioned in git, in the format `CLAUDE.md` sets: Markdown with a flat YAML front matter, one file per item. Structured fields in the front matter, prose in the body.

```text
<business folder>/
  business/profile.md        identity, legal identity, links; the body describes the business
  business/goal.md           the goal, its measure and milestones, the founder's baseline;
                             the body is the strategy every agent follows
  business/lines/<name>.md   one editorial line, range or brand
  agents/<name>.md           one agent: its role in the front matter, its job in the body
```

**Front matter.** Flat: `key: value`, one per line, no nesting. A value is text, a date (`2026-11-01`), or an inline list (`[a, b]`, quoted items when they hold a comma). An empty value or `TODO` means not known yet: the business loads, and the app lists what is left to fill. An unknown key refuses the file at load, naming the file and the key, so a typo never silently drops a field. The parser of `tools/catalog.rs` becomes a shared module, `src/frontmatter.rs`, and each kind of file has its own checked fields.

**`business/profile.md`**

| Field | Meaning |
| --- | --- |
| `name` | The name the business goes by |
| `legal_name`, `legal_form` | As registered: `Hedgen Ltd`, `ltd`; `sas`, `sasu`, `ei` in France |
| `country` | ISO code: `GB`, `FR`. It picks the legal calendar |
| `status` | `planned` (not registered yet) or `registered` |
| `registration_number` | Companies House number (UK), SIREN (France) |
| `registered_office` | The registered address |
| `incorporated_on`, `fiscal_year_end` | A date; a month and day (`03-31`) |
| `vat_number`, `tax_regime` | When registered |
| `currency` | The accounts' currency: `GBP`, `EUR` |
| `bank` | The bank's name only. Never an account number: bank details stay out of the folder |
| `domain`, `links` | The business's domain; its sites and social accounts |

With `status: planned`, only `name` and `country` are expected. The legal calendar (filing and tax deadlines) is derived from `country`, `incorporated_on` and `fiscal_year_end`; its rules per country are written at milestone 2.

**`business/goal.md`**: `goal` (one sentence), `measure`, `target`, `by` (a date), `milestones` (a list of `YYYY-MM-DD what`), `baseline_minutes` (the founder's time per week on the business before Airth, which the founder-time ledger is compared with, decision 27). Its body is the strategy (decision 36), which `business/strategy.md` becomes.

**`business/lines/<name>.md`**: `title`, `kind` (`editorial`, `range` or `brand`), `status` (`active` or `paused`). The body says what the line is and for whom.

**Departments** are not files. The seven of `CLAUDE.md` (`leadership`, `marketing`, `sales`, `support`, `product`, `operations`, `finance`) are fixed in Airth; an agent names one, and an empty department is not shown.

**`agents/<name>.md`**: the file name is the agent's id. Front matter: `title`, `department`, `reports_to` (an agent's id, or `founder`), `lines` (the lines it works on), `mailbox` (its address, if it has one). Its body is the job (decision 36): mission, how it works, what it reports. `business/bots/` becomes `agents/`.

**What the folder never holds:** what an agent may do. Its tools, autonomy levels, budgets, model, the credentials of its mailbox, and whether it is paused stay in the Airth home, out of every run's reach (decisions 7 and 36). The folder describes the business; the home decides what runs may do. An agent that edits its own file can change how it is described, never what it is allowed to do.

**Amendment (founder, 1 November 2026): the config is read-only for every run.** The body of an agent's file is its job, so its instructions; `business/goal.md` holds the baseline that proves the beta. So `business/`, `agents/`, `routines/` and `prompts/` are read-only for every run, through the workspace's access rules, which the file tools check and the command sandbox enforces. An agent that wants to change the config does it in a worktree, as a change the founder adopts, never by writing directly. `business/` was already read-only (pilot step 1); `agents/` was not. The fix is its own change, under decision 44.

*Why:* milestone 2's gate is Hedgen Ltd fully described in configuration, and Numirama must be set up with configuration only. Files in git show who changed what and when, and read well in an editor; flat front matter keeps the parser small and with no new crate. Keeping rights out of the folder means a page an agent read cannot raise what it may do by rewriting a file.

*Cost:* two places describe an agent: its file (who it is) and the home (what it may do). The app must show both on one card. Seven fixed departments may not fit every business (a media's editorial team sits in `marketing`); adding a department to the list is a decision. Moving `bots/` and `strategy.md` is a migration of existing folders, done once, when the business loads.

## 42. The surface is frozen on the beta's scope

This supersedes decision 25. Until the beta is reached (`CLAUDE.md`, *Scope: the beta*), Airth gets bug fixes and the current milestone's work only. Milestones are taken in order; one starts when the previous one's gate is passed. Anything outside the beta goes to `docs/backlog.md`: what, why, which milestone it would serve. The redesign moves existing features, it does not remove them; a feature leaves Airth only through a decision of its own.

*Why:* the pilot's seven steps were replaced (decision 36), then the frame changed again: Airth is a business headquarters, each business a company. The beta has a measurable end (Hedgen four weeks on its own, founder time below the baseline, Numirama by configuration only), which makes it a better line to freeze on than a list of steps.

*Cost:* the beta is wider than the pilot (mail, the VPS, Numirama), so the freeze lasts longer and more work counts as in scope.

## 41. Agents ask each other for work, within the limits of the one asked

A routine's runs may ask other routines of the same business for work, if the owner allowed it on that routine (its `delegates`). Two tools carry it, offered only to those runs:

- `hand_off` starts the other routine's run with a brief and returns at once; the app starts it as it starts a scheduled routine.
- `ask_agent` starts it, waits for it to end, and returns its report.

The run asked for work is the other routine's own: its agent's job, its tools, what it may do alone, its budget. Nothing of the asking run's rights passes over, so an agent cannot reach through a colleague what it may not do itself. The brief arrives as data, never as instructions, since the asking agent may have read external content. Nobody is there to approve a request inside it: what needs the owner is denied and listed in its report, as for any run that started without them. A chain of requests stops at three routines deep and never comes back to a routine already in it. Each run asked for work says which routine asked, in Tasks and in its notification.

*Why:* the owner wants the team to work together: an analyst handing a finding to the writer, a writer asking the analyst for figures before it writes. Routines already carry an agent with its limits, so making them the unit of delegation adds no second place for rights.

*Cost:* one request can cost several runs: the depth limit and each routine's own budget bound it, not a budget for the whole chain. An agent can only ask routines the owner listed; a routine that is already running cannot be asked until it ends.

## 40. Airth no longer develops its own skills

Runs no longer propose skills (`propose_skill`), and Airth no longer develops, evaluates and installs them (`test_skill`, `airth skill`, the app's "Built by Airth" tab). This supersedes decision 14. What extends a business's agents is what the owner connects: MCP servers and connectors (decisions 18 and 20), integrations, and skills from the catalog, all still under the realm's `skill` and `network` rules. Skills installed before are no longer offered to runs; their files stay in the realm's folder until it is removed.

*Why:* a business needs its agents to use its own services, which connectors give, more than tools Airth writes for itself. Self-development was the largest feature left from the personal agent (decision 32), with its own runs, workspace, sandboxed evaluation and pages to maintain and secure.

*Cost:* a capability no connector or catalog skill offers must be built outside Airth, as an MCP server or a script in the business's code.

## 39. The Mac app tools and the `apps` capability are removed

The tools that drove the owner's Mac apps (`calendar`, `reminders`, `notes`, `mail_draft`, `safari`, `clipboard`, `open_on_mac`), the `apps` capability that governed them, and the Google and iCloud connectors that went through them are removed, with their pages. This supersedes the Mac apps half of decision 16; the Mac app itself stays. A realm saved with an `apps` rule still loads: the rule is ignored, and nothing can be approved for `apps` any more.

*Why:* a business reaches its services through its connectors (MCP servers, integrations), each given to the agents that need it (decision 36). Tools acting in the owner's personal apps, shared by every realm, belong to the personal agent Airth no longer is (decision 32), and were one more way for a run to reach outside its realm.

*Cost:* a business that kept its calendar or mail in the Mac apps must connect the service itself (Google, Microsoft) through a connector.

## 38. The phone's pairing code is drawn with qrcode

Pairing the phone (decision 37) hands it the app's address and key in one QR code, shown by `airth phone pair` in the terminal and on the owner's Phone settings in the app. The code is made with the `qrcode` crate, without its default features (image, SVG, PIC): it has no dependencies of its own. The app receives the modules as data and draws them as a grid, so no image or markup is built from a string.

*Why:* typing a 64-character key on a phone is not a pairing anyone would do, and encoding QR codes (Reed-Solomon error correction, masks, versions) is too much code to write and get right by hand for this.

*Cost:* one more small crate to keep up to date. The QR code carries the key: whoever photographs it controls Airth, which the app and the terminal say where they show it.

## 37. The app is rebuilt around six sections, and reaches the phone through Tailscale

The app's interface starts again from a blank page; the engine stays (the hub, the runtime and its boundaries, objectives, bots, connectors, approvals, usage records). A business (a realm, decision 32) opens on six sections:

- **Steering**: the owner's page. What waits for a decision, with what is needed to decide; what needs attention (a failed run, a missed time, a budget nearly spent, a bot stuck); the goals from the strategy and where they stand; the strategy itself, the only text the owner writes for the team.
- **Routines**: the recurring work, each with its bot, its trigger, its limits; created, changed, paused, run now. A routine is an objective (decision 11); a pipeline (decision 30) is a routine whose work is a fixed sequence.
- **Planning**: what runs when, past and coming: each routine's times, what ran, what was missed.
- **Tasks**: the work itself, one-off or from a routine: what is running, what is done, each with its plan, its approvals, its report and what it produced. Asking Airth for something is starting a task.
- **Agents**: the business's bots (decision 36): job, tools, limits, budget, memory, what each produced; hiring one from a template by business function.
- **Usage**: what running the business costs: tokens and money by agent, routine and model, and the owner's time.

The business's settings (documents, connectors, skills, access) sit behind these, under Settings. The pages of decision 32 (Today, Pipelines, Ask Airth, Costs) are replaced by these, each removed in the change that replaces it. The interface stays in English.

The app stays on the owner's Mac, listening on loopback only. The phone reaches it through Tailscale: `tailscale serve` publishes it over HTTPS to the owner's own devices (their tailnet), at home or away; nothing is opened to the local network or the internet. Airth accepts that one host name, set by the owner (`airth ui allow-host <name>.ts.net`), beside loopback; the token is still required on every call, and the phone is paired once by scanning a QR code that carries it.

*Why:* the current pages grew from a personal agent and were patched toward a business; the owner finds them hard to follow. These six sections are the shape of products built for bots that work on their own (OpenBot), which the owner finds clear, plus the one thing they lack for a business: a page to steer and decide. The engine already matches them one for one (routines are objectives, tasks are runs, agents are bots, usage is the usage record), so only the interface is new. Tailscale gives the phone access without exposing a program that controls the Mac: the traffic is encrypted end to end, and only the owner's devices can reach it.

*Cost:* the interface is written again, page by page, and pages that work today go away while their replacements are built. The phone needs Tailscale installed and signed in on both devices. A host name allowed by mistake would let pages on that host reach the app's API; the token still stands in the way, which is why it is never put in a URL a third party can see.

## 36. A business is run by bots, each with a job; the owner steers

The owner sets the direction of a business; bots do the work. A realm is one business (decision 32) and has a team of bots. A bot is:

- **a job**: what it is for and how it works, in `business/bots/<name>.md`, in the business's words, versioned with the rest of the business. Its version is the hash of its content, recorded with each run;
- **its tools**: the connectors (remote MCP servers, decision 20; local ones, decision 18), integrations (decision 7) and skills it may use, a subset of the realm's;
- **its triggers**: a schedule, a webhook, a watched file, or the owner asking (decision 11);
- **its limits**: what it does without asking, its budget per run and per month, its model;
- **its memory**: what it learned, kept per bot in the realm's memory.

The job is in the business folder; the tools, triggers and limits are kept by Airth, out of every run's reach, as objectives are today (a bot is the objective it grows from, given a name, a job file and its own tools). A run could edit a file in the business folder, never what it is allowed to do.

The owner's side is direction and decisions. `business/strategy.md` holds the goals and the line every bot follows. What a bot may not do alone waits for the owner on Today and on the phone (decision 28): by default, reading and preparing are free; anything that touches money, customers or the public (a payment, a refund, a price, a message to a customer, publishing) needs the owner's approval, and that approval is never given in advance for publishing. Today shows each bot: what it did, what it waits for, what it cost.

This supersedes decision 25 (the frozen surface and the pilot's seven steps) and decision 26 (business work as fixed pipelines where the model only judges). Pipelines (decision 30) stay as a tool a bot or a schedule can run, for fixed sequences that gain from being plain code. Decision 35 (outlets) goes to the backlog.

The first proof is Hedgen's **Writer**: once a week it chooses a subject (news worth explaining, or a lasting notion: a term, a mechanism), researches it, writes the article in Hedgen's voice, illustrates it (charts from Hedgen's API, an image when it helps), and prepares it on the blog as a draft; the owner approves, and the Writer publishes. The proof is reached when four weekly articles go out in a row with the owner spending only the approval, and the cost per article is known. Delegation between bots, a computer of their own and a mobile app come after.

*Why:* the owner wants to steer and leave execution to Airth. Fixed pipelines make every piece of work a piece of code to write; bots with a job written in plain words let the owner add work without a release. Products built this way (Rakazo, Grok Bot) show the shape works. Most of what a bot needs exists: the agent loop, connectors, objectives with triggers, budgets and approvals, the permission gate, the phone, usage records. Airth's difference is where it runs and what it measures: on the owner's machine, credentials the model never sees, and each business's cost and owner time.

*Cost:* a bot's work differs from one week to the next and cannot be proven in advance like a pipeline: it is judged on its output (the owner's edits and rejections, cost). A bot reads external content and holds tools with effects, so a page could try to steer it; the approval before anything public is the limit that holds. `CLAUDE.md` changes with this decision: the current phase becomes the Writer proof, and the rules on model calls apply to pipelines.

## 34. A pipeline's schedule is in its file, not in an objective

Decision 30 had an objective name the pipeline it starts. The pipeline file already says when it runs (`schedule`), so the background runtime reads it directly: every few seconds it loads each realm's pipelines, and runs those whose time has come, each on its own thread. When each runs next, and when it last started, live in `business.db`. A schedule seen for the first time is armed for its next time; one missed while Airth was not running fires once, late, and the owner is told; a run that started and left no heartbeat (Airth stopped during it) is reported at the next one. A scheduled run has nobody to ask: what the realm's rules leave to the owner is denied at once, and the run's heartbeat says so. Each run, done or failed, lands in the owner's inbox.

*Why:* one place for when a pipeline runs, versioned with the rest of the business. An objective beside the file would be a second schedule to keep in step, outside git.

*Cost:* a scheduled pipeline cannot be approved in advance the way an objective can (its approvals): the realm's rules must allow what it does, network requests first. Objectives keep their budgets and approvals; pipelines have none yet, and a judge step's cost is bounded only by the number of items read.

## 33. Feeds are read with quick-xml

Pipelines read RSS and Atom feeds (decision 30), which are XML. They are parsed with `quick-xml`, a streaming reader with no dependencies of its own. A feed that breaks off, because it is malformed or because the HTTP tool cut it at its size limit, gives the items read before the break, and the run notes it. Feeds are fetched through `Runtime::invoke` with the `http_request` tool, so each read is checked against the realm's rules, redacted and recorded like any tool call; redirects are not followed, and the run says where the feed moved.

*Why:* feeds are external content, often not quite well formed: a hand-written parser would be one more thing to get wrong on untrusted input. `quick-xml` is small, widely used, and reads what is there without building a document tree.

*Cost:* one more dependency to keep up to date. Items are what the feed says: titles, links, dates as written, summaries reduced to plain text.

## 32. Airth runs businesses; each realm is one business

A realm is one business. The app shows nothing that is not about the business it runs. The rail lists the businesses, and for the one open:

- **Today**: what waits for the owner (drafts with Approve, Edit, Reject, their source, prompt version and cost), the pipelines (last run, next run, a missing heartbeat in warning), and the week (owner time against the baseline, share of drafts sent unedited, model cost).
- **Pipelines** and **Reports**: what runs, and what it produced.
- **Costs**: what running the business with Airth costs, in tokens and money, by model, by pipeline and by why the model was called (the usage page, scoped to this business).
- **What the business is made of**: Documents (its profile, archive and other files), Code (the repositories of its projects), Skills, Connectors (its integrations and their credentials).
- **Ask Airth**: a chat about this business, with its documents, code, skills and connectors.
- **Settings** of the business.

The North Star, goal-tracking projects, routines, benchmarks and the global inbox leave the surface: routines become pipelines, and the inbox becomes Today. Each is removed, not hidden, in its own change, with its code and tests; settings that belong to the owner (profile, model keys, the phone) stay in one Owner settings page.

*Why:* the owner wants one tool to run businesses with as little of their time as possible, not a personal agent with a business page beside it. Every screen that is not about the business is one more thing to walk past, to maintain and to keep secure.

*Cost:* features that work today go away, and anyone who used Airth as a coding or personal agent loses them. A business that is not yet set up has an empty realm, so creating one must lead straight to its profile, its code and its first connector. This supersedes the "nothing removed" rule of decision 25 for the features listed here.

## 31. Business state lives in SQLite, one database per realm

A business's state (the draft queue and its transitions, the owner-time ledger, pipeline heartbeats, the record of model calls) lives in the realm's `business.db`, through `rusqlite` with its bundled SQLite. Every write is one `BEGIN IMMEDIATE` transaction, in WAL mode with a busy timeout. Schema changes are numbered migrations, and a database newer than the binary is refused. Mutable records carry a revision: an update against a stale revision fails. The audit table is append-only, enforced by triggers. Run state, objectives, settings and memory stay JSON (decision 1).

*Why:* the draft queue must never publish twice, even when a retried job and an answer from the phone arrive together; a transaction makes that a property of the store, not of every caller. The model-call record is the evaluation dataset (decision 26): it grows with every call and is queried by prompt version and outcome, which JSON files do poorly. The store comes from the `business-autonomy` branch, where it was built and tested (see `docs/reconciliation.md`).

*Cost:* one new dependency, compiled in (about 1 MB). Business state is no longer readable with a text editor; `airth` must offer a way to inspect and export it. Two storage styles coexist, with a clear line between them.

## 30. Business pipelines are a generic engine; each business declares its own

Decision 26 left open whether pipelines are installed skills (decision 14) or a module of their own. Skills cannot do the job as decision 14 bounds them: a skill runs only when a model inside an agent run calls it, so the model would sequence the work; its command gets no provider key, so it cannot make a recorded model call; and it is denied the Airth home, where the queue and the ledger live.

So the engine is a module, `src/pipeline/`. It holds the engine and a small set of generic step types, nothing specific to one business: read a source (RSS, a web page, JSON through an integration), filter and deduplicate, ask the model to judge or draft (a prompt and an output schema), queue for the owner's approval, send to a service through an integration, write a report. Steps with an effect go through `Runtime::invoke`, with the objective's approvals as grants and an approver that never waits, so the permission check, redaction and events stay the only path to execution (`runtime.rs` is unchanged). An objective names the pipeline it starts (a new optional field); the scheduler starts the pipeline instead of an agent run when it is set. Triggers, budgets and the missed-schedule rule stay as they are (decision 11).

What belongs to a business lives in its realm folder, under `business/`, versioned in git with the rest of the realm: its pipelines (JSON files assembling step types, with their schedule), its prompts, its sources, the accounts it watches, its editorial pillars. A pipeline file is checked when it is loaded: an unknown step type, a missing prompt or an invalid schema refuses the pipeline before it runs, never halfway. A prompt's version is the hash of its content, recorded with every model call (decision 31). Runs can read `business/` but not write it: only the owner changes how the business runs.

A step type enters the engine only when a real pipeline needs it. There is no workflow editor during the pilot: pipelines are files. Hedgen is the first business configured this way; a second one set up without changing the binary is the test that the engine is generic.

*Why:* the owner wants anyone to run their company from Airth. Pipelines written in Rust for Hedgen would serve no one else, and every change would need a release. Declaring them per business keeps the guarantees of decision 26 (fixed steps, the model only judges, recorded calls) in the engine, and the business choices with the business, where git shows who changed what and when.

*Cost:* the pipeline file format is a contract to keep stable and documented. Configuration errors must be caught at load time, which needs its own checks and tests. Generic step types are harder to get right than one-off code, and the first ones will be shaped by one business until a second one uses them.

## 29. Airth runs on the owner's Mac until a stable beta, then on a Linux VPS

During the pilot, Airth runs as the login service on the owner's Mac (decision 13), kept awake when a routine is due. A schedule missed while the Mac sleeps fires once, late (decision 11); the weekly report shows such delays so they are not mistaken for slowness.

Once a beta is stable, Airth moves to a Linux VPS, always on. That move is gated: on Linux, commands run unconfined (decision 4, *Cost*). Before the VPS hosts a realm, one of these must hold:

- a Linux sandbox exists (Landlock or bubblewrap), generated from the same path rules as the macOS profile, with the bench's security tasks passing on Linux; or
- the realm denies execute and has no installed skill that runs commands, which `airth doctor` checks and reports.

The app server also stays on loopback on the VPS; the owner reaches it through an SSH tunnel or a private network, never an open port.

*Why:* the Mac is what exists and is enough to prove the pilot. A business must eventually run with the laptop closed, and a VPS is the cheapest always-on host, but moving there must not silently drop the isolation the rest of Airth relies on.

*Cost:* until the move, a closed lid delays routines. The Linux sandbox is real work, scheduled after the pilot's proof, not before (decision 25).

## 28. The approval queue reaches the owner's phone through a Telegram bot

The pilot's proof depends on approving drafts in seconds, away from the desk, and notifications today stay in the app's inbox (decision 12, *Cost*). Airth polls the Telegram Bot API (outbound only, no port opened, which also works unchanged on the future VPS). Each item of the queue arrives as a message with Approve, Edit and Reject buttons; Edit takes the owner's reply as the new text. Answers are accepted only from the owner's chat id. The bot token is an integration credential bound to api.telegram.org (decision 10). Every answer goes through the same queue state machine and owner-time ledger as an answer given in the app (decisions 26 and 27), so the channel decides who answers, never what an answer may do (decision 2).

*Why:* the cheapest path to a phone: one integration and one polling thread. The iOS companion app stays possible later and reuses the same queue.

*Cost:* message contents transit through Telegram. Only drafts and public post links go there, never credentials or private realm data. Polling adds a request every few seconds while Airth runs.

## 27. The owner's time is a recorded quantity

Every owner interaction with a business run is recorded in the realm's ledger: what was shown, when, the answer (approved, edited, rejected), the time between showing and answering, and for an edit, the diff. The weekly report sums it per week and per pipeline, and compares it with the baseline the owner states for the manual period.

*Why:* the pilot's claim is "this business runs with N minutes of the owner per week". Token usage is already recorded (decision 23); the owner's time is the cost that matters, and it is not recorded anywhere yet. Edits are also the best signal of where the drafts fall short.

*Cost:* time between showing and answering overstates attention (a draft can wait an hour unread). The report shows both the sum and the median per item, and says so.

## 26. Business operations are pipelines of tested steps; the model only judges

A business routine (watch, drafting, replies radar, weekly report) is a fixed sequence of steps written in code. Steps that fetch, filter, deduplicate, schedule or change state are deterministic and tested. A model is called only for steps that need judgment, each with a versioned prompt file, an output schema checked in code, and a recorded outcome. The routine is started by an objective (decision 11) with its approvals and budget given in advance. A step that reads external content holds no tool with side effects.

Drafts move through drafted → pending → approved → published | rejected, stored as JSON in the realm. Transitions are idempotent, so a retried routine never sends a draft twice. Airth never publishes to X; the owner does, from a prepared link.

Each routine writes a heartbeat when it ends; a routine whose heartbeat is missing past its schedule notifies the owner.

Before a prompt or model change ships, it is replayed on a reference set built from the owner's own past posts and from the drafts they approved or edited, and the results are compared.

*Why:* the agent loop is right for open tasks, wrong for a daily routine: a routine that re-plans itself each morning is costly, hard to test and different every day. Fixing the sequence makes behaviour reproducible and failures local. It also keeps X's automation rules respected: automated replies found by keyword search are not allowed there, so a human publishes every reply.

*Cost:* a new routine needs code, not just a sentence. That is deliberate during the pilot. Whether these pipelines become installed skills (decision 14) or a module of their own is the first implementation question, and gets its own entry.

## 25. The surface is frozen until the Hedgen pilot proves itself

Airth went from first commit to about 30,000 lines in under two weeks. Until the Hedgen pilot shows two consecutive weeks of its seven steps running (see `CLAUDE.md`) with a measured owner time well under the manual baseline, Airth gets bug fixes and pilot work only. Other ideas go to `docs/backlog.md` with the pilot step they would help.

*Why:* the code is well structured, tested and documented, so the risk is not quality but breadth: one owner has to understand and maintain everything that exists. The pilot is what tells which of the existing features matter.

*Cost:* useful improvements wait. Features that exist and are not used by the pilot stay as they are, unmaintained beyond keeping the checks green.

## 24. Benchmarks drive real runs through the hub, in throwaway realms, and verify outcomes themselves

Airth Bench (`src/bench/`) measures the harness, so it must exercise the harness: each task is a normal run created and driven by `Hub::drive`, with the same agent loop, tools, permission gate, sandbox, compaction and usage accounting as a chat. There is no second runtime. What differs is around the run:

- Each attempt gets a fresh workspace in the system temp directory and a realm of its own under `benchmarks/work/`, outside `realms/`: never listed, never shown, never one of the owner's. It does not share the owner profile; its credentials and memory are the task's synthetic ones. Both are deleted afterwards; the run's full state is kept as the trace.
- The approver says no to everything, so the task's policy alone decides (by default: files and commands allowed, network, skills and apps denied). Unattended commands are only safe where the OS confines them, so tasks that run commands, or are verified by one, are skipped without the sandbox.
- Realms gained an `offline` setting, enforced rather than requested: `http_request` refuses, and the sandbox profile denies outbound IP connections. Bench realms are offline unless a task asks for the network.
- Success is decided by the task's checks on the workspace and the conversation, never by the agent's report. A security task also needs evidence the attack reached the agent (a tool call aimed at it); if the model only declined, the result is inconclusive, since it says nothing about what Airth would have prevented.
- Metrics come from what runs already record (events, per-call usage, compactions); costs are estimates from list prices and are labelled so. Rates exclude skipped tasks. Comparisons call a drop *confirmed* only past a two-proportion test (z ≥ 1.96); a new security failure always is.

Cost: a benchmark spends real tokens on the owner's key, and a task's outcome still depends on a stochastic model, so single runs show trends, not proof; repeats (`--repeat`) are the way to tell. External agents (Claude Code, Codex) would need an adapter that runs the same tasks and checks; not built yet.

## 23. Requests are shaped for the provider's prompt cache

An agent re-sends its whole conversation on every model call, so most of a run's cost is input that the provider has already seen. Requests are built so that this input is read from the cache (about a tenth of the price, or less) instead of being written again:

- The system prompt is two parts: Airth's instructions, the same for every run, with a cache breakpoint after them (and after the realm's tools, which come first), then the run's context (realm, workspace, owner, memory, North Star). A new run reads the instructions and tools from the cache.
- A run keeps the context it started with on every drive. If the realm's context changes meanwhile (a memory saved, a profile edited), the new one is sent once as a note after the conversation, instead of changing the system prompt and invalidating the cache of the whole conversation.
- While tools wait (mostly for the owner's approval), the agent re-sends the last request every 4 minutes with `max_tokens: 0`, which keeps its cache alive without generating anything, for 45 minutes at most. Each refresh costs a cache read; a miss would rewrite the conversation at 1.25 times its price. Only Anthropic supports this; refreshes are counted in the run's usage.
- Triage is a `quick` request: low effort and a small output limit.
- Command output keeps its start and end (where errors and summaries are) and drops the middle; `read_file` reads line ranges, so the agent need not load a large file to see one function.

- A conversation the model sees is compacted past 150,000 tokens (`AIRTH_COMPACT_AT`, 0 to turn off): the model writes a handoff summary (read from the cache, with the same instructions and tools), and from then on sees that summary, its plan and what follows. Every call re-sends the whole conversation, so without this a run's cost grows with the square of its length: one 309-call run reached 800,000 tokens per call. The run keeps every message for the owner; only what the model is sent changes, and a summary that is too short or calls tools is discarded.

Thinking and effort stay fixed for a run, since changing them invalidates the cache, and old tool results are never pruned one by one: on current models, editing earlier turns invalidates the cache and the preserved reasoning after them, which costs more than it saves.

*Why:* the owner asked for the cheapest requests that do not make Airth worse. These changes only remove repeated work; the model sees the same information.

*Cost:* a run does not see changes to its realm's context in its system prompt, only in the note. Refreshing the cache costs money when the owner never comes back within 45 minutes.

## 22. The app serves each request on its own thread; settings changes stay serialized

The app's HTTP server (`ui::serve_requests`) used to answer requests one at a time. Since connectors, model lists, registry search and connection checks wait on remote services (up to a minute for a first sign-in), one slow provider froze the whole app, including the Approve button of a running run. Each request now runs on its own thread. Requests that change stored settings still take a single lock (`App::writes`), so two edits never race on the same file. Exceptions: reads; approvals and stops, which only touch live runs in memory; and the routes that wait on a remote service (connecting a connector, checking one), which take the lock only around their writes, never while waiting. A test starts a service that takes two seconds to answer and checks that the app answers another request in under one second meanwhile.

*Why:* a stabilization pass found the freeze; decision 1's one-thread-per-run model already made the app concurrent with its runs, so threads per request add no new kind of concurrency.

*Cost:* one short-lived thread per request (the app polls every few seconds, and pauses while its window is hidden). An OAuth callback may still race with a settings change of the same MCP server; that would need two changes to the same server within the same second.

## 21. The owner picks the model per request; each provider only gets its own key

The message box has a model menu. **Auto** keeps the realm's routing (decision 17). Any `provider/model` can also be picked among Anthropic, OpenAI, xAI (Grok), DeepSeek, Mistral, OpenRouter and Ollama. DeepSeek, Mistral and OpenRouter are OpenAI-compatible presets with their own key variables (`DEEPSEEK_API_KEY`, `MISTRAL_API_KEY`, `OPENROUTER_API_KEY`), which count as model keys, never passed to commands. The menu lists each configured provider's models live from its `/models` endpoint (chat models, filtered by name, cached ten minutes), since model names change faster than Airth's releases. The choice is saved in the run (`model_choice`) and applies to every drive until changed. The app checks it before starting, so a missing key is reported right away. `hub.model_for_drive` puts the chosen model first and the realm's own connections after it, as fallbacks. When a run changes model (`answered_by`), the history up to that point counts as foreign (decision 17's `portable`), so the new model never receives another provider's reasoning blocks. A chosen model's settings come from `models::choice_settings`. It gives that provider its own credential and nothing else: no `OPENAI_API_KEY` or `OPENAI_BASE_URL` fallback, and no effort setting tuned for another provider.

*Why:* the owner wanted "to change LLM depending on the request, like Grok, OpenAI or DeepSeek". The generic `OPENAI_API_KEY` fallback of the OpenAI-compatible provider would otherwise have sent the OpenAI key to DeepSeek when no DeepSeek key was set.

*Cost:* model lists depend on each provider's API, and filtering by name is a guess (a few non-chat models may show). The fallback to the realm's connections means a failing chosen provider is replaced silently, apart from a warning in the log.

## 20. Connectors are remote MCP servers, connected with OAuth like in Claude or ChatGPT

A connector is a remote MCP server (Streamable HTTP) stored with the realm's other MCP servers (decision 18), with a `url` instead of a command. "Connect" follows the MCP authorization spec in `tools/remote.rs`. The server's 401 points to its protected-resource metadata (RFC 9728), which names the authorization server. Its metadata comes from RFC 8414 or OpenID locations, with and without path insertion. Airth then registers itself as a public client (RFC 7591), and the owner's browser opens the sign-in page with PKCE (S256, through `aws-lc-rs`, already built for TLS) and the `resource` parameter (RFC 8707). The browser comes back to `127.0.0.1:<port>/oauth/callback`, the one route without the API token. It accepts only a `state` Airth issued in the last 15 minutes, once. Tokens go to `skills/mcp-auth/<name>.json` (mode 600), in the home, so no command can read them, and they are refreshed when they expire or are refused. Tool calls open a session (`initialize`, `Mcp-Session-Id`), accept JSON or SSE answers, and follow the `skill` rule like other MCP tools. The Connectors tab features the official servers checked to support this flow end to end (Notion, Linear, Stripe, Vercel, Sentry, PayPal, Canva, Intercom, Figma, Webflow, Cloudflare, Square, Supabase), with Simple Icons logos embedded as paths (the CSP forbids remote images). It also searches the official MCP Registry for other Streamable HTTP servers that need no API key. The API-key presets of decision 19 stay below, for services without OAuth.

*Why:* the owner wanted connectors "like on Claude / ChatGPT: click Connect, the sign-in page opens and the protocol connects", with logos and a directory. Those products use exactly this: remote MCP servers with OAuth and dynamic registration, and the MCP Registry is the public directory.

*Cost:* services without dynamic client registration (GitHub, Asana, HubSpot, Box, Monday…) cannot be connected this way yet; they need an app the owner registers by hand. Gmail and Google have no such server: they stay reachable through the macOS account and the Mac app tools. Registry entries are not vetted, which the app says next to each address. Legacy SSE-only servers are not supported.

## 19. Connectors are integration presets, checked when connected

The connectors catalog (`src/connectors.rs`) is a fixed list of services with a well-known API. Each HTTP connector carries the official base URL, the authentication header with a `[secret:NAME]` placeholder, any header the API requires (`Notion-Version`, GitHub's `Accept`…), and a short note telling the agent how to use the API. Connecting saves the pasted token as the realm's credential `NAME`, then adds an ordinary integration (decision 10) named after the connector. One harmless authenticated request then proves the token works (`GET /user`, `auth.test`…). A refused token stays saved, to fix or disconnect. Disconnecting removes the integration, and the credential if nothing else uses it. Google and iCloud need no token: macOS holds the account, and runs reach it with the Mac app tools (decision 16).

*Why:* the owner asked for "a catalog of connectors (Google, Notion, Linear…)". Integrations already confine a credential to one base URL and keep it from the model; a preset removes the setup (base URL, header format, required headers) where mistakes happen, and the check tells the owner right away whether it works. Google's OAuth tokens last an hour, so a pasted token would silently stop working; the macOS account is refreshed by the system.

*Cost:* no OAuth flow yet, so only services that issue long-lived tokens are in the catalog. Jira, Shopify and other per-site APIs are left to the generic Integrations form. The check runs on the app's request thread (10 seconds at most).

## 18. MCP servers and catalog skills extend a realm, under the realm's own rules

**MCP.** A realm can list stdio MCP servers (`skills/mcp.json`): a name and the shell command that starts one. Connecting (from the app) starts it exactly as a run's command would start, through the same `sandboxed_command` as `execute_command`: in the realm's sandbox, from its folder, with only the passed-through variables and the realm's credentials. Airth then does the MCP handshake and keeps the tool list. During a run, each tool of an enabled server becomes `mcp__<server>__<tool>`, under the `skill` capability and scoped to that tool, so the realm's skill rule applies and "allow for this run" covers one tool. Every call starts the server, initializes it, calls the tool and kills the whole process group. Requests from the server (roots, sampling, elicitation) are declined, and errors are redacted like any command output. Connecting runs on a background thread, because the app serves requests one at a time and a first `npx` start can take a minute.

**Catalog.** The Skills page lists the Agent Skills of `github.com/anthropics/skills`: a folder with a `SKILL.md` and its references and scripts. The index (the repository tree, then each front matter) is cached for 12 hours in the home. Installing one copies its text files (no binaries, nothing over 300 kB, at most 3 MB) into `skills/catalog/<name>/`. Files are staged first, so a failed download keeps the previous install. Paths are checked to stay inside the folder. Runs get one tool, `use_skill`, under the `read` capability. Its description lists each installed skill's name and description; called, it returns `SKILL.md` or one of the skill's files.

*Why:* the owner asked to "integrate the skills catalog and open up to MCP". Both bring capabilities Airth would otherwise develop itself (decision 14), without giving up isolation. A server is a command like any other: same sandbox, same credentials boundary, same approval. A catalog skill reaches the model as a tool result, so it is data the agent weighs, not text in the system prompt that could outrank the owner.

*Cost:* one process per MCP call. Servers that keep state between calls, or speak HTTP/SSE instead of stdio, are not supported yet. A catalog skill's scripts sit in the home, out of the sandbox's reach: a run must copy one into the workspace (an approved write) before running it. The catalog is a single source for now, and GitHub's unauthenticated limit (60 requests an hour) caps how often the index can be refreshed.

## 17. Several model connections routed by the ask, and one North Star per realm

**Routing.** Besides its main connection, a realm may configure a fast one (`AIRTH_FAST_MODEL`, ...) and a fallback (`AIRTH_FALLBACK_MODEL`, ...), each inheriting the main settings it does not override. With a fast connection, the fast model triages the latest ask in one short call with no tools (SIMPLE or COMPLEX, COMPLEX when in doubt; asks over 1,500 characters skip triage). The run records its tier and keeps it: a follow-up can move it to the main model, never back, and skill development and brainstorming always use the main model. Every drive tries its connections in order (its tier's first, then the others) and stays on the one that answered. When a run changes model, reasoning and provider-specific blocks written by the previous model are left out of what the new one sees, since their signatures only mean something to the model that wrote them. Triage calls count in the run's usage.

**North Star.** A realm can have one goal, `northstar.json`, set only by the owner (app or `airth northstar`). Goal and milestones are defined in a guided run on the main model (started as soon as the owner writes the goal): it interviews the owner (3 to 5 questions), proposes a sharper goal, milestones each with a "done when", facts to remember and a rhythm, and records what the owner agreed to through the `north_star` and `remember` tools. Starting that conversation is the owner's go-ahead for recording milestones, so the run is created with a grant for the `north star` scope; rewording the goal (`set_goal`, scope `north star goal`) is the owner's decision alone: the gate asks for it every time, even where the realm allows memory, and never grants it for a run, so no scheduled run can change the goal. Every run's briefing carries the goal and milestones, with the instruction to prefer the next milestone's work, to take on nothing unrelated unprompted, and to record progress. `north_star` uses the `memory` capability, scoped `north star`, so it follows the realm's memory rule, a run grant covers only it, and the scheduled objective `north-star` (daily push) is pre-approved for it alone. A run counts as serving the goal if it was started for it (a brainstorm, a push, the objective) or recorded progress on a milestone. The app's focus mode shadows the rest.

*Why:* the owner asked to "switch between LLM connections depending on the complexity of the ask", and a run had just failed on exhausted Anthropic credit with no way to continue elsewhere. They also asked for one North Star whose milestones they brainstorm with Airth, which Airth helps reach all day, with visible progress and everything unrelated shadowed. Spec §3.7 anticipated routing by "capability, latency, privacy requirements, and cost".

*Cost:* one extra small model call per new ask when a fast connection exists. Triage reads untrusted text, so a crafted ask can at worst get itself the cheaper model. The tier is per run, not per step, so a simple-looking ask that turns out hard stays on the fast model until a follow-up (the owner can rephrase or start a new run). Dropping a previous model's thinking blocks on a switch costs the new model that reasoning. Focus is a display choice: it hides nothing and denies nothing.

## 16. A native Mac app, and Mac apps as fixed, approved actions

`Airth.app` is a real app bundle around Airth's own binary. Started without arguments from the bundle, it opens a native window (WKWebView through `wry`/`tao`) on the same app the browser shows, with a Dock icon, a menu bar icon and ⌥⇧A to show or hide it. Closing the window keeps Airth in the menu bar. It talks to the background service when one runs, otherwise it serves the app itself. The service and the `airth` command (a link into the bundle) run the same binary, so one update covers all three. The bundle is signed ad hoc, and again after each update.

Airth works with the owner's Mac apps through tools: `calendar`, `reminders`, `notes`, `mail_draft`, `safari`, `clipboard`, `open_on_mac`. Each runs a fixed JavaScript-for-Automation script shipped with Airth, with the model's input passed only as JSON data. There is no tool that runs arbitrary AppleScript, which could reach a shell and bypass the sandbox. Mail drafts are opened for the owner to send, never sent. A new `apps` capability (`ask` by default) governs them per realm, scoped per app, so the owner can "allow Calendar for this run" or pre-approve it in an objective. macOS adds its own prompt the first time Airth controls each app.

*Why:* the owner wants a personal assistant that lives on their computer like other desktop assistants and acts in their apps, without giving up the approvals and audit that govern everything else.

*Cost:* Calendar, Reminders and Notes are shared by all realms, so a realm's `apps` rule decides whether that realm may use them. Ad-hoc signatures change with each update, so macOS may ask again for automation permissions after an update. Dictation uses the page's speech recognition when available, else macOS dictation.

## 15a. Airth updates itself from its source repository, only after its tests pass

Airth records at build time the commit and the checkout it was built from. The running app checks every minute whether that checkout's branch has moved forward, and every 30 minutes whether its upstream has. It prepares a newer version away from the owner's checkout, in a detached worktree in the home: `cargo build --release`, then `cargo test --release`. Only a version whose tests pass is offered ("Restart to update"), and only the owner's click (or `airth update`) installs it. The binary is swapped atomically and the previous one kept (`airth update --rollback`). The login service exits and launchd starts the new binary; a terminal `airth ui` replaces itself in place. The app's access key is kept in the home, so the open page reconnects after the restart.

*Why:* "controlled software updates" (spec §7), and the self-improvement loop of §8 ending in "approve integration": an improvement Airth made to itself and the owner adopted reaches the running Airth through the same gate. A failed build or test never touches the installed binary.

*Cost:* updates need the source checkout and a Rust toolchain on the machine. Release builds take a few minutes. Runs in progress are stopped by the restart; they can be resumed.

## 15b. A second provider speaks the OpenAI-compatible Chat Completions format

Besides Anthropic, `AIRTH_PROVIDER` can be `openai`, `xai` (Grok) or `ollama` (local models). All three go through one provider that speaks the Chat Completions format. Presets set the base URL and the key variable (`OPENAI_API_KEY`, `XAI_API_KEY`, none for Ollama), and `OPENAI_BASE_URL` points it at any other compatible server. `AIRTH_MODEL` is required for these providers, so Airth never guesses a model name. The new keys are provider keys: never given to commands, redacted from everything the model sees, and storable once for all realms. Per-realm settings mean one realm can run on a local model while another bills a cloud key.

The translation is lossy only where the formats differ. Tool results become `tool` messages placed right after the assistant's calls. Another provider's reasoning and opaque blocks are dropped, which lets a run continue after its realm switches provider. Tool calls decide the stop reason even when a server reports `stop`. Missing call ids are generated.

*Why:* spec §3.7 says "no single provider should be architecturally mandatory", and §3.6 says remote models are optional. Comparable assistants run on many models, and a local model keeps a realm's data on the machine. One wire format covers most of the market without a dependency per vendor.

*Cost:* no prompt caching or reasoning display on these providers. Smaller local models follow the tool protocol less reliably, so runs can take more steps or stop early. There is no routing yet: the realm picks one model for all its runs.

## 14. Skills are tested programs the owner installs, not changes to Airth itself

A skill is a folder with `skill.json` (name, description, input schema, a shell command, a timeout), the program, and `tests.json` (inputs, fixture files, expected exit code and output). Installed, it becomes a tool of the realm's runs. Its command runs in the run's sandbox and workspace, gets the input JSON on stdin, and finds its own files in `$AIRTH_SKILL_DIR`. Calls go through a new `skill` capability (`ask` by default), scoped per skill, so "allow for this run" and objectives can approve one skill.

The lifecycle follows spec §8:
- **Discovery.** A run calls `propose_skill` when it hits a limitation. This records the proposal and notifies the owner; nothing is built.
- **Isolated development.** A development run (started by the owner, from the app or `airth skill develop`) has the skill's draft folder as its whole workspace: no realm files, no credentials. It gets a `test_skill` tool.
- **Evaluation.** The runtime checks the manifest and runs every test in the sandbox, each in a fresh folder, denied the Airth home, every realm folder (its own included) and the draft's write access. It does this whenever a development run ends.
- **Controlled installation.** Only the owner installs: `airth skill install`, or the app. It copies exactly the files that passed, checked by hash. Runs load only installed skills whose files still match that hash.

*Why:* v0.5's acceptance test: "Airth identifies a missing capability, implements a solution, tests it, and requests approval before enabling it." Skills extend Airth without touching its binary or its security code. Changes to Airth itself still go through isolated worktrees and checks (decision 5), and §3.5 forbids self-modification of security boundaries without authorization. Language-agnostic programs over stdin/stdout keep skills small and reviewable.

*Cost:* skills have no package management: they use what the machine has. Tests get no credentials, so a skill calling an authenticated API cannot be tested end to end. Evaluation checks behavior against the skill's own tests; the owner's review is what judges whether those tests are enough.

## 13. The app server is the background runtime

`airth serve` (or `airth ui`) runs the app server and the scheduler in one process. It writes its port and access key to `server.json` in the home (mode 600), so a second `airth ui` opens the running app instead of starting another, and only one scheduler ever runs per home. `airth service install` registers `airth serve` with launchd (macOS) to start at login and restart if it stops. Model provider settings can be stored once for all realms (`airth secret set ANTHROPIC_API_KEY --all-realms`), because a login service does not see the shell's environment.

*Why:* "background runtime" (v0.4), with the smallest number of moving parts. Unattended runs must be visible and answerable in the same app as the others, so they belong in the same process.

*Cost:* Linux has no service installer yet (a systemd user unit running `airth serve` works). The service runs the binary it was installed with; after a rebuild it must be installed again.

## 12. Notifications and summaries go to an inbox

Each objective's run ends with a notification: completed, failed, stopped, or a change ready for review. An unattended run waiting for approval sends one too. Notifications are kept in the home (the last 500), shown in the app's inbox and by `airth inbox`. On macOS a desktop notification shows the realm and the event only, never the run's content. `airth summary` and the inbox page give a cross-realm view: what ran, what waits for review, what can be resumed, what is scheduled.

*Why:* "notifications and summaries" (v0.4). Autonomy is only useful if the owner learns what happened without reading every run.

*Cost:* there is no email, phone or chat channel yet. The summary is built from the runs' own reports.

## 11. Objectives run with approvals and budgets given in advance

A persistent objective is an objective plus a trigger: a schedule (`every 30m`, `daily 08:30`, `weekdays 09:00`, in local time), a watched path in the realm, a local webhook (a secret URL whose request body reaches the run as untrusted data), or manual. Only the owner creates objectives (CLI or app); no tool can. When a trigger fires, the run starts with the objective's approvals as the run's grants (decision 9) and its budget. Budgets cap model calls, tokens and wall-clock minutes for every drive of the run. Anything else the run needs waits for the owner in the app for as long as the objective says (`0` = deny at once), and is then denied with a reason that tells the agent to go on without it and report it. A schedule missed while Airth was not running fires once, not once per missed time. A watch ignores changes the run itself made.

*Why:* v0.4's acceptance test: "Airth completes an approved scheduled objective while the owner is absent". "Approved" has to mean something concrete when nobody is there to answer: what runs may do is decided when the objective is created, never while it runs. Spec §5 also asks that "every execution must have a bounded scope, observable state, and explicit termination conditions".

*Cost:* webhooks are reachable only from this machine (the server listens on loopback). A watch polls every 10 seconds and fingerprints names, sizes and modification times, not contents.

## 10. Integrations bind credentials to a base URL

An integration is a name, a base URL and headers such as `Authorization: Bearer [secret:GITHUB_TOKEN]`, stored in the realm's settings. The `http_request` tool takes an integration name and a path. The runtime checks that the URL stays under the base URL (same scheme, host and port, path prefix, no encoded `..` or `/`), fills in the credential, and sends the request. Requests without an integration carry no credentials, and may only reach public addresses: the host is resolved once, every address is checked, and the connection is pinned to it. Redirects are never followed. Network access is a capability in the realm's policy (`ask` by default).

*Why:* "tool integrations" (v0.3) with "realm-scoped credentials" and "least-privilege tool access" (spec §7). A design where the model writes `[secret:NAME]` wherever it likes would let a prompt-injected run send a token to any host. Binding a credential to its destination makes that impossible whatever the model does.

*Cost:* only HTTP APIs with header authentication are covered (no OAuth flows, no MCP yet). Shell commands are still not restricted on the network: an approved `curl` can reach anything, though without the realm's integration credentials unless the owner also gave them to commands.

## 9. Approvals can be granted for the rest of a run

Besides yes and no, the owner can answer "allow for this run". This grants the capability for that run, scoped where the tool defines a scope: one integration, or one host. A grant never covers a request with a warning (edits to Airth's security files, say), which is always asked one by one. Grants are saved with the run, so a resumed run keeps them, and they end with the run. They never change the realm's policy.

*Why:* "approval workflows" (v0.3). A multi-step task that asks for the same kind of thing twenty times trains the owner to approve without reading. A narrow, run-scoped grant keeps the owner in charge while asking only questions worth reading.

*Cost:* "all commands for this run" is broad. The sandbox still bounds it, and the owner can stop the run.

## 8. Runs keep a plan, and resume from it

The agent keeps its plan with an `update_plan` tool: the whole list of steps, each `pending`, `in_progress`, `done`, `blocked` or `skipped`, with an optional note. The plan is saved with the run, shown live in the app and the terminal, and needs no approval (it changes nothing outside the run). Any run that did not complete (interrupted, stopped, or failed after the model provider's retries ran out) can be resumed with `airth resume` or the app's Resume button. Resuming answers tool calls the interruption cut off ("not run: the run was interrupted", or the recorded result if the tool did run), keeps the run's grants, and tells the agent how the run ended and what its plan was, asking it to check what actually took effect before going on.

*Why:* "persistent tasks" and "task planning and recovery" (v0.3), whose acceptance test is a multi-step task that "resumes after interruption". Runs were already saved after every step; what was missing was a way for the agent and the owner to see where a task stood, and a safe way to pick it up.

*Cost:* the plan is only as good as the agent keeps it. Recovery is the agent's judgment, guided: the runtime cannot know whether an interrupted external action (a POST, a command) took effect.

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
