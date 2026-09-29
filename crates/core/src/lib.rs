//! Core client logic for OpenConv: MLS group state and message encryption.
//!
//! The server never sees plaintext. Everything in this crate runs on the
//! client; the wire carries only TLS-serialised MLS messages.

use openmls::prelude::{tls_codec::*, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use std::collections::VecDeque;
use std::path::PathBuf;

pub mod store;
pub use store::{Provider, Snapshot, Vault};

uniffi::setup_scaffolding!();

pub mod ffi;

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Anything that can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An operation needed a group, but this member has not joined one.
    #[error("no group joined yet")]
    NoGroup,
    /// A message arrived where a different kind was required.
    #[error("expected a {expected} message")]
    UnexpectedMessage {
        /// The kind that was required.
        expected: &'static str,
    },
    /// The MLS layer rejected an operation. Carries its description, so
    /// callers never depend on `openmls` error types.
    #[error("mls: {0}")]
    Mls(String),
    /// A message could not be serialised or deserialised.
    #[error("codec: {0}")]
    Codec(#[from] tls_codec::Error),
    /// A relayed frame was empty or carried an unknown tag.
    #[error("malformed frame")]
    MalformedFrame,
    /// Reading or writing persisted state failed.
    #[error("store: {0}")]
    Store(String),
    /// Another add is already staged and waiting on the relay's ordering.
    #[error("an add is already in flight")]
    AddInFlight,
    /// The vault decrypted but its contents do not form a usable client.
    #[error("saved state is unreadable: {0}")]
    CorruptState(&'static str),
}

/// Convenience alias for this crate's fallible operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Wraps any openmls error into our own, so callers never depend on
/// openmls error types leaking through the FFI boundary.
fn mls<E: std::fmt::Display>(e: E) -> Error {
    Error::Mls(e.to_string())
}

/// One participant on one device.
pub struct Member {
    provider: Provider,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    group: Option<MlsGroup>,
    /// Absent for in-memory members, which keep nothing across a restart.
    vault: Option<Vault>,
    /// An add staged but not yet confirmed by the relay's ordering.
    ///
    /// Not persisted: a restart mid-add drops it, and the add is simply
    /// retried. Persisting it would mean reconciling a staged commit against
    /// an ordering this client has not yet seen.
    pending: Option<PendingAdd>,
    /// Recently sent messages, so their echoes off the relay can be
    /// recognised without putting sender identity on the wire.
    sent: VecDeque<Sent>,
}

/// A message sent and not yet seen coming back.
///
/// Holds the plaintext, because an echo arriving after a commit means the
/// message has to be encrypted again. Deliberately no derived `Debug`: it
/// would print message contents into any log line that formats one.
struct Sent {
    /// Exactly the bytes sent, used to spot this message coming back.
    wire: Vec<u8>,
    /// The epoch it was encrypted in.
    epoch: u64,
    /// What to resend if that epoch turns out to be stale.
    text: String,
}

/// An add waiting to find out whether it won its epoch.
#[derive(Debug)]
struct PendingAdd {
    /// Exactly the bytes sent, used to spot this commit coming back.
    commit: Vec<u8>,
    /// Held until the commit is confirmed; useless if the commit loses.
    welcome: Vec<u8>,
}

/// How many recently sent frames to remember. Only needs to outlive a round
/// trip through the relay.
const SENT_HISTORY: usize = 32;

/// What processing a relayed frame produced.
#[derive(Debug)]
pub enum Event {
    /// A decrypted application message.
    Message(String),
    /// A staged add won its epoch. Send this `Welcome` to the new member.
    Admitted {
        /// Deliver to the member being added.
        welcome: Vec<u8>,
    },
    /// A staged add lost its epoch to another member's commit, which has been
    /// applied instead. The joiner was probably admitted by that commit, so
    /// this is a notice rather than something to retry blindly.
    AddSuperseded,
    /// Group state moved forward; nothing to display.
    Advanced,
    /// This client's own frame, echoed back by the relay.
    Echo,
    /// This client's own message came back from the relay behind a commit it
    /// was not encrypted for. Every receiver has discarded it as
    /// [`Event::Stale`]. Send `message` — the same text, encrypted in the
    /// current epoch — as an application frame.
    Resend {
        /// Deliver to the group in place of the stale original.
        message: Vec<u8>,
    },
    /// Another member's message from an epoch this member has already left,
    /// discarded unread. Its sender sees the same ordering and resends it
    /// (see [`Event::Resend`]), so this is not a loss.
    Stale,
}

/// Redacting by hand rather than deriving: this type holds identity keys and
/// ratchet state, and a derived `Debug` would print them into any log or
/// panic message that formats a `Member`.
impl std::fmt::Debug for Member {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Member")
            .field("identity", &self.identity())
            .field("members", &self.member_count())
            .field("persisted", &self.vault.is_some())
            .finish_non_exhaustive()
    }
}

impl Member {
    /// Create a fresh identity. `identity` is the display name carried in
    /// the MLS basic credential.
    ///
    /// # Errors
    /// If a signature key pair cannot be generated or stored.
    pub fn new(identity: &str) -> Result<Self> {
        let provider = Provider::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).map_err(mls)?;
        signer.store(provider.storage()).map_err(mls)?;

        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };

        Ok(Self {
            provider,
            signer,
            credential,
            group: None,
            vault: None,
            pending: None,
            sent: VecDeque::new(),
        })
    }

    /// Open a member backed by an encrypted file, restoring previous state if
    /// the file exists and starting fresh otherwise.
    ///
    /// # Errors
    /// If the data key is unavailable, the vault cannot be decrypted, or the
    /// restored state is missing its key pair or group.
    pub fn open(path: impl Into<PathBuf>, identity: &str) -> Result<Self> {
        Self::restore(Vault::open(path, identity)?, identity)
    }

    /// Open a member against an already-configured vault.
    ///
    /// # Errors
    /// As [`Member::open`].
    pub fn restore(vault: Vault, identity: &str) -> Result<Self> {
        let Some(snapshot) = vault.load()? else {
            let mut member = Self::new(identity)?;
            member.vault = Some(vault);
            member.persist()?;
            return Ok(member);
        };

        let Snapshot {
            identity,
            signature_public_key,
            group_id,
            storage,
        } = snapshot;
        let provider = Provider::with_storage(store::restore(storage)?);

        let signer = SignatureKeyPair::read(
            provider.storage(),
            &signature_public_key,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or(Error::CorruptState("signature key pair missing from store"))?;

        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signature_public_key.into(),
        };

        let group = match &group_id {
            Some(id) => MlsGroup::load(provider.storage(), &GroupId::from_slice(id))
                .map_err(mls)?
                .ok_or(Error::CorruptState(
                    "group id recorded but no group in store",
                ))
                .map(Some)?,
            None => None,
        };

        Ok(Self {
            provider,
            signer,
            credential,
            group,
            vault: Some(vault),
            pending: None,
            sent: VecDeque::new(),
        })
    }

    /// Write current state to the vault. A no-op for in-memory members.
    fn persist(&self) -> Result<()> {
        let Some(vault) = &self.vault else {
            return Ok(());
        };

        vault.save(&Snapshot {
            identity: self.identity(),
            signature_public_key: self.signer.to_public_vec(),
            group_id: self
                .group
                .as_ref()
                .map(|g| g.group_id().as_slice().to_vec()),
            storage: store::dump(self.provider.storage())?,
        })
    }

    fn config() -> MlsGroupCreateConfig {
        MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            // Ship the ratchet tree inside the Welcome so a joiner needs
            // nothing from the server beyond the Welcome itself.
            .use_ratchet_tree_extension(true)
            .build()
    }

    /// A published `KeyPackage`, which anyone can use to add this member to a
    /// group. Safe to hand to the server.
    ///
    /// # Errors
    /// If the key package cannot be built or serialised.
    pub fn key_package(&self) -> Result<Vec<u8>> {
        let bundle = KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .map_err(mls)?;
        Ok(MlsMessageOut::from(bundle.key_package().clone()).tls_serialize_detached()?)
    }

    /// Start a new group with just this member in it.
    ///
    /// # Errors
    /// If the group cannot be created, or its state cannot be persisted.
    pub fn create_group(&mut self) -> Result<()> {
        let group = MlsGroup::new(
            &self.provider,
            &self.signer,
            &Self::config(),
            self.credential.clone(),
        )
        .map_err(mls)?;
        self.group = Some(group);
        self.persist()
    }

    /// Stage adding a member from their published `KeyPackage`.
    ///
    /// Returns the commit to send. The commit is *not* applied yet: two
    /// members can stage a commit at the same epoch, and only the one the
    /// relay orders first may take effect. [`Member::receive`] resolves it,
    /// yielding [`Event::Admitted`] with the `Welcome` if this commit won or
    /// [`Event::AddSuperseded`] if it lost.
    ///
    /// # Errors
    /// If the key package is invalid, this member has no group, or another
    /// add is already staged.
    pub fn propose_add(&mut self, key_package: &[u8]) -> Result<Vec<u8>> {
        if self.pending.is_some() {
            return Err(Error::AddInFlight);
        }

        let msg = MlsMessageIn::tls_deserialize_exact(key_package)?;
        let MlsMessageBodyIn::KeyPackage(kp) = msg.extract() else {
            return Err(Error::UnexpectedMessage {
                expected: "KeyPackage",
            });
        };
        // Verifies the signature and lifetime before we trust it.
        let kp = kp
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(mls)?;

        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        let (commit, welcome, _) = group
            .add_members(&self.provider, &self.signer, &[kp])
            .map_err(mls)?;

        let commit = commit.tls_serialize_detached()?;
        self.pending = Some(PendingAdd {
            commit: commit.clone(),
            welcome: welcome.tls_serialize_detached()?,
        });
        self.persist()?;
        Ok(commit)
    }

    /// Join a group from a `Welcome` released by [`Member::propose_add`].
    ///
    /// # Errors
    /// If the message is not a `Welcome`, or does not admit this member.
    pub fn join(&mut self, welcome: &[u8]) -> Result<()> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome)?;
        let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
            return Err(Error::UnexpectedMessage {
                expected: "Welcome",
            });
        };

        let group = StagedWelcome::new_from_welcome(
            &self.provider,
            Self::config().join_config(),
            welcome,
            None, // ratchet tree travels in the Welcome
        )
        .map_err(mls)?
        .into_group(&self.provider)
        .map_err(mls)?;

        self.group = Some(group);
        self.persist()
    }

    /// Encrypt an application message for the group.
    ///
    /// # Errors
    /// If this member has no group, or encryption fails.
    pub fn send(&mut self, text: &str) -> Result<Vec<u8>> {
        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        let epoch = group.epoch().as_u64();
        // Encrypting ratchets the sender key, so the new state must be saved.
        let wire = group
            .create_message(&self.provider, &self.signer, text.as_bytes())
            .map_err(mls)?
            .tls_serialize_detached()?;
        self.remember_sent(Sent {
            wire: wire.clone(),
            epoch,
            text: text.to_owned(),
        });
        self.persist()?;
        Ok(wire)
    }

    /// Process an incoming message. Returns `Some` for application messages,
    /// `None` for handshake traffic that was applied to group state.
    ///
    /// # Errors
    /// If the message is malformed, this member has no group, or the message
    /// cannot be decrypted.
    pub fn receive(&mut self, wire: &[u8]) -> Result<Event> {
        // A staged commit coming back means the relay ordered it first and
        // nothing beat it: apply it and release the Welcome.
        if let Some(pending) = self.pending.take_if(|p| p.commit == wire) {
            let welcome = pending.welcome;
            let group = self.group.as_mut().ok_or(Error::NoGroup)?;
            group.merge_pending_commit(&self.provider).map_err(mls)?;
            self.persist()?;
            return Ok(Event::Admitted { welcome });
        }
        if let Some(sent) = self.take_sent(wire) {
            let group = self.group.as_ref().ok_or(Error::NoGroup)?;
            if sent.epoch == group.epoch().as_u64() {
                return Ok(Event::Echo);
            }
            // A commit was ordered between sending this and seeing it come
            // back, so every member has left the epoch it was encrypted in
            // and will discard it. They all saw the same order, so they all
            // agree it is stale; encrypting it again in the current epoch is
            // what gets it delivered.
            //
            // Keeping past-epoch secrets so receivers could still read it was
            // considered and rejected: a member admitted by that commit never
            // had the old epoch's keys, so some members would see the message
            // and others would not.
            let message = self.send(&sent.text)?;
            return Ok(Event::Resend { message });
        }

        let msg = MlsMessageIn::tls_deserialize_exact(wire)?;
        let protocol: ProtocolMessage =
            msg.try_into_protocol_message()
                .map_err(|_| Error::UnexpectedMessage {
                    expected: "application or handshake",
                })?;

        // Another member's commit at this epoch beats anything we staged,
        // because the relay ordered theirs first. Drop ours before applying
        // theirs; openmls will not stage two commits at once.
        let superseded = protocol.content_type() == ContentType::Commit && self.pending.is_some();
        if superseded {
            self.pending = None;
            let group = self.group.as_mut().ok_or(Error::NoGroup)?;
            group
                .clear_pending_commit(self.provider.storage())
                .map_err(mls)?;
        }

        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        // Frames from an epoch we have already left cannot be decrypted. A
        // stale commit lost its race and has nothing to apply. A stale
        // message will be resent by its sender, and must not be reported as
        // though nothing had arrived.
        if protocol.epoch() < group.epoch() {
            return Ok(match protocol.content_type() {
                ContentType::Application => Event::Stale,
                ContentType::Commit | ContentType::Proposal => Event::Advanced,
            });
        }
        let processed = group
            .process_message(&self.provider, protocol)
            .map_err(mls)?;

        let event = match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                Event::Message(String::from_utf8_lossy(&app.into_bytes()).into_owned())
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                group
                    .merge_staged_commit(&self.provider, *commit)
                    .map_err(mls)?;
                if superseded {
                    Event::AddSuperseded
                } else {
                    Event::Advanced
                }
            }
            _ => Event::Advanced,
        };
        // Receiving advances ratchet state whether or not it was a message.
        self.persist()?;
        Ok(event)
    }

    /// Record a message so the relay's echo of it can be recognised.
    fn remember_sent(&mut self, sent: Sent) {
        if self.sent.len() == SENT_HISTORY {
            self.sent.pop_front();
        }
        self.sent.push_back(sent);
    }

    /// Claim the record of a sent message whose echo has arrived.
    ///
    /// Removed rather than just matched, so a message is resent at most once
    /// per echo: a second copy of a stale echo must not send it twice.
    fn take_sent(&mut self, wire: &[u8]) -> Option<Sent> {
        let at = self.sent.iter().position(|sent| sent.wire == wire)?;
        self.sent.remove(at)
    }

    /// Display name of this member.
    #[must_use]
    pub fn identity(&self) -> String {
        String::from_utf8_lossy(self.credential.credential.serialized_content()).into_owned()
    }

    /// Number of members currently in the group.
    #[must_use]
    pub fn member_count(&self) -> usize {
        self.group.as_ref().map_or(0, |g| g.members().count())
    }
}

/// What a relayed frame carries. The relay never reads this; only clients do.
#[derive(Clone, Copy, PartialEq, Eq, Debug, uniffi::Enum)]
pub enum FrameKind {
    /// A published `KeyPackage`, offering to be added to a group.
    KeyPackage,
    /// A Welcome admitting someone to the group.
    Welcome,
    /// A commit that existing members must apply.
    Commit,
    /// An encrypted application message.
    Application,
}

impl FrameKind {
    fn tag(self) -> u8 {
        match self {
            Self::KeyPackage => 1,
            Self::Welcome => 2,
            Self::Commit => 3,
            Self::Application => 4,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::KeyPackage),
            2 => Some(Self::Welcome),
            3 => Some(Self::Commit),
            4 => Some(Self::Application),
            _ => None,
        }
    }
}

/// Prefix a payload with its frame tag.
#[must_use]
pub fn encode_frame(kind: FrameKind, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 1);
    v.push(kind.tag());
    v.extend_from_slice(body);
    v
}

/// Width of the sequence number the relay prepends to every frame.
const SEQ_LEN: usize = 8;

/// A frame as the relay delivers it: its sequence number, then the frame.
#[derive(Debug)]
pub struct Envelope {
    /// The relay's position for this frame. Monotonic per channel, and the
    /// same for every client, which is what decides competing commits.
    pub seq: u64,
    /// What the frame carries.
    pub kind: FrameKind,
    /// The payload behind the tag.
    pub body: Vec<u8>,
}

/// Split a frame delivered by the relay into sequence number, kind, payload.
///
/// # Errors
/// If the frame is too short, or carries an unknown tag.
pub fn decode_envelope(wire: &[u8]) -> Result<Envelope> {
    if wire.len() < SEQ_LEN {
        return Err(Error::MalformedFrame);
    }
    let (seq, frame) = wire.split_at(SEQ_LEN);
    let seq = u64::from_be_bytes(seq.try_into().map_err(|_| Error::MalformedFrame)?);
    let (kind, body) = decode_frame(frame)?;
    Ok(Envelope { seq, kind, body })
}

/// Split a client frame into its kind and payload.
///
/// # Errors
/// If the frame is empty or carries an unknown tag.
pub fn decode_frame(wire: &[u8]) -> Result<(FrameKind, Vec<u8>)> {
    let (&tag, body) = wire.split_first().ok_or(Error::MalformedFrame)?;
    let kind = FrameKind::from_tag(tag).ok_or(Error::MalformedFrame)?;
    Ok((kind, body.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert an event is a message with the given text.
    #[track_caller]
    fn assert_message(event: Event, expected: &str) {
        match event {
            Event::Message(text) => assert_eq!(text, expected),
            other => panic!("expected a message, got {other:?}"),
        }
    }

    /// Drive an add to completion the way the relay would when nothing
    /// competes: the proposer's own commit comes back first, confirming it.
    fn admit(host: &mut Member, joiner: &mut Member) {
        let commit = host.propose_add(&joiner.key_package().unwrap()).unwrap();
        let Event::Admitted { welcome } = host.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        joiner.join(&welcome).unwrap();
    }

    /// The whole point of the slice: two members, one group, real MLS
    /// ciphertext on the wire, plaintext out the other side.
    #[test]
    fn two_members_exchange_encrypted_messages() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();

        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        assert_eq!(alice.member_count(), 2);
        assert_eq!(bob.member_count(), 2);

        let wire = alice.send("hello bob").unwrap();
        assert_message(bob.receive(&wire).unwrap(), "hello bob");

        let wire = bob.send("hi alice").unwrap();
        assert_message(alice.receive(&wire).unwrap(), "hi alice");
    }

    /// Two members staging an add at the same epoch must not fork the group.
    /// Whichever commit the relay orders first wins, and everyone converges.
    #[test]
    fn competing_commits_resolve_to_one_winner() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        let dave = Member::new("dave").unwrap();

        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        // Alice and Bob both try to admit someone, at the same epoch.
        let alice_commit = alice.propose_add(&carol.key_package().unwrap()).unwrap();
        let bob_commit = bob.propose_add(&dave.key_package().unwrap()).unwrap();

        // The relay orders Alice's first, and delivers both to everyone in
        // that order.
        let Event::Admitted { welcome } = alice.receive(&alice_commit).unwrap() else {
            panic!("alice's commit was ordered first, so it should win");
        };
        carol.join(&welcome).unwrap();

        // Bob sees Alice's commit while his own is staged: he lost.
        assert!(
            matches!(bob.receive(&alice_commit).unwrap(), Event::AddSuperseded),
            "bob's staged add should have been superseded"
        );

        // Bob's commit then arrives at everyone. It is stale — it was built
        // against the previous epoch — so every member must skip it rather
        // than apply it or error.
        for (who, member) in [
            ("bob", &mut bob),
            ("alice", &mut alice),
            ("carol", &mut carol),
        ] {
            assert!(
                matches!(member.receive(&bob_commit), Ok(Event::Advanced)),
                "{who} mishandled the superseded commit"
            );
        }

        // Everyone agrees on the same group.
        assert_eq!(alice.member_count(), 3);
        assert_eq!(bob.member_count(), 3);
        assert_eq!(carol.member_count(), 3);

        // And the group still works for everyone in it.
        let wire = alice.send("did we survive that?").unwrap();
        assert_message(bob.receive(&wire).unwrap(), "did we survive that?");
        assert_message(carol.receive(&wire).unwrap(), "did we survive that?");
    }

    /// A message sent while another member's commit is in flight is encrypted
    /// in an epoch everyone leaves before it arrives. It must reach everyone
    /// anyway — including the member that commit admitted, who never had the
    /// old epoch's keys at all.
    #[test]
    fn message_racing_a_commit_is_resent_to_everyone() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        // Alice admits Carol while Bob, not having seen that commit, speaks.
        let commit = alice.propose_add(&carol.key_package().unwrap()).unwrap();
        let raced = bob.send("crossed in the post").unwrap();

        // The relay orders the commit first.
        let Event::Admitted { welcome } = alice.receive(&commit).unwrap() else {
            panic!("alice's commit was ordered first, so it should win");
        };
        carol.join(&welcome).unwrap();
        assert!(matches!(bob.receive(&commit).unwrap(), Event::Advanced));

        // Bob's message lands a stale epoch behind for everyone. Receivers
        // discard it, and say so rather than claiming the group advanced.
        assert!(matches!(alice.receive(&raced).unwrap(), Event::Stale));
        assert!(matches!(carol.receive(&raced).unwrap(), Event::Stale));

        // Bob sees his own message land after the commit and sends it again.
        let Event::Resend { message } = bob.receive(&raced).unwrap() else {
            panic!("bob's message landed after a commit, so it must be resent");
        };
        // Once. A second copy of the stale echo must not send it again.
        assert!(matches!(bob.receive(&raced).unwrap(), Event::Stale));

        assert_message(alice.receive(&message).unwrap(), "crossed in the post");
        assert_message(carol.receive(&message).unwrap(), "crossed in the post");
        assert!(matches!(bob.receive(&message).unwrap(), Event::Echo));
    }

    /// A resend can race a commit too. It has to keep going until it lands
    /// in the epoch everyone is in, not give up after one attempt.
    #[test]
    fn a_resend_that_races_again_is_resent_again() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);
        let commit = alice.propose_add(&carol.key_package().unwrap()).unwrap();
        let Event::Admitted { welcome } = alice.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        carol.join(&welcome).unwrap();
        bob.receive(&commit).unwrap();

        let mut attempt = bob.send("third time lucky").unwrap();
        let mut newcomers = Vec::new();

        // Twice over, Alice admits someone and her commit is ordered just
        // ahead of Bob's latest attempt.
        for name in ["dave", "erin"] {
            let mut joiner = Member::new(name).unwrap();
            let commit = alice.propose_add(&joiner.key_package().unwrap()).unwrap();
            let Event::Admitted { welcome } = alice.receive(&commit).unwrap() else {
                panic!("alice's commit was ordered first, so it should win");
            };
            for member in std::iter::once(&mut bob)
                .chain(std::iter::once(&mut carol))
                .chain(newcomers.iter_mut())
            {
                member.receive(&commit).unwrap();
            }
            joiner.join(&welcome).unwrap();
            newcomers.push(joiner);

            for member in [&mut alice, &mut carol] {
                assert!(matches!(member.receive(&attempt).unwrap(), Event::Stale));
            }
            let Event::Resend { message } = bob.receive(&attempt).unwrap() else {
                panic!("an attempt that landed after a commit must be resent");
            };
            attempt = message;
        }

        for member in [&mut alice, &mut carol]
            .into_iter()
            .chain(newcomers.iter_mut())
        {
            assert_message(member.receive(&attempt).unwrap(), "third time lucky");
        }
        assert!(matches!(bob.receive(&attempt).unwrap(), Event::Echo));
    }

    /// The relay echoes a sender its own frames; they must not be mistaken
    /// for incoming traffic.
    #[test]
    fn own_frames_echoed_back_are_recognised() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        let wire = alice.send("talking to myself").unwrap();
        assert!(matches!(alice.receive(&wire).unwrap(), Event::Echo));
    }

    /// The server must never be able to read traffic it relays.
    #[test]
    fn wire_bytes_do_not_contain_plaintext() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        let secret = "launch codes are 1234";
        let wire = alice.send(secret).unwrap();
        assert!(
            !wire.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "plaintext leaked onto the wire"
        );
    }

    /// An outsider holding the ciphertext cannot read it.
    #[test]
    fn non_member_cannot_decrypt() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut eve = Member::new("eve").unwrap();

        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        // Eve builds her own group and tries to process alice's traffic.
        eve.create_group().unwrap();
        let wire = alice.send("secret").unwrap();
        assert!(
            eve.receive(&wire).is_err(),
            "outsider decrypted group traffic"
        );
    }

    /// Frames arrive from the network, so every shape of rubbish must be
    /// rejected rather than panic. A panic here would cross the FFI boundary
    /// and take the client down.
    #[test]
    fn malformed_frames_are_rejected_not_fatal() {
        let short = &[0u8, 1, 2][..]; // shorter than the sequence prefix
        let unknown_tag = &[0, 0, 0, 0, 0, 0, 0, 1, 0xff][..];
        assert!(decode_envelope(short).is_err());
        assert!(decode_envelope(unknown_tag).is_err());
        assert!(decode_frame(&[]).is_err());
    }

    /// The same for the crypto path: hostile bytes must surface as errors,
    /// and must not disturb group state.
    #[test]
    fn hostile_payloads_error_without_losing_the_group() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        let real = alice.send("a real message").unwrap();
        for (name, payload) in [
            ("empty", &[][..]),
            ("garbage", &[9u8; 40][..]),
            ("truncated", &real[..real.len() / 2]),
        ] {
            assert!(
                bob.receive(payload).is_err(),
                "{name} payload should have been rejected"
            );
        }

        // Still a working group afterwards: rejecting junk must not have
        // torn down any state.
        assert_eq!(bob.member_count(), 2);
        let wire = alice.send("still fine").unwrap();
        assert_message(bob.receive(&wire).unwrap(), "still fine");
    }

    /// Two overlapping adds from one member would stage two commits at one
    /// epoch, which openmls will not do. Fail early and clearly instead.
    #[test]
    fn only_one_add_can_be_in_flight() {
        let mut alice = Member::new("alice").unwrap();
        let bob = Member::new("bob").unwrap();
        let carol = Member::new("carol").unwrap();
        alice.create_group().unwrap();

        alice.propose_add(&bob.key_package().unwrap()).unwrap();
        assert!(matches!(
            alice.propose_add(&carol.key_package().unwrap()),
            Err(Error::AddInFlight)
        ));
    }

    /// A vault in a unique temp dir with a fixed key, so tests never touch
    /// the real Keychain.
    fn test_vault(name: &str) -> (std::path::PathBuf, Vault) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "openconv-test-{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed),
            name
        ));
        let path = dir.join("state.vault");
        (path.clone(), Vault::with_key(path, [7u8; 32]))
    }

    /// The point of persistence: identity and group state outlive the process.
    #[test]
    fn group_state_survives_a_restart() {
        let (path, vault) = test_vault("alice");
        let mut bob = Member::new("bob").unwrap();

        let identity_before;
        {
            let mut alice = Member::restore(vault, "alice").unwrap();
            alice.create_group().unwrap();
            identity_before = alice.identity();
            admit(&mut alice, &mut bob);
            assert_eq!(alice.member_count(), 2);
        } // alice is dropped: everything now has to come off disk

        let mut alice = Member::restore(Vault::with_key(&path, [7u8; 32]), "alice").unwrap();
        assert_eq!(alice.identity(), identity_before);
        assert_eq!(alice.member_count(), 2, "group did not survive the restart");

        // The restored ratchet state must still work in both directions.
        let wire = alice.send("still here after a restart").unwrap();
        assert_message(bob.receive(&wire).unwrap(), "still here after a restart");

        let wire = bob.send("so am i").unwrap();
        assert_message(alice.receive(&wire).unwrap(), "so am i");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Reopening must restore the existing identity rather than mint a new
    /// one. A fresh key pair each launch would silently break every group
    /// this client belongs to.
    #[test]
    fn identity_is_stable_across_reopen() {
        let (path, vault) = test_vault("newcomer");
        let first = Member::restore(vault, "newcomer").unwrap();
        let key = first.key_package().unwrap();
        drop(first);

        assert!(path.exists(), "opening a fresh path did not write a vault");

        let again = Member::restore(Vault::with_key(&path, [7u8; 32]), "newcomer").unwrap();
        assert_eq!(again.identity(), "newcomer");
        // Key packages are single-use and differ per call, but they are signed
        // by the identity key, which must have survived.
        assert_ne!(
            key,
            again.key_package().unwrap(),
            "key packages should be fresh"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A wrong key is one failure; a flipped byte is another. AEAD should
    /// catch tampering, and the vault must refuse rather than load partial
    /// state.
    #[test]
    fn tampered_vault_is_rejected() {
        let (path, vault) = test_vault("tampered");
        Member::restore(vault, "tampered")
            .unwrap()
            .create_group()
            .unwrap();

        let mut raw = std::fs::read(&path).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        std::fs::write(&path, &raw).unwrap();

        let reopened = Member::restore(Vault::with_key(&path, [7u8; 32]), "tampered");
        assert!(matches!(reopened, Err(Error::Store(_))));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Key material must never be readable on disk.
    #[test]
    fn vault_file_is_encrypted() {
        let (path, vault) = test_vault("secretive");
        let mut member = Member::restore(vault, "secretive").unwrap();
        member.create_group().unwrap();

        let raw = std::fs::read(&path).unwrap();
        assert!(raw.starts_with(b"OCV1"), "missing vault magic");
        assert!(
            !raw.windows(9).any(|w| w == b"secretive"),
            "identity readable in the vault file"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A wrong key must fail loudly rather than silently discarding state.
    #[test]
    fn wrong_key_is_an_error_not_a_fresh_start() {
        let (path, vault) = test_vault("guarded");
        Member::restore(vault, "guarded")
            .unwrap()
            .create_group()
            .unwrap();

        let wrong = Vault::with_key(&path, [9u8; 32]);
        assert!(matches!(
            Member::restore(wrong, "guarded"),
            Err(Error::Store(_))
        ));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Exercises the real macOS Keychain, so it is not part of the normal
    /// run: `cargo test -p openconv-core -- --ignored keychain`.
    #[test]
    #[ignore = "touches the real macOS Keychain"]
    fn keychain_backed_vault_round_trips() {
        let dir = std::env::temp_dir().join(format!("openconv-keychain-{}", std::process::id()));
        let path = dir.join("state.vault");
        let account = format!("openconv-test-{}", std::process::id());

        {
            let mut m = Member::open(&path, &account).unwrap();
            m.create_group().unwrap();
        }
        let m = Member::open(&path, &account).unwrap();
        assert_eq!(m.member_count(), 1);

        #[cfg(target_os = "macos")]
        security_framework::passwords::delete_generic_password("com.openconv.vault", &account).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sending_before_joining_is_an_error() {
        let mut alice = Member::new("alice").unwrap();
        assert!(matches!(alice.send("nope"), Err(Error::NoGroup)));
    }
}
