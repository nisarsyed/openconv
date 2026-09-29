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

What that is not yet: **one implicit conversation, everyone online at once.**
There is a single global broadcast channel and the relay stores nothing, so a
message sent while you are disconnected is gone permanently. There is no
identity beyond a self-declared name.

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

## 1. Addressed, durable relay

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

Tracked as #103–#107, plus #110 — messages sent concurrently with a commit
are silently dropped, which has to be fixed before catch-up can deliver "the
same message set as everyone else".

MIMI was read before #103 set the wire format (2026-09-29; see
`architecture-notes.md`). It does not constrain the client↔relay protocol, but
it does suggest four cheap choices here:

- the MLS group id as the channel id;
- a non-channel path for KeyPackages and Welcomes;
- an accepted timestamp stored beside the sequence;
- a receiver epoch tolerance.

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

Decide here whether message content is the MIMI content format
(`draft-ietf-mimi-content`: CBOR, stable message ids, replies, edits,
reactions) or deliberately not. Today a message is bare UTF-8. The transcript
schema is where that choice becomes expensive to change.

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

- The whole MLS store is rewritten on every send and receive. Re-measured
  2026-09-29 at 7.4x overhead per message. A persisted send already takes
  1.21 ms at fifty members, and the store grows with group size: 26 KiB at two
  members, 197 KiB at fifty. The fix no longer means writing 72 trait methods.
  `openmls_sqlite_storage` 0.3 is published and matches our `openmls_traits`
  0.6. What remains is encryption at rest, which that crate does not provide.
  The trigger is milestone (1), which multiplies the cost by the number of
  channels. Tracked as #107; see `crates/core/examples/measure.rs`.
- `rust-version` was `1.85`, which could never have built: openmls 0.9
  declares 1.91. Corrected to 1.91; still not verified by CI.
- The relay's lag-disconnect path has no test.
