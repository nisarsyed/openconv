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

**The join handshake spans three files** and is hard to follow from any one of
them. A new member publishes a `KeyPackage`; an existing member calls
`add_member`, which returns *both* a `Welcome` (for the joiner) and a `commit`
(for members already present). Both must be sent — dropping the commit strands
existing members in the old epoch. See `ChatModel.handle` in
`clients/macos/Sources/OpenConv/ChatModel.swift`, `Member::add_member`, and
`crates/server/tests/relay.rs` for the same flow in test form.

**Only the host admits new members.** This is a stopgap, marked in `ChatModel`.
Every joined member responding to a `KeyPackage` produces competing commits at
the same epoch and forks the group. The real fix is for the relay to serialise
commits the way an MLS delivery service does.

**The relay never parses payloads.** `crates/server` moves opaque bytes and has
no code path that inspects one. Frame tags (`FrameKind`) are a client-side
concern defined once in Rust and exported through UniFFI — Swift does not get
its own copy.

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

`planning/v2-design-requirements.md` carries forward findings from an audit of
the previous implementation, tagged by the milestone where each applies.

This is a from-scratch rewrite. The previous version — a Tauri/React client
with libsignal — is archived at tag `archive/v1-final` and branch `archive/v1`,
and is worth consulting only for prior art, never as a pattern to follow.

There is no CI, so nothing above is mechanically enforced. Run `just check`.
