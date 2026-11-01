# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

`just --list` shows everything. The ones that matter:

```sh
just check              # formatting, lints, tests, FFI bridge, real-app smoke test
just test               # cargo test --workspace
just lint               # clippy -D warnings, plus swift format lint
just fmt                # format both languages
just bindings           # regenerate Swift bindings from crates/core/src/ffi.rs
just theme              # regenerate the SwiftUI theme from design/tokens.json
just relay              # start the relay on 127.0.0.1:8080
just client alice       # launch a client (click Host, or paste a channel id and Join)
just labs               # real relay and apps through crashes, restarts, retention
```

The labs (`labs/README.md`) are manual, not in `just check` or CI: run the
ones touching your change before a PR. `just lab-setup` once for their Python;
`just lab <name>` runs one against what is already built.

Single test: `cargo test -p openconv-core group_state_survives_a_restart`

One test is `#[ignore]`d because it touches the real macOS Keychain:
`cargo test -p openconv-core -- --ignored keychain`

Two clients side by side without sharing state:
`OPENCONV_DATA_DIR=/tmp/a just client alice`

Clients remember how far they have read the relay's history, so reset relay
and client data together. The relay's store is `OPENCONV_RELAY_DB`, default
`openconv-relay.db` relative to where it starts; a fresh one makes clients
resync (and the relay warns), but a mismatched pair is confusing to debug.

## Architecture

Three pieces, two languages, one FFI seam:

```
  SwiftUI app  ──uniffi──▶  openconv-core (Rust)
  (clients/macos)           MLS group state, encrypt/decrypt, frame tags
       │                          │
       │ WebSocket                └─ openconv-wire: the relay envelope
       ▼                          ┌─ (address, sequence), shared by both
  openconv-server (Rust/Axum) — blind relay, routes by address
```

**The build chain is not obvious and bites if missed.** `crates/core` compiles
to a staticlib. `scripts/gen-bindings.sh` runs `uniffi-bindgen` against that
archive and writes generated Swift into `clients/macos/Sources/OpenConvCore/`
and a C header into `Sources/openconv_coreFFI/include/`. Both are gitignored —
they are build output, not source. **After changing `crates/core/src/ffi.rs`,
run `just bindings` or the Swift side silently builds against stale bindings.**

The theme is the other way round: `scripts/gen-theme.swift` writes
`Sources/OpenConv/Theme/Tokens.swift` from `design/tokens.json`, and that file
*is* checked in, already formatted. Change the tokens and run `just theme`;
never edit the Swift. `just check` and CI fail if the two disagree.

**Encryption is MLS** (RFC 9420, via `openmls`), not Signal. One group state
rather than pairwise sessions per device pair. `Member` in `crates/core/src/lib.rs`
is the whole client protocol surface; `ffi.rs` wraps it in a mutex because
UniFFI objects take `&self`.

**Adds are two-phase, and this is the subtlest thing in the codebase.** A new
member publishes a `KeyPackage`. Any existing member may call `propose_add`,
which *stages* a commit without applying it and holds the `Welcome` back. The
commit goes to the relay. Only when it comes back — meaning the relay ordered
it first and nothing beat it — does `receive` return `Event::Admitted` with
the `Welcome` to send. A member whose commit loses gets `Event::AddSuperseded`
and applies the winner's commit instead.

Without this, two members admitting the same joiner produce competing commits
at one epoch and fork the group, which surfaces as `AEAD decryption failed`
much later. Follow it through `Member::propose_add` and `Member::receive`,
`ChatModel.handle`/`apply`, and `crates/server/tests/relay.rs`.

**The relay never parses payloads**, but it does route and order them. A
client subscribes to addresses and publishes to an address; the relay reads
that envelope (`crates/wire`) and nothing past it. An address is either a
channel — the MLS group id — or a joiner's mailbox, their `KeyPackageRef`,
where their `Welcome` is delivered. The relay cannot tell which.

Each address has its own sequence, numbered and delivered under that
address's lock, and every subscriber sees the same order: that is what
resolves commit races. A publisher subscribed to the address gets its own
frame back, so it learns where the frame landed; clients recognise their own
frames by exact byte match, which keeps sender identity off the wire.

**Catch-up (#105).** The relay stores every frame. A client keeps a cursor per
address it reads (its channel, or its mailbox while waiting to join) in the
same transaction as its MLS state, and on connecting sends `Resume` from each
cursor. The relay answers a page at a time and registers the live
subscription in the same writer step as the last page, so catch-up and live
traffic meet without a gap. `Member::deliver` takes whatever the relay sent,
drops duplicates, applies the frame, and returns `Effect`s for the app; Swift
does not track sequence numbers or dispatch frame kinds. Catch-up serves members who were
away; it does not give a new member history. MLS forward secrecy means a
member cannot read frames from before it joined, however long they are kept.
That is the design, not a bug.

**Membership (#112).** Any member may remove another: a commit staged like an
add, resolved by the relay's order. Leaving is a Remove proposal for oneself
(`FrameKind::Proposal`), committed by any live member; the leaver never
commits pending proposals, and re-makes its request in each new epoch because
a proposal is epoch-bound. A removed member deletes its group state, but
keeps its conversation (see below). A member
stranded below the retention floor drops the group and publishes a
`FrameKind::Rejoin`: a fresh `KeyPackage` plus the key its place holds now,
signed by its identity key. Members re-add it only while that place still
holds that key, so a request is spent once acted on and cannot be replayed.
The app sends nothing until caught up on its channel.

Joining needs a member's approval (#125): a `KeyPackage` frame becomes a
pending `JoinRequest`, and `Member::accept` approves the identity (single use,
cancelled on removal), so an expired request is answered when the next one
arrives. Declines and removals are remembered locally; there is no shared
control message, because the content format is milestone 2's decision.
Re-adds of stranded members stay automatic. Requests are `KeyPackage`s good
for a week, renewed while waiting; members remember what they admitted so an
old request cannot be replayed (#123). Joining skips the lifetime check on
existing places (`skip_lifetime_validation`): a quiet member's place keeps
its `KeyPackage`'s lifetime, and checking it would lock newcomers out. Headless
runs set `OPENCONV_AUTO_ACCEPT=1`; the app never auto-accepts on its own.

Joining: learn the channel id out of band, then `offer(channel)` and send
`connect()`'s requests: `Resume` on the mailbox, then the `KeyPackage`
published to the channel. The order matters: the relay handles a connection's
requests in order, so the mailbox is read before anyone can answer. The
`Welcome` carries the sequence number of the commit that admitted the joiner,
who then reads the channel from just after it. A waiting joiner re-sends its
offer on every reconnect, and members answer requests seen during catch-up
once live, unless the joiner is already a member.

**Message content is MIMI content** (`crates/core/src/content.rs`, pinned to
`draft-ietf-mimi-content-09`): deterministic CBOR, with a message id derived
per §3.3 from provisional sender and room URIs (milestone 3 makes them real).
The sender URI comes from the key MLS authenticated, never from the content.
A resend reuses the original bytes, so it keeps its id. The id derivation is
tested against the draft's own example; change it only with the draft.

**The transcript (#129)** is the `transcript` table in the same store, one
row per channel and message id. A received message is written inside
`receive_unit`, in the transaction that decrypts it and advances the cursor,
so a message that cannot be stored is not decrypted either and is retried. A
sent one is written unplaced when encrypted, and placed when its echo comes
back in the current epoch (not a stale echo, which nobody could read; the
resend keeps the id and so the row). Order is a local `position` assigned as
frames are applied, not the relay's `seq`, which restarts after a relay
reset. Positions come from one counter that only goes up (schema v9), so a
read marker stays past a deleted history. `Member::history` pages by
position. It is kept after removal or leaving and until the user deletes it
(decided in #133).

**Several groups per client (#134).** `Member` holds a map of groups by
channel, each with its own staged commit, request to leave and sent-message
record, and every group-level call takes the channel (`send`, `members`,
`accept`, `leave`, …). The store keys everything about a group by channel
(schema v8), and `Effect`s name the channel they are about. A failed
operation rebuilds every group from the store, which is safe because each
in-memory change is written alongside it; a failure in one group leaves the
others as they were. A `Welcome` is joined only if it is for the channel the
mailbox's offer asked to join, since anyone holding a published `KeyPackage`
could otherwise add its owner to a group of their choosing. Several offers to
different channels wait at once, each on its own mailbox. The Swift app is
still a one-conversation harness; its real UI is #130, built to
`planning/design.md`.

**Who changed what, and notices (#130).** A commit is read before it is
merged (`settle`, beside `note_commit` and `rename_in`) for who it let in and
took out, each credited to the MLS-authenticated sender of the proposal:
`Effect::Membership` with `MemberChange`s, and `Effect::Removed` carries a
`Departure` saying whether this client left or who removed it. A removal
proposed by the removed member is a leave, not a removal by whoever committed
it; a re-add of a stranded member replaces a place and is no change at all.
The same facts, plus renames, "you joined", a reset relay and a stranding,
are written as notices in the `notices` table, in the transaction that
applied them and in the same position sequence as messages, but never as
messages (the design says so). `history` returns `HistoryEntry`: a message
or a notice. Whether a notice names this client is worked out on reading,
from the URI, not stored.

**Conversations (#130)** are the `conversations` table, one row per channel,
outliving the group: the last name and members seen while a member, the
state (member, waiting, rejoining, left at T, removed at T by X) and a read
marker. It is brought up to date whenever group state may have changed (on
joining, after every merged commit, and on opening), so a removed member's
row still has what it showed. `Member::conversations` lists every one with
what a list row needs. `delete_history` keeps the conversation;
`delete_conversation` is refused with `StillInConversation` until this client
has left or been removed. Names reach the screen through `display_name`,
which replaces control and bidirectional characters with U+FFFD rather than
stripping them, so two names never look alike.

**Conversation names (#134)** live in the group context, as a private-use
extension (`NAME_EXTENSION`, 0xF0C0): changed by a `GroupContextExtensions`
commit staged and resolved by the relay's order like any other change, and
delivered to joiners in the `Welcome`. Commits are `PrivateMessage`s, so the
relay never sees a name. Applying a commit that changes the name, one's own or
another member's, yields `Effect::Renamed` after the `Advanced` or
`Superseded` for it, crediting the MLS-authenticated sender of the proposal.
A rename that lost its race is never applied, so never announced, and a
joiner reads the name rather than being told of it. openmls accepts such an extension only if the group
requires every member to support it, so groups are created with a
`RequiredCapabilities` extension, every `KeyPackage` lists the capability,
and a `KeyPackage` without it is refused as a request: adding it would fail
every time, and a held request that fails takes the frame that tried it with
it. Consequences: groups made before #134 cannot be named, and clients built
before it cannot join groups made after.

Frame tags (`FrameKind`) are a client-side concern defined once in Rust and
exported through UniFFI — Swift does not get its own copy.

**State persists in an encrypted SQLite database** (`crates/core/src/store.rs`):
openmls's own SQLite `StorageProvider` (`openmls_sqlite_storage`) over a
SQLCipher connection, key from the macOS Keychain. Every `Member` operation runs
in a savepoint and is rolled back, with the group reloaded from disk, if any
part fails, so a failed operation leaves no trace. The storage crate is
vendored in `vendor/` until upstream releases it on `rusqlite` 0.40; see
`VENDORED.md` there. Do not edit it.

## Traps

**A client that hangs with no output at all is the Keychain.** An unsigned
binary gets a new code identity on every rebuild, so the Keychain raises an ACL
prompt for items an earlier build wrote — invisible headlessly, hangs forever.
Setting `OPENCONV_DATA_DIR` switches to a key file and avoids it. To inspect:
`security find-generic-password -s com.openconv.vault -a <identity>`.

**Xcode is not installed — Command Line Tools only.** No `xcodebuild`, no
`.xcodeproj`, no `-create-xcframework`, and neither XCTest nor swift-testing
can be imported by a SwiftPM test target. This is why `BridgeCheck` is a plain
executable rather than a test target. Do not "fix" that.

**`openmls` 0.9 requires its companion crates at 0.6.x**
(`openmls_rust_crypto`, `openmls_traits`, `openmls_basic_credential`). Pinning
them to 0.4 pulls a second `openmls_traits` and every trait bound fails with
confusing errors.

**A stale relay squatting port 8080** makes protocol failures look like
protocol bugs. `scripts/smoke.sh` clears it; otherwise `lsof -ti:8080`.

**Group behaviour needs three members to test.** Two cannot expose commit
fan-out or concurrent-commit bugs — both were live here and invisible to every
two-member test.

## Conventions

`CONTRIBUTING.md` is the authority: lint policy, error handling, naming,
documentation, and the product rules. Read it before writing code. The points
most often gotten wrong:

- **The build is warning-clean.** Lint policy is `[workspace.lints]`; an
  `#[allow]` needs a comment saying why.
- **Types holding key material implement `Debug` by hand and redact.**
  `Member`, `Vault`, `Provider`, `Database`. A derived `Debug` prints identity
  keys and ratchet state into any log line that formats one.
- **Generated bindings are never formatted or linted.**
- No `print!` in Rust — logging goes through `tracing`.

## Context

`planning/architecture-notes.md` records decisions already taken and the
reasoning — why MLS rather than Signal, where OpenConv sits against Matrix,
why no Erlang/C/asm, retention, unsafe. Read it before reopening any of those;
they were argued, not assumed.

`planning/roadmap.md` is the milestone order and why. Check it before
proposing what to build next — it exists because that decision was previously
made per-session and drifted.

`planning/design.md` is the agreed design for the macOS app (#133): structure,
decisions, and every state and its wording. Token values live in
`design/tokens.json`, and `design/frames/` has PNG snapshots of every agreed
frame (`design/README.md`). Build UI from these, and don't reopen a decision
recorded there without asking.

`planning/v2-design-requirements.md` carries forward findings from an audit of
the previous implementation, tagged by the milestone where each applies.

This is a from-scratch rewrite. The previous version — a Tauri/React client
with libsignal — is archived at tag `archive/v1-final` and branch `archive/v1`,
and is worth consulting only for prior art, never as a pattern to follow.

CI runs everything `just check` does: `cargo fmt --check`, `clippy -D
warnings`, `cargo test`, `cargo deny check`, the theme drift check, the Swift
build, `BridgeCheck`, and the three-client smoke test.

The workflow takes its toolchain from `rust-toolchain.toml`; don't pin a
version in the workflow as well, or the two will drift.
