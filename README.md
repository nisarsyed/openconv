# OpenConv

A privacy-focused, lightweight desktop chat application. Self-hosted, with
end-to-end encryption the server cannot read.

> Rewritten from scratch. The previous implementation is archived at tag
> `archive/v1-final` and branch `archive/v1`.

## Architecture

```
  SwiftUI app  ──uniffi──▶  openconv-core (Rust)
  (macOS)                   MLS group state, encrypt/decrypt
       │
       │ WebSocket, opaque frames
       ▼
  openconv-server (Rust/Axum)
  blind relay — fans out bytes, never parses them
```

- **Encryption is MLS** ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420),
  via [openmls](https://github.com/openmls/openmls)). One group state rather
  than pairwise sessions per device pair.
- **The server is blind.** It relays opaque frames and has no code path that
  inspects a payload. Privacy comes from that plus self-hosting.
- **The client is native.** SwiftUI on macOS, with crypto in a shared Rust core
  rather than reimplemented per platform.

## License

Dual licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option — matching the Rust ecosystem convention and the openmls stack
underneath.

## Requirements

Rust (stable) and Swift 6. Xcode is *not* required — the Command Line Tools
are enough, and the build deliberately avoids `xcodebuild`.

## Try it

Three terminals:

```sh
just relay            # 1: start the relay
just client alice     # 2: click Host
just client bob       # 3: click Join
```

Bob's client publishes a KeyPackage, Alice's admits him and returns a Welcome,
and from there both can chat. Everything on the wire is MLS ciphertext.

## State

Each client keeps its MLS state — identity, group membership, ratchet state —
in a single encrypted file, so a restart resumes where it left off.

- **Encrypted at rest.** XChaCha20-Poly1305, written `0600` via a temp file and
  rename so an interrupted save cannot truncate good state.
- **The data key lives in the macOS Keychain**, not beside the vault.
- **`OPENCONV_DATA_DIR` overrides the location** and switches the key to a file
  inside that directory. That is for development and testing: an unsigned
  binary changes code identity on every rebuild, and the Keychain then raises
  an ACL prompt that is invisible to a headless run and hangs it. Real installs
  are code-signed and use the Keychain.

```sh
# two independent instances, neither touching your real state
OPENCONV_DATA_DIR=/tmp/a just client alice
OPENCONV_DATA_DIR=/tmp/b just client bob
```

## Checks

```sh
just check            # rust tests + swift bridge verification
```

## Layout

| Path | |
|---|---|
| `crates/core` | MLS group state, message encryption, wire framing, UniFFI surface |
| `crates/server` | Blind WebSocket relay |
| `clients/macos` | SwiftUI app and generated bindings |
| `scripts/gen-bindings.sh` | Regenerates Swift bindings from the Rust core |
| `planning/v2-design-requirements.md` | Lessons carried over from the v1 audit |

## Status

Encrypted messaging between several clients in one implicit group, with state
that survives a restart.

Not yet: accounts or identity verification, guilds and channels, message
history, multi-device, and offline delivery.

Two known stopgaps, both marked in the code:

- **Only the host admits new members.** Any member responding to a KeyPackage
  produces competing commits at the same epoch and forks the group. The real
  fix is for the relay to serialise commits the way an MLS delivery service
  does.
- **The whole MLS store is rewritten on every change.** Fine at this size;
  implementing openmls's `StorageProvider` over SQLite is the scaling path.
