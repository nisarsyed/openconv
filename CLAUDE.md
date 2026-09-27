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
just client alice       # launch a client (then click Host or Join in the UI)
```

Single test: `cargo test -p openconv-core group_state_survives_a_restart`

One test is `#[ignore]`d because it touches the real macOS Keychain:
`cargo test -p openconv-core -- --ignored keychain`

Two clients side by side without sharing state:
`OPENCONV_DATA_DIR=/tmp/a just client alice`

## Architecture

Three pieces, two languages, one FFI seam:

```
  SwiftUI app  ──uniffi──▶  openconv-core (Rust)
  (clients/macos)           MLS group state, encrypt/decrypt, wire framing
       │
       │ WebSocket, opaque frames
       ▼
  openconv-server (Rust/Axum) — blind relay
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

**The relay never parses payloads**, but it does order them. `crates/server`
moves opaque bytes and prepends an 8-byte sequence number without looking past
it. Every client sees the same order, which is what resolves commit races.
Frames are echoed back to their sender too, so a sender learns where its own
frame landed; clients recognise their own frames by exact byte match, which
keeps sender identity off the wire.

Frame tags (`FrameKind`) are a client-side concern defined once in Rust and
exported through UniFFI — Swift does not get its own copy.

**State persists in an encrypted vault** (`crates/core/src/store.rs`): the whole
MLS store snapshotted, XChaCha20-Poly1305, key from the macOS Keychain. The
`StorageProvider` trait has 72 methods, which is why this snapshots rather than
implementing it over SQLite.

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
  `Member`, `Vault`, `Provider`, `Snapshot`. A derived `Debug` prints identity
  keys and ratchet state into any log line that formats one.
- **Generated bindings are never formatted or linted.**
- No `print!` in Rust — logging goes through `tracing`.

## Context

`planning/roadmap.md` is the milestone order and why. Check it before
proposing what to build next — it exists because that decision was previously
made per-session and drifted.

`planning/v2-design-requirements.md` carries forward findings from an audit of
the previous implementation, tagged by the milestone where each applies.

This is a from-scratch rewrite. The previous version — a Tauri/React client
with libsignal — is archived at tag `archive/v1-final` and branch `archive/v1`,
and is worth consulting only for prior art, never as a pattern to follow.

CI runs the Rust half — `cargo fmt --check`, `clippy -D warnings`, and
`cargo test` — on every push and pull request. The Swift build and
`BridgeCheck` are not in CI yet (#95), nor is the smoke test (#97), so
`just check` locally still covers more than CI does.

The workflow takes its toolchain from `rust-toolchain.toml`; don't pin a
version in the workflow as well, or the two will drift.
