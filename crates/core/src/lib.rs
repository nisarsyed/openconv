//! Core client logic for OpenConv: MLS group state and message encryption.
//!
//! The server never sees plaintext. Everything in this crate runs on the
//! client; the wire carries only TLS-serialised MLS messages.

use openconv_wire::{Address, Inbound, Request};
use openmls::prelude::{tls_codec::*, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

pub mod store;
use store::{Database, Saved};
pub use store::{JsonCodec, Provider, Vault};

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
    /// The relay envelope around a frame was malformed, or an address could
    /// not be encoded.
    #[error("envelope: {0}")]
    Wire(#[from] openconv_wire::Error),
    /// Text that should have been a channel id was not one.
    #[error("not a channel id")]
    InvalidChannelId,
    /// Reading or writing persisted state failed.
    #[error("store: {0}")]
    Store(String),
    /// Another add is already staged and waiting on the relay's ordering.
    #[error("an add is already in flight")]
    AddInFlight,
    /// The vault decrypted but its contents do not form a usable client.
    #[error("saved state is unreadable: {0}")]
    CorruptState(&'static str),
    /// The relay delivered a frame past the one this client expected next,
    /// so frames in between were missed. [`Member::deliver`] recovers from
    /// this itself by resuming; the variant remains for callers below it.
    #[error("missed frames: expected {expected}, got {got}")]
    Gap {
        /// The sequence number this client expected.
        expected: u64,
        /// The one that arrived.
        got: u64,
    },
}

/// Convenience alias for this crate's fallible operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Wraps any openmls error into our own, so callers never depend on
/// openmls error types leaking through the FFI boundary.
///
/// A storage failure inside openmls becomes [`Error::Store`], not
/// [`Error::Mls`], because the two mean opposite things to
/// [`Member::deliver`]: a frame openmls rejected will be rejected every time
/// and is stepped over, while one that failed to save must be retried.
/// openmls's storage variants do not expose the underlying error as a source,
/// and their messages vary, but every one in 0.9 says "storage". That is
/// what this matches, and `a_frame_that_fails_to_save_is_retried_not_skipped`
/// pins it: an openmls upgrade that changes the wording fails that test
/// rather than silently skipping frames.
fn mls<E: std::fmt::Display>(e: E) -> Error {
    let message = e.to_string();
    if message.to_ascii_lowercase().contains("storage") {
        Error::Store(message)
    } else {
        Error::Mls(message)
    }
}

impl Error {
    /// Whether this failure is a property of the frame being applied, so it
    /// would fail the same way every time, rather than of this client's
    /// circumstances (its disk, its store), which may pass.
    fn is_frame_fault(&self) -> bool {
        matches!(
            self,
            Self::Codec(_)
                | Self::MalformedFrame
                | Self::UnexpectedMessage { .. }
                | Self::Wire(_)
                | Self::Mls(_)
        )
    }
}

/// One participant on one device.
pub struct Member {
    crypto: RustCrypto,
    /// Where openmls keeps group state and keys, and where this client
    /// records who it is. In memory for members made with [`Member::new`],
    /// encrypted on disk for the rest.
    db: Database,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    group: Option<MlsGroup>,
    /// An add staged but not yet confirmed by the relay's ordering.
    ///
    /// Persisted. openmls keeps its half (the staged commit) in the store, so
    /// a client that restarted mid-add would otherwise meet its own commit
    /// in catch-up without recognising it, and be unable to process it.
    pending: Option<PendingAdd>,
    /// Recently sent messages, so their echoes off the relay can be
    /// recognised without putting sender identity on the wire.
    sent: VecDeque<Sent>,
    /// Addresses caught up on since this client last connected. Join
    /// requests are only answered live; see [`Member::deliver`].
    live: HashSet<Vec<u8>>,
    /// Addresses this client has asked the relay to resume, after finding a
    /// gap, and not yet heard are live again.
    resuming: HashSet<Vec<u8>>,
}

/// A message sent and not yet seen coming back.
///
/// Holds the plaintext, because an echo arriving after a commit means the
/// message has to be encrypted again. Deliberately no derived `Debug`: it
/// would print message contents into any log line that formats one.
#[derive(Clone)]
struct Sent {
    /// Exactly the bytes sent, used to spot this message coming back.
    wire: Vec<u8>,
    /// The epoch it was encrypted in.
    epoch: u64,
    /// What to resend if that epoch turns out to be stale.
    text: String,
}

/// An add waiting to find out whether it won its epoch.
#[derive(Debug, Clone)]
struct PendingAdd {
    /// Exactly the bytes sent, used to spot this commit coming back.
    commit: Vec<u8>,
    /// Held until the commit is confirmed; useless if the commit loses.
    welcome: Vec<u8>,
    /// Where the `Welcome` goes: the joiner's `KeyPackageRef`.
    mailbox: Vec<u8>,
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
        /// The address to deliver it to: the new member's mailbox, which
        /// they subscribed to before publishing their `KeyPackage`.
        mailbox: Vec<u8>,
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

/// What the app should do after [`Member::deliver`], in order.
#[derive(Debug)]
pub enum Effect {
    /// Write these bytes to the relay.
    Send(Vec<u8>),
    /// A decrypted message from another member.
    Message(String),
    /// This client joined the group from a `Welcome`.
    Joined,
    /// This client's add won its epoch; the new member has been let in.
    Admitted,
    /// This client's add lost to another member's commit, which was applied
    /// instead. The joiner was probably admitted by it.
    AddSuperseded,
    /// Group state moved on, membership possibly with it.
    Advanced,
    /// Caught up on `address`; frames from here on are live.
    Live {
        /// The address.
        address: Vec<u8>,
    },
    /// The relay's history for `address` is behind this client: its store
    /// was reset or replaced. This client now follows the relay's numbering
    /// from its oldest frame. Nothing it already applied is applied twice:
    /// MLS refuses an old commit or a message whose key is spent.
    RelayReset {
        /// The address.
        address: Vec<u8>,
    },
    /// The relay cannot serve this client's cursor on `address`. Below
    /// `floor`, retention removed frames this client never saw, including
    /// possibly a commit, so it cannot follow the group until another member
    /// re-adds it (#112).
    Unavailable {
        /// The address.
        address: Vec<u8>,
        /// The oldest sequence number the relay holds.
        floor: u64,
        /// The next sequence number the relay will assign.
        next: u64,
    },
}

/// Redacting by hand rather than deriving: this type holds identity keys and
/// ratchet state, and a derived `Debug` would print them into any log or
/// panic message that formats a `Member`.
impl std::fmt::Debug for Member {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Member")
            .field("identity", &self.identity())
            .field("members", &self.member_count())
            .field("persisted", &self.db.is_persistent())
            .finish_non_exhaustive()
    }
}

impl Member {
    /// Create a fresh identity that lives only in memory. `identity` is the
    /// display name carried in the MLS basic credential.
    ///
    /// # Errors
    /// If a signature key pair cannot be generated or stored.
    pub fn new(identity: &str) -> Result<Self> {
        Self::create(Database::in_memory()?, identity)
    }

    /// Mint a new identity and record it in `db`.
    fn create(db: Database, identity: &str) -> Result<Self> {
        let crypto = RustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).map_err(mls)?;
        db.atomically(|| {
            signer.store(db.provider(&crypto).storage()).map_err(mls)?;
            db.save(&Saved {
                identity: identity.to_owned(),
                signature_public_key: signer.to_public_vec(),
                group_id: None,
            })
        })?;

        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };

        Ok(Self {
            crypto,
            db,
            signer,
            credential,
            group: None,
            pending: None,
            sent: VecDeque::new(),
            live: HashSet::new(),
            resuming: HashSet::new(),
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
        let db = vault.connect()?;
        let Some(Saved {
            identity,
            signature_public_key,
            group_id,
        }) = db.load()?
        else {
            return Self::create(db, identity);
        };

        let crypto = RustCrypto::default();
        let provider = db.provider(&crypto);
        let signer = SignatureKeyPair::read(
            provider.storage(),
            &signature_public_key,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or(Error::CorruptState("signature key pair missing from store"))?;
        let group = load_group(&provider, group_id.as_deref())?;

        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signature_public_key.into(),
        };

        let pending = db
            .pending_add()?
            .map(|(commit, welcome, mailbox)| PendingAdd {
                commit,
                welcome,
                mailbox,
            });
        let sent = db
            .sent()?
            .into_iter()
            .map(|(wire, epoch, text)| Sent { wire, epoch, text })
            .collect();

        Ok(Self {
            crypto,
            db,
            signer,
            credential,
            group,
            pending,
            sent,
            live: HashSet::new(),
            resuming: HashSet::new(),
        })
    }

    /// Run `op` as one unit: its writes reach the store together or not at
    /// all.
    ///
    /// On failure the store is rolled back, and this member's in-memory
    /// state is put back to match it. openmls may have moved the group
    /// partway before failing, so the group is reloaded from the store rather
    /// than trusted. A failed operation, such as a message that will not
    /// decrypt, therefore leaves no trace: never destroying key state on a
    /// decryption failure holds by construction rather than by care.
    fn atomically<T>(&mut self, op: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let pending = self.pending.clone();
        let sent = self.sent.clone();
        self.db.begin()?;
        let result = op(self).and_then(|value| self.db.release().map(|()| value));
        if result.is_err() {
            self.db.rollback();
            self.pending = pending;
            self.sent = sent;
            self.reload_group()?;
        }
        result
    }

    /// Replace the in-memory group with what the store holds.
    fn reload_group(&mut self) -> Result<()> {
        let group_id = self.db.load()?.and_then(|saved| saved.group_id);
        self.group = load_group(&self.db.provider(&self.crypto), group_id.as_deref())?;
        Ok(())
    }

    /// Record which group this member is in, after creating or joining one.
    fn record_group(&self) -> Result<()> {
        self.db.save(&Saved {
            identity: self.identity(),
            signature_public_key: self.signer.to_public_vec(),
            group_id: self.channel(),
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

    /// The address a `Welcome` for `key_package` will be sent to.
    ///
    /// A joiner subscribes to this before publishing the `KeyPackage`, and
    /// the member that admits them addresses the `Welcome` here. It is the
    /// `KeyPackageRef`: a hash, so it identifies the offer without naming
    /// anyone, and the same one the `Welcome` itself carries.
    ///
    /// # Errors
    /// If `key_package` is not a valid `KeyPackage`.
    pub fn mailbox(&self, key_package: &[u8]) -> Result<Vec<u8>> {
        let kp = self.validate_key_package(key_package)?;
        self.key_package_ref(&kp)
    }

    /// A validated `KeyPackage`'s reference: the joiner's mailbox.
    fn key_package_ref(&self, kp: &KeyPackage) -> Result<Vec<u8>> {
        Ok(kp.hash_ref(&self.crypto).map_err(mls)?.as_slice().to_vec())
    }

    /// Parse and verify a `KeyPackage` from the network. It is hostile input
    /// until this passes.
    fn validate_key_package(&self, key_package: &[u8]) -> Result<KeyPackage> {
        let msg = MlsMessageIn::tls_deserialize_exact(key_package)?;
        let MlsMessageBodyIn::KeyPackage(kp) = msg.extract() else {
            return Err(Error::UnexpectedMessage {
                expected: "KeyPackage",
            });
        };
        // Verifies the signature and lifetime before we trust it.
        kp.validate(&self.crypto, ProtocolVersion::Mls10)
            .map_err(mls)
    }

    /// A published `KeyPackage`, which anyone can use to add this member to a
    /// group. Safe to hand to the server.
    ///
    /// # Errors
    /// If the key package cannot be built or serialised.
    pub fn key_package(&self) -> Result<Vec<u8>> {
        // Building one stores its private init key, which joining with the
        // resulting Welcome needs, possibly after a restart.
        let provider = self.db.provider(&self.crypto);
        self.db.atomically(|| {
            self.db.flush_on_commit();
            let bundle = KeyPackage::builder()
                .build(
                    CIPHERSUITE,
                    &provider,
                    &self.signer,
                    self.credential.clone(),
                )
                .map_err(mls)?;
            Ok(MlsMessageOut::from(bundle.key_package().clone()).tls_serialize_detached()?)
        })
    }

    /// Start a new group with just this member in it.
    ///
    /// # Errors
    /// If the group cannot be created, or its state cannot be persisted.
    pub fn create_group(&mut self) -> Result<()> {
        self.atomically(Self::create_group_unit)
    }

    /// [`Member::create_group`], minus the unit-of-work wrapper.
    fn create_group_unit(&mut self) -> Result<()> {
        self.db.flush_on_commit();
        let group = MlsGroup::new(
            &self.db.provider(&self.crypto),
            &self.signer,
            &Self::config(),
            self.credential.clone(),
        )
        .map_err(mls)?;
        self.group = Some(group);
        self.record_group()?;
        let channel = self.channel().ok_or(Error::NoGroup)?;
        self.db.follow(&channel, 0, None)
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

        let kp = self.validate_key_package(key_package)?;
        self.atomically(|m| m.propose_add_unit(kp))
    }

    /// [`Member::propose_add`] for a validated `KeyPackage`, minus the
    /// unit-of-work wrapper.
    fn propose_add_unit(&mut self, kp: KeyPackage) -> Result<Vec<u8>> {
        let mailbox = self.key_package_ref(&kp)?;
        self.db.flush_on_commit();
        let provider = self.db.provider(&self.crypto);
        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        let (commit, welcome, _) = group
            .add_members(&provider, &self.signer, &[kp])
            .map_err(mls)?;

        let pending = PendingAdd {
            commit: commit.tls_serialize_detached()?,
            welcome: welcome.tls_serialize_detached()?,
            mailbox,
        };
        self.db
            .set_pending_add(Some((&pending.commit, &pending.welcome, &pending.mailbox)))?;
        let commit = pending.commit.clone();
        self.pending = Some(pending);
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

        self.atomically(|m| m.join_unit(welcome))
    }

    /// [`Member::join`] for a parsed `Welcome`, minus the unit-of-work
    /// wrapper.
    fn join_unit(&mut self, welcome: Welcome) -> Result<()> {
        let provider = self.db.provider(&self.crypto);
        let group = StagedWelcome::new_from_welcome(
            &provider,
            Self::config().join_config(),
            welcome,
            None, // ratchet tree travels in the Welcome
        )
        .map_err(mls)?
        .into_group(&provider)
        .map_err(mls)?;

        self.group = Some(group);
        self.record_group()
    }

    /// Encrypt an application message for the group.
    ///
    /// # Errors
    /// If this member has no group, or encryption fails.
    pub fn send(&mut self, text: &str) -> Result<Vec<u8>> {
        // Encrypting ratchets the sender key. The ciphertext is returned only
        // once that step is flushed to the drive, so neither a crash nor a
        // power cut can make this client encrypt again with the same key.
        self.atomically(|m| {
            m.db.flush_on_commit();
            let provider = m.db.provider(&m.crypto);
            let group = m.group.as_mut().ok_or(Error::NoGroup)?;
            let epoch = group.epoch().as_u64();
            let wire = group
                .create_message(&provider, &m.signer, text.as_bytes())
                .map_err(mls)?
                .tls_serialize_detached()?;
            m.remember_sent(Sent {
                wire: wire.clone(),
                epoch,
                text: text.to_owned(),
            })?;
            Ok(wire)
        })
    }

    /// Process an incoming message. Returns `Some` for application messages,
    /// `None` for handshake traffic that was applied to group state.
    ///
    /// # Errors
    /// If the message is malformed, this member has no group, or the message
    /// cannot be decrypted.
    pub fn receive(&mut self, wire: &[u8]) -> Result<Event> {
        // Receiving advances ratchet state whether or not it was a message,
        // and a failure must leave none of that behind.
        self.atomically(|m| m.receive_unit(wire))
    }

    /// [`Member::receive`], minus the unit-of-work wrapper.
    fn receive_unit(&mut self, wire: &[u8]) -> Result<Event> {
        // A staged commit coming back means the relay ordered it first and
        // nothing beat it: apply it and release the Welcome.
        if let Some(pending) = self.pending.take_if(|p| p.commit == wire) {
            self.db.set_pending_add(None)?;
            let provider = self.db.provider(&self.crypto);
            let group = self.group.as_mut().ok_or(Error::NoGroup)?;
            group.merge_pending_commit(&provider).map_err(mls)?;
            return Ok(Event::Admitted {
                welcome: pending.welcome,
                mailbox: pending.mailbox,
            });
        }
        if let Some(sent) = self.take_sent(wire)? {
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
        let provider = self.db.provider(&self.crypto);
        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        if superseded {
            self.pending = None;
            self.db.set_pending_add(None)?;
            group
                .clear_pending_commit(provider.storage())
                .map_err(mls)?;
        }

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
        let processed = group.process_message(&provider, protocol).map_err(mls)?;

        let event = match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                Event::Message(String::from_utf8_lossy(&app.into_bytes()).into_owned())
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                group.merge_staged_commit(&provider, *commit).map_err(mls)?;
                if superseded {
                    Event::AddSuperseded
                } else {
                    Event::Advanced
                }
            }
            _ => Event::Advanced,
        };
        Ok(event)
    }

    /// Record a message so the relay's echo of it can be recognised, even
    /// after a restart.
    fn remember_sent(&mut self, sent: Sent) -> Result<()> {
        self.db
            .push_sent(&sent.wire, sent.epoch, &sent.text, SENT_HISTORY)?;
        if self.sent.len() == SENT_HISTORY {
            self.sent.pop_front();
        }
        self.sent.push_back(sent);
        Ok(())
    }

    /// Claim the record of a sent message whose echo has arrived.
    ///
    /// Removed rather than just matched, so a message is resent at most once
    /// per echo: a second copy of a stale echo must not send it twice.
    fn take_sent(&mut self, wire: &[u8]) -> Result<Option<Sent>> {
        let Some(at) = self.sent.iter().position(|sent| sent.wire == wire) else {
            return Ok(None);
        };
        self.db.remove_sent(wire)?;
        Ok(self.sent.remove(at))
    }

    // ---- Following the relay (#105) ----

    /// The requests to send on connecting to the relay, in order.
    ///
    /// One `Resume` per address this client reads, from its saved cursor, so
    /// it catches up on whatever it missed and then goes live. While waiting
    /// to be admitted, the offer is published again too: if nobody was
    /// online to answer it before, someone may be now, and a member who has
    /// already admitted this client ignores the repeat.
    ///
    /// # Errors
    /// If the store cannot be read, or a saved address is invalid.
    pub fn connect(&mut self) -> Result<Vec<Vec<u8>>> {
        self.live.clear();
        self.resuming.clear();
        let mut requests = Vec::new();
        for follow in self.db.follows()? {
            requests.push(
                Request::Resume {
                    address: Address::new(follow.address)?,
                    from: follow.next_seq,
                }
                .encode()?,
            );
            if let Some((to, offer)) = follow.offer {
                requests.push(encode_publish(&to, FrameKind::KeyPackage, &offer)?);
            }
        }
        Ok(requests)
    }

    /// Whether this client has asked to join a group and is still waiting
    /// for its `Welcome`. Survives a restart: [`Member::connect`] keeps
    /// reading the mailbox and re-sends the offer.
    ///
    /// # Errors
    /// If the store cannot be read.
    pub fn waiting_to_join(&self) -> Result<bool> {
        Ok(self.group.is_none() && self.db.follows()?.iter().any(|f| f.offer.is_some()))
    }

    /// Ask to join the group on `channel`: make a `KeyPackage` and read its
    /// mailbox, where the `Welcome` will come. Send [`Member::connect`]'s
    /// requests afterwards; they include the offer itself.
    ///
    /// # Errors
    /// If the key package cannot be built or recorded.
    pub fn offer(&mut self, channel: &[u8]) -> Result<()> {
        self.atomically(|m| {
            let offer = m.key_package()?;
            let mailbox = m.mailbox(&offer)?;
            m.db.follow(&mailbox, 0, Some((channel, &offer)))
        })
    }

    /// Process anything the relay sent, returning what to do about it.
    ///
    /// Frames are applied in sequence order, once each: a frame at or below
    /// this client's cursor is a duplicate (catch-up and live delivery can
    /// overlap at a reconnect) and is dropped. The cursor advances in the
    /// same transaction as the state the frame changed, so a crash can never
    /// apply a frame twice or skip one.
    ///
    /// Join requests are answered only once this client is live on its
    /// channel. One seen during catch-up is held, and once live it is
    /// answered only if the joiner is not already a member, since someone
    /// online may well have admitted them meanwhile.
    ///
    /// A frame that fails is handled by why it failed. One that is malformed,
    /// or that openmls rejects, would fail the same way every time, since the
    /// relay's order is fixed, so it is stepped over: otherwise anyone able
    /// to publish garbage to a channel could wedge every member of it. One
    /// that fails to save (a full disk, say) leaves the cursor where it was.
    /// The next frame then arrives past it, and this client resumes from the
    /// cursor, which fetches the failed frame again and retries it. Skipping
    /// a commit for that reason would leave this client an epoch behind for
    /// good.
    ///
    /// # Errors
    /// If the message is malformed, or a frame could not be applied.
    pub fn deliver(&mut self, wire: &[u8]) -> Result<Vec<Effect>> {
        match Inbound::decode(wire)? {
            Inbound::Delivery(delivery) => {
                let address = delivery.address.into_bytes();
                let Some(next) = self.db.cursor(&address)? else {
                    return Ok(Vec::new()); // not an address this client reads
                };
                if delivery.seq < next {
                    return Ok(Vec::new());
                }
                if delivery.seq > next {
                    // Frames were missed: one failed to save, or the relay
                    // skipped some. Ask for them again from the cursor, once;
                    // anything that arrives meanwhile is in those pages too.
                    if !self.resuming.insert(address.clone()) {
                        return Ok(Vec::new());
                    }
                    let resume = Request::Resume {
                        address: Address::new(address)?,
                        from: next,
                    };
                    return Ok(vec![Effect::Send(resume.encode()?)]);
                }
                let seq = delivery.seq;
                let applied = self.atomically(|m| {
                    let effects = m.apply(&address, seq, &delivery.payload)?;
                    m.db.advance(&address, seq + 1)?;
                    Ok(effects)
                });
                if let Err(error) = &applied
                    && error.is_frame_fault()
                {
                    self.db.atomically(|| self.db.advance(&address, seq + 1))?;
                }
                applied
            }
            Inbound::Page {
                address,
                next,
                live,
            } => {
                let address = address.into_bytes();
                let Some(cursor) = self.db.cursor(&address)? else {
                    return Ok(Vec::new());
                };
                if !live {
                    let more = Request::Resume {
                        address: Address::new(address)?,
                        from: cursor.max(next),
                    };
                    return Ok(vec![Effect::Send(more.encode()?)]);
                }
                self.resuming.remove(&address);
                self.live.insert(address.clone());
                let mut effects = vec![Effect::Live {
                    address: address.clone(),
                }];
                if self.channel().as_deref() == Some(address.as_slice()) {
                    effects.extend(self.atomically(Self::answer_offers)?);
                }
                Ok(effects)
            }
            Inbound::Unavailable {
                address,
                floor,
                next,
            } => {
                let address = address.into_bytes();
                let Some(cursor) = self.db.cursor(&address)? else {
                    return Ok(Vec::new());
                };
                if cursor <= next {
                    return Ok(vec![Effect::Unavailable {
                        address,
                        floor,
                        next,
                    }]);
                }
                // Ahead of everything the relay has: it lost or replaced its
                // store, and numbering started again. Missing frames are not
                // the risk here, so follow its numbering from the start.
                self.db.atomically(|| self.db.advance(&address, floor))?;
                let resume = Request::Resume {
                    address: Address::new(address.clone())?,
                    from: floor,
                };
                Ok(vec![
                    Effect::RelayReset { address },
                    Effect::Send(resume.encode()?),
                ])
            }
        }
    }

    /// Apply one frame from `address`, at `seq`.
    fn apply(&mut self, address: &[u8], seq: u64, payload: &[u8]) -> Result<Vec<Effect>> {
        let (kind, body) = decode_frame(payload)?;
        let channel = self.channel();
        let on_channel = channel.as_deref() == Some(address);
        match kind {
            FrameKind::KeyPackage if on_channel => {
                self.db.defer(&body)?;
                self.answer_offers()
            }
            FrameKind::Welcome if self.group.is_none() => {
                let (admitted_at, welcome) = split_welcome(&body)?;
                let msg = MlsMessageIn::tls_deserialize_exact(welcome)?;
                let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
                    return Err(Error::UnexpectedMessage {
                        expected: "Welcome",
                    });
                };
                self.join_unit(welcome)?;
                let channel = self.channel().ok_or(Error::NoGroup)?;
                // Read the channel from just after the commit that admitted
                // this client. Anything ordered after it, even if it raced
                // the Welcome here, is then applied rather than lost.
                self.db.unfollow_mailboxes()?;
                self.db.follow(&channel, admitted_at + 1, None)?;
                let resume = Request::Resume {
                    address: Address::new(channel)?,
                    from: admitted_at + 1,
                };
                Ok(vec![Effect::Joined, Effect::Send(resume.encode()?)])
            }
            FrameKind::Commit | FrameKind::Application if on_channel => {
                let channel = channel.unwrap_or_default();
                let mut effects = Vec::new();
                match self.receive_unit(&body)? {
                    Event::Message(text) => effects.push(Effect::Message(text)),
                    Event::Admitted { welcome, mailbox } => {
                        let body = join_welcome(seq, &welcome);
                        effects.push(Effect::Send(encode_publish(
                            &mailbox,
                            FrameKind::Welcome,
                            &body,
                        )?));
                        effects.push(Effect::Admitted);
                        effects.extend(self.answer_offers()?);
                    }
                    Event::AddSuperseded => {
                        effects.push(Effect::AddSuperseded);
                        effects.extend(self.answer_offers()?);
                    }
                    Event::Advanced => effects.push(Effect::Advanced),
                    Event::Resend { message } => effects.push(Effect::Send(encode_publish(
                        &channel,
                        FrameKind::Application,
                        &message,
                    )?)),
                    Event::Echo | Event::Stale => {}
                }
                Ok(effects)
            }
            // A Welcome once already in a group, or channel traffic for a
            // channel this client is not in.
            _ => Ok(Vec::new()),
        }
    }

    /// Answer the oldest held join request whose joiner is not already a
    /// member, if this client is live on its channel and not mid-add.
    ///
    /// One at a time: openmls stages one commit at once, and the next request
    /// is answered when this one's commit resolves.
    fn answer_offers(&mut self) -> Result<Vec<Effect>> {
        let Some(channel) = self.channel() else {
            return Ok(Vec::new());
        };
        if self.pending.is_some() || !self.live.contains(&channel) {
            return Ok(Vec::new());
        }
        while let Some(offer) = self.db.take_deferred()? {
            // Expired or malformed: nothing to answer.
            let Ok(kp) = self.validate_key_package(&offer) else {
                continue;
            };
            if self.is_member(&kp) {
                continue;
            }
            let commit = self.propose_add_unit(kp)?;
            let publish = encode_publish(&channel, FrameKind::Commit, &commit)?;
            return Ok(vec![Effect::Send(publish)]);
        }
        Ok(Vec::new())
    }

    /// Whether the client that made `kp` is already in the group, matched
    /// exactly by signature key.
    fn is_member(&self, kp: &KeyPackage) -> bool {
        let key = kp.leaf_node().signature_key().as_slice();
        self.group
            .as_ref()
            .is_some_and(|g| g.members().any(|m| m.signature_key == key))
    }

    /// Display name of this member.
    #[must_use]
    pub fn identity(&self) -> String {
        String::from_utf8_lossy(self.credential.credential.serialized_content()).into_owned()
    }

    /// The channel this member's group lives on, if it has joined one.
    ///
    /// It is the MLS group id: random, and already carried in the clear on
    /// every message the group sends, so using it as the relay address
    /// discloses nothing a separate id would have hidden.
    #[must_use]
    pub fn channel(&self) -> Option<Vec<u8>> {
        self.group
            .as_ref()
            .map(|g| g.group_id().as_slice().to_vec())
    }

    /// Number of members currently in the group.
    #[must_use]
    pub fn member_count(&self) -> usize {
        self.group.as_ref().map_or(0, |g| g.members().count())
    }
}

/// The group `id` names, from the store, or `None` if there is no id.
fn load_group(provider: &Provider<'_>, id: Option<&[u8]>) -> Result<Option<MlsGroup>> {
    let Some(id) = id else {
        return Ok(None);
    };
    MlsGroup::load(provider.storage(), &GroupId::from_slice(id))
        .map_err(mls)?
        .ok_or(Error::CorruptState(
            "group id recorded but no group in store",
        ))
        .map(Some)
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

/// A `Welcome` frame's body: the sequence number of the commit that admitted
/// the joiner, then the `Welcome`. The joiner reads the channel from just
/// after that commit.
fn join_welcome(admitted_at: u64, welcome: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(8 + welcome.len());
    body.extend_from_slice(&admitted_at.to_be_bytes());
    body.extend_from_slice(welcome);
    body
}

/// Split a body made by [`join_welcome`].
fn split_welcome(body: &[u8]) -> Result<(u64, &[u8])> {
    let (seq, welcome) = body.split_at_checked(8).ok_or(Error::MalformedFrame)?;
    let seq = u64::from_be_bytes(seq.try_into().map_err(|_| Error::MalformedFrame)?);
    Ok((seq, welcome))
}

/// Prefix a payload with its frame tag.
fn encode_frame(kind: FrameKind, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 1);
    v.push(kind.tag());
    v.extend_from_slice(body);
    v
}

/// Split a frame into its kind and payload.
fn decode_frame(wire: &[u8]) -> Result<(FrameKind, Vec<u8>)> {
    let (&tag, body) = wire.split_first().ok_or(Error::MalformedFrame)?;
    let kind = FrameKind::from_tag(tag).ok_or(Error::MalformedFrame)?;
    Ok((kind, body.to_vec()))
}

/// A request asking the relay to deliver `kind` and `body` to everyone
/// subscribed to `to`.
///
/// # Errors
/// If `to` is empty or longer than an address can be.
pub fn encode_publish(to: &[u8], kind: FrameKind, body: &[u8]) -> Result<Vec<u8>> {
    Ok(openconv_wire::Request::Publish {
        to: openconv_wire::Address::new(to.to_vec())?,
        payload: encode_frame(kind, body),
    }
    .encode()?)
}

/// A request to start receiving whatever is published to `addresses`.
///
/// # Errors
/// If any address is empty or too long, or there are more than 255.
pub fn encode_subscribe(addresses: &[Vec<u8>]) -> Result<Vec<u8>> {
    let addresses = addresses
        .iter()
        .map(|a| openconv_wire::Address::new(a.clone()))
        .collect::<std::result::Result<_, _>>()?;
    Ok(openconv_wire::Request::Subscribe(addresses).encode()?)
}

/// A frame as the relay delivers it.
#[derive(Debug)]
pub struct Envelope {
    /// The address it was published to: a channel, or this client's mailbox.
    pub address: Vec<u8>,
    /// The relay's position for this frame. Monotonic per address, and the
    /// same for every subscriber, which is what decides competing commits.
    pub seq: u64,
    /// When the relay accepted it, in milliseconds since the Unix epoch. For
    /// display only; order by `seq`.
    pub accepted_ms: u64,
    /// What the frame carries.
    pub kind: FrameKind,
    /// The payload behind the tag.
    pub body: Vec<u8>,
}

/// Split a delivery from the relay into its address, position, kind and
/// payload.
///
/// # Errors
/// If the envelope is malformed, or the frame inside it is empty or carries
/// an unknown tag.
pub fn decode_delivery(wire: &[u8]) -> Result<Envelope> {
    let delivery = openconv_wire::Delivery::decode(wire)?;
    let (kind, body) = decode_frame(&delivery.payload)?;
    Ok(Envelope {
        address: delivery.address.into_bytes(),
        seq: delivery.seq,
        accepted_ms: delivery.accepted_ms,
        kind,
        body,
    })
}

/// A channel id as text, for sharing out of band until there are invites.
/// Lowercase hex.
#[must_use]
pub fn format_channel_id(channel: &[u8]) -> String {
    use std::fmt::Write;
    channel.iter().fold(String::new(), |mut out, b| {
        // Writing to a String cannot fail.
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Parse a channel id from [`format_channel_id`]'s text. Surrounding
/// whitespace is ignored, since this usually arrives by copy and paste.
///
/// # Errors
/// If the text is not an even-length run of hex digits of a valid address
/// length.
pub fn parse_channel_id(text: &str) -> Result<Vec<u8>> {
    let text = text.trim();
    // Checked up front because `from_str_radix` accepts a leading `+`, so
    // "+f" would otherwise parse as a byte.
    if text.is_empty()
        || !text.len().is_multiple_of(2)
        || text.len() / 2 > openconv_wire::MAX_ADDRESS_LEN
        || !text.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::InvalidChannelId);
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or(Error::InvalidChannelId)
        })
        .collect()
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
        let Event::Admitted { welcome, .. } = host.receive(&commit).unwrap() else {
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
        let Event::Admitted { welcome, .. } = alice.receive(&alice_commit).unwrap() else {
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
        let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
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
        let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
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
            let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
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

    /// Routing only works if the admitter addresses the `Welcome` to exactly
    /// the mailbox the joiner subscribed to — and that has to be the
    /// `KeyPackageRef` MLS encrypted the `Welcome` for, or the right member
    /// receives a `Welcome` meant for someone else.
    #[test]
    fn welcome_is_addressed_to_the_joiners_mailbox() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let carol = Member::new("carol").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);

        let offer = carol.key_package().unwrap();
        let subscribed = carol.mailbox(&offer).unwrap();

        // Bob admits, not the creator: any member's Welcome must route.
        let commit = bob.propose_add(&offer).unwrap();
        let Event::Admitted { welcome, mailbox } = bob.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        assert_eq!(mailbox, subscribed);

        let msg = MlsMessageIn::tls_deserialize_exact(&welcome).unwrap();
        let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
            panic!("expected a Welcome");
        };
        assert!(
            welcome
                .secrets()
                .iter()
                .any(|s| s.new_member().as_slice() == mailbox.as_slice()),
            "the Welcome is not encrypted for the mailbox it is addressed to"
        );

        // Two offers from one member are two mailboxes, so a Welcome for an
        // old offer cannot land in a newer one's inbox.
        let other = carol.key_package().unwrap();
        assert_ne!(carol.mailbox(&other).unwrap(), subscribed);
    }

    /// Every member must agree on the channel, or they publish into
    /// different rooms. It is the group id, so it is shared by construction;
    /// this checks nobody made it something else.
    #[test]
    fn every_member_is_on_the_same_channel() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        assert_eq!(alice.channel(), None);

        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);
        let commit = alice.propose_add(&carol.key_package().unwrap()).unwrap();
        let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        carol.join(&welcome).unwrap();

        let channel = alice.channel().unwrap();
        assert_eq!(channel.len(), 16, "expected openmls's random group id");
        assert_eq!(bob.channel().as_ref(), Some(&channel));
        assert_eq!(carol.channel().as_ref(), Some(&channel));

        // A second group is a second channel.
        let mut dave = Member::new("dave").unwrap();
        dave.create_group().unwrap();
        assert_ne!(dave.channel().unwrap(), channel);
    }

    #[test]
    fn channel_ids_round_trip_through_text() {
        let channel = vec![0x00, 0x0f, 0xa5, 0xff];
        let text = format_channel_id(&channel);
        assert_eq!(text, "000fa5ff");
        assert_eq!(parse_channel_id(&text).unwrap(), channel);
        assert_eq!(parse_channel_id("  000FA5FF\n").unwrap(), channel);

        for bad in ["", "abc", "zz", "0x00", "+f+f", &"00".repeat(256)] {
            assert!(
                matches!(parse_channel_id(bad), Err(Error::InvalidChannelId)),
                "{bad:?} should not parse"
            );
        }
    }

    /// The core builds requests and the relay parses them with the same
    /// crate, but check the frame tag survives the trip inside the payload.
    #[test]
    fn requests_are_what_the_relay_decodes() {
        let wire = encode_publish(b"room", FrameKind::Commit, b"body").unwrap();
        let openconv_wire::Request::Publish { to, payload } =
            openconv_wire::Request::decode(&wire).unwrap()
        else {
            panic!("expected a publish");
        };
        assert_eq!(to.as_bytes(), b"room");
        assert_eq!(
            decode_frame(&payload).unwrap(),
            (FrameKind::Commit, b"body".to_vec())
        );

        assert!(matches!(
            encode_publish(&[], FrameKind::Commit, b"body"),
            Err(Error::Wire(_))
        ));
        assert!(matches!(
            encode_subscribe(&[b"room".to_vec(), vec![]]),
            Err(Error::Wire(_))
        ));
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
        let delivery = |payload: &[u8]| {
            openconv_wire::Delivery {
                address: openconv_wire::Address::new(vec![1; 16]).unwrap(),
                seq: 0,
                accepted_ms: 0,
                payload: payload.to_vec(),
            }
            .encode()
        };
        // A broken envelope, then a sound envelope around a broken frame.
        assert!(matches!(decode_delivery(&[1, 2, 3]), Err(Error::Wire(_))));
        assert!(matches!(
            decode_delivery(&delivery(&[0xff, 1])),
            Err(Error::MalformedFrame)
        ));
        assert!(matches!(
            decode_delivery(&delivery(&[])),
            Err(Error::MalformedFrame)
        ));
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

    /// A wrong key is one failure; a flipped byte is another. SQLCipher's
    /// per-page MAC should catch tampering, and the vault must refuse rather
    /// than load partial state.
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

    /// Key material must never be readable on disk: not in the database,
    /// and not in the WAL or shared-memory files beside it.
    #[test]
    fn vault_file_is_encrypted() {
        let (path, vault) = test_vault("secretive");
        let mut member = Member::restore(vault, "secretive").unwrap();
        member.create_group().unwrap();
        member.send("a message").unwrap();

        // Checked while still open, so the WAL is there to be read too.
        let dir = path.parent().unwrap();
        let mut files = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let raw = std::fs::read(entry.unwrap().path()).unwrap();
            files += 1;
            assert!(
                !raw.starts_with(b"SQLite format 3"),
                "a store file has a plaintext SQLite header"
            );
            assert!(
                !raw.windows(9).any(|w| w == b"secretive"),
                "identity readable in a store file"
            );
        }
        assert!(
            files >= 2,
            "expected the database and its WAL, found {files} files"
        );
        drop(member);
        std::fs::remove_dir_all(dir).ok();
    }

    /// Whatever other members will see must be flushed to the drive before
    /// it leaves: a power cut that rolled back a sender's ratchet would make
    /// it encrypt again under used keys. Receives are not flushed, because a
    /// lost one is simply applied again, and one flush per received frame
    /// would make catching up slow.
    #[test]
    fn only_what_others_see_is_flushed_to_the_drive() {
        let (path, vault) = test_vault("flusher");
        let mut bob = Member::restore(vault, "flusher").unwrap();
        let mut alice = Member::new("alice").unwrap();
        let mut carol = Member::new("carol").unwrap();
        let flushed = |m: &Member| m.db.flushed.get();

        // A key package is a promise to accept a Welcome later.
        let before = flushed(&bob);
        let offer = bob.key_package().unwrap();
        assert_eq!(flushed(&bob), before + 1, "key package not flushed");

        alice.create_group().unwrap();
        let commit = alice.propose_add(&offer).unwrap();
        let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        let before = flushed(&bob);
        bob.join(&welcome).unwrap();
        assert_eq!(flushed(&bob), before, "joining is a receive");

        let before = flushed(&bob);
        let wire = bob.send("hello").unwrap();
        assert_eq!(flushed(&bob), before + 1, "send not flushed");
        alice.receive(&wire).unwrap();

        let before = flushed(&bob);
        bob.receive(&alice.send("hi").unwrap()).unwrap();
        assert_eq!(flushed(&bob), before, "an ordinary receive was flushed");

        let before = flushed(&bob);
        let commit = bob.propose_add(&carol.key_package().unwrap()).unwrap();
        assert_eq!(flushed(&bob), before + 1, "commit not flushed");
        let Event::Admitted { welcome, .. } = bob.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        carol.join(&welcome).unwrap();
        alice.receive(&commit).unwrap();

        // A receive that resends produces new ciphertext, so it must flush,
        // even though the send happens inside the receive.
        let raced = bob.send("crossed").unwrap();
        let commit = alice
            .propose_add(&Member::new("dave").unwrap().key_package().unwrap())
            .unwrap();
        alice.receive(&commit).unwrap();
        bob.receive(&commit).unwrap();
        let before = flushed(&bob);
        assert!(matches!(bob.receive(&raced).unwrap(), Event::Resend { .. }));
        assert_eq!(
            flushed(&bob),
            before + 1,
            "a resend inside a receive was not flushed"
        );

        drop(bob);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A relay delivery of `body` on `address` at `seq`, as the wire carries it.
    fn delivery(address: &[u8], seq: u64, kind: FrameKind, body: &[u8]) -> Vec<u8> {
        openconv_wire::Delivery {
            address: openconv_wire::Address::new(address.to_vec()).unwrap(),
            seq,
            accepted_ms: 0,
            payload: encode_frame(kind, body),
        }
        .encode()
    }

    /// Catch-up and live delivery can overlap at a reconnect, so the same
    /// frame can arrive twice. The second copy must be dropped: applying an
    /// MLS message twice fails, and would be reported as an error.
    #[test]
    fn a_frame_delivered_twice_is_applied_once() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);
        let channel = alice.channel().unwrap();
        // Alice already holds the admitting commit; her cursor starts at 0.
        let wire = delivery(
            &channel,
            0,
            FrameKind::Application,
            &bob.send("once").unwrap(),
        );

        let first = alice.deliver(&wire).unwrap();
        assert!(matches!(first.as_slice(), [Effect::Message(t)] if t == "once"));
        assert!(
            alice.deliver(&wire).unwrap().is_empty(),
            "the repeat was applied"
        );
    }

    /// A frame beyond the next expected one means frames were missed. The
    /// client asks for them again from its cursor, once, and ignores what
    /// arrives meanwhile, since the resumed pages will carry it.
    #[test]
    fn a_gap_resumes_from_the_cursor_once() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);
        let channel = alice.channel().unwrap();
        let ahead = delivery(&channel, 5, FrameKind::Application, &bob.send("x").unwrap());

        let effects = alice.deliver(&ahead).unwrap();
        let [Effect::Send(request)] = effects.as_slice() else {
            panic!("a gap should ask the relay to resume");
        };
        let expected = Request::Resume {
            address: Address::new(channel.clone()).unwrap(),
            from: 0,
        };
        assert_eq!(request, &expected.encode().unwrap());
        assert_eq!(alice.db.cursor(&channel).unwrap(), Some(0));
        assert!(alice.deliver(&ahead).unwrap().is_empty(), "resumed twice");
    }

    /// A frame that fails because this client could not save it is not a bad
    /// frame. Skipping it would be permanent, and for a commit that means
    /// being an epoch behind for good. The cursor must stay, so the frame is
    /// fetched and applied once saving works again. Uses openmls's own
    /// storage writes failing, which is what pins the classification in
    /// `mls()`.
    #[test]
    fn a_frame_that_fails_to_save_is_retried_not_skipped() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);
        let channel = alice.channel().unwrap();
        let commit = bob.propose_add(&carol.key_package().unwrap()).unwrap();
        let Event::Admitted { welcome, .. } = bob.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        carol.join(&welcome).unwrap();
        let wire = delivery(&channel, 0, FrameKind::Commit, &commit);

        alice.db.fail_group_writes(true);
        let failed = alice.deliver(&wire);
        assert!(
            matches!(failed, Err(Error::Store(_))),
            "a save failure was not classed as one: {failed:?}"
        );
        alice.db.fail_group_writes(false);
        assert_eq!(
            alice.db.cursor(&channel).unwrap(),
            Some(0),
            "the commit was skipped"
        );

        assert!(matches!(
            alice.deliver(&wire).unwrap().as_slice(),
            [Effect::Advanced]
        ));
        assert_eq!(alice.member_count(), 3);
        let hello = carol.send("hello").unwrap();
        assert_message(alice.receive(&hello).unwrap(), "hello");
    }

    /// A frame openmls rejects on its merits fails the same way every time,
    /// so it is stepped over rather than blocking everything behind it.
    #[test]
    fn a_frame_openmls_rejects_is_stepped_over() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut bob);
        let channel = alice.channel().unwrap();
        let mut forged = bob.send("tampered").unwrap();
        let last = forged.len() - 1;
        forged[last] ^= 0xff;

        assert!(matches!(
            alice.deliver(&delivery(&channel, 0, FrameKind::Application, &forged)),
            Err(Error::Mls(_))
        ));
        assert_eq!(alice.db.cursor(&channel).unwrap(), Some(1));
        let next = delivery(
            &channel,
            1,
            FrameKind::Application,
            &bob.send("fine").unwrap(),
        );
        assert!(
            matches!(alice.deliver(&next).unwrap().as_slice(), [Effect::Message(t)] if t == "fine")
        );
    }

    /// A relay whose history is behind the client was reset: the client
    /// follows its numbering from the start. One whose history begins after
    /// the client's cursor dropped frames the client never saw, which cannot
    /// be resynced away, and is reported.
    #[test]
    fn a_reset_relay_is_followed_but_a_lost_history_is_reported() {
        let mut alice = Member::new("alice").unwrap();
        alice.create_group().unwrap();
        let channel = alice.channel().unwrap();
        alice.db.advance(&channel, 40).unwrap();
        let unavailable = |floor, next| {
            openconv_wire::Inbound::Unavailable {
                address: Address::new(channel.clone()).unwrap(),
                floor,
                next,
            }
            .encode()
        };

        // The relay holds 0..3: it was reset after this client read to 40.
        let effects = alice.deliver(&unavailable(0, 3)).unwrap();
        let [Effect::RelayReset { .. }, Effect::Send(request)] = effects.as_slice() else {
            panic!("a reset relay should be resynced to, got {effects:?}");
        };
        let expected = Request::Resume {
            address: Address::new(channel.clone()).unwrap(),
            from: 0,
        };
        assert_eq!(request, &expected.encode().unwrap());
        assert_eq!(alice.db.cursor(&channel).unwrap(), Some(0));

        // The relay holds 50..60 and this client is at 0: frames it never
        // saw are gone.
        let effects = alice.deliver(&unavailable(50, 60)).unwrap();
        assert!(matches!(
            effects.as_slice(),
            [Effect::Unavailable { floor: 50, .. }]
        ));
        assert_eq!(
            alice.db.cursor(&channel).unwrap(),
            Some(0),
            "the cursor moved"
        );
    }

    /// A client that restarts between staging an add and seeing its commit
    /// come back must still recognise that commit as its own. Catch-up will
    /// deliver it, and openmls cannot process a member's own commit as if
    /// someone else had sent it.
    #[test]
    fn a_staged_add_survives_a_restart() {
        let (path, vault) = test_vault("stager");
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        let commit = {
            let mut alice = Member::restore(vault, "stager").unwrap();
            alice.create_group().unwrap();
            admit(&mut alice, &mut bob);
            alice.propose_add(&carol.key_package().unwrap()).unwrap()
        };

        let mut alice = Member::restore(Vault::with_key(&path, [7u8; 32]), "stager").unwrap();
        let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
            panic!("the restarted client did not recognise its own commit");
        };
        bob.receive(&commit).unwrap();
        carol.join(&welcome).unwrap();
        assert_eq!(alice.member_count(), 3);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A message whose echo arrives after a restart is still recognised, so
    /// one that landed behind a commit is still resent.
    #[test]
    fn a_sent_message_survives_a_restart() {
        let (path, vault) = test_vault("sender");
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();
        let raced = {
            let mut alice = Member::restore(vault, "sender").unwrap();
            alice.create_group().unwrap();
            admit(&mut alice, &mut bob);
            alice.send("sent before a restart").unwrap()
        };
        // Bob admits carol, and the relay orders that before alice's message.
        let commit = bob.propose_add(&carol.key_package().unwrap()).unwrap();
        let Event::Admitted { welcome, .. } = bob.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        carol.join(&welcome).unwrap();

        let mut alice = Member::restore(Vault::with_key(&path, [7u8; 32]), "sender").unwrap();
        alice.receive(&commit).unwrap();
        let Event::Resend { message } = alice.receive(&raced).unwrap() else {
            panic!("the restarted client did not recognise its own message");
        };
        assert_message(carol.receive(&message).unwrap(), "sent before a restart");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A failed operation must leave nothing behind, on disk or in memory.
    /// openmls writes several rows per operation, so without the savepoint a
    /// failure partway would persist half of it.
    #[test]
    fn a_failed_operation_leaves_no_trace() {
        let (path, vault) = test_vault("undone");
        let mut alice = Member::restore(vault, "undone").unwrap();

        // Create a group, then fail after it is written.
        let failed = alice.atomically(|m| {
            m.create_group_unit()?;
            Err::<(), _>(Error::NoGroup)
        });
        assert!(failed.is_err());
        assert_eq!(alice.channel(), None, "the group survived in memory");
        drop(alice);

        let alice = Member::restore(Vault::with_key(&path, [7u8; 32]), "undone").unwrap();
        assert_eq!(alice.channel(), None, "the group survived on disk");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// A joiner's private init key is stored when its `KeyPackage` is made,
    /// so the `Welcome` can arrive after a restart: what lets someone ask to
    /// join, go offline, and join when they come back.
    #[test]
    fn a_key_package_survives_a_restart() {
        let (path, vault) = test_vault("joiner");
        let offer = Member::restore(vault, "joiner")
            .unwrap()
            .key_package()
            .unwrap();

        let mut alice = Member::new("alice").unwrap();
        let mut carol = Member::new("carol").unwrap();
        alice.create_group().unwrap();
        admit(&mut alice, &mut carol);
        let commit = alice.propose_add(&offer).unwrap();
        let Event::Admitted { welcome, .. } = alice.receive(&commit).unwrap() else {
            panic!("uncontested commit should have been admitted");
        };
        carol.receive(&commit).unwrap();

        let mut joiner = Member::restore(Vault::with_key(&path, [7u8; 32]), "joiner").unwrap();
        joiner.join(&welcome).unwrap();
        assert_eq!(joiner.member_count(), 3);
        let wire = joiner.send("back, and in").unwrap();
        assert_message(alice.receive(&wire).unwrap(), "back, and in");
        assert_message(carol.receive(&wire).unwrap(), "back, and in");
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
