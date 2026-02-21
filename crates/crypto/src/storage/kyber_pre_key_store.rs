//! KyberPreKeyStore trait implementation for CryptoStore.

use async_trait::async_trait;
use libsignal_protocol::{
    GenericSignedPreKey, KyberPreKeyId, KyberPreKeyRecord, KyberPreKeyStore, PublicKey,
    SignalProtocolError, SignedPreKeyId,
};

use crate::storage::CryptoStore;

#[async_trait(?Send)]
impl KyberPreKeyStore for CryptoStore<'_> {
    async fn get_kyber_pre_key(
        &self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<KyberPreKeyRecord, SignalProtocolError> {
        let id: u32 = kyber_prekey_id.into();
        let record_bytes: Vec<u8> = self
            .conn
            .query_row(
                "SELECT record FROM crypto_kyber_pre_keys WHERE key_id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| {
                SignalProtocolError::InvalidState("get_kyber_pre_key", "key not found".into())
            })?;

        KyberPreKeyRecord::deserialize(&record_bytes)
    }

    async fn save_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        record: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let id: u32 = kyber_prekey_id.into();
        let record_bytes = record.serialize()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                SignalProtocolError::InvalidState(
                    "save_kyber_pre_key",
                    "system clock before epoch".into(),
                )
            })?
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO crypto_kyber_pre_keys (key_id, record, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, record_bytes, now],
            )
            .map_err(|e| SignalProtocolError::InvalidState("save_kyber_pre_key", e.to_string()))?;

        Ok(())
    }

    async fn mark_kyber_pre_key_used(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        _ec_prekey_id: SignedPreKeyId,
        _base_key: &PublicKey,
    ) -> Result<(), SignalProtocolError> {
        // For last-resort Kyber pre-keys, we don't delete on use.
        // For one-time Kyber pre-keys (future), delete after use.
        // For V1, all Kyber pre-keys are last-resort, so this is a no-op.
        let _id: u32 = kyber_prekey_id.into();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;
    use crate::storage::CryptoStore;
    use libsignal_protocol::kem;

    fn create_kyber_pre_key_record(id: u32) -> KyberPreKeyRecord {
        let signing_key = libsignal_protocol::KeyPair::generate(&mut rand::rng());
        KyberPreKeyRecord::generate(
            kem::KeyType::Kyber1024,
            KyberPreKeyId::from(id),
            &signing_key.private_key,
        )
        .unwrap()
    }

    #[test]
    fn save_then_get_kyber_pre_key_round_trips() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let record = create_kyber_pre_key_record(1);

        futures::executor::block_on(store.save_kyber_pre_key(KyberPreKeyId::from(1), &record))
            .unwrap();
        let loaded =
            futures::executor::block_on(store.get_kyber_pre_key(KyberPreKeyId::from(1))).unwrap();

        assert_eq!(loaded.serialize().unwrap(), record.serialize().unwrap());
    }

    #[test]
    fn get_kyber_pre_key_returns_error_for_nonexistent_id() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let result =
            futures::executor::block_on(store.get_kyber_pre_key(KyberPreKeyId::from(99999)));
        assert!(result.is_err());
    }

    #[test]
    fn mark_kyber_pre_key_used_does_not_error() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let record = create_kyber_pre_key_record(1);
        let dummy_key = libsignal_protocol::KeyPair::generate(&mut rand::rng());

        futures::executor::block_on(store.save_kyber_pre_key(KyberPreKeyId::from(1), &record))
            .unwrap();

        let result = futures::executor::block_on(store.mark_kyber_pre_key_used(
            KyberPreKeyId::from(1),
            SignedPreKeyId::from(1),
            &dummy_key.public_key,
        ));
        assert!(result.is_ok());
    }
}
