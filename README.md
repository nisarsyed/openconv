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

Thinnest vertical slice: two clients, one implicit group, encrypted messages
end to end. No accounts, guilds, channels, persistence, or multi-device yet.
