# Testing Airth by hand

What to check by hand, and how, when a milestone reaches a point the founder tests. Automated checks are in `CLAUDE.md` (`cargo test`, and `npm run check` in `web/`); this file is for what needs the real business, the real model and the founder's eyes.

## Point A: a dry run of Hedgen's routines (milestone 3, step 4)

A dry run reads the feeds and asks the model for real, takes every approval as given, and only shows what each act would do. Nothing leaves Airth but the feed reads and the model calls, which cost tokens on the realm's model (DeepSeek for Hedgen).

### 1. The build

From an up-to-date `main`, once the step 4 pull requests are merged:

```bash
cd ~/Personnel/airth
git switch main && git pull
cargo build --release
./target/release/airth service install
```

Quit and reopen Airth.app if it is open, so it runs the new build.

### 2. The routines in the Hedgen folder

The routines are in the `hedgen-business` repository, pull request #1 (`routines/migrate`). Check it out in the Hedgen folder:

```bash
cd ~/Personnel/hedgen
git fetch && git switch routines/migrate
```

Then let Airth check them:

```bash
airth routines --realm hedgen
```

Each routine is `✓` with its blocks and agent, or `✗` with the block at fault and why.

### 3. Declare Typefully, for `daily-posts`

`daily-posts` ends by sending the approved post to Typefully as a draft, through Typefully's API v2 (v1 stopped on 15 June 2026). The action must be declared in the realm before the routine loads.

Create a v2 API key in Typefully, then give it to Airth (you type it; Airth never shows it):

```bash
airth secret set TYPEFULLY_API_KEY --realm hedgen
airth integration add typefully https://api.typefully.com --realm hedgen \
  --header "Authorization: Bearer [secret:TYPEFULLY_API_KEY]" \
  --description "Typefully: drafts of posts for X"
```

Find the id of Hedgen's social set (the account drafts go to). Paste the key when asked; it stays in this terminal only:

```bash
read -rs "TF?Typefully v2 key: " && echo && \
  curl -s -H "Authorization: Bearer $TF" https://api.typefully.com/v2/social-sets; unset TF
```

Note the `id` of Hedgen's X account. Then declare the action from a file, for example `/tmp/create_draft.json`, with that id in place of `123`:

```json
{
  "name": "create_draft",
  "method": "POST",
  "path": "/v2/social-sets/{social_set_id}/drafts",
  "fields": [
    {"name": "social_set_id", "place": "path", "value": "123"},
    {"name": "text", "place": "body", "required": true}
  ],
  "body": {"platforms": {"x": {"enabled": true, "posts": [{"text": "{text}"}]}}},
  "class": "reversible"
}
```

- `value` fixes the social set: no routine or agent can send a draft elsewhere.
- `body` is the only body ever sent, the post's text in its slot: nothing else can be added, a `publish_at` that would publish included. That is what makes the action `reversible`.

```bash
airth actions declare typefully /tmp/create_draft.json --realm hedgen
airth actions --realm hedgen
airth routines --realm hedgen
```

`airth actions` lists `typefully.create_draft` as `reversible`, and both routines load.

### 4. The dry runs

```bash
airth routines dry-run daily-watch --realm hedgen
airth routines dry-run daily-posts --realm hedgen
```

What to look at in each report:

- every block, with how many items went in and out, and its notes (a feed down, items too old, answers that did not match their schema);
- for an approval: `would ask the founder:` and the text you would see;
- for an act: `would <action> [<routine>/<source>/<block>]:` with what it would call. The part in brackets is the act's key: the same feed item gives the same key on every run, whatever the model drafted.

Nothing is sent to Typefully, and nothing goes to your queue. The model calls are recorded in the realm's database, like a real run's.

### What to report

- A routine refused that should load, or the reverse.
- A report that is hard to read or misses something you need to judge the run.
- Drafts or picks that are off: that is the prompts' work, in `prompts/` of the Hedgen folder.

## Point B: a routine waits for you on Telegram (milestone 3, steps 5 and 6)

A real run of `daily-posts`: it reads, drafts, and each draft waits for you on Telegram; what you approve goes to Typefully as a draft, once. Nothing is published: Typefully only receives drafts, and you publish from there.

### 1. The build

As for point A, from an up-to-date `main`: `cargo build --release`, then `./target/release/airth service install`, and reopen Airth.app. The service now runs the Telegram bot, the scheduled routines and the delayed actions.

### 2. Your bot

1. In Telegram, talk to @BotFather: `/newbot`, give it a name. Keep the token it gives you to yourself.
2. On the Mac:

   ```bash
   airth telegram connect
   ```

   Paste the token when asked (it is read from the keyboard, never from the command line). Airth prints a link: open it **on your phone**, in Telegram, and press Start. The bot answers "Paired."
3. `airth telegram status` says `connected and paired`.

Only your chat and your user count; a forwarded message never does.

### 3. Run the routine

The Hedgen folder on `routines/migrate`, Typefully declared (point A, step 3), then:

```bash
airth routines run daily-posts --realm hedgen
```

The report says, for each draft, `asked the founder (item N)`. Within half a minute each one reaches Telegram: `hedgen · Needs you`, its title, the whole draft (a long one comes in several messages, the buttons under the last), and **Approve / Edit / Reject**.

### 4. Answer from the phone

- **Edit**: tap it, then **reply to the item's message** (swipe it in Telegram) with the new text. A message that is not a reply, or a reply to another item, is ignored and the bot asks again. The edited draft comes back with new buttons; the old ones no longer work.
- **Approve**: the draft goes to Typefully once; the bot answers `hedgen · done: …` with what the routine did. Approve again: nothing happens twice.
- **Reject**: nothing is sent.

### 5. Check what was kept

```bash
airth queue --realm hedgen          # nothing left waiting
airth time show --realm hedgen      # your answers, with their time
```

In Typefully: one draft per approval, with your edits.

### 6. Scheduled runs

`daily-posts` runs every morning at its time (`daily 07:30`) as long as the service runs. The first time the service sees it, it only arms it; a time missed while the Mac was off runs once, late, and you are told. A failed run comes to Telegram too.

### The old pipelines are off

The Hedgen folder's `business/pipelines/` is gone on `routines/migrate`, so the service no longer runs them beside the routines (that would draft twice). To check:

```bash
airth pipeline check --realm hedgen        # "No pipelines"
grep "pipeline hedgen" ~/.airth/logs/serve.log | tail -3
```

The last `pipeline hedgen/… started` lines are from before you switched the folder to `routines/migrate`; none should come after.

### What to report

- An item that does not reach Telegram, reaches it twice, or arrives cut.
- An edit taken from a message that was not a reply to its item.
- A draft in Typefully you did not approve, or one approved that is not there.
- Your time looking wrong in `airth time show`.

## Point C: the watch and the weekly report, dry, from /next (milestone 4)

The Routines page of the new interface shows each routine as its blocks, and runs it dry: reads and AI blocks for real, approvals taken as given, acts only shown. Nothing is sent and nothing goes to your queue; the model calls are recorded as a dry run's.

### 1. The build, without touching the installed service

While point B runs, the installed service stays as it is. Run the new build beside it, on another port, with no scheduler and no Telegram:

```bash
cd ~/Personnel/airth
git fetch && git switch --detach cd44a07
cargo build --release
./target/release/airth ui --preview --port 7460
```

**While point B runs, build from that commit (#66 merged), never a later main.** From decision 59 on (#68), a new build migrates `business.db` to a newer schema the first time it opens it, and the installed service, older, then refuses the database ("newer than this Airth knows") until it is updated.

It uses your real home, so the dry runs read the feeds and ask DeepSeek with Hedgen's key, as `airth routines dry-run` does. `--preview` starts nothing on its own: no schedule, no Telegram, no delayed action. Ctrl-C stops it. Once point B is over, build from main and `airth service install`: the service then migrates the database itself.

### 2. The routines in the Hedgen folder

`weekly-report` and its agent, the Chief of staff, are in the `hedgen-business` repository, pull request #2 (`routines/weekly-report`, on top of #1). Once #1 is merged or checked out, check it out too:

```bash
cd ~/Personnel/hedgen
git fetch && git switch routines/weekly-report
airth routines --realm hedgen      # three routines, each ✓
```

The weekly report compares your time with `baseline_minutes` in `business/goal.md`: set it to your weekly minutes on Hedgen before Airth, or the report says it is not set.

### 3. The dry runs

Open the link the preview prints, then `/next`, then Hedgen's **Routines**:

- **daily-watch**: Dry run. Each block shows how many items went in and out; choose a block to see what it did: the feed counts, the items kept with their score and pillar, and what `airth.report` would report.
- **weekly-report**: Dry run. Choose `airth.report`: the week's figures, as the Monday report will give them. It reads only your business's records: no feed, no model, no cost.

### What to report

- A block whose summary or result is hard to read.
- A figure of the weekly report that looks wrong, or one you miss.
- Where a report should reach you (Needs you and Telegram, one report per run): that is decision 59, waiting for your answer.
