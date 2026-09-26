# v2 design requirements

Carried-over findings from the v1 audit (85 issues under tracking issue #10,
`audit` label, filed against d6e798b — now on `archive/v1`).

Most of those issues describe v1 code that no longer exists. The items below
are the ones that describe a *mistake worth not repeating*, restated against
the v2 architecture: self-hosted blind relay, MLS group crypto, Rust core with
a SwiftUI client.

Each is tagged with the milestone where it first bites, so it can be checked
at the point it becomes real rather than tracked as open work now.

## Resolved by the architecture change

- **libsignal's AGPL obligation.** v1 took AGPL-3.0 because libsignal is
  AGPL-3.0. The whole openmls stack is MIT, so the project license is now a
  free choice. The workspace currently says `AGPL-3.0-only` as a placeholder
  and should be decided deliberately.
- **Per-device session sprawl.** `sender_device_id` foreign keys, pre-key
  bundles scoped per device, and fan-out to a sender's own devices were all
  artifacts of pairwise Signal sessions. MLS keeps one group state, so this
  class of bug is gone rather than fixed. (The general lesson — every FK gets
  an explicit `ON DELETE` — still applies once a schema exists.)

## Crypto layer — applies now

- **Never destroy key state on a decryption failure.** A MAC failure is the
  exact signal an attacker wants to use to knock a client out of a session.
  `Member::receive` returns an error and leaves group state untouched; keep it
  that way. Any future "recover from bad state" path must be explicit and
  user-visible, never automatic on failure.
- **Validate KeyPackages before trusting them.** `add_member` runs
  `KeyPackageIn::validate` (signature, lifetime, protocol version) rather than
  accepting the deserialized form. A KeyPackage arrives from the network and is
  hostile input.

## Relay — applies at the next milestone

- **Do not trust `X-Forwarded-For` for rate limiting.** Behind no proxy it is
  attacker-controlled, so it must only be honoured from a configured trusted
  proxy list; otherwise use the socket peer address.
- **Revoke live WebSockets on logout, kick, or ban.** v1 checked authorization
  at connect time only, so an already-open socket kept receiving traffic. The
  connection registry needs to be addressable and closable out of band.
- **Ack with the client's nonce.** Delivery confirmation has to identify *which*
  message it confirms, or clients cannot distinguish delivered from dropped.
- **Order by a commit-ordered sequence, not timestamps.** Sync and replay
  cursors need a monotonic server sequence. MLS epochs give a natural ordering
  for handshake traffic; application traffic needs its own relay sequence.

## Auth — applies when accounts arrive

- **Domain-separate challenge signatures.** A login challenge signature must be
  bound to its purpose, or a signature obtained in one context can be replayed
  in another.
- **Check-and-decrement recovery codes atomically.** A non-atomic read-then-write
  lets a code be redeemed twice under concurrency.
- **Validate role position on update**, so a user cannot grant themselves a role
  above their own.

## CI — applies when workflows return

- **No untrusted interpolation in workflow `run:` blocks.** v1's `release.yml`
  interpolated attacker-influenced values directly into shell. Pass them through
  `env:` and quote.
