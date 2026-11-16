// What changed in each version of Airth, written for the founder: shown in
// Settings → What's new, and built into the app, so it reads offline. The
// version bump's pull request adds the new version's entry on top
// (docs/RELEASING.md); a test checks it matches Cargo.toml.

export type Entry = {
  version: string
  /** The day it was published, YYYY-MM-DD. */
  date: string
  /** One line: what the version is about. */
  title: string
  added?: string[]
  improved?: string[]
  fixed?: string[]
}

export const CHANGELOG: Entry[] = [
  {
    version: "0.8.1",
    date: "2026-11-16",
    title: "Budgets hold for every kind of work",
    fixed: [
      "Agent routines respect the monthly budgets, as tasks do: past a cap they wait, and what they spend counts.",
      "A task that cannot start ends properly: its plan, its routine and the chief of staff hear of it, and it holds none of the month's budget.",
      "A routine run from the terminal is recorded like a scheduled one, and tells you in Needs you when it fails. A misspelt routine name is refused.",
      "An agent's look is saved whole, never half-written.",
    ],
  },
  {
    version: "0.8.0",
    date: "2026-11-15",
    title: "Projects, redesigned",
    added: [
      "On the phone, swipe sideways to move between Chat, Needs you and Projects.",
    ],
    improved: [
      "Projects in Linear's way: Projects and Tasks in two tabs, each one dense list grouped by status.",
      "On the phone, the business menu is compact and easier to read.",
    ],
  },
  {
    version: "0.7.1",
    date: "2026-11-15",
    title: "An agent opens on its conversation",
    improved: [
      "Edit opens an agent's settings on a page of their own.",
    ],
  },
  {
    version: "0.7.0",
    date: "2026-11-15",
    title: "Monthly budgets",
    added: [
      "A monthly budget for the business and for each agent. Work stops when one is reached, and Finance shows where each stands.",
      "An agent's page: its rights, tools, routines, memory and mail, in tabs.",
      "Agent levels. An agent can be exported as a package and imported into another business.",
    ],
    improved: [
      "On the phone, the home input sits higher, with Conversation beside the logo.",
    ],
  },
  {
    version: "0.6.0",
    date: "2026-11-15",
    title: "Agent mail",
    added: [
      "A role can have its own mailbox. Routines read it, mail that looks like a change of bank details is flagged for you, and nothing is sent without your approval.",
      "Mailboxes are set up in Settings, and airth doctor checks their DNS.",
    ],
    improved: [
      "On the phone: a floating glass bar, icon tabs, no header and larger menus.",
      "Airth's logo in greys.",
    ],
    fixed: [
      "Signing in to a connector brings you back to the address you used.",
    ],
  },
  {
    version: "0.5.0",
    date: "2026-11-14",
    title: "The day's plan",
    added: [
      "Each day, the chief of staff gives the team its tasks toward the goal.",
      "The chief of staff can propose an agent to hire, for your approval.",
      "An X connector, read only, to find the posts worth answering.",
    ],
    improved: [
      "The conversation shows no reports, and no message is wider than the screen.",
    ],
  },
  {
    version: "0.4.0",
    date: "2026-11-12",
    title: "Projects",
    added: [
      "Projects: a name, a description, milestones and tasks, shown on the Home, the Overview and in the morning brief.",
      "A CSV file opens as a table in Documents.",
      "Rejecting on Telegram asks why.",
    ],
    improved: [
      "Needs you puts what to decide first; what broke is folded and last. A late routine is one incident until you have seen it.",
      "The Overview says where things stand, with the week folded.",
      "The rail in groups, and conversations in time order.",
    ],
  },
  {
    version: "0.3.0",
    date: "2026-11-12",
    title: "The Home is a conversation",
    added: [
      "The Home asks \"What shall we do today?\" and the chief of staff takes it from there.",
      "Ask the team from any page.",
      "Answer an agent in your own words, and teach it lessons it keeps.",
    ],
    improved: [
      "Three pages first, Chat, Needs you and Projects, and More for the rest.",
    ],
  },
  {
    version: "0.2.0",
    date: "2026-11-12",
    title: "The chief of staff and the new interface",
    added: [
      "A guide to create a business: its record from the official register, its team, its tools.",
      "The chief of staff: one interlocutor who passes your orders to the team, with a morning brief in figures.",
      "Plans: steps that wait for one another, edited before you approve them.",
      "Revolut statements imported in Finance.",
      "A stopped task goes on with a remedy you choose.",
      "Talk to the agents on Telegram. Drafts come to Needs you in batches.",
      "Each device asks for access and gets a key of its own. Each Airth has a name.",
      "An agent may bring up an idea, once a day for the team.",
      "Choose the language the agents write to you in.",
    ],
    improved: [
      "Pages load faster, finished work cleans up after itself, and updates use less disk.",
    ],
  },
  {
    version: "0.1.5",
    date: "2026-10-21",
    title: "A lighter app",
    improved: [
      "A smaller and faster app, with up-to-date dependencies.",
    ],
  },
  {
    version: "0.1.4",
    date: "2026-10-17",
    title: "Goals and several repositories",
    added: [
      "A business holds goals with milestones and a board of tasks.",
      "A business can work in several repositories, each with its own checks and delivery.",
    ],
  },
  {
    version: "0.1.3",
    date: "2026-10-16",
    title: "Pull requests",
    added: [
      "Changes are delivered as GitHub pull requests, and merged from Airth once their checks pass.",
    ],
    improved: [
      "A clearer sidebar and Routines page.",
    ],
  },
  {
    version: "0.1.2",
    date: "2026-10-14",
    title: "Documents as an editor",
    added: [
      "Documents become a small code editor: files, tabs, colours, save, go to file.",
      "Conversations can be archived or deleted.",
    ],
    fixed: [
      "The fast model works again with older Claude models.",
    ],
  },
  {
    version: "0.1.1",
    date: "2026-10-14",
    title: "Documents",
    added: [
      "Documents for each business.",
    ],
  },
  {
    version: "0.1.0",
    date: "2026-10-14",
    title: "The first Airth",
    added: [
      "A signed Mac app that updates itself, with a guide for the first launch.",
      "Businesses with their memory, plans, routines, skills and connectors.",
    ],
  },
]
