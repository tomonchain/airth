# Airth interface design

The mockups in `mockups/` are the validated reference for the new interface (1 November 2026). Open them in a browser. They are static HTML with inline styles and sample data: copy the look, the layout and the copy, never the markup. Build the pages with the tokens and components below, in the app's own stylesheet.

| Mockup | Page |
|---|---|
| `hq.html` | HQ: one section per business, never totals across businesses |
| `needs-you.html` | Needs you, inside a business: agent list, thread, routine details |
| `team.html` | Team: one card per agent, by department |
| `tasks.html` | Tasks: the order box, then To review, Running, Waiting |
| `calendar.html` | Calendar: routines and deadlines for the week, legal ones included |
| `routine-editor.html` | Routine editor: the graph, block details on the right |

All figures in the mockups are illustrative.

## Principles

- Dark, calm, few colours. Colour carries meaning, not decoration.
- A value turns amber or red only when it leaves its normal range. Everything in range stays neutral.
- Finance is per business. Never add up cash, revenue or runway across businesses.
- Agents read as a team: each has a colour and an icon, used everywhere they appear.
- Product words only, from the glossary in CLAUDE.md. Copy in plain English.
- Every interactive target is at least 40 px high (44 px in the rail).

## Tokens

Define these once as CSS custom properties and use only them.

### Colour

| Token | Value | Use |
|---|---|---|
| `--bg` | `#0F1011` | Window background, header, rail |
| `--panel` | `#17181A` | Main panel |
| `--card` | `#1D1F22` | Cards, tiles, inputs |
| `--raised` | `#23262A` | Selected rail item, hovered row |
| `--chip` | `#26292D` | Neutral chips and tags |
| `--line` | `#2A2D31` | Card borders, dividers |
| `--line-soft` | `#24272B` | Panel border, row dividers inside cards |
| `--text` | `#ECEDEE` | Primary text |
| `--text-2` | `#C2C7CE` | Body text in sentences |
| `--text-3` | `#A3A9B1` | Secondary text, section labels |
| `--text-4` | `#8B929B` | Captions, times, units |
| `--text-5` | `#6F767F` | Sources, footnotes, empty values |
| `--accent` | `#8FA6FF` | Links (hover `#B8C6FF`) |
| `--ok` | `#5BD08D` | Healthy, on track, done (background `#16301F`) |
| `--warn` | `#F5B353` | Out of range, waiting for you (background `#2A2213`, chip `#3A2C14`, muted text `#D9C29A`) |
| `--bad` | `#FF9A9A` | At risk, failed, reject |
| Primary button | `#ECEDEE` on `#0F1011` | One per view at most |

**Light theme** (the founder's choice, 3 November 2026, from the classic interface's palette): the same tokens redefined under `:root[data-theme="light"]`: `--bg` `#F4F4F5`, `--panel` `#FFFFFF`, `--card` `#FAFAFA`, `--line` `#E4E4E7`, `--text` `#09090B`, `--text-3` `#5F5F66`, `--accent` `#3B5BDB`, `--ok` `#15803D`, `--warn` `#B45309`, `--bad` `#DC2626`. The theme follows the system until the founder picks one with the rail's toggle; the choice is kept in the browser and shared with the classic interface (`/theme.js`).

Agent colours (background, icon stroke): Watcher `#4C8DFF` / `#0B1A33` (eye), Post writer `#F39C4A` / `#2A1404` (pen), Writer `#8B6CF6` / `#170D33` (book), Airth `#4FCB8F` / `#08261A` (spark). A new agent gets a colour from this family, never one of the status colours.

### Type

- Inter 400, 500 and 600; JetBrains Mono for code, ids and logs. Bundle the font files with the app: no request to a font CDN at runtime.
- Sizes, the classic interface's scale (the founder's choice, 3 November 2026), as tokens `--fs-*`: 11 small labels · 11.5 captions and labels · 12 · 12.5 secondary · 13 rows and body · 14 section titles and sentences · 15 card titles · 18 values and business names · 22 page headline (20 under 720 px). Pages use the tokens, never a size of their own.
- Section labels: 11.5 px, 600, uppercase, letter-spacing 0.04em, `--text-3`.
- Headline: 600, letter-spacing −0.015em, line-height 1.2. It is one sentence that says what matters today.

### Shape and spacing

- Radius: 8 chips and small buttons · 10–12 buttons, tiles, rail items · 14–16 cards · 20 large cards · 999 status pills.
- Spacing on a 4 px grid; usual gaps 8, 10, 12, 16, 20, 28.
- Main panel: margin 0 12 12 0, radius 16, padding 32–36. The HQ content column is centred, max 880 px.

## Page layouts (decision 72)

Every page is one of three layouts, with the same page header at the panel's top left: a label (the business, or "Airth"), the page's name as its title (`--fs-headline`), an optional one-line summary that says what matters today, and the page's actions on the right. A page that fits none of the three is a decision.

- **Reading:** one column under the header, aligned with it, 880 px at most. HQ, Business, Tasks, Settings, Workshop, a task's run, the profile.
- **Board:** the panel's full width under the header. Team, Calendar, Usage.
- **Workspace:** under the header, a list (300 px), the work in the middle, its details (320 px), in one bordered block. Needs you, Routines, Documents.

Pages are Preact components (`html` from htm); the header and the layouts are `PageHeader`, `Reading`, `Board` and `Workspace` in `src/ui/next/js/13-ui.js`.

## Components

- **Shell:** header (business switcher on the left, primary action on the right), icon rail 72 px wide (HQ, Needs you, Team, Routines, Tasks, Calendar; Workshop, Settings, the theme toggle and the founder's profile pinned at the bottom, in view as the page scrolls), main panel. Needs you shows an amber dot when something waits.
- **Status pill:** dot + word (Healthy, Watch, At risk, Not started, On track, Waiting for you). Not started uses a hollow grey dot.
- **Vital tile:** label 12, value 22/600, sub-line 12. Amber background and text when out of range.
- **Finance strip:** one bordered row of four cells (Cash, In, Out, Runway) with the source and sync time on the right of its label.
- **Search (⌘K):** from the classic interface. A dialog over the page, opened with ⌘K (Ctrl+K) or the search icon at the top of the rail. It lists pages, businesses, the open business's tasks and the theme, filtered as you type; arrows choose, Enter goes, Esc closes.
- **Activity row:** agent avatar 24 px, bold agent name, one sentence, time on the right; 44 px high, ellipsis on overflow.
- **Agent avatar:** rounded square in the agent's colour with its icon; a small count badge for pending items, red `!` for an incident.
- **Approval card:** tag (`Draft for X · needs you`), expiry on the right, the content, its source, what approving does, then Approve (primary), Edit, Reject (`--bad` text).
- **Routine node:** card with block type, title, one-line result; the selected node opens its details in the right panel.
- **Empty states** say what will fill the space and when ("From the Numirama MCP", "No bank account yet"), never a bare dash alone.

## Behaviour to keep

- Clicking a business on the HQ opens its pages. Clicking an activity row opens the run.
- Telegram mirrors Needs you; the web page and Telegram show the same items.
- Responsive: under 720 px the rail becomes a top row and grids collapse to one column.
