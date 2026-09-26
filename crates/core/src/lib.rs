//! Core client logic for OpenConv: MLS group state and message encryption.
//!
//! The server never sees plaintext. Everything in this crate runs on the
//! client; the wire carries only TLS-serialized MLS messages.

use openmls::prelude::{tls_codec::*, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use std::path::PathBuf;

pub mod store;
pub use store::{Provider, Snapshot, Vault};

uniffi::setup_scaffolding!();

pub mod ffi;

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no group joined yet")]
    NoGroup,
    #[error("expected a {expected} message")]
    UnexpectedMessage { expected: &'static str },
    #[error("mls: {0}")]
    Mls(String),
    #[error("codec: {0}")]
    Codec(#[from] tls_codec::Error),
    #[error("malformed frame")]
    MalformedFrame,
    #[error("store: {0}")]
    Store(String),
    #[error("saved state is unreadable: {0}")]
    CorruptState(&'static str),
}

type Result<T> = std::result::Result<T, Error>;

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
}

impl Member {
    /// Create a fresh identity. `identity` is the display name carried in
    /// the MLS basic credential.
    pub fn new(identity: &str) -> Result<Self> {
        let provider = Provider::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).map_err(mls)?;
        signer.store(provider.storage()).map_err(mls)?;

        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };

        Ok(Self { provider, signer, credential, group: None, vault: None })
    }

    /// Open a member backed by an encrypted file, restoring previous state if
    /// the file exists and starting fresh otherwise.
    pub fn open(path: impl Into<PathBuf>, identity: &str) -> Result<Self> {
        Self::restore(Vault::open(path, identity)?, identity)
    }

    /// Open a member against an already-configured vault.
    pub fn restore(vault: Vault, identity: &str) -> Result<Self> {
        let Some(snapshot) = vault.load()? else {
            let mut member = Self::new(identity)?;
            member.vault = Some(vault);
            member.persist()?;
            return Ok(member);
        };

        let Snapshot { identity, signature_public_key, group_id, storage } = snapshot;
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
                .ok_or(Error::CorruptState("group id recorded but no group in store"))
                .map(Some)?,
            None => None,
        };

        Ok(Self { provider, signer, credential, group, vault: Some(vault) })
    }

    /// Write current state to the vault. A no-op for in-memory members.
    fn persist(&self) -> Result<()> {
        let Some(vault) = &self.vault else { return Ok(()) };

        vault.save(&Snapshot {
            identity: self.identity(),
            signature_public_key: self.signer.to_public_vec(),
            group_id: self.group.as_ref().map(|g| g.group_id().as_slice().to_vec()),
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

    /// A published KeyPackage, which anyone can use to add this member to a
    /// group. Safe to hand to the server.
    pub fn key_package(&self) -> Result<Vec<u8>> {
        let bundle = KeyPackage::builder()
            .build(CIPHERSUITE, &self.provider, &self.signer, self.credential.clone())
            .map_err(mls)?;
        Ok(MlsMessageOut::from(bundle.key_package().clone()).tls_serialize_detached()?)
    }

    /// Start a new group with just this member in it.
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

    /// Add a member using their published KeyPackage. Returns the Welcome to
    /// deliver to them, and the commit to fan out to existing members.
    pub fn add_member(&mut self, key_package: &[u8]) -> Result<Invite> {
        let msg = MlsMessageIn::tls_deserialize_exact(key_package)?;
        let MlsMessageBodyIn::KeyPackage(kp) = msg.extract() else {
            return Err(Error::UnexpectedMessage { expected: "KeyPackage" });
        };
        // Verifies the signature and lifetime before we trust it.
        let kp = kp.validate(self.provider.crypto(), ProtocolVersion::Mls10).map_err(mls)?;

        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        let (commit, welcome, _) = group
            .add_members(&self.provider, &self.signer, &[kp])
            .map_err(mls)?;
        group.merge_pending_commit(&self.provider).map_err(mls)?;

        let invite = Invite {
            welcome: welcome.tls_serialize_detached()?,
            commit: commit.tls_serialize_detached()?,
        };
        self.persist()?;
        Ok(invite)
    }

    /// Join a group from a Welcome produced by [`Member::add_member`].
    pub fn join(&mut self, welcome: &[u8]) -> Result<()> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome)?;
        let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
            return Err(Error::UnexpectedMessage { expected: "Welcome" });
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
    pub fn send(&mut self, text: &str) -> Result<Vec<u8>> {
        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        // Encrypting ratchets the sender key, so the new state must be saved.
        let wire = group
            .create_message(&self.provider, &self.signer, text.as_bytes())
            .map_err(mls)?
            .tls_serialize_detached()?;
        self.persist()?;
        Ok(wire)
    }

    /// Process an incoming message. Returns `Some` for application messages,
    /// `None` for handshake traffic that was applied to group state.
    pub fn receive(&mut self, wire: &[u8]) -> Result<Option<String>> {
        let msg = MlsMessageIn::tls_deserialize_exact(wire)?;
        let protocol: ProtocolMessage = msg
            .try_into_protocol_message()
            .map_err(|_| Error::UnexpectedMessage { expected: "application or handshake" })?;

        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        // Our own messages come back off the relay; MLS cannot decrypt them.
        if protocol.epoch() < group.epoch() {
            return Ok(None);
        }
        let processed = group.process_message(&self.provider, protocol).map_err(mls)?;

        let text = match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                Some(String::from_utf8_lossy(&app.into_bytes()).into_owned())
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                group.merge_staged_commit(&self.provider, *commit).map_err(mls)?;
                None
            }
            _ => None,
        };
        // Receiving advances ratchet state whether or not it was a message.
        self.persist()?;
        Ok(text)
    }

    /// Display name of this member.
    pub fn identity(&self) -> String {
        String::from_utf8_lossy(self.credential.credential.serialized_content()).into_owned()
    }

    /// Number of members currently in the group.
    pub fn member_count(&self) -> usize {
        self.group.as_ref().map_or(0, |g| g.members().count())
    }
}

/// What a relayed frame carries. The relay never reads this; only clients do.
#[derive(Clone, Copy, PartialEq, Eq, Debug, uniffi::Enum)]
pub enum FrameKind {
    /// A published KeyPackage, offering to be added to a group.
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
pub fn encode_frame(kind: FrameKind, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 1);
    v.push(kind.tag());
    v.extend_from_slice(body);
    v
}

/// Split a relayed frame into its kind and payload.
pub fn decode_frame(wire: &[u8]) -> Result<(FrameKind, Vec<u8>)> {
    let (&tag, body) = wire.split_first().ok_or(Error::MalformedFrame)?;
    let kind = FrameKind::from_tag(tag).ok_or(Error::MalformedFrame)?;
    Ok((kind, body.to_vec()))
}

/// The two messages produced by adding a member.
pub struct Invite {
    /// Deliver to the new member so they can join.
    pub welcome: Vec<u8>,
    /// Fan out to members who were already in the group.
    pub commit: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the slice: two members, one group, real MLS
    /// ciphertext on the wire, plaintext out the other side.
    #[test]
    fn two_members_exchange_encrypted_messages() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();

        alice.create_group().unwrap();
        assert_eq!(alice.member_count(), 1);

        let invite = alice.add_member(&bob.key_package().unwrap()).unwrap();
        bob.join(&invite.welcome).unwrap();

        assert_eq!(alice.member_count(), 2);
        assert_eq!(bob.member_count(), 2);

        let wire = alice.send("hello bob").unwrap();
        assert_eq!(bob.receive(&wire).unwrap().as_deref(), Some("hello bob"));

        let wire = bob.send("hi alice").unwrap();
        assert_eq!(alice.receive(&wire).unwrap().as_deref(), Some("hi alice"));
    }

    /// The server must never be able to read traffic it relays.
    #[test]
    fn wire_bytes_do_not_contain_plaintext() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        alice.create_group().unwrap();
        let invite = alice.add_member(&bob.key_package().unwrap()).unwrap();
        bob.join(&invite.welcome).unwrap();

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
        let invite = alice.add_member(&bob.key_package().unwrap()).unwrap();
        bob.join(&invite.welcome).unwrap();

        // Eve builds her own group and tries to process alice's traffic.
        eve.create_group().unwrap();
        let wire = alice.send("secret").unwrap();
        assert!(eve.receive(&wire).is_err(), "outsider decrypted group traffic");
    }

    /// Adding a third member advances the group epoch. Members who were
    /// already present must apply the commit or they fall out of sync.
    #[test]
    fn existing_member_needs_the_commit_when_a_third_joins() {
        let mut alice = Member::new("alice").unwrap();
        let mut bob = Member::new("bob").unwrap();
        let mut carol = Member::new("carol").unwrap();

        alice.create_group().unwrap();
        let invite = alice.add_member(&bob.key_package().unwrap()).unwrap();
        bob.join(&invite.welcome).unwrap();

        // Alice admits Carol. This moves alice and carol to a new epoch.
        let invite = alice.add_member(&carol.key_package().unwrap()).unwrap();
        carol.join(&invite.welcome).unwrap();

        // Bob must be given the commit, or he is left in the old epoch.
        bob.receive(&invite.commit).unwrap();

        let wire = alice.send("everyone still here?").unwrap();
        assert_eq!(bob.receive(&wire).unwrap().as_deref(), Some("everyone still here?"));
        assert_eq!(carol.receive(&wire).unwrap().as_deref(), Some("everyone still here?"));
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

            let invite = alice.add_member(&bob.key_package().unwrap()).unwrap();
            bob.join(&invite.welcome).unwrap();
            assert_eq!(alice.member_count(), 2);
        } // alice is dropped: everything now has to come off disk

        let mut alice = Member::restore(Vault::with_key(&path, [7u8; 32]), "alice").unwrap();
        assert_eq!(alice.identity(), identity_before);
        assert_eq!(alice.member_count(), 2, "group did not survive the restart");

        // The restored ratchet state must still work in both directions.
        let wire = alice.send("still here after a restart").unwrap();
        assert_eq!(bob.receive(&wire).unwrap().as_deref(), Some("still here after a restart"));

        let wire = bob.send("so am i").unwrap();
        assert_eq!(alice.receive(&wire).unwrap().as_deref(), Some("so am i"));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Opening a fresh path must produce a working member, not an error.
    #[test]
    fn first_run_creates_a_vault() {
        let (path, vault) = test_vault("newcomer");
        let member = Member::restore(vault, "newcomer").unwrap();
        assert_eq!(member.identity(), "newcomer");
        assert_eq!(member.member_count(), 0);
        assert!(path.exists(), "vault file was not written");
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
        Member::restore(vault, "guarded").unwrap().create_group().unwrap();

        let wrong = Vault::with_key(&path, [9u8; 32]);
        assert!(matches!(Member::restore(wrong, "guarded"), Err(Error::Store(_))));
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
