//! Message encryption and decryption using the Signal protocol (Double Ratchet).
//!
//! Provides `encrypt_message` and `decrypt_message` functions for pairwise
//! end-to-end encrypted messaging. Uses libsignal's `message_encrypt`,
//! `message_decrypt_prekey`, and `message_decrypt_signal` under the hood.
//!
//! Auto-recovery: when decryption detects a corrupted session, it deletes the
//! session via `recover_session` and returns `CryptoError::SessionCorrupted`
//! so the caller can request a fresh pre-key bundle and re-establish.

use libsignal_protocol::{
    CiphertextMessageType, PreKeySignalMessage, ProtocolAddress, SignalMessage, SignalProtocolError,
};
use rusqlite::Connection;

use crate::error::CryptoError;
use crate::session::recover_session;
use crate::storage::CryptoStore;

/// The type of Signal protocol message, indicating how it should be decrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// First message in a session — contains X3DH initial key exchange material.
    /// Recipient uses `message_decrypt_prekey()` which also establishes the session.
    PreKey,
    /// Subsequent messages in an established session (Double Ratchet).
    /// Recipient uses `message_decrypt_signal()`.
    Signal,
}

impl MessageType {
    /// Returns the string tag used in the nonce field of the wire format.
    pub fn as_nonce_tag(&self) -> &'static str {
        match self {
            MessageType::PreKey => "prekey",
            MessageType::Signal => "signal",
        }
    }

    /// Parse from the nonce field string. Returns `None` for unrecognized values.
    /// Matching is case-sensitive: only `"prekey"` and `"signal"` are recognized.
    pub fn from_nonce_tag(tag: &str) -> Option<Self> {
        match tag {
            "prekey" => Some(MessageType::PreKey),
            "signal" => Some(MessageType::Signal),
            _ => None,
        }
    }
}

/// An encrypted message produced by `encrypt_message`.
#[derive(Debug)]
pub struct EncryptedMessage {
    /// The ciphertext bytes (Signal protocol encoded, including embedded nonce).
    pub ciphertext: Vec<u8>,
    /// Whether this is a PreKey message (first in session) or Signal message (subsequent).
    pub message_type: MessageType,
}

/// Encrypt a plaintext message to a remote recipient.
///
/// Requires an established session (created via `create_outgoing_session`).
/// Returns `CryptoError::SessionNotFound` if no session exists.
///
/// The session ratchet advance and ciphertext creation are atomic — wrapped
/// in a transaction so a partial failure cannot desync ratchet state.
pub fn encrypt_message(
    conn: &Connection,
    recipient: &ProtocolAddress,
    plaintext: &[u8],
) -> Result<EncryptedMessage, CryptoError> {
    let tx = conn.unchecked_transaction()?;

    let mut session_store = CryptoStore::new(conn);
    let mut identity_store = CryptoStore::new(conn);

    // Check session exists
    let session = futures::executor::block_on(libsignal_protocol::SessionStore::load_session(
        &session_store,
        recipient,
    ))
    .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;

    if session.is_none() {
        return Err(CryptoError::SessionNotFound {
            address: recipient.name().to_string(),
        });
    }

    let now = std::time::SystemTime::now();
    let ciphertext_message = futures::executor::block_on(libsignal_protocol::message_encrypt(
        plaintext,
        recipient,
        &mut session_store,
        &mut identity_store,
        now,
        &mut rand::rng(),
    ))
    .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;

    let message_type = match ciphertext_message.message_type() {
        CiphertextMessageType::PreKey => MessageType::PreKey,
        CiphertextMessageType::Whisper => MessageType::Signal,
        other => {
            return Err(CryptoError::SignalProtocolError(format!(
                "unexpected ciphertext message type: {other:?}"
            )));
        }
    };

    let result = EncryptedMessage {
        ciphertext: ciphertext_message.serialize().to_vec(),
        message_type,
    };

    tx.commit()?;
    Ok(result)
}

/// Decrypt a ciphertext message from a remote sender.
///
/// For `MessageType::PreKey` messages, this also establishes the session on the
/// recipient side. All store mutations are wrapped in a transaction.
///
/// On session corruption, attempts auto-recovery (deletes the session) and
/// returns `CryptoError::SessionCorrupted` so the caller can re-establish.
pub fn decrypt_message(
    conn: &Connection,
    sender: &ProtocolAddress,
    ciphertext: &[u8],
    message_type: MessageType,
) -> Result<Vec<u8>, CryptoError> {
    let tx = conn.unchecked_transaction()?;

    let result = decrypt_inner(conn, sender, ciphertext, message_type);

    match result {
        Ok(plaintext) => {
            tx.commit()?;
            Ok(plaintext)
        }
        Err(e) => {
            // Drop tx (implicit rollback) before attempting recovery
            drop(tx);

            if should_attempt_recovery(&e) {
                if let Err(recovery_err) = recover_session(conn, sender) {
                    tracing::warn!(
                        address = sender.name(),
                        error = %recovery_err,
                        "session recovery failed"
                    );
                }
                Err(CryptoError::SessionCorrupted {
                    address: sender.name().to_string(),
                    detail: e.to_string(),
                })
            } else {
                Err(e)
            }
        }
    }
}

/// Inner decrypt logic, separated so the caller can handle transaction + recovery.
fn decrypt_inner(
    conn: &Connection,
    sender: &ProtocolAddress,
    ciphertext: &[u8],
    message_type: MessageType,
) -> Result<Vec<u8>, CryptoError> {
    let mut session_store = CryptoStore::new(conn);
    let mut identity_store = CryptoStore::new(conn);
    let mut pre_key_store = CryptoStore::new(conn);
    let signed_pre_key_store = CryptoStore::new(conn);
    let mut kyber_pre_key_store = CryptoStore::new(conn);

    match message_type {
        MessageType::PreKey => {
            let msg = PreKeySignalMessage::try_from(ciphertext)
                .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

            futures::executor::block_on(libsignal_protocol::message_decrypt_prekey(
                &msg,
                sender,
                &mut session_store,
                &mut identity_store,
                &mut pre_key_store,
                &signed_pre_key_store,
                &mut kyber_pre_key_store,
                &mut rand::rng(),
            ))
            .map_err(classify_decrypt_error)
        }
        MessageType::Signal => {
            let msg = SignalMessage::try_from(ciphertext)
                .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

            futures::executor::block_on(libsignal_protocol::message_decrypt_signal(
                &msg,
                sender,
                &mut session_store,
                &mut identity_store,
                &mut rand::rng(),
            ))
            .map_err(classify_decrypt_error)
        }
    }
}

/// Classify a libsignal decrypt error into a CryptoError.
///
/// Only `InvalidMessage`, `InvalidSessionStructure`, and `InvalidState` are
/// considered recoverable (mapped to `SignalProtocolError` which triggers
/// auto-recovery). All other errors are mapped to non-recoverable variants.
fn classify_decrypt_error(err: SignalProtocolError) -> CryptoError {
    match &err {
        // Recoverable session errors — trigger auto-recovery
        SignalProtocolError::InvalidMessage(_, _)
        | SignalProtocolError::InvalidSessionStructure(_)
        | SignalProtocolError::InvalidState(_, _) => {
            CryptoError::SignalProtocolError(err.to_string())
        }
        // Non-recoverable errors
        SignalProtocolError::DuplicatedMessage(_, _) => {
            CryptoError::DecryptionFailed(err.to_string())
        }
        SignalProtocolError::SessionNotFound(addr) => CryptoError::SessionNotFound {
            address: addr.name().to_string(),
        },
        SignalProtocolError::NoSenderKeyState { .. } => CryptoError::SessionNotFound {
            address: err.to_string(),
        },
        // All other errors are non-recoverable — do NOT trigger session deletion
        _ => CryptoError::DecryptionFailed(err.to_string()),
    }
}

/// Decide whether a CryptoError warrants auto-recovery (session deletion).
///
/// Only errors classified as `SignalProtocolError` by `classify_decrypt_error`
/// trigger recovery — these correspond to `InvalidMessage`, `InvalidSessionStructure`,
/// and `InvalidState`.
fn should_attempt_recovery(err: &CryptoError) -> bool {
    matches!(err, CryptoError::SignalProtocolError(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_identity;
    use crate::prekeys::generate_pre_key_bundle;
    use crate::session::create_outgoing_session;
    use crate::storage::init_test_db;
    use libsignal_protocol::DeviceId;

    /// Creates two in-memory test databases, generates identities for Alice and Bob,
    /// creates pre-key bundles, and establishes an outgoing session from Alice to Bob.
    fn setup_alice_bob_session() -> (Connection, Connection, ProtocolAddress, ProtocolAddress) {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let bob_address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();
        let alice_address = ProtocolAddress::new(
            "alice-user-id".to_string(),
            DeviceId::new(1).expect("valid"),
        );

        (alice_conn, bob_conn, bob_address, alice_address)
    }

    #[test]
    fn encrypt_message_with_established_session_returns_encrypted_message() {
        let (alice_conn, _bob_conn, bob_address, _alice_address) = setup_alice_bob_session();

        let result = encrypt_message(&alice_conn, &bob_address, b"hello");
        assert!(result.is_ok());

        let encrypted = result.unwrap();
        assert!(!encrypted.ciphertext.is_empty());
        assert_eq!(encrypted.message_type, MessageType::PreKey);
    }

    #[test]
    fn encrypt_message_without_session_returns_session_not_found() {
        let alice_conn = init_test_db();
        generate_identity(&alice_conn).unwrap();

        let unknown_address =
            ProtocolAddress::new("unknown-user".to_string(), DeviceId::new(1).expect("valid"));
        let result = encrypt_message(&alice_conn, &unknown_address, b"hello");
        assert!(matches!(result, Err(CryptoError::SessionNotFound { .. })));
    }

    #[test]
    fn encrypt_then_decrypt_round_trips_plaintext() {
        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        let encrypted = encrypt_message(&alice_conn, &bob_address, b"hello world").unwrap();
        let decrypted = decrypt_message(
            &bob_conn,
            &alice_address,
            &encrypted.ciphertext,
            encrypted.message_type,
        )
        .unwrap();

        assert_eq!(decrypted, b"hello world");
    }

    #[test]
    fn decrypt_prekey_message_establishes_session_on_recipient_side() {
        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        let encrypted = encrypt_message(&alice_conn, &bob_address, b"first message").unwrap();
        assert_eq!(encrypted.message_type, MessageType::PreKey);

        decrypt_message(
            &bob_conn,
            &alice_address,
            &encrypted.ciphertext,
            encrypted.message_type,
        )
        .unwrap();

        // Bob should now have a session with Alice
        let session_count: u32 = bob_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions WHERE address = ?1",
                [alice_address.name()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count, 1);
    }

    #[test]
    fn multiple_messages_in_sequence_decrypt_correctly() {
        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        // Before Bob responds, all of Alice's messages are PreKey type
        // (the unacknowledged pre-key flag is only cleared on round-trip)
        let first = encrypt_message(&alice_conn, &bob_address, b"message one").unwrap();
        assert_eq!(first.message_type, MessageType::PreKey);

        // Bob decrypts first message (establishes session)
        let d1 = decrypt_message(
            &bob_conn,
            &alice_address,
            &first.ciphertext,
            first.message_type,
        )
        .unwrap();
        assert_eq!(d1, b"message one");

        // Bob replies to Alice using the session established by decrypt
        let bob_reply = encrypt_message(&bob_conn, &alice_address, b"reply").unwrap();
        let _ = decrypt_message(
            &alice_conn,
            &bob_address,
            &bob_reply.ciphertext,
            bob_reply.message_type,
        )
        .unwrap();

        // Now Alice's subsequent messages should be Signal type
        let messages = [b"message two".as_ref(), b"message three"];
        for msg in &messages {
            let enc = encrypt_message(&alice_conn, &bob_address, msg).unwrap();
            assert_eq!(enc.message_type, MessageType::Signal);

            let dec = decrypt_message(&bob_conn, &alice_address, &enc.ciphertext, enc.message_type)
                .unwrap();
            assert_eq!(dec, *msg);
        }
    }

    #[test]
    fn out_of_order_messages_decrypt_correctly() {
        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        let m1 = encrypt_message(&alice_conn, &bob_address, b"m1").unwrap();
        let m2 = encrypt_message(&alice_conn, &bob_address, b"m2").unwrap();
        let m3 = encrypt_message(&alice_conn, &bob_address, b"m3").unwrap();

        // Decrypt m1 first (PreKey message establishes session)
        let d1 =
            decrypt_message(&bob_conn, &alice_address, &m1.ciphertext, m1.message_type).unwrap();
        assert_eq!(d1, b"m1");

        // Decrypt m3 (skipping m2)
        let d3 =
            decrypt_message(&bob_conn, &alice_address, &m3.ciphertext, m3.message_type).unwrap();
        assert_eq!(d3, b"m3");

        // Now decrypt m2
        let d2 =
            decrypt_message(&bob_conn, &alice_address, &m2.ciphertext, m2.message_type).unwrap();
        assert_eq!(d2, b"m2");
    }

    #[test]
    fn message_from_unknown_sender_fails_with_decryption_error() {
        let bob_conn = init_test_db();
        generate_identity(&bob_conn).unwrap();

        let unknown_address = ProtocolAddress::new(
            "unknown-sender".to_string(),
            DeviceId::new(1).expect("valid"),
        );

        // Fabricate some bytes that aren't a valid SignalMessage
        let fake_ciphertext = vec![0u8; 64];
        let result = decrypt_message(
            &bob_conn,
            &unknown_address,
            &fake_ciphertext,
            MessageType::Signal,
        );
        assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
    }

    #[test]
    fn corrupted_ciphertext_returns_decryption_failed_or_session_corrupted() {
        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        let mut encrypted = encrypt_message(&alice_conn, &bob_address, b"hello").unwrap();

        // Corrupt the ciphertext by flipping bytes in the middle
        if encrypted.ciphertext.len() > 10 {
            for i in 5..10 {
                encrypted.ciphertext[i] ^= 0xFF;
            }
        }

        let result = decrypt_message(
            &bob_conn,
            &alice_address,
            &encrypted.ciphertext,
            encrypted.message_type,
        );
        // Corrupted ciphertext triggers either DecryptionFailed (deserialization)
        // or SessionCorrupted (libsignal decrypt error with recovery)
        assert!(matches!(
            result,
            Err(CryptoError::DecryptionFailed(_)) | Err(CryptoError::SessionCorrupted { .. })
        ));
    }

    #[test]
    fn message_type_correctly_identifies_prekey_vs_signal() {
        assert_ne!(MessageType::PreKey, MessageType::Signal);

        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        // First message should be PreKey
        let first = encrypt_message(&alice_conn, &bob_address, b"first").unwrap();
        assert_eq!(first.message_type, MessageType::PreKey);

        // Bob decrypts — establishes session on Bob's side
        decrypt_message(
            &bob_conn,
            &alice_address,
            &first.ciphertext,
            first.message_type,
        )
        .unwrap();

        // Bob replies using the session created by decrypt_prekey
        let bob_reply = encrypt_message(&bob_conn, &alice_address, b"reply").unwrap();
        decrypt_message(
            &alice_conn,
            &bob_address,
            &bob_reply.ciphertext,
            bob_reply.message_type,
        )
        .unwrap();

        // Now Alice's next message should be Signal type (session acknowledged)
        let second = encrypt_message(&alice_conn, &bob_address, b"second").unwrap();
        assert_eq!(second.message_type, MessageType::Signal);
    }

    #[test]
    fn message_type_nonce_tag_round_trips() {
        assert_eq!(MessageType::PreKey.as_nonce_tag(), "prekey");
        assert_eq!(MessageType::Signal.as_nonce_tag(), "signal");

        assert_eq!(
            MessageType::from_nonce_tag("prekey"),
            Some(MessageType::PreKey)
        );
        assert_eq!(
            MessageType::from_nonce_tag("signal"),
            Some(MessageType::Signal)
        );
        assert_eq!(MessageType::from_nonce_tag("unknown"), None);
        assert_eq!(MessageType::from_nonce_tag("PreKey"), None);
    }

    #[test]
    fn auto_recovery_deletes_session_on_corruption() {
        let (alice_conn, bob_conn, bob_address, alice_address) = setup_alice_bob_session();

        // Alice encrypts a message to Bob
        let encrypted = encrypt_message(&alice_conn, &bob_address, b"hello").unwrap();

        // Bob decrypts it (establishes session)
        decrypt_message(
            &bob_conn,
            &alice_address,
            &encrypted.ciphertext,
            encrypted.message_type,
        )
        .unwrap();

        // Verify Bob has a session with Alice
        let session_count: u32 = bob_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions WHERE address = ?1",
                [alice_address.name()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count, 1);

        // Now corrupt Bob's session by directly mangling the stored session data
        bob_conn
            .execute(
                "UPDATE crypto_sessions SET session_data = X'DEADBEEF' WHERE address = ?1",
                [alice_address.name()],
            )
            .unwrap();

        // Alice sends another message (Signal type, after round-trip with Bob)
        // But we'll use a fresh PreKey message since session was corrupted
        let encrypted2 = encrypt_message(&alice_conn, &bob_address, b"hello again").unwrap();

        // Bob tries to decrypt with the corrupted session
        let result = decrypt_message(
            &bob_conn,
            &alice_address,
            &encrypted2.ciphertext,
            encrypted2.message_type,
        );

        // Should get SessionCorrupted (auto-recovery triggered)
        assert!(
            matches!(result, Err(CryptoError::SessionCorrupted { .. })),
            "expected SessionCorrupted, got: {result:?}"
        );

        // The corrupted session should have been deleted by auto-recovery
        let session_count_after: u32 = bob_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions WHERE address = ?1",
                [alice_address.name()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count_after, 0);
    }
}
