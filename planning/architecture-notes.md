# Architecture notes

Decisions taken and the reasoning behind them, so they are not re-litigated
from memory. `roadmap.md` says what to build next; this says why the shape is
what it is.

Where something is genuinely open, it says so rather than pretending.

Every factual claim below was re-checked on 2026-09-29 — against the code, the
dependency tree, re-run measurements, and primary sources. Several were wrong
and have been corrected in place; the corrections are listed at the end so the
mistakes stay visible.

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
| **MLS** (RFC 9420, what v2 uses) | O(1) ciphertext | **O(log N)** at best, O(N) at worst |

v1 used pairwise Signal sessions only — it has a sender-key store because
libsignal's store traits require one, but nothing calls `group_encrypt`. In a
group of N members with M devices each, every message meant N×M encryptions
and N×M copies fanned out. The v1 commit log is a record of that pain —
`scope pre-key bundles to a device`, `make Signal device ids server-assigned`,
`deliver messages to the sender's own other devices`. None of those were bugs.
They were the architecture asserting itself.

MLS collapses the per-message cost to one ciphertext. That is why the v2 relay
is 143 lines and why multi-device is a deliberate later milestone rather than
an existential threat.

**The O(log N) is a best case, and it is not the case OpenConv is in today.** A
commit encrypts to the resolution of each copath node, and that resolution is
only small when the tree is full. Blank and unmerged nodes widen it. Today only
the group's creator ever commits and nobody sends updates, so the tree stays
sparse. Measured commit size is linear: 681 B at 2 members, 4,795 B at 50,
about 84 B per member (`crates/core/examples/measure.rs`). A Welcome carries
the whole ratchet tree (`use_ratchet_tree_extension(true)`), so it is O(N) by
construction. The logarithmic figure needs members to commit updates
periodically. That is an operational policy, and v2 does not have one yet.

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

## What "blind relay" actually means

The phrase is used throughout this repo and it needs to be precise, because it
is easy to believe it means more than it does.

**The relay is content-blind by construction and metadata-blind by choice.**
It cannot read message content. But everything below crosses it in the clear,
and it declines to look rather than being unable to:

- The `FrameKind` tag on every frame, which says outright whether a frame is a
  KeyPackage, Welcome, Commit or Application message.
- The MLS `group_id`, `epoch` and `content_type` fields. RFC 9420 leaves these
  unencrypted in every `PrivateMessage`, commits included.
- The credential in every published KeyPackage. Today that is a
  `BasicCredential` carrying the member's display name.
- The `KeyPackageRef` in each Welcome, which links a joiner to the group they
  are joining.

What the design does hide is who sent an application message: sender data is
encrypted, and clients recognise their own echoes by byte match. It also hides
who is in the group, because commits are `PrivateMessage` (the openmls default
`PURE_CIPHERTEXT` wire-format policy), so the relay never sees the tree.

Two consequences. The privacy argument rests partly on relay code staying
disinterested, not only on cryptography. And **a channel id does not have to
be the thing that hides channel identity**, because the MLS `group_id` is
already on every frame. See #103 under MIMI below.

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
  decrypt epochs from before they joined. Solvable by existing members
  deliberately sharing history. MIMI models this as a per-room policy (who may
  share, and how far back), and Matrix shipped it for encrypted rooms only in
  2026 (MSC4268, spec v1.19), off by default for new rooms. Sharing history
  means *choosing* to weaken forward secrecy (RFC 9750 §8.2.2), so it should be
  explicit and per-channel, never silent.
- **Moderation.** You cannot moderate what you cannot read. The known technique
  is **message franking**: a reporter submits plaintext plus cryptographic
  proof the sender really sent it, so reports cannot be forged. Reactive only,
  never proactive filtering. Core MLS defines none. MIMI specifies it in
  `draft-ietf-mimi-protocol` §5.4.1, with abuse reports to the hub in §5.9.
- **Scale.** Every member processes every commit, so a large group with churn
  becomes a commit storm. RFC 9420 says "two to thousands"; RFC 9750 aims at
  tens of thousands. A 100,000-member public community is not one MLS group,
  and the reason is the commit storm, not a limit in the spec.

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

Researched 2026-09-29, and re-checked the same day against primary sources.

### Where OpenConv could genuinely be better

- **Already on MLS.** Matrix is still on Olm/Megolm. MLS there is open MSCs
  (MSC2883, and BWI's MSC4256) and experiments (DMLS, Project Tachyon). No
  Matrix client ships it. OpenConv has no migration debt.
- **Radical simplicity.** 143-line relay, single static binary, no runtime, no
  federation machinery, no cloud push. The Matrix.org Foundation's December
  2025 review says Element spiked a ~100x reduction in Synapse's database usage
  and has been too busy to ship it.
- **Air-gapped deployment** is arguably the strongest axis. Matrix is deployed
  in isolated government settings, but carries federation machinery it does
  not need there. OpenConv is natively what theirs must be cut down to.
  (The notes previously said "the UN runs air-gapped Matrix". The only source
  for "air-gapped" is The Register paraphrasing Matthew Hodgson. The primary
  source is UNICC self-hosting Element and federating with partner
  organisations, which is not air-gapped.)

### Where it will not win

- **Decentralisation.** OpenConv is one self-hosted relay per community. Matrix
  is federated. Different products — and self-hostable is not decentralised.
  Conflating the two in any pitch would be dishonest.
- **Institutional trust.** Per Hodgson, Matrix is talking to about 35
  countries. The Foundation's own count is 25+ actively deploying. Sovereignty
  buyers need audits, formal approvals, support contracts, liability and
  decade-long maintenance. In German government that means BSI approval up to
  VS-NfD, which the Matrix-based BwMessenger has held since 2021. BSI C5 is an
  attestation for cloud services, not a certification, and does not apply to
  self-hosted software. That is an organisation, not a codebase, and it is the
  actual moat.
- **Ecosystem and mobile.** Clients everywhere, bridges, ten years of spec.
  OpenConv has one unsigned macOS app.

### On "making a standard better than Matrix"

The wrong frame, because **the standard already exists and it is not Matrix's**.

- **MLS is RFC 9420**, an IETF standard. OpenConv is already on it.
- **MIMI** (More Instant Messaging Interoperability) is the IETF working group
  standardising interoperable messaging *on top of MLS*.
  `draft-ietf-mimi-protocol-06` was published 25 April 2026. Two of its six
  authors are listed under the Matrix.org Foundation.

So Matrix is converging on the standard OpenConv already uses. The drafts have
now been read (next section), and they sharpen this: being "an excellent MIMI
implementation" in full would mean giving up the blind relay. What is
achievable without that is **alignment at the MLS layer**, so a future
interop mode is additive rather than a rewrite.

---

## MIMI, read

Read 2026-09-29: `draft-ietf-mimi-arch-03` (6 July 2026),
`draft-ietf-mimi-protocol-06` (25 April 2026), `draft-ietf-mimi-room-policy-04`
(6 July 2026), `draft-ietf-mimi-content-09` (4 July 2026). All are drafts and
will move.

### Two findings that decide everything else

**1. MIMI does not govern the client↔relay protocol.** It standardises
server-to-server interaction and what goes inside the MLS group. The
architecture draft (§5) puts "client-server interactions internal to a
provider" out of scope. The room-policy draft lists "how group IDs are
constructed" as not relevant to MIMI. OpenConv's wire format, including
everything #103 and #105 decide, is ours to choose.

**2. A MIMI hub is not a blind relay.** It is content-blind only. The hub
keeps the GroupInfo and ratchet tree, "manages the list of group members",
stores the participant list and room metadata (name included), and checks
every commit against room policy before fanning it out. That is why MIMI
requires handshake messages to be `PublicMessage`, or `SemiPrivateMessage`
encrypted to the members plus the hub (which openmls 0.9 does not
implement). Even MIMI's "minimal metadata rooms" only replace identities with
pseudonyms; the hub still sees the membership structure. MIMI servers are
also MLS external senders, with keys inside the group.

**So OpenConv cannot be a MIMI hub without abandoning the blind-relay rule.**
Interop, if it is ever wanted, is a per-channel mode — structurally the same
decision as the public tier above. A channel whose relay participates in MIMI
is a channel whose server sees membership, and the UI should say so.

### What MIMI already solves that OpenConv had not

- **Epoch races on application messages.** The hub rejects an application
  message from a stale epoch with `epochTooOld` plus the current epoch, and
  the sender re-encrypts. Room policy carries an explicit `epoch_tolerance`.
  OpenConv now does the blind equivalent of the first (#110): the sender
  notices its own echo landing behind a commit and resends. It deliberately
  does not do the second — see "Messages racing a commit" below.
- **Explicit accept/reject for commits.** `wrongEpoch` / `notAllowed`
  responses, rather than inferring a win from an echo. OpenConv's echo-match
  is the blind equivalent and remains the right choice for a relay that does
  not parse.
- **Leaving.** The leaving client sends `Remove`/`SelfRemove` proposals, and
  the next committer must include them. `SelfRemove` is in openmls 0.9 behind
  the `extensions-draft` feature.
- **Duplicates.** Receivers must tolerate their own messages coming back and
  byte-identical redelivery. OpenConv already does the first.

### Cheap alignment worth taking

Each of these is a choice that has to be made anyway. Making it MIMI's way
costs nothing now, and retrofitting it later would not be cheap.

- **Channel id = MLS group id.** One room is one MLS group in MIMI, and the
  group id is already on every frame in the clear. A separate channel id
  would add a mapping without hiding anything. It must still be random, per
  #103. MIMI's example `mimi://hub/g/clubhouse` URIs are illustrative; the
  construction is a provider choice.
- **KeyPackages and Welcomes are not channel traffic.** MIMI serves
  KeyPackages from a per-user directory and routes a Welcome by the
  `KeyPackageRef` it contains. A joiner is by definition not yet subscribed to
  the channel they are joining, so #103 needs an addressing path that isn't a
  channel.
- **An accepted timestamp alongside the sequence.** MIMI content requires the
  hub's acceptance time to reach clients for display ordering. The relay
  already knows it without parsing anything, and #106's time window needs it
  stored anyway. Delivery order stays the sequence number, not the timestamp,
  consistent with `v2-design-requirements.md`.
- **Message content format.** MIMI content (CBOR, message ids derived from
  sender and room URIs, replies, edits, reactions) is the most interop-relevant
  thing inside the encryption. Milestone 2 defines the transcript and is the
  natural point to adopt it or deliberately not.
- **Credentials.** MIMI identifies users and clients by URI. Milestone 3
  replaces `BasicCredential` and should pick a form that can carry one.

### What MIMI does not help with

Cursors, catch-up and retention (#104–#106). MIMI's delivery between servers
is push with retries and byte-exact deduplication. Client sync is the
provider's business. Its message-expiry and history policies are client-side
behaviour, not server retention.

Sources: [arch-03](https://www.ietf.org/archive/id/draft-ietf-mimi-arch-03.txt),
[protocol-06](https://www.ietf.org/archive/id/draft-ietf-mimi-protocol-06.txt),
[room-policy-04](https://www.ietf.org/archive/id/draft-ietf-mimi-room-policy-04.txt),
[content-09](https://www.ietf.org/archive/id/draft-ietf-mimi-content-09.txt),
[RFC 9750](https://www.rfc-editor.org/rfc/rfc9750),
[Matrix holiday special 2025](https://matrix.org/blog/2025/12/24/matrix-holiday-special/),
[Matrix in government](https://www.theregister.com/on-prem/2026/02/09/matrix-messaging-gaining-ground-in-government-it/4663932),
[UNICC and Element](https://element.io/blog/unicc-selects-element-for-secure-communications/),
[Element history sharing](https://element.io/blog/seamless-encrypted-history-sharing-arrives-in-element/),
[BSI C5](https://www.bsi.bund.de/EN/Themen/Unternehmen-und-Organisationen/Informationen-und-Empfehlungen/Empfehlungen-nach-Angriffszielen/Cloud-Computing/Kriterienkatalog-C5/C5_Einfuehrung/C5_Einfuehrung_node.html).

---

## Language and stack decisions

Three separate questions, all answered the same way and for the same reason:
**there is a profile, and it points at an algorithm.**

```
send/receive in-memory            77 µs/msg
send/receive persisted           574 µs/msg     ← 7.4x
persisted send at  2 members     0.41 ms
persisted send at 50 members     1.21 ms
vault at  2 members               26 KiB
vault at 50 members              197 KiB
```

Re-run 2026-09-29 (first run recorded 85 µs, 558 µs, 6.5x; one sample each, so
treat as orders of magnitude). The overhead is the MLS store being rewritten
whole on every message (`crates/core/examples/measure.rs`). It is an
algorithm problem, tracked as #107. No language change touches it.

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

Rust and C share the LLVM backend and emit comparable code. Rust marks
function arguments `noalias` in optimised builds: `&T` when `T` has no
`UnsafeCell`, and `&mut T` when `T: Unpin`. That gives the optimiser *more* to
work with than C, where pointers are assumed to alias unless declared
`restrict`. "Rewrite in C for speed" was a 2005 strategy.

### Assembly — no, but not for the reason previously given

The notes used to say MLS runs on `libcrux`'s formally verified code. **It
does not.** `libcrux-*` crates appear in `Cargo.lock`, but the only one
compiled is `libcrux-sha3`. `hpke-rs` uses it for X-Wing and ML-KEM key
derivation, which our X25519 ciphersuite never reaches.

What actually runs is RustCrypto, via `openmls_rust_crypto`: `aes-gcm`,
`x25519-dalek`, `ed25519-dalek`, `sha2`, `hkdf`. These are constant-time by
design and have been externally audited. They are not formally verified.

On x86_64 (the relay, CI) they detect AES-NI and carry-less multiply at
runtime. **On Apple Silicon — the only client platform — they do not use the
hardware.** The ARMv8 backends in `aes`, `polyval` and `sha2` are opt-in:
`--cfg aes_armv8`, `--cfg polyval_armv8`, and sha2's `asm` feature. None is
set, so the client runs portable software AES, GHASH and SHA-256. Setting the
flags is a one-line change in `.cargo/config.toml` and could be tried now. It
is not the bottleneck, though: persistence is (see the numbers above).

The conclusion stands: hand-rolled crypto is the canonical catastrophic
mistake, and the hardware path already exists behind a flag. If formal
verification matters, the move is `openmls_libcrux_crypto`, a drop-in provider
from the same project. That is a dependency swap, not assembly.

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

**Caveat found on re-checking: evicting a commit is not like evicting a
message.** A member who misses an application message loses that message. A
member who misses a commit can never process anything after it, because every
later frame belongs to an epoch they cannot reach. A size cap can evict
commits well inside the 30-day window, so on a busy channel the real catch-up
window is whichever of the two limits is hit first. Beyond it, a returning
member is stranded, not merely behind. So #105/#106 need two things:

- the relay tells a client whose cursor is below the retention floor, since it
  knows this without parsing anything;
- a recovery path: another member removes and re-adds the stranded one. openmls
  has helpers for this behind its `fork-resolution` feature.

---

## Still open

- **MIMI interop** — the drafts are read; see above. MLS-layer alignment is
  cheap and recommended. Participating as a MIMI hub or follower is a
  per-channel trust-mode decision, not something to build toward now.
- **Tiered trust model** — if large public communities are ever a goal, the
  private/public split is a product decision, not an architectural one. Nothing
  now forecloses it.
- **Federation** — deliberately deferred. Nothing above assumes it; nothing
  above prevents it.
- **Leaving a group.** The client core exposes `propose_add` and no removal at
  all. Once you are in a group you cannot leave and cannot be removed. A real
  protocol hole, noticed 2026-09-28, not yet tracked as an issue. MIMI's leave
  flow (`SelfRemove` proposals, committed by the next committer) is the model.

---

## Messages racing a commit — fixed in #110

openmls keeps no past-epoch secrets by default (`MaxEpochs(0)`), and
`Member::receive` used to return `Event::Advanced` for any frame from an
earlier epoch. An application message ordered after a concurrent commit was
dropped by every receiver, while its sender got `Echo` and believed it was
delivered.

**The fix is sender retry, and only that.** Every client processes frames in
the relay's order, so when a sender's echo arrives behind a commit, all
members agree the frame is stale:

- receivers return `Event::Stale`;
- the sender re-encrypts the same text in the current epoch and gets
  `Event::Resend` to send;
- a resend that races again is resent again.

This keeps the relay blind and costs no forward secrecy.

**Receiver tolerance (`max_past_epochs`) was considered and rejected**, though
it is MIMI's `epoch_tolerance` and the obvious first fix. A member admitted
*by* the racing commit never had the old epoch's keys. With tolerance, some
members would read the message and the newcomer would not. The sender would
have to resend for them anyway, and everyone else would then see it twice.

What this does not cover: a sender that quits or restarts between sending and
seeing its echo. The record of what it sent is not persisted, so that message
can still be lost. #105 is where that becomes likely, since catch-up widens
the window.

---

## Corrections made on 2026-09-29

So the mistakes stay visible rather than being quietly rewritten:

- MLS "runs formally verified libcrux code" — wrong; it runs RustCrypto, and
  on Apple Silicon in software, because the ARMv8 backends are opt-in.
- MLS membership change "O(log N)" — best case only; measured linear here.
- "MLS targets thousands" — RFC 9750 aims at tens of thousands.
- "The UN runs air-gapped Matrix" — unsupported by a primary source.
- "~35 countries" — countries Matrix is talking to, per Hodgson; 25+ are
  deploying.
- "BSI C5 certification" — C5 is a cloud attestation; the relevant German
  approval for messaging is VS-NfD.
- Matrix history sharing — real for encrypted rooms only since 2026, off by
  default.
- MIMI architecture draft — now -03, single author (Cisco); Matrix Foundation
  co-authors are on the protocol draft.
- "The relay never learns which frames are commits" — it receives that in the
  clear and chooses not to look.
- Measurements re-run: 7.4x rather than 6.5x, same conclusion.
