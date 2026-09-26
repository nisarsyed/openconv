//! Core client logic for OpenConv: MLS group state and message encryption.
//!
//! The server never sees plaintext. Everything in this crate runs on the
//! client; the wire carries only TLS-serialized MLS messages.

use openmls::prelude::{tls_codec::*, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;

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
}

type Result<T> = std::result::Result<T, Error>;

/// Wraps any openmls error into our own, so callers never depend on
/// openmls error types leaking through the FFI boundary.
fn mls<E: std::fmt::Display>(e: E) -> Error {
    Error::Mls(e.to_string())
}

/// One participant on one device.
pub struct Member {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    group: Option<MlsGroup>,
}

impl Member {
    /// Create a fresh identity. `identity` is the display name carried in
    /// the MLS basic credential.
    pub fn new(identity: &str) -> Result<Self> {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).map_err(mls)?;
        signer.store(provider.storage()).map_err(mls)?;

        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };

        Ok(Self { provider, signer, credential, group: None })
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
        Ok(())
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

        Ok(Invite {
            welcome: welcome.tls_serialize_detached()?,
            commit: commit.tls_serialize_detached()?,
        })
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
        Ok(())
    }

    /// Encrypt an application message for the group.
    pub fn send(&mut self, text: &str) -> Result<Vec<u8>> {
        let group = self.group.as_mut().ok_or(Error::NoGroup)?;
        Ok(group
            .create_message(&self.provider, &self.signer, text.as_bytes())
            .map_err(mls)?
            .tls_serialize_detached()?)
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

        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                Ok(Some(String::from_utf8_lossy(&app.into_bytes()).into_owned()))
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                group.merge_staged_commit(&self.provider, *commit).map_err(mls)?;
                Ok(None)
            }
            _ => Ok(None),
        }
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

    #[test]
    fn sending_before_joining_is_an_error() {
        let mut alice = Member::new("alice").unwrap();
        assert!(matches!(alice.send("nope"), Err(Error::NoGroup)));
    }
}
