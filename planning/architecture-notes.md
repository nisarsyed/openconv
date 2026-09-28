# Architecture notes

Decisions taken and the reasoning behind them, so they are not re-litigated
from memory. `roadmap.md` says what to build next; this says why the shape is
what it is.

Where something is genuinely open, it says so rather than pretending.

---

## Why v2 exists, and what actually changed

v1 was ~50,000 lines: a Tauri/React client, an Axum server, and **libsignal**
for encryption. It was discarded on 2026-09-27 and is archived at
`archive/v1-final`.

The stated reasons were "architecture wrong, implementation messy", but the
specific failure is worth naming because it is the whole reason v2 is
tractable at 2,500 lines.

### Crypto fan-out is the thing that decides the architecture

| Approach | Per message | On membership change |
|---|---|---|
| Pairwise Signal (Double Ratchet per pair) | **O(N)** ciphertexts — one per recipient *device* | O(N) |
| Sender Keys (WhatsApp/Signal groups) | O(1) ciphertext | **O(N)** key redistribution |
| **MLS** (RFC 9420, what v2 uses) | O(1) ciphertext | **O(log N)** via the ratchet tree |

v1 used pairwise Signal sessions. In a group of N members with M devices each,
every message meant N×M encryptions and N×M copies fanned out. The v1 commit
log is a record of that pain — `scope pre-key bundles to a device`,
`make Signal device ids server-assigned`, `deliver messages to the sender's own
other devices`. None of those were bugs. They were the architecture asserting
itself.

MLS collapses it to one ciphertext plus a logarithmic tree operation. That is
why the v2 relay is 143 lines and why multi-device is a deliberate later
milestone rather than an existential threat.

**Corollary worth remembering:** switching from libsignal (AGPL-3.0) to
openmls (MIT) is also what made the project licence a free choice. See
`deny.toml`.

---

## The three families of messaging architecture

Useful frame for deciding what OpenConv is. They differ mainly in **who is
allowed to read messages**, and that determines nearly everything else.

**1. Store-and-forward** — WhatsApp, Signal. The server holds ciphertext until
delivered, then deletes. A queue, not an archive. History lives on the device.
**This is the family OpenConv is in**, and milestone 1 is essentially building
it.

**2. Server-as-source-of-truth** — Discord, Slack, Telegram cloud chats.
Everything stored permanently in the clear, sharded by channel id, with
fan-out on read so one write serves a thousand readers. Buys search, full
history, new-device history, web clients, moderation. Costs: the server must
be trusted completely.

**3. Federated** — Matrix, XMPP. Many servers speaking a protocol; Matrix
replicates rooms as event DAGs with state resolution. The hardest of the three
to implement correctly.

OpenConv is **(1) with (2)'s product shape**, which is a real tension and the
source of most of the hard questions below.

---

## Discord-level functionality under E2E

Most of it is achievable. Three things genuinely fight the architecture.

**Achievable:** search (client-side index over what you decrypted — v1 did this
with SQLite FTS5), channels and roles (membership operations, not content),
kick/ban (MLS `Remove`), notifications (contentless push, client fetches and
decrypts), multi-device (each device an MLS leaf), link previews (client
fetches, which leaks to the linked site).

**The three walls:**

- **History for new members.** MLS forward secrecy means a member cannot
  decrypt epochs from before they joined. Solvable by deliberately sharing a
  per-channel history key, as Matrix does with room history visibility — but
  that is *choosing* to weaken forward secrecy, and should be explicit and
  per-channel, never silent.
- **Moderation.** You cannot moderate what you cannot read. The known technique
  is **message franking**: a reporter submits plaintext plus cryptographic
  proof the sender really sent it, so reports cannot be forged. Reactive only,
  never proactive filtering.
- **Scale.** MLS targets groups up to roughly thousands; every member processes
  every commit, so a large group with churn becomes a commit storm. A
  100,000-member public community is not an MLS group.

### The way out: tier the trust model

Private spaces (DMs, small channels, teams) get E2E. Large public communities
get a server-visible mode with search, instant history and moderation — a
50,000-person public server has no real privacy expectation anyway.

Matrix does exactly this; encryption is per-room and optional. The critical
part is being **explicit in the UI about which mode a space is in**, so the
guarantee is never ambiguous.

**This requires no architectural change now.** A blind relay is the permissive
choice: you can always add a tier that reads messages, but you cannot retrofit
blindness onto a server that already reads them. The channel addressing in
\#103 is the natural seam — a channel gets a mode.

---

## Versus Matrix and Element

Researched 2026-09-29.

### Where OpenConv could genuinely be better

- **Already on MLS.** Matrix is still on Olm/Megolm; MLS is an ongoing
  initiative there, not shipped. OpenConv has no migration debt.
- **Radical simplicity.** 143-line relay, single static binary, no runtime, no
  federation machinery, no cloud push. Element themselves acknowledge Synapse's
  weight — they demonstrated a ~100x database reduction and have not had time
  to ship it.
- **Air-gapped deployment** is arguably the strongest axis. Matrix does it (the
  UN runs air-gapped Matrix) but carries federation machinery it does not need
  there. OpenConv is natively what theirs must be cut down to.

### Where it will not win

- **Decentralisation.** OpenConv is one self-hosted relay per community. Matrix
  is federated. Different products — and self-hostable is not decentralised.
  Conflating the two in any pitch would be dishonest.
- **Institutional trust.** Matrix is in conversation with ~35 countries and has
  the UN. Sovereignty buyers need audits, certifications (BSI C5, Common
  Criteria), support contracts, liability, decade-long maintenance. That is an
  organisation, not a codebase, and it is the actual moat.
- **Ecosystem and mobile.** Clients everywhere, bridges, ten years of spec.
  OpenConv has one unsigned macOS app.

### On "making a standard better than Matrix"

The wrong frame, because **the standard already exists and it is not Matrix's**.

- **MLS is RFC 9420**, an IETF standard. OpenConv is already on it.
- **MIMI** (More Instant Messaging Interoperability) is the IETF working group
  standardising interoperable messaging *on top of MLS*. Protocol draft
  published April 2026, with Matrix Foundation people co-authoring.

So Matrix is converging on the standard OpenConv already uses. The achievable
ambition is not "invent a rival" — it is **be an excellent MIMI/MLS
implementation**: radically simpler, Rust, air-gap-native. That is
implementable by a small team, puts you on the standards track rather than
against it, and makes you eventually interoperable with Matrix rather than a
competitor.

**Open action:** read `draft-ietf-mimi-protocol` and `draft-ietf-mimi-arch`
before finalising channel addressing in #103. If alignment is wanted, the time
to learn what MIMI says about room addressing and identity is before the wire
format is set.

Sources: [MIMI protocol draft](https://datatracker.ietf.org/doc/html/draft-ietf-mimi-protocol-06),
[MIMI architecture](https://datatracker.ietf.org/doc/html/draft-ietf-mimi-arch-02),
[Matrix in government](https://www.theregister.com/on-prem/2026/02/09/matrix-messaging-gaining-ground-in-government-it/4663932),
[matrix.org](https://matrix.org/category/general/).

---

## Language and stack decisions

Three separate questions, all answered the same way and for the same reason:
**there is a profile, and it points at an algorithm.**

```
send/receive in-memory     85 µs/msg
send/receive persisted    558 µs/msg     ← 6.5x
vault at  2 members        26 KiB
vault at 50 members       197 KiB
```

That 6.5x is the MLS store being rewritten whole on every message
(`crates/core/examples/measure.rs`). It is an algorithm problem, tracked as
\#107. No language change touches it.

### Erlang / BEAM — no

WhatsApp needed Erlang because they ran **one service for billions**; all
concurrency concentrated in one place. OpenConv runs **one relay per
community** — the load never concentrates, so the scale problem BEAM solves
never arises. BEAM is also worse for crypto (drops to C NIFs anyway), and
adding a runtime would damage the single-static-binary advantage that is the
strongest differentiator against Synapse.

Would make sense only if OpenConv became a hosted multi-tenant service with
uptime obligations, where supervision trees and hot code reload are genuinely
compelling and Rust has no good answer.

### C — no

Rust and C share the LLVM backend and emit comparable code. Rust emits
`noalias` by default, which gives the optimiser *more* to work with than C,
where pointers are assumed to alias. "Rewrite in C for speed" was a 2005
strategy.

### Assembly — already there

The crypto stack is `libcrux` (Cryspen, HACL*/F* lineage): **formally verified**
implementations with SIMD intrinsics (`libcrux-intrinsics`) and runtime CPU
feature dispatch (`libcrux-platform`). MLS operations already execute
assembly-grade code written by people who prove it constant-time. Hand-writing
there trades a proof for a hazard, and hand-rolled crypto is the canonical
catastrophic mistake.

---

## Unsafe policy

`unsafe_code` is **`deny`, not `forbid`** — deliberately reconsidered
2026-09-29.

`forbid` cannot be overridden anywhere, which is a wall rather than a high bar.
Unsafe is a tool and FFI, SIMD and some zero-copy work genuinely need it. The
opt-in costs a `#[allow(unsafe_code)]` with a reason, a `// SAFETY:` comment on
every block, one unsafe operation per block, and no implicit unsafe inside an
`unsafe fn`.

Reach for it when it is **measured**, not assumed. There is still zero unsafe
in the tree. Full reasoning in `CONTRIBUTING.md`.

---

## Retention

**30-day window plus a per-channel size cap, operator-configurable.** Decided
2026-09-28; full reasoning in `roadmap.md` under milestone 1.

The short version: acknowledgement-based retention is the design that first
suggests itself and it is wrong here, because computing "everyone has received
it" requires the relay to know channel membership — which it does not and
should not. And MLS forward secrecy caps what retention can buy anyway, since a
new member cannot decrypt old frames however long they were kept. So the number
answers exactly one question: how long may someone be offline and still catch
up.

---

## Still open

- **MIMI alignment** — read the drafts before #103 sets the wire format.
- **Tiered trust model** — if large public communities are ever a goal, the
  private/public split is a product decision, not an architectural one. Nothing
  now forecloses it.
- **Federation** — deliberately deferred. Nothing above assumes it; nothing
  above prevents it.
- **Leaving a group.** The client core exposes `propose_add` and no removal at
  all. Once you are in a group you cannot leave and cannot be removed. A real
  protocol hole, noticed 2026-09-28, not yet tracked as an issue.
