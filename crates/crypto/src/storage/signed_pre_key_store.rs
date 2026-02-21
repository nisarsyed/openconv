//! SignedPreKeyStore trait implementation for CryptoStore.

use async_trait::async_trait;
use libsignal_protocol::{
    GenericSignedPreKey, SignalProtocolError, SignedPreKeyId, SignedPreKeyRecord, SignedPreKeyStore,
};

use crate::storage::CryptoStore;

#[async_trait(?Send)]
impl SignedPreKeyStore for CryptoStore<'_> {
    async fn get_signed_pre_key(
        &self,
        signed_prekey_id: SignedPreKeyId,
    ) -> Result<SignedPreKeyRecord, SignalProtocolError> {
        let id: u32 = signed_prekey_id.into();
        let record_bytes: Vec<u8> = self
            .conn
            .query_row(
                "SELECT record FROM crypto_signed_pre_keys WHERE key_id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| SignalProtocolError::InvalidSignedPreKeyId)?;

        SignedPreKeyRecord::deserialize(&record_bytes)
    }

    async fn save_signed_pre_key(
        &mut self,
        signed_prekey_id: SignedPreKeyId,
        record: &SignedPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let id: u32 = signed_prekey_id.into();
        let record_bytes = record.serialize()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                SignalProtocolError::InvalidState(
                    "save_signed_pre_key",
                    "system clock before epoch".into(),
                )
            })?
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO crypto_signed_pre_keys (key_id, record, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, record_bytes, now],
            )
            .map_err(|e| SignalProtocolError::InvalidState("save_signed_pre_key", e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;
    use crate::storage::CryptoStore;
    use libsignal_protocol::{KeyPair, SignedPreKeyRecord, Timestamp};

    fn create_signed_pre_key_record(id: u32) -> SignedPreKeyRecord {
        let identity_pair = KeyPair::generate(&mut rand::rng());
        let signed_key_pair = KeyPair::generate(&mut rand::rng());
        let signature = identity_pair
            .private_key
            .calculate_signature(&signed_key_pair.public_key.serialize(), &mut rand::rng())
            .unwrap();
        let timestamp = Timestamp::from_epoch_millis(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        );

        SignedPreKeyRecord::new(
            SignedPreKeyId::from(id),
            timestamp,
            &signed_key_pair,
            &signature,
        )
    }

    #[test]
    fn save_then_get_signed_pre_key_round_trips() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let record = create_signed_pre_key_record(1);

        futures::executor::block_on(store.save_signed_pre_key(SignedPreKeyId::from(1), &record))
            .unwrap();
        let loaded =
            futures::executor::block_on(store.get_signed_pre_key(SignedPreKeyId::from(1))).unwrap();

        assert_eq!(loaded.serialize().unwrap(), record.serialize().unwrap());
    }

    #[test]
    fn get_signed_pre_key_returns_error_for_nonexistent_id() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let result =
            futures::executor::block_on(store.get_signed_pre_key(SignedPreKeyId::from(99999)));
        assert!(result.is_err());
    }
}
