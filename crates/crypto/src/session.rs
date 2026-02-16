//! Signal protocol session management.
//!
//! Provides X3DH/PQXDH-based outgoing session creation, session recovery
//! on corruption, and skipped message key pruning.

use libsignal_protocol::{
    DeviceId, IdentityKey, PreKeyBundle, ProtocolAddress, PublicKey,
    SignedPreKeyId, KyberPreKeyId, kem,
};
use rusqlite::Connection;

use crate::error::CryptoError;
use crate::prekeys::SerializedPreKeyBundle;
use crate::storage::CryptoStore;

/// Describes the result of a session recovery attempt.
#[derive(Debug, PartialEq)]
pub enum RecoveryAction {
    /// Session was deleted. Caller should request a new pre-key bundle
    /// from the server and re-establish the session.
    SessionReset,
    /// Session is unrecoverable. The user should be notified.
    Unrecoverable(String),
}

/// Create an outgoing session with a remote party using their pre-key bundle.
///
/// The `remote_bundle` is JSON-serialized `SerializedPreKeyBundle` data from the server.
/// The bundle's `user_id` field (server-assigned UUID) is used as the address name.
///
/// Returns the `ProtocolAddress` for subsequent message encryption calls.
pub fn create_outgoing_session(
    conn: &Connection,
    remote_bundle: &[u8],
) -> Result<ProtocolAddress, CryptoError> {
    let bundle: SerializedPreKeyBundle = serde_json::from_slice(remote_bundle)?;

    // Reconstruct libsignal types from serialized bundle
    let identity_key = IdentityKey::decode(&bundle.identity_key)?;

    let signed_pre_key_public = PublicKey::deserialize(&bundle.signed_pre_key)
        .map_err(|e| CryptoError::InvalidKey(e.to_string()))?;

    let kyber_pre_key_public = kem::PublicKey::deserialize(&bundle.kyber_pre_key)
        .map_err(|e| CryptoError::InvalidKey(format!("invalid kyber key: {e}")))?;

    let pre_key_bundle = PreKeyBundle::new(
        bundle.registration_id,
        DeviceId::new(1).expect("device ID 1 is valid"),
        None, // no one-time pre-key in V1 bundle
        SignedPreKeyId::from(bundle.signed_pre_key_id),
        signed_pre_key_public,
        bundle.signed_pre_key_signature.clone(),
        KyberPreKeyId::from(bundle.kyber_pre_key_id),
        kyber_pre_key_public,
        bundle.kyber_pre_key_signature.clone(),
        identity_key,
    )
    .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;

    let remote_address = ProtocolAddress::new(bundle.user_id, DeviceId::new(1).expect("valid"));

    // Wrap in transaction so identity save + session save are atomic
    let tx = conn.unchecked_transaction()?;

    let mut session_store = CryptoStore::new(conn);
    let mut identity_store = CryptoStore::new(conn);
    let now = std::time::SystemTime::now();

    futures::executor::block_on(libsignal_protocol::process_prekey_bundle(
        &remote_address,
        &mut session_store,
        &mut identity_store,
        &pre_key_bundle,
        now,
        &mut rand::rng(),
    ))
    .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;

    tx.commit()?;

    Ok(remote_address)
}

/// Recover from a corrupted session by deleting it and its associated state.
///
/// Deletes the session record and all associated skipped message keys.
/// The caller should request a new pre-key bundle and call `create_outgoing_session`.
pub fn recover_session(
    conn: &Connection,
    address: &ProtocolAddress,
) -> Result<RecoveryAction, CryptoError> {
    let tx = conn.unchecked_transaction()?;

    let addr_name = address.name();
    let device_id: u32 = address.device_id().into();

    conn.execute(
        "DELETE FROM crypto_sessions WHERE address = ?1 AND device_id = ?2",
        rusqlite::params![addr_name, device_id],
    )?;

    conn.execute(
        "DELETE FROM crypto_skipped_message_keys WHERE session_address = ?1 AND session_device_id = ?2",
        rusqlite::params![addr_name, device_id],
    )?;

    tx.commit()?;
    Ok(RecoveryAction::SessionReset)
}

/// Delete skipped message keys older than `max_age_seconds`.
///
/// Returns the number of entries deleted. Recommended to call on app startup
/// with `max_age_seconds = 604800` (7 days).
pub fn prune_old_skipped_keys(
    conn: &Connection,
    max_age_seconds: u64,
) -> Result<u32, CryptoError> {
    let store = CryptoStore::new(conn);
    store.prune_skipped_message_keys(max_age_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_identity;
    use crate::prekeys::generate_pre_key_bundle;
    use crate::storage::init_test_db;

    #[test]
    fn create_outgoing_session_with_valid_bundle_establishes_session() {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();
        assert_eq!(address.name(), "bob-user-id");

        // Session should exist in Alice's DB
        let session_count: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count, 1);
    }

    #[test]
    fn create_outgoing_session_stores_session_in_crypto_sessions() {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();

        let addr_name = address.name();
        let (session_data, created_at, last_used_at): (Vec<u8>, i64, i64) = alice_conn
            .query_row(
                "SELECT session_data, created_at, last_used_at FROM crypto_sessions WHERE address = ?1",
                [addr_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert!(!session_data.is_empty());
        assert!(created_at > 0);
        assert!(last_used_at > 0);
    }

    #[test]
    fn create_outgoing_session_with_invalid_bundle_returns_error() {
        let alice_conn = init_test_db();
        generate_identity(&alice_conn).unwrap();

        let result = create_outgoing_session(&alice_conn, b"invalid json garbage");
        assert!(result.is_err());

        // No session should be stored
        let session_count: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count, 0);
    }

    #[test]
    fn recover_session_deletes_session_from_db() {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();
        recover_session(&alice_conn, &address).unwrap();

        let session_count: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_count, 0);
    }

    #[test]
    fn recover_session_deletes_associated_skipped_message_keys() {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();

        // Insert dummy skipped message keys
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        alice_conn
            .execute(
                "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
                 VALUES (?1, ?2, X'AA', 1, X'BB', ?3)",
                rusqlite::params![address.name(), u32::from(address.device_id()), now],
            )
            .unwrap();

        recover_session(&alice_conn, &address).unwrap();

        let skipped_count: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_skipped_message_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(skipped_count, 0);
    }

    #[test]
    fn recover_session_returns_session_reset_action() {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();
        let action = recover_session(&alice_conn, &address).unwrap();
        assert_eq!(action, RecoveryAction::SessionReset);
    }

    #[test]
    fn prune_old_skipped_keys_deletes_old_entries() {
        let conn = init_test_db();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let thirty_days_ago = now - (30 * 86400);

        conn.execute(
            "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
             VALUES ('addr', 1, X'AA', 1, X'BB', ?1)",
            [thirty_days_ago],
        )
        .unwrap();

        let deleted = prune_old_skipped_keys(&conn, 7 * 86400).unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn prune_old_skipped_keys_keeps_recent_entries() {
        let conn = init_test_db();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
             VALUES ('addr', 1, X'AA', 1, X'BB', ?1)",
            [now],
        )
        .unwrap();

        let deleted = prune_old_skipped_keys(&conn, 7 * 86400).unwrap();
        assert_eq!(deleted, 0);

        let remaining: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_skipped_message_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn prune_old_skipped_keys_returns_correct_count() {
        let conn = init_test_db();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let old = now - (30 * 86400);

        // 3 old entries
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
                 VALUES ('addr', 1, X'AA', ?1, X'BB', ?2)",
                rusqlite::params![i, old],
            )
            .unwrap();
        }

        // 2 recent entries
        for i in 4..=5 {
            conn.execute(
                "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
                 VALUES ('addr', 1, X'AA', ?1, X'BB', ?2)",
                rusqlite::params![i, now],
            )
            .unwrap();
        }

        let deleted = prune_old_skipped_keys(&conn, 7 * 86400).unwrap();
        assert_eq!(deleted, 3);

        let remaining: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_skipped_message_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 2);
    }

    #[test]
    fn transaction_rollback_on_invalid_bundle_leaves_db_unchanged() {
        let alice_conn = init_test_db();
        generate_identity(&alice_conn).unwrap();

        let session_before: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let identity_before: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_trusted_identities",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let _ = create_outgoing_session(&alice_conn, b"bad data");

        let session_after: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_sessions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let identity_after: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_trusted_identities",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(session_before, session_after);
        assert_eq!(identity_before, identity_after);
    }

    #[test]
    fn create_outgoing_session_persists_remote_identity() {
        let alice_conn = init_test_db();
        let bob_conn = init_test_db();

        generate_identity(&alice_conn).unwrap();
        generate_identity(&bob_conn).unwrap();

        let bob_bundle = generate_pre_key_bundle(&bob_conn, "bob-user-id").unwrap();
        let bundle_json = serde_json::to_vec(&bob_bundle).unwrap();

        let address = create_outgoing_session(&alice_conn, &bundle_json).unwrap();

        let identity_count: u32 = alice_conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_trusted_identities WHERE address = ?1",
                [address.name()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(identity_count, 1);
    }

    #[test]
    fn recover_session_on_nonexistent_address_succeeds() {
        let conn = init_test_db();
        let address = ProtocolAddress::new(
            "nonexistent-user".to_string(),
            DeviceId::new(1).expect("valid"),
        );
        let action = recover_session(&conn, &address).unwrap();
        assert_eq!(action, RecoveryAction::SessionReset);
    }
}
