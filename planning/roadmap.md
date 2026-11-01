# Roadmap

There was no plan before this file. Each milestone was chosen a session at a
time, which worked while the codebase was small and started producing
inconsistent advice once it wasn't. This is the ordering and the reasoning
behind it, so the next decision does not get re-litigated from scratch.

It is a plan, not a promise. Revise it when something is learned — but revise
it here, rather than quietly doing something else.

Decisions already taken, with their reasoning, are in
`architecture-notes.md`. This file is what to build; that one is why the shape
is what it is.

## How work is tracked

This file holds the order and the reasoning. GitHub issues hold the execution
detail for the milestone currently being worked, and are closed by commits.

**Issues are filed at most one milestone ahead.** The repo has already run the
other experiment: 86 issues were filed against v1, 85 of them were closed
unbuilt when the architecture changed, and the one survivor described a
refactor of files that no longer exist. They were not bad issues. They were
filed further ahead than the decisions they depended on.

So: decompose a milestone into issues when starting it, not before.

Milestone 0 was tracked as #94–#97, all closed. `gh` defaults to the enterprise host on this machine, so commands
against this repo need `GH_HOST=github.com`.

## Where we are

Encrypted group messaging works end to end: MLS via openmls, a blind relay
that orders frames, a native SwiftUI client, and state that survives a
restart. Three or more members, commit races resolved correctly.

The relay routes frames by channel and stores them durably (#104), and a
client that was away catches up on what it missed when it reconnects (#105):
messages, commits, and join requests nobody was online to answer.

Frames are kept for 30 days and at most 64 MiB per channel, both
operator-configurable (#106). Members can be removed and can leave, and one
away longer than the relay keeps frames is re-added automatically (#112).
Milestone 1 is done.

Milestone 2 is under way. The core keeps a local transcript (#129) and holds
several conversations with shared names (#134), and the app's design is
agreed (#133, `planning/design.md`). What that is not yet: an app to match.
The macOS client is still a one-conversation harness until #130, and there is
no identity beyond a self-declared name.

## The ordering principle

Changes that alter the **wire format** or the **trust model** come first.
Everything built before them has to be retrofitted; everything built after
them is additive. Features that only add UI or local state come last, because
they are cheap to add and cheap to move.

That is the rule that decides the order below. It is also the rule that was
missing when "message history" was proposed as the next step: history is local
state, and it would have been built against a relay that is about to change
shape.

## 0. Continuous integration — done

Four jobs on every push and pull request: Rust (fmt, clippy `-D warnings`,
tests), Dependencies (`cargo deny`), Swift (bindings, build, `BridgeCheck`,
format lint), and Smoke (three real clients through a real relay). CI now
covers everything `just check` does.

Three things it found on the way in, all of which would have bitten later:

- `KEYCHAIN_SERVICE` was dead code on non-macOS targets, which `-D warnings`
  turns into a failed build.
- `gen-bindings.sh` copied into directories that hold only gitignored files,
  so it worked locally and failed on any fresh clone.
- `smoke.sh` discarded its build output and carried on, so a failed build
  reported itself as three clients that "wrote no vault".

`cargo-deny` also settled a licensing question: UniFFI and hpke-rs are
MPL-2.0 and both are load-bearing. Allowed deliberately — see `deny.toml` and
the dependencies section of `CONTRIBUTING.md` for the obligation that comes
with them.

## 1. Addressed, durable relay — done

The foundational one, and the reason it comes before anything user-visible.

Today every frame goes to every connected client and is then forgotten. Three
things change:

- **Addressing.** Frames carry a channel id. The relay routes rather than
  broadcasts. This is a wire format change, which is why it is early.
- **Durability.** The relay stores encrypted frames per channel, keyed by the
  sequence it already assigns. It still never parses them.
- **Cursors.** A client reconnects saying how far it has read, and receives
  what it missed.

This unlocks offline delivery, late joiners getting context, and multiple
conversations. It also turns local history into a cache of something
authoritative rather than the only copy.

Tracked as #103–#107, #110 (fixed), #112 and #117. #110: messages sent
concurrently with a commit were silently dropped, which had to be fixed before
catch-up could deliver "the same message set as everyone else".

**Order: 103 → 104 → 107 → 117 → 105 → 106**, with #112 alongside #105.

- #107 moved ahead of #105, because #105 adds per-channel cursors to the
  client's persisted state. They should land in the new storage, not in a
  snapshot format that is about to be replaced.
- #117 (group commit) was found after #104 merged: one flush per frame,
  under one lock, capped the whole relay at about 250 frames a second with a
  true disk flush. It goes before #105, so catch-up is built on the relay's
  final write path.
- #106 can go any time after #104.
- #112 (removal, leaving, re-adding a stranded member) was split out of #105.
  Without the split, #105 could not be finished without quietly building
  removal inside it.

MIMI was read before #103 set the wire format (2026-09-29; see
`architecture-notes.md`). It does not constrain the client↔relay protocol, but
it does suggest three cheap choices here:

- the MLS group id as the channel id;
- a non-channel path for KeyPackages and Welcomes;
- an accepted timestamp stored beside the sequence.

(A fourth, a receiver epoch tolerance, was rejected while fixing #110. A
member admitted by the racing commit never had the old epoch's keys, so
tolerance would let some members read a message that others cannot.)

### Retention, decided

**A 30 day window plus a per-channel size cap, both operator-configurable.**

Not acknowledgement-based, which is the design that first suggests itself.
Dropping a frame once every member has received it requires the relay to know
who is in a channel, and it does not: MLS membership is client-side state and
the relay sees connections, not rosters. Teaching it membership would hand the
server a social graph to solve a storage problem, and the blind-relay rule
outranks that.

A window is also less limiting than it appears. MLS forward secrecy means a
member who joins at epoch N cannot decrypt frames from earlier epochs however
long they were kept, so retention only ever serves members already in the
group who went offline. It buys nothing for new members or new devices.

The number therefore answers exactly one question — how long may someone be
offline and still catch up — and thirty days covers a holiday or a broken
laptop. Seven is defensible and more privacy-maximising. Because this is
self-hosted, the operator can set either; the documentation should say plainly
that lowering it costs only the offline-delivery window.

**Evicting a commit strands a member; evicting a message only loses it.** A
member who misses a commit can process nothing after it. A size cap can evict
commits well inside the 30-day window, so on a busy channel the real catch-up
window is whichever limit is hit first. The relay therefore has to tell a
returning client whose cursor is below the retention floor. It can do that
without parsing anything. The client then needs a recovery path: another
member removes and re-adds it (openmls `fork-resolution`). That belongs in #105
and #106, not after them.

**Done when** a client can be closed, miss traffic, reopen, and catch up.

## 2. History and transcript UI

Cheap once (1) exists, because the frames can be fetched and decrypted. Local
store keyed by channel, a transcript that survives restart, and pagination.

**Decided 2026-11-01: MIMI content** (`draft-ietf-mimi-content-09`), with
provisional URIs until milestone 3; see `architecture-notes.md`. History
first: the format carries replies, edits, deletes and reactions from the
start, and their UI follows.

Tracked as #128 (content), #129 (transcript store), #133 (design pass) and
#134 (several groups per client), all done; then #130 (the main window),
#141 (first run, starting and joining, requests and members), #142 (the
Settings window) and #131 (replies, edits, deletes, reactions).

**Order: 130, then 141 and 142, then 131.** #141 and #142 build on #130's
window and theme, and don't depend on each other.

- **The design was signed off on 2026-11-01** after three rounds. It is
  recorded in `planning/design.md`: structure, every decision, the states
  and their wording, and the tokens. The canvas is linked from there. The
  design was bigger than the old #130 ("show the stored transcript"), so
  #130 was rewritten as the main window and #141 and #142 were split from it.
- **#133 was added on 2026-11-01.** No UI design existed anywhere in this
  roadmap, and #130 and #131 are mostly UI. Building them on the current
  harness would mean building them twice. Agreed for the pass: two panes
  (conversation list and transcript) with room for a server rail at milestone
  4; native macOS with a light identity of its own; OpenConv's own design
  system, sharing nothing with an existing one; clickable mockups on a Design
  canvas; three review rounds (structure and flows, core screens, every
  state), each signed off before the next.
- **#134 comes from the design.** A conversation list needs a client that
  holds more than one group, and `Member` holds one. It starts once round 1
  settles what the list needs, and lands before #130.
- **#129 doesn't depend on the design,** and the design doesn't depend on it.
  It is core only, and its schema already carries the channel id, so #134
  doesn't break it. It has to land before #130.

Schema carries a channel id from the first version even though it is obvious —
the point of doing this after (1) is that the shape is already known.

**Done when** reopening a client shows the conversation, not an empty window.

## 3. Identity and verification

MLS credentials are currently unverified `BasicCredential`s: anyone can claim
any name, and nothing stops an impostor joining. This is the deepest gap for a
product whose pitch is privacy, and it changes the trust model, so it comes
before anything that builds on membership.

The credential form should be able to carry a URI identifier, which is how
MIMI names users and clients.

Includes accounts, verified credentials, and a way for two people to confirm
they are talking to who they think — a safety-number equivalent.

Several carried-over v1 audit findings land here: domain-separated challenge
signatures, atomic recovery-code redemption, revoking live WebSockets on
logout. See `v2-design-requirements.md`.

**Done when** an impostor cannot silently join a conversation.

## 4. Guilds, channels, roles

The Discord surface: a server with many channels, membership, and
permissions. Deliberately after identity, because roles without verified
identity are decoration.

Role position validation on update is a v1 finding that applies here.

## 5. Multi-device

Deliberately late. Per-device session sprawl is what sank v1. MLS handles
multiple devices per member far better than pairwise Signal sessions did, but
this is still the single highest-risk feature in the product and it should
land on a codebase that is otherwise stable and tested.

## 6. Files and media

Encrypted upload, download, thumbnails. Additive, and genuinely last.

## Deliberately not planned yet

**Federation.** The topology choice was self-hosted central relay, revisitable
later. Nothing above forecloses it, but nothing above assumes it either.

**Mobile.** The Rust core is already the portable half; the client is not. A
second client is a large commitment and there is no reason to make it before
the product shape is settled.

## Known debt, carried

- `openmls_sqlite_storage` is vendored in `vendor/` with upstream's
  `rusqlite` 0.40 bump (openmls#2256), which is merged but unreleased. Delete
  the copy and its `[patch]` entry once upstream publishes it (#107).
- `rust-version` was `1.85`, which could never have built: openmls 0.9
  declares 1.91. Corrected to 1.91; still not verified by CI.
- The relay's lag-disconnect path has no test.
- The local transcript (#129) is kept until the user deletes it, and kept
  after leaving or removal (decided in #133). Deleting a conversation's
  history is planned in #130 (core and the Info tab) and #142 (Settings ›
  Storage); there is no size cap.
