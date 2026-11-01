# macOS app design

The agreed design for the macOS app, from the design pass in #133 (three
review rounds, signed off 2026-11-01). The SwiftUI build (#130 and the issues
split from it) follows this file and the canvas. Where they disagree, ask:
neither one is automatically right.

**In the repo:** [`design/`](../design/) holds the tokens and PNG snapshots
of every agreed frame.

**Canvas:** https://claude.ai/artifact/8dJr9w1ZyUTyV9bMAZV5qQ is the
workspace where the design was made. It's private to its owner, so ask them
for access. There are three pages:

- **Round 3 · Every state:** what to build.
- **Round 2 · Screens:** the core screens and the design system.
- **Round 1 · Structure:** the wireframes and the information architecture.

The canvas also keeps the option sheets for each decision, as a record of
what was compared.

OpenConv's design system is its own and shares nothing with any existing
one. The mockups are HTML that approximates the native look; build with
standard SwiftUI controls and these values, not a pixel copy of the HTML.

## Structure

- **One window, two panes:** the conversation list on the left, the open
  conversation on the right. A server rail is left room for at milestone 4;
  it isn't built.
- **A right-hand panel,** toggled from the header, with three tabs: Members,
  Requests (with a count badge) and Info.
- **The account bar** sits at the bottom of the list: your name, the
  connection, and Settings.
- **The header, the message bar and the account bar are one height** (52pt;
  44pt at compact density), so the top borders of the message bar and the
  account bar line up. This holds in every state.
- **The Settings window has a sidebar,** like System Settings, with sections
  for Profile, Server, Notifications, Appearance, Privacy, Storage, Advanced
  and About.
- **One relay per app,** set at first run and changed in Settings › Server.
- **The channel id is called a join code** everywhere in the UI.

## Decisions

### Round 1: structure and flows

- **Names:**
  - Conversations get shared names (#134). Until one is named, the list and
    the header show the members' names.
- **Transcript:**
  - The transcript is left-aligned rows: avatar, then name and time, then
    text. There are no bubbles.
  - Notices (joined, left, removed, renamed) are kept apart from messages and
    are never stored as messages.
- **Joining:**
  - Join requests show as a badge on the list row and in the Requests tab.
    There's no banner in the transcript.
  - Starting a conversation lands you in it, with the join code shown.
  - "Stop waiting" only affects this Mac. It forgets the offer. There's no
    withdrawal in the protocol.
- **Leaving and history:**
  - Leave is at the bottom of the members panel, and each member has a Remove
    action. Both are confirmed.
  - After leaving or being removed, the conversation stays in the list,
    read-only, with its history.
  - Local history is kept until the user deletes it.
  - A message that never reached the relay shows as **Not sent**, with Retry.

### Round 2: look, and what users can change

The default look:

- the Teal accent;
- a plain sidebar;
- transcript rows at comfortable density;
- the system font;
- tinted primary buttons;
- round avatars coloured from the member's key;
- a question-mark badge for "name not verified";
- a bordered message field.

Users can change these in **Settings › Appearance**, which has a live preview
and Restore defaults:

| Setting | Options (first is the default) |
|---|---|
| Appearance | System, Light, Dark |
| Accent | Teal, Ink, Persimmon, Moss |
| Sidebar | Plain, Tinted (an accent wash), Dark (dark in both appearances) |
| Messages | Rows, Compact (no avatars; time and name in a column), Highlight mine |
| Density | Comfortable, Compact |
| Typeface | System (SF Pro), Rounded (SF Pro Rounded throughout), Serif titles (New York for titles) |
| Text size | Smaller, Default, Larger (every text style steps one point) |
| Buttons | Tinted, Filled, Outlined |

Fixed, and not settings:

- corner radii;
- avatar shape;
- the "not verified" mark;
- the message field's shape.

The appearance settings live on this Mac only, and other members never see
them.

### Round 3: states

| State | Where it shows | What it says |
|---|---|---|
| Offline | A banner under the header, and the account bar's dot | "Offline. Trying to reach `<relay>` again in 8 seconds." [Try now] |
| Your message, in flight | Beside the timestamp | "Sending…" until the relay echoes it back |
| Your message, never reached the relay | Beside the timestamp | "Not sent · Retry" |
| Catching up | The message bar, the row subtitle and the account bar; the transcript stays where you left off | Bar: "Catching up. You can send in a moment." Transcript: a "12 new messages" jump |
| Away too long (stranded) | The message bar, in warning colours, and a notice | "Away too long. This Mac has asked to be added back; the first member online does it automatically." Notice: "Messages sent while you were away can't be shown" |
| Leaving | The message bar | "Leaving. Waiting for another member to come online to confirm it." |
| Removed | The message bar, a notice, and a dimmed row | "You were removed. What you'd read stays here; nothing new will arrive." [Delete conversation…] |
| Left | The message bar | "You left on `<date>`. What you'd read stays here." [Delete conversation…] |
| Can't reach the server | Under the address field, with a red outline (first run and Settings › Server) | "Couldn't reach this server: no answer after 10 seconds. Check the address, or ask whoever runs it." |

These rules hold in every state:

- **The message bar's job:** it keeps its height and top line. When you
  can't send, it says why in one line in place of the field.
- **Nothing is sent until the conversation is caught up.** The core already
  requires this.
- **Notices sit between hairlines,** like day dividers, and name who made the
  change: "Chen joined · let in by Bilal", "Bilal removed you", "Aisha
  renamed the conversation to …". When your rename loses a race, the notice
  reads "Bilal renamed it to "Crit" at the same time as you. Theirs went
  through."
- **Where unread starts** after being away, an accent "New since you were
  away" divider marks it.
- **The Info tab** starts with the conversation's avatar and name. The pencil
  turns the name into a field with Cancel and Save. Below them are the join
  code (with Copy), history on this Mac (with Delete history…), and Leave.
- **Confirmations** (delete, leave, remove) use the standard centred alert,
  with a destructive button.
- **Settings › Storage** lists history per conversation (message count and
  size), each with Delete history…, plus Delete all history….
- **Deleting history doesn't leave the conversation.** Deleted messages can't
  be fetched again, because the relay's copies are encrypted for keys this
  Mac has already used up.

## Tokens

The values live in [`design/tokens.json`](../design/tokens.json). Don't copy
them here or into Swift by hand: the SwiftUI theme is generated from that
file (#130).

[`design/frames/`](../design/frames/) has PNG snapshots of every agreed frame,
in light and dark. Build and review against those. [`design/README.md`](../design/README.md)
covers how both are exported from the canvas.

In short:

- **Type:** the system font, at macOS sizes (Body 13/18).
- **Space:** a 4pt grid.
- **Radii:** control 6, row 8, card 10, window 12.
- **Bars:** 52pt; 44pt at compact density.
- **Accent:** Teal (#0B7470) by default, plus three alternatives. Each has
  separate values for fills, text and tints in light and dark.
- **Avatars:** six colour pairs, chosen from the member's key, never their
  name.
- **Destructive fill:** stays #C2271D with white text in both appearances,
  because white on the dark-mode red fails contrast.

## Privacy shown in the UI

- **Unverified names:** until milestone 3, a name that stands for a person
  carries the question-mark "not verified" mark: in the header's member
  summary, the members panel and join requests.
- **Settings › Privacy** says plainly what stays on this Mac, what the relay
  can't see (messages, conversation names, members, who sent what) and what
  it can see. It can see which connections read and post to which
  conversation and when, which IP addresses can turn into a social graph,
  and your name in join requests. The wording follows
  `architecture-notes.md`.
- **Conversation names:** any member can change one, and the relay can't
  read it. Names are shown with control and bidirectional-override
  characters neutralised (#130).
