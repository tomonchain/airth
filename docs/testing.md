# Testing Airth by hand

What to check by hand, and how, when a milestone reaches a point the founder tests. Automated checks are in `CLAUDE.md` (`cargo test`, `node scripts/ui_smoke.js`); this file is for what needs the real business, the real model and the founder's eyes.

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

`daily-posts` ends by sending the approved post to Typefully, so the action must be declared in the realm before the routine loads. The key is yours to enter; Airth never sees it in clear:

```bash
airth secret set TYPEFULLY_API_KEY --realm hedgen
airth integration add typefully https://api.typefully.com --realm hedgen \
  --header "X-API-KEY: Bearer [secret:TYPEFULLY_API_KEY]" \
  --description "Typefully: drafts of posts for X"
```

Check the header and path against Typefully's API documentation before you rely on them. Then declare the action from a file, for example `/tmp/create_draft.json`:

```json
{
  "name": "create_draft",
  "method": "POST",
  "path": "/v1/drafts/",
  "fields": [{"name": "content", "place": "body", "required": true}],
  "class": "reversible"
}
```

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
airth time --realm hedgen           # your answers, with their time
```

In Typefully: one draft per approval, with your edits.

### 6. Scheduled runs

`daily-posts` runs every morning at its time (`daily 07:30`) as long as the service runs. The first time the service sees it, it only arms it; a time missed while the Mac was off runs once, late, and you are told. A failed run comes to Telegram too.

### What to report

- An item that does not reach Telegram, reaches it twice, or arrives cut.
- An edit taken from a message that was not a reply to its item.
- A draft in Typefully you did not approve, or one approved that is not there.
- Your time looking wrong in `airth time`.
