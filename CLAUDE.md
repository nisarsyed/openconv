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
a proposal is epoch-bound. A removed member deletes its group state. A member
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

`planning/v2-design-requirements.md` carries forward findings from an audit of
the previous implementation, tagged by the milestone where each applies.

This is a from-scratch rewrite. The previous version — a Tauri/React client
with libsignal — is archived at tag `archive/v1-final` and branch `archive/v1`,
and is worth consulting only for prior art, never as a pattern to follow.

CI runs everything `just check` does: `cargo fmt --check`, `clippy -D
warnings`, `cargo test`, `cargo deny check`, the Swift build, `BridgeCheck`,
and the three-client smoke test.

The workflow takes its toolchain from `rust-toolchain.toml`; don't pin a
version in the workflow as well, or the two will drift.
