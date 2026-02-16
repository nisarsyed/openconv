//! Pre-key management for the Signal protocol.
//!
//! Generates signed pre-key bundles and one-time pre-keys for X3DH
//! asynchronous key exchange. Handles upload tracking, replenishment
//! thresholds, and signed pre-key rotation.

use libsignal_protocol::{
    GenericSignedPreKey, KeyPair, PreKeyId, PreKeyRecord, PreKeyStore, SignedPreKeyId,
    SignedPreKeyRecord, SignedPreKeyStore, Timestamp,
};
use rusqlite::Connection;

use crate::error::CryptoError;
use crate::identity::get_identity;
use crate::storage::CryptoStore;

/// A serialized pre-key bundle containing the identity key and signed pre-key,
/// ready for upload to the server. Recipients use this to establish X3DH sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SerializedPreKeyBundle {
    /// The local identity public key (Curve25519), serialized via libsignal
    pub identity_key: Vec<u8>,
    /// The ID of the signed pre-key
    pub signed_pre_key_id: u32,
    /// The signed pre-key public key bytes, serialized via libsignal
    pub signed_pre_key: Vec<u8>,
    /// Signature over the signed pre-key, created with the identity private key
    pub signed_pre_key_signature: Vec<u8>,
    /// The local registration ID (14-bit range: 1..=16380)
    pub registration_id: u32,
}

/// A serialized one-time pre-key for upload to the server.
/// Each key is used at most once during session establishment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SerializedPreKey {
    /// Unique ID for this pre-key
    pub key_id: u32,
    /// The pre-key public key bytes, serialized via libsignal
    pub public_key: Vec<u8>,
}

/// Generate a pre-key bundle containing the identity key and a new signed pre-key.
///
/// The returned bundle should be uploaded to the server so that other clients
/// can establish X3DH sessions with this device.
pub fn generate_pre_key_bundle(conn: &Connection) -> Result<SerializedPreKeyBundle, CryptoError> {
    let identity = get_identity(conn)?;

    let mut store = CryptoStore::new(conn);
    let reg_bytes = store
        .get_config("registration_id")?
        .ok_or_else(|| CryptoError::IdentityNotInitialized)?;
    let arr: [u8; 4] = reg_bytes
        .try_into()
        .map_err(|_| CryptoError::StorageError("invalid registration_id length".into()))?;
    let registration_id = u32::from_be_bytes(arr);

    // Determine next signed pre-key ID
    let next_spk_id: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(key_id), 0) + 1 FROM crypto_signed_pre_keys",
            [],
            |row| row.get(0),
        )?;

    // Generate signed pre-key
    let spk_pair = KeyPair::generate(&mut rand::rng());
    let signature = identity
        .private_key()
        .calculate_signature(&spk_pair.public_key.serialize(), &mut rand::rng())
        .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;
    let timestamp = Timestamp::from_epoch_millis(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_millis() as u64,
    );
    let spk_record = SignedPreKeyRecord::new(
        SignedPreKeyId::from(next_spk_id),
        timestamp,
        &spk_pair,
        &signature,
    );

    // Store signed pre-key
    futures::executor::block_on(
        store.save_signed_pre_key(SignedPreKeyId::from(next_spk_id), &spk_record),
    )
    .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;

    Ok(SerializedPreKeyBundle {
        identity_key: identity.public_key().serialize().to_vec(),
        signed_pre_key_id: next_spk_id,
        signed_pre_key: spk_pair.public_key.serialize().to_vec(),
        signed_pre_key_signature: Vec::from(signature.as_ref()),
        registration_id,
    })
}

/// Generate a batch of one-time pre-keys for upload to the server.
///
/// Each pre-key is stored in the database with `uploaded = 0`. After uploading
/// to the server, call `mark_pre_keys_uploaded` with the key IDs.
pub fn generate_one_time_pre_keys(
    conn: &Connection,
    count: u32,
) -> Result<Vec<SerializedPreKey>, CryptoError> {
    let tx = conn.unchecked_transaction()?;

    let start_id: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(key_id), 0) + 1 FROM crypto_pre_keys",
            [],
            |row| row.get(0),
        )?;

    let mut keys = Vec::with_capacity(count as usize);
    let mut store = CryptoStore::new(conn);

    for i in 0..count {
        let key_id = start_id + i;
        let pair = KeyPair::generate(&mut rand::rng());
        let record = PreKeyRecord::new(PreKeyId::from(key_id), &pair);

        futures::executor::block_on(store.save_pre_key(PreKeyId::from(key_id), &record))
            .map_err(|e| CryptoError::SignalProtocolError(e.to_string()))?;

        keys.push(SerializedPreKey {
            key_id,
            public_key: pair.public_key.serialize().to_vec(),
        });
    }

    tx.commit()?;
    Ok(keys)
}

/// Mark one-time pre-keys as uploaded to the server.
///
/// Sets the `uploaded` flag to 1 for each key ID in the provided slice.
pub fn mark_pre_keys_uploaded(conn: &Connection, key_ids: &[u32]) -> Result<(), CryptoError> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = conn.prepare("UPDATE crypto_pre_keys SET uploaded = 1 WHERE key_id = ?1")?;
        for &id in key_ids {
            stmt.execute([id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Check if the number of uploaded pre-keys is below the replenishment threshold.
///
/// Returns `true` if fewer than `threshold` pre-keys have been uploaded,
/// indicating that new pre-keys should be generated and uploaded.
pub fn needs_pre_key_replenishment(conn: &Connection, threshold: u32) -> Result<bool, CryptoError> {
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM crypto_pre_keys WHERE uploaded = 1",
        [],
        |row| row.get(0),
    )?;
    Ok(count < threshold)
}

/// Rotate the signed pre-key by generating a new one with a new ID.
///
/// The old signed pre-key is retained in storage because existing sessions
/// may still reference it. Returns an updated bundle for server upload.
pub fn rotate_signed_pre_key(conn: &Connection) -> Result<SerializedPreKeyBundle, CryptoError> {
    generate_pre_key_bundle(conn)
}

/// Check if the most recent signed pre-key is older than `max_age_days`.
///
/// Returns `true` if the most recent signed pre-key was created more than
/// `max_age_days` ago, or if no signed pre-key exists at all.
pub fn is_signed_pre_key_stale(conn: &Connection, max_age_days: u32) -> Result<bool, CryptoError> {
    let result: Result<i64, _> = conn.query_row(
        "SELECT MAX(created_at) FROM crypto_signed_pre_keys",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(created_at) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before epoch")
                .as_secs() as i64;
            let max_age_secs = (max_age_days as i64) * 86400;
            Ok(now - created_at > max_age_secs)
        }
        Err(rusqlite::Error::InvalidColumnType(..)) => {
            // MAX returns NULL when table is empty
            Ok(true)
        }
        Err(e) => Err(CryptoError::from(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_identity;
    use crate::storage::init_test_db;

    #[test]
    fn generate_pre_key_bundle_returns_bundle_with_correct_fields() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let bundle = generate_pre_key_bundle(&conn).unwrap();
        assert!(!bundle.identity_key.is_empty());
        assert!(!bundle.signed_pre_key.is_empty());
        assert!(!bundle.signed_pre_key_signature.is_empty());
        assert!(bundle.signed_pre_key_id > 0);
        assert!(bundle.registration_id >= 1 && bundle.registration_id <= 16380);
    }

    #[test]
    fn generate_pre_key_bundle_stores_signed_pre_key_in_db() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let bundle = generate_pre_key_bundle(&conn).unwrap();

        let row_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_signed_pre_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(row_count, 1);

        let stored_id: u32 = conn
            .query_row(
                "SELECT key_id FROM crypto_signed_pre_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_id, bundle.signed_pre_key_id);
    }

    #[test]
    fn generate_one_time_pre_keys_creates_correct_number_of_keys_in_db() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let keys = generate_one_time_pre_keys(&conn, 50).unwrap();
        assert_eq!(keys.len(), 50);

        let row_count: u32 = conn
            .query_row("SELECT COUNT(*) FROM crypto_pre_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(row_count, 50);
    }

    #[test]
    fn generate_one_time_pre_keys_assigns_sequential_key_ids() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let keys = generate_one_time_pre_keys(&conn, 5).unwrap();
        let ids: Vec<u32> = keys.iter().map(|k| k.key_id).collect();
        for i in 1..ids.len() {
            assert_eq!(ids[i], ids[i - 1] + 1);
        }
    }

    #[test]
    fn generate_one_time_pre_keys_sets_uploaded_to_zero() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        generate_one_time_pre_keys(&conn, 10).unwrap();

        let uploaded_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_pre_keys WHERE uploaded = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(uploaded_count, 0);
    }

    #[test]
    fn mark_pre_keys_uploaded_updates_uploaded_flag() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let keys = generate_one_time_pre_keys(&conn, 5).unwrap();
        let ids_to_mark: Vec<u32> = keys[0..3].iter().map(|k| k.key_id).collect();
        mark_pre_keys_uploaded(&conn, &ids_to_mark).unwrap();

        let uploaded: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_pre_keys WHERE uploaded = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(uploaded, 3);

        let not_uploaded: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_pre_keys WHERE uploaded = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(not_uploaded, 2);
    }

    #[test]
    fn needs_pre_key_replenishment_returns_true_when_below_threshold() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let keys = generate_one_time_pre_keys(&conn, 10).unwrap();
        let ids: Vec<u32> = keys[0..5].iter().map(|k| k.key_id).collect();
        mark_pre_keys_uploaded(&conn, &ids).unwrap();

        assert!(needs_pre_key_replenishment(&conn, 20).unwrap());
    }

    #[test]
    fn needs_pre_key_replenishment_returns_false_when_above_threshold() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let keys = generate_one_time_pre_keys(&conn, 30).unwrap();
        let all_ids: Vec<u32> = keys.iter().map(|k| k.key_id).collect();
        mark_pre_keys_uploaded(&conn, &all_ids).unwrap();

        assert!(!needs_pre_key_replenishment(&conn, 20).unwrap());
    }

    #[test]
    fn rotate_signed_pre_key_creates_new_signed_pre_key_with_new_id() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let first = generate_pre_key_bundle(&conn).unwrap();
        let second = rotate_signed_pre_key(&conn).unwrap();

        assert_ne!(first.signed_pre_key_id, second.signed_pre_key_id);

        let row_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_signed_pre_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(row_count, 2);
    }

    #[test]
    fn is_signed_pre_key_stale_returns_false_for_recently_created_key() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        generate_pre_key_bundle(&conn).unwrap();
        assert!(!is_signed_pre_key_stale(&conn, 7).unwrap());
    }

    #[test]
    fn is_signed_pre_key_stale_returns_true_when_no_signed_pre_key_exists() {
        let conn = init_test_db();
        assert!(is_signed_pre_key_stale(&conn, 7).unwrap());
    }

    #[test]
    fn generate_one_time_pre_keys_sequential_ids_across_batches() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        let batch1 = generate_one_time_pre_keys(&conn, 5).unwrap();
        let last_id_batch1 = batch1.last().unwrap().key_id;

        // Delete some keys from the first batch (simulates consumption)
        conn.execute("DELETE FROM crypto_pre_keys WHERE key_id = ?1", [batch1[2].key_id])
            .unwrap();

        let batch2 = generate_one_time_pre_keys(&conn, 3).unwrap();
        let first_id_batch2 = batch2[0].key_id;

        // Second batch should start after the max existing key_id, not reuse deleted IDs
        assert!(first_id_batch2 > last_id_batch1);
        for i in 1..batch2.len() {
            assert_eq!(batch2[i].key_id, batch2[i - 1].key_id + 1);
        }
    }

    #[test]
    fn is_signed_pre_key_stale_returns_true_for_old_key() {
        let conn = init_test_db();
        generate_identity(&conn).unwrap();

        generate_pre_key_bundle(&conn).unwrap();

        // Backdate the signed pre-key to 8 days ago
        let eight_days_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - (8 * 86400);
        conn.execute(
            "UPDATE crypto_signed_pre_keys SET created_at = ?1",
            [eight_days_ago],
        )
        .unwrap();

        assert!(is_signed_pre_key_stale(&conn, 7).unwrap());
    }
}
