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
  inspects a payload. Its one semantic job is ordering: every frame gets a
  sequence number and all clients see the same order, which is what decides a
  race when two members commit at the same epoch — without the relay ever
  knowing which frames are commits.
- **The client is native.** SwiftUI on macOS, with crypto in a shared Rust core
  rather than reimplemented per platform.

## License

[MIT](LICENSE).

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
in an encrypted SQLite database, so a restart resumes where it left off.

The relay keeps every frame, so a client that was closed catches up on what
it missed when it reconnects: messages, membership changes, and requests to
join that arrived while nobody was online to answer. A new member does not
get the history from before they joined, and cannot: MLS forward secrecy
means those frames were never encrypted for them.

- **Encrypted at rest.** SQLCipher encrypts and authenticates every page, and
  the files are `0600`. Each operation is one transaction, so a crash or an
  error cannot leave half an update on disk.
- **The data key lives in the macOS Keychain**, not beside the database.
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

**Relay and client data belong together.** Each client remembers how far it
has read the relay's history, and the relay keeps that history in
`OPENCONV_RELAY_DB`, which defaults to `openconv-relay.db` in the directory the
relay starts in (`just relay` always uses the repo root). If the relay starts
with a fresh store, clients find themselves ahead of it and follow its new
numbering from the start, saying so; the relay logs a warning whenever it
creates a new store. For throwaway runs, point both somewhere disposable and
reset them together:

```sh
OPENCONV_RELAY_DB=/tmp/run/relay.db just relay
OPENCONV_DATA_DIR=/tmp/run just client alice
```

## Retention

The relay keeps frames so members who were offline can catch up, and no
longer than that: **30 days, and at most 64 MiB per channel**, whichever is hit
first. Set `OPENCONV_RETENTION` (such as `7d` or `24h`) and
`OPENCONV_CHANNEL_CAP` (such as `16MiB`) to change them; a malformed value
stops the relay at startup. Lowering either costs exactly one thing: how long
a member can be away and still catch up. It never costs new members anything,
because MLS forward secrecy means they cannot read frames from before they
joined however long those are kept. On a busy channel the size cap can bite
before the time window, and then it is what sets that limit. A member away
longer than the limit is told it cannot catch up, rather than being shown a
history with holes in it. It then asks to be re-added, and the first member
online does so automatically; only that member can make the request, and it
works once.

**Removing someone is not a ban yet.** Anyone with a channel's id can ask to
join, and members let them in automatically, so a removed member who still
has the id can ask again and be back in. Removal is for someone leaving, a
lost or compromised device, and members stranded by retention; it is not
moderation until join requests need a member's approval (#125). Join requests
are only good for an hour, so an old one cannot be replayed to put back
someone who left (#123).

**Retention limits history, not abuse.** Until identity exists (milestone 3),
anyone who can reach the relay can publish to any address, including made-up
ones, and a per-channel cap does nothing against frames spread across
thousands of them. Two limits bound that: **4 GiB of frames in all**
(`OPENCONV_RELAY_CAP`), over which the oldest frames anywhere go first, and **1
MiB per message** (`OPENCONV_MAX_FRAME`), over which the connection is closed.
The first keeps the disk from filling, which would stop the relay storing
anything; its cost is that a flood can push out real history, shortening the
offline window for everyone. Neither stops a determined flood, so a relay
reachable from the open internet should sit behind something that limits who
can connect until there are accounts.

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

## Where this is going

`planning/roadmap.md` has the milestone order and the reasoning. The short
version: continuous integration, then giving the relay addressing and memory
so messages survive being offline, then history, then identity.

## Status

Encrypted messaging between several clients in one implicit group, with state
that survives a restart.

Not yet: accounts or identity verification, guilds and channels, message
history, multi-device, and offline delivery.

Any member can admit a new one. Adds are two-phase: a member stages a commit
and holds the `Welcome` back until the relay's ordering says the commit won.
A member whose commit loses applies the winner's instead.

One known stopgap, marked in the code: **the whole MLS store is rewritten on
every change.** Fine at this size; implementing openmls's `StorageProvider`
over SQLite is the scaling path.
